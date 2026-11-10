use super::*;
use crate::tips::Tip as _;
use capture_core::video::{
    editor::Timing,
    motion_path::{Purpose, Reduction},
};
/// Cursor path reduction: the mode, plus the "Move before click" lead time for
/// direct paths or the distance threshold (source pixels) for distance mode.
pub(crate) fn reduction_picker(
    ui: &mut egui::Ui,
    id: &str,
    value: &mut Reduction,
    has_clicks: bool,
) -> bool {
    let mut changed = false;
    let mut distance = match *value {
        Reduction::Distance(px) => px,
        _ => Reduction::DEFAULT_DISTANCE,
    };
    let mut lead = match *value {
        Reduction::Clicks(ms) => ms,
        _ => Reduction::DEFAULT_LEAD,
    };
    egui::ComboBox::from_id_salt(("cursor-reduction", id))
        .width(ui.available_width())
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for option in [
                Reduction::Off,
                Reduction::Clicks(lead),
                Reduction::Distance(distance),
            ] {
                if ui
                    .add_enabled(
                        has_clicks || !matches!(option, Reduction::Clicks(_)),
                        egui::Button::selectable(
                            std::mem::discriminant(value) == std::mem::discriminant(&option),
                            option.label(),
                        ),
                    )
                    .tip_disabled(
                        "This recording has no click data, because it was made before ClipCraft \
                         recorded clicks without Step by Step.",
                    )
                    .clicked()
                    && *value != option
                {
                    *value = option;
                    changed = true;
                }
            }
        });
    if let Reduction::Distance(_) = value
        && crate::property_layout::slider(
            ui,
            "Distance",
            egui::Slider::new(
                &mut distance,
                Reduction::MIN_DISTANCE..=Reduction::MAX_DISTANCE,
            )
            .suffix(" px"),
        )
        .changed()
    {
        *value = Reduction::Distance(distance);
        changed = true;
    }
    if let Reduction::Clicks(_) = value
        && crate::property_layout::slider(
            ui,
            "Move before click",
            egui::Slider::new(&mut lead, Reduction::MIN_LEAD..=Reduction::MAX_LEAD).suffix(" ms"),
        )
        .tip("The cursor rests at each click, then glides to the next one starting this long before it.")
        .changed()
    {
        *value = Reduction::Clicks(lead);
        changed = true;
    }
    changed
}
/// "1 of 5" with Previous and Next through every flagged moment, in time order.
fn review_strip(ui: &mut egui::Ui, nav: &mut super::jump::ReviewNav) {
    let Some((at, total)) = nav.position else {
        if nav.finished {
            ui.label("All redactions are reviewed.");
            nav.reopen = ui
                .button("Reopen Export video")
                .tip("Go back to Export video to finish exporting.")
                .clicked();
        }
        return;
    };
    let amber = egui::Color32::from_rgb(242, 201, 76);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{at} of {total}")).strong());
        let mut step = |ui: &mut egui::Ui, name: &str, tip: &str, to: Option<_>| {
            if ui
                .add_enabled(to.is_some(), egui::Button::new(name))
                .tip(tip)
                .clicked()
            {
                nav.chosen = to;
            }
        };
        let (previous, next) = (nav.previous, nav.next);
        step(
            ui,
            "Previous",
            "Go to the previous flagged moment.",
            previous,
        );
        step(ui, "Next", "Go to the next flagged moment.", next);
    });
    if let Some(stop) = nav.stop {
        let time = |ms: u64| format!("{}:{:04.1}", ms / 60_000, (ms % 60_000) as f64 / 1000.);
        ui.colored_label(
            amber,
            format!("Needs review {}–{}", time(stop.start), time(stop.end)),
        );
    }
}
pub(super) fn controls(
    ui: &mut egui::Ui,
    t: &mut Timing,
    cursor: &mut u64,
    nav: &mut super::jump::ReviewNav,
) {
    let Some(path) = t.motion_path.as_ref() else {
        return;
    };
    let privacy = path.purpose == Purpose::Redaction;
    super::property_design::heading(
        ui,
        if privacy { "Redact" } else { "Move" },
        if privacy {
            "Tracked coverage"
        } else {
            "Recorded cursor"
        },
    );
    if privacy {
        review_strip(ui, nav);
    }
    let age = cursor.saturating_sub(t.start);
    let at = age + path.offset_ms;
    let mut enabled = path.enabled;
    let mut smooth = path.smooth;
    let mut reviewed = path.reviewed;
    let follow_changed =
        crate::controls::toggle(ui, &mut enabled, "Follow recorded path").changed();
    let mut changed = follow_changed;
    let mut reduction = path.reduction;
    let mut reduce = false;
    if !privacy {
        changed |= crate::controls::toggle(ui, &mut smooth, "Ease between positions").changed();
        crate::property_layout::section_help(
            ui,
            "Path reduction",
            "Direct paths between clicks removes every movement key between consecutive clicks. \
             Distance-based reduction removes keys closer than the distance to the previous one. \
             Click positions, click times and drags never change, and the recording is kept, so \
             you can switch modes or turn reduction off at any time. Changing the mode replaces \
             positions you edited by hand; Undo restores them.",
        );
        reduce = reduction_picker(ui, "properties", &mut reduction, !path.inputs.is_empty());
    }
    let mut node = path.sample(age);
    node.at = at;
    let mut edited = false;
    ui.columns(2, |col| {
        col[0].label("X");
        edited |= col[0]
            .add(
                egui::DragValue::new(&mut node.rect[0])
                    .speed(1.)
                    .suffix(" px"),
            )
            .changed();
        col[1].label("Y");
        edited |= col[1]
            .add(
                egui::DragValue::new(&mut node.rect[1])
                    .speed(1.)
                    .suffix(" px"),
            )
            .changed();
    });
    if privacy {
        ui.columns(2, |col| {
            col[0].label("Width");
            edited |= col[0]
                .add(
                    egui::DragValue::new(&mut node.rect[2])
                        .range(1. ..=100000.)
                        .suffix(" px"),
                )
                .changed();
            col[1].label("Height");
            edited |= col[1]
                .add(
                    egui::DragValue::new(&mut node.rect[3])
                        .range(1. ..=100000.)
                        .suffix(" px"),
                )
                .changed();
        });
    }
    edited |= crate::controls::toggle(ui, &mut node.visible, "Visible at this position").changed();
    let mut remove = false;
    ui.horizontal(|ui| {
        if ui.button("Previous").clicked()
            && let Some(n) = path.nodes.iter().rev().find(|n| n.at < at)
        {
            *cursor = t.start + n.at.saturating_sub(path.offset_ms);
        }
        if ui.button("Next").clicked()
            && let Some(n) = path.nodes.iter().find(|n| n.at > at)
        {
            *cursor = (t.start + n.at.saturating_sub(path.offset_ms)).min(t.end - 1);
        }
        remove = ui
            .add_enabled(
                path.nodes.len() > 1 && path.nodes.iter().any(|n| n.at == at),
                egui::Button::new("Remove point"),
            )
            .clicked();
    });
    ui.small(format!(
        "{} positions · {:.3} s",
        path.nodes.len(),
        age as f64 / 1000.
    ));
    if privacy {
        ui.small("Red outlines mark held positions where tracking was lost. Review every frame, then adjust or remove these positions.");
        changed |=
            crate::controls::toggle(ui, &mut reviewed, "I reviewed this entire tracked range")
                .changed();
        if nav.position.is_some() {
            nav.reopen = ui
                .button("Back to Export video")
                .tip("Go back to Export video and see what still needs review.")
                .clicked();
        }
    } else {
        ui.small("Move or resize the object on the canvas to offset its entire path. Edit X/Y here to create a position at the playhead.");
    }
    if changed || edited || remove || reduce {
        let p = Arc::make_mut(t.motion_path.as_mut().unwrap());
        p.enabled = enabled;
        p.smooth = smooth;
        if reduce {
            p.set_reduction(reduction);
        }
        p.reviewed = reviewed && !(privacy && follow_changed);
        if edited {
            let index = p.nodes.partition_point(|n| n.at < at);
            if p.nodes.get(index).is_some_and(|n| n.at == at) {
                p.nodes[index] = node;
            } else {
                p.nodes.insert(index, node);
            }
            if privacy {
                p.reviewed = false;
            }
        }
        if remove {
            p.nodes.retain(|n| n.at != at);
            if privacy {
                p.reviewed = false;
            }
        }
    }
    super::property_design::end(ui);
}
