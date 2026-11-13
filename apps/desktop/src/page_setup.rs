//! One document-backed Page Setup surface, shared by images, templates and guides.
use crate::tips::Tip as _;
use crate::{Command, Editor, egui};
use capture_core::guide_sections::{Action, Alignment, PageSetup};

#[derive(Clone)]
pub(super) struct Draft {
    pub(super) settings: PageSetup,
    pub(super) tab: usize,
    scope: usize,
    current: usize,
    count: usize,
    range: String,
    pub(super) scale: bool,
    pub(super) automatic: bool,
    pub(super) distribute: bool,
    error: String,
    /// The change that would fix the last Apply error (guided errors).
    guidance: Option<Guidance>,
}

/// What Apply needs changed, the tab and row to ring, and how to tell when
/// it has been done.
#[derive(Clone)]
struct Guidance {
    tab: usize,
    row: String,
    note: &'static str,
    message: &'static str,
    ready: &'static str,
    want: Want,
}
#[derive(Clone, Copy)]
enum Want {
    Scale,
    Overflow,
    /// Any margin smaller than these.
    Margins([f64; 4]),
}
impl Guidance {
    fn met(&self, d: &Draft) -> bool {
        match self.want {
            Want::Scale => d.scale,
            Want::Overflow => d.automatic,
            Want::Margins(m) => (0..4).any(|i| d.settings.margins[i] < m[i]),
        }
    }
}
/// The guided version of an Apply error that a setting here can fix.
fn guidance_for(error: &str, d: &Draft) -> Option<Guidance> {
    let unit = if d.settings.centimeters { "cm" } else { "in" };
    let low = error.to_lowercase();
    if low.contains("outside the image") && !d.scale {
        return Some(Guidance {
            tab: 1,
            row: "page-setup:Resize behavior".into(),
            note: "Choose Scale proportionally so objects shrink with the page.",
            message: "Some objects would end up off the page. Choose Scale proportionally, or make the page larger on Paper.",
            ready: "Ready. Objects will scale with the page. Click Apply.",
            want: Want::Scale,
        });
    }
    if (low.starts_with("section exceeds its page") || low.starts_with("page is full"))
        && !d.automatic
    {
        return Some(Guidance {
            tab: 1,
            row: "page-setup:Overflow pages".into(),
            note: "Turn on Overflow pages so sections continue on a new page.",
            message: "A section doesn't fit its page. Turn on Overflow pages, or make the page taller on Paper.",
            ready: "Ready. Sections will continue on new pages. Click Apply.",
            want: Want::Overflow,
        });
    }
    if low.contains("taller than the printable area") || low.contains("no printable area") {
        return Some(Guidance {
            tab: 1,
            row: format!("page-setup:Top ({unit})"),
            note: "Make the Top and Bottom margins smaller.",
            message: "A section doesn't fit between the margins. Reduce Top and Bottom margins, make the page taller, or choose Scale proportionally.",
            ready: "Margins changed. Click Apply to check again.",
            want: Want::Margins(d.settings.margins),
        });
    }
    None
}
fn key() -> egui::Id {
    egui::Id::unique("page-setup-draft")
}
fn pages(d: &Draft) -> Result<Vec<usize>, String> {
    if d.scope == 0 {
        return Ok((0..d.count).collect());
    }
    if d.scope == 1 {
        return Ok(vec![d.current]);
    }
    let mut result = std::collections::BTreeSet::new();
    for part in d.range.split(',') {
        let parts: Vec<_> = part.trim().split('-').collect();
        if parts.len() > 2 {
            return Err("Use page numbers or ranges, for example 1, 3-5".into());
        }
        let a = parts[0]
            .trim()
            .parse::<usize>()
            .map_err(|_| "Enter a page range")?;
        let b = parts
            .last()
            .unwrap()
            .trim()
            .parse::<usize>()
            .map_err(|_| "Enter a page range")?;
        if a == 0 || a > b || b > d.count {
            return Err(format!("Choose pages 1–{}", d.count));
        }
        result.extend((a - 1)..b);
    }
    if result.is_empty() {
        return Err("Choose at least one page".into());
    }
    Ok(result.into_iter().collect())
}
impl Editor {
    pub(super) fn open_page_setup(&mut self, ctx: &egui::Context) {
        if !self.commit_text() {
            return;
        }
        let doc = &self.session.document;
        let g = doc.guide.as_ref();
        let count = g.map_or(1, |g| g.page_count());
        let point = self
            .selected_annotation()
            .map(|a| a.start)
            .or_else(|| {
                self.view
                    .current_transform()
                    .map(|t| t.image_point(t.viewport.center()))
            })
            .unwrap_or([0.; 2]);
        let current = g.map_or(0, |g| g.page_at(point));
        let settings = g
            .and_then(|g| g.page_setup(current))
            .cloned()
            .unwrap_or_else(|| {
                PageSetup::from_pixels(g.map_or([doc.width, doc.height], |g| {
                    g.page_size(current, [doc.width, doc.height])
                }))
            });
        let d = Draft {
            settings,
            tab: 0,
            scope: 0,
            current,
            count,
            range: format!("1-{count}"),
            scale: false,
            automatic: g
                .and_then(|g| g.pages.as_ref())
                .is_none_or(|p| p.auto_overflow),
            distribute: false,
            error: String::new(),
            guidance: None,
        };
        ctx.data_mut(|data| data.insert_temp(key(), d));
    }
    pub(super) fn page_setup_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        let r = ui.add_sized([108., height], egui::Button::new("").corner_radius(9));
        let c = r.rect.center() - egui::vec2(0., 12.);
        let p = ui.painter();
        let stroke = egui::Stroke::new(1.8, egui::Color32::WHITE);
        p.add(egui::Shape::line(
            vec![
                c + egui::vec2(-13., -18.),
                c + egui::vec2(5., -18.),
                c + egui::vec2(14., -9.),
                c + egui::vec2(14., 18.),
                c + egui::vec2(-13., 18.),
                c + egui::vec2(-13., -18.),
            ],
            stroke,
        ));
        p.add(egui::Shape::line(
            vec![
                c + egui::vec2(5., -18.),
                c + egui::vec2(5., -9.),
                c + egui::vec2(14., -9.),
            ],
            stroke,
        ));
        p.rect_stroke(
            egui::Rect::from_center_size(c + egui::vec2(0., 4.), egui::vec2(15., 17.)),
            0,
            egui::Stroke::new(1., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
        p.text(
            egui::pos2(r.rect.center().x, r.rect.bottom() - 16.),
            egui::Align2::CENTER_CENTER,
            "Page Setup",
            egui::FontId::proportional(13.),
            egui::Color32::WHITE,
        );
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Page Setup"));
        let r = r.tip("Page Setup · Paper, margins, headers, numbering and print marks");
        if r.clicked() {
            self.open_page_setup(ui.ctx());
        }
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("page-setup-ribbon"), r.rect));
    }
    pub(super) fn show_page_setup(&mut self, ctx: &egui::Context) {
        let Some(mut d) = ctx.data(|data| data.get_temp::<Draft>(key())) else {
            return;
        };
        let before = (
            d.settings.clone(),
            d.scope,
            d.current,
            d.range.clone(),
            d.scale,
            d.automatic,
            d.distribute,
        );
        let mut apply = false;
        let mut cancel = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("page-setup-window"))
            .area(
                egui::Modal::default_area(egui::Id::unique("page-setup-window")).anchor(
                    egui::Align2::CENTER_TOP,
                    egui::vec2(
                        0.,
                        ((ctx.content_rect().height()
                            - ((ctx.content_rect().height() - 230.).clamp(230., 462.) + 224.))
                            / 2.)
                            .max(12.),
                    ),
                ),
            )
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(52, 59, 62))
                    // Dialog audit (BJ): the same blue edge and corners as every dialog.
                    .stroke(egui::Stroke::new(1.5, crate::shell::ACCENT))
                    .corner_radius(10),
            )
            .show(ctx, |ui| {
                let width = (ctx.content_rect().width() - 48.).clamp(360., 760.);
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(6., 0.);
                ui.visuals_mut().widgets.inactive.weak_bg_fill =
                    egui::Color32::from_rgb(68, 76, 80);
                ui.visuals_mut().widgets.inactive.bg_stroke =
                    egui::Stroke::new(1., egui::Color32::from_rgb(88, 100, 106));
                let dark = egui::Color32::from_rgb(37, 39, 41);
                egui::Frame::new()
                    .fill(dark)
                    .inner_margin(egui::Margin::symmetric(15, 11))
                    .show(ui, |ui| {
                        ui.set_width(width - 30.);
                        // Dialog audit (BJ): the standard dialog title; the
                        // close button sits at the top right.
                        ui.label(crate::panel_ui::title("Page Setup", 20.));
                    });
                let tabs = egui::Frame::new()
                    .fill(dark)
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 10,
                        top: 8,
                        bottom: 0,
                    })
                    .show(ui, |ui| {
                        ui.set_width(width - 20.);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.;
                            for (i, label) in
                                ["Paper", "Layout", "Headers & Footers", "Print Marks"]
                                    .iter()
                                    .enumerate()
                            {
                                let r = ui.add_sized(
                                    [if i == 2 { 156. } else { 96. }, 32.],
                                    egui::Button::new("")
                                        .fill(if d.tab == i {
                                            crate::shell::ACCENT
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        })
                                        .stroke(egui::Stroke::NONE)
                                        .corner_radius(egui::CornerRadius {
                                            nw: 5,
                                            ne: 5,
                                            sw: 5,
                                            se: 5,
                                        }),
                                );
                                let text = ui.painter().layout_no_wrap(
                                    (*label).into(),
                                    crate::panel_ui::font(13.),
                                    egui::Color32::WHITE,
                                );
                                let left = r.rect.center().x - (text.size().x + 23.) / 2.;
                                tab_icon(ui.painter(), egui::pos2(left + 7., r.rect.center().y), i);
                                ui.painter().galley(
                                    egui::pos2(left + 23., r.rect.center().y - text.size().y / 2.),
                                    text,
                                    egui::Color32::WHITE,
                                );
                                r.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::Role::Tab,
                                        true,
                                        d.tab == i,
                                        *label,
                                    )
                                });
                                // A tab that needs a change gets an amber dot.
                                if d.tab != i
                                    && d.guidance
                                        .as_ref()
                                        .is_some_and(|g| g.tab == i && !g.met(&d))
                                {
                                    ui.painter().circle_filled(
                                        r.rect.right_top() + egui::vec2(-8., 8.),
                                        3.5,
                                        crate::guided::AMBER,
                                    );
                                }
                                if r.clicked() {
                                    d.tab = i;
                                }
                            }
                        });
                    })
                    .response
                    .rect;
                ui.painter().line_segment(
                    [tabs.left_bottom(), tabs.right_bottom()],
                    egui::Stroke::new(2., crate::shell::ACCENT),
                );
                let height = (ctx.content_rect().height() - 230.).clamp(230., 462.);
                let (body, _) =
                    ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
                let preview_width = 230.;
                let split = body.right() - preview_width;
                let form = egui::Rect::from_min_max(body.min, egui::pos2(split, body.bottom()))
                    .shrink(15.);
                fixed_pane(
                    ui,
                    egui::UiBuilder::new()
                        .max_rect(form)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        ui.set_clip_rect(form.intersect(ui.clip_rect()));
                        ui.set_min_size(form.size());
                        ui.spacing_mut().item_spacing = egui::vec2(9., 12.);
                        ui.spacing_mut().interact_size.y = 28.;
                        egui::ScrollArea::vertical()
                            .id_salt(("page-setup-fields", d.tab))
                            .max_height(form.height())
                            .auto_shrink([false, false])
                            .show(ui, |ui| fields(ui, &mut d));
                    },
                );
                let pane = egui::Rect::from_min_max(egui::pos2(split, body.top()), body.max);
                ui.painter().rect_filled(pane, 0, dark);
                ui.painter().line_segment(
                    [pane.left_top(), pane.left_bottom()],
                    egui::Stroke::new(1., egui::Color32::from_rgb(88, 100, 106)),
                );
                fixed_pane(
                    ui,
                    egui::UiBuilder::new()
                        .max_rect(pane.shrink2(egui::vec2(12., 15.)))
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                    |ui| {
                        ui.set_clip_rect(pane.intersect(ui.clip_rect()));
                        ui.spacing_mut().item_spacing.y = 4.;
                        ui.horizontal(|ui| {
                            ui.label("Preview");
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    egui::ComboBox::from_id_salt("page-preview-number")
                                        .width(95.)
                                        .selected_text(format!("Page {}", d.current + 1))
                                        .show_ui(ui, |ui| {
                                            for page in 0..d.count {
                                                ui.selectable_value(
                                                    &mut d.current,
                                                    page,
                                                    format!("Page {}", page + 1),
                                                );
                                            }
                                        });
                                },
                            );
                        });
                        preview(ui, &d);
                    },
                );
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("page-setup-preview-pane"), pane)
                });
                ui.separator();
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(15, 12))
                    .show(ui, |ui| {
                        ui.set_width(width - 30.);
                        ui.horizontal(|ui| {
                            ui.label("Apply to");
                            egui::ComboBox::from_id_salt("page-setup-scope")
                                .width(160.)
                                .selected_text(["All pages", "Current page", "Page range"][d.scope])
                                .show_ui(ui, |ui| {
                                    for (i, label) in ["All pages", "Current page", "Page range"]
                                        .iter()
                                        .enumerate()
                                    {
                                        ui.selectable_value(&mut d.scope, i, *label);
                                    }
                                });
                            if d.scope == 2 {
                                ui.add(
                                    crate::unicode_input::TextEdit::singleline(&mut d.range)
                                        .hint_text("1, 3-5")
                                        .desired_width(100.),
                                );
                            }
                        });
                    });
                if before
                    != (
                        d.settings.clone(),
                        d.scope,
                        d.current,
                        d.range.clone(),
                        d.scale,
                        d.automatic,
                        d.distribute,
                    )
                {
                    d.error.clear();
                }
                let validation = d
                    .settings
                    .validate()
                    .map_err(|e| e.to_string())
                    .and_then(|_| pages(&d).map(|_| ()));
                let footer = egui::Frame::new()
                    .fill(dark)
                    .inner_margin(egui::Margin::symmetric(15, 10))
                    .show(ui, |ui| {
                        ui.set_width(width - 30.);
                        let (bar, _) = ui.allocate_exact_size(
                            egui::vec2(width - 30., 58.),
                            egui::Sense::hover(),
                        );
                        // Guided errors: the message says what to change, the
                        // row is ringed, and once it's changed the message
                        // turns green (approved Page Setup example).
                        use crate::guided::Kind;
                        let (kind, message, show_me) = match (&validation, &d.guidance) {
                            (Err(e), _) => (Kind::Error, e.clone(), false),
                            (Ok(()), Some(g)) if g.met(&d) => {
                                crate::guided::clear(ui.ctx(), &g.row);
                                (Kind::Done, g.ready.to_owned(), false)
                            }
                            (Ok(()), Some(g)) => {
                                (Kind::Error, g.message.to_owned(), d.tab != g.tab)
                            }
                            (Ok(()), None) => (Kind::Error, d.error.clone(), false),
                        };
                        let message_rect = egui::Rect::from_min_max(
                            bar.min,
                            egui::pos2(bar.right() - 190., bar.bottom()),
                        );
                        // Centered on the buttons' row (BZ4).
                        let show = crate::guided::footer_message(
                            ui,
                            message_rect,
                            bar.center().y,
                            kind,
                            &message,
                            show_me,
                        );
                        if show && let Some(g) = d.guidance.clone() {
                            d.tab = g.tab;
                            crate::guided::request(ui.ctx(), &g.row, g.note.into());
                        }
                        fixed_pane(
                            ui,
                            egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                egui::pos2(bar.right() - 176., bar.center().y - 21.),
                                egui::vec2(176., 42.),
                            )),
                            |ui| {
                                (apply, cancel) = crate::controls::dialog_actions(
                                    ui,
                                    "Apply",
                                    "Cancel",
                                    validation.is_ok(),
                                    false,
                                )
                            },
                        );
                    });
                #[cfg(test)]
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("page-setup-footer"), footer.response.rect)
                });
                #[cfg(not(test))]
                let _ = footer;
            });
        cancel |= crate::text_edit::cancel_on_escape(&modal);
        if cancel {
            if let Some(g) = &d.guidance {
                crate::guided::clear(ctx, &g.row);
            }
            ctx.data_mut(|data| data.remove::<Draft>(key()));
            return;
        }
        if apply && let Ok(pages) = pages(&d) {
            let command = Command::Guide {
                action: Action::PageSetup {
                    settings: Box::new(d.settings.clone()),
                    pages,
                    scale_content: d.scale,
                    auto_overflow: d.automatic,
                    distribute: d.distribute,
                },
            };
            // Validate first so a full-page/locked-object error stays in this window.
            if let Command::Guide { action } = &command {
                match self.session.document.edit_guide(action) {
                    Ok(_) => {
                        if let Some(g) = &d.guidance {
                            crate::guided::clear(ctx, &g.row);
                        }
                        self.execute(command);
                        ctx.data_mut(|data| data.remove::<Draft>(key()));
                        self.status =
                            "Page Setup applied. Undo restores the previous layout.".into();
                        return;
                    }
                    Err(e) => {
                        let raw = e.to_string();
                        // A setting here can fix it: take them there and ring it.
                        d.guidance = guidance_for(&raw, &d);
                        match &d.guidance {
                            Some(g) => {
                                d.tab = g.tab;
                                crate::guided::request(ctx, &g.row, g.note.into());
                                d.error.clear();
                            }
                            None => d.error = crate::guided::friendly(&raw),
                        }
                    }
                }
            }
        }
        ctx.data_mut(|data| data.insert_temp(key(), d));
    }
}
use crate::page_setup_fields::fields;
fn tab_icon(p: &egui::Painter, c: egui::Pos2, index: usize) {
    let s = egui::Stroke::new(1.2, egui::Color32::WHITE);
    let at = |x, y| c + egui::vec2(x, y);
    match index {
        0 => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(11., 14.)),
                1,
                s,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(-3., -2.), at(3., -2.)], s);
            p.line_segment([at(-3., 2.), at(3., 2.)], s);
        }
        1 => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(14., 14.)),
                1,
                s,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(-7., -2.), at(7., -2.)], s);
            p.line_segment([at(-1., -2.), at(-1., 7.)], s);
        }
        2 => {
            p.line_segment([at(-6., -6.), at(6., -6.)], s);
            p.line_segment([at(0., -6.), at(0., 6.)], s);
            p.line_segment([at(-3., 6.), at(3., 6.)], s);
        }
        _ => {
            p.add(egui::Shape::line(
                vec![at(-4., -8.), at(-4., 4.), at(8., 4.)],
                s,
            ));
            p.add(egui::Shape::line(
                vec![at(-8., -4.), at(4., -4.), at(4., 8.)],
                s,
            ));
        }
    }
}
fn fixed_pane(ui: &mut egui::Ui, builder: egui::UiBuilder, add: impl FnOnce(&mut egui::Ui)) {
    let mut child = ui.new_child(builder);
    add(&mut child);
}
fn preview(ui: &mut egui::Ui, d: &Draft) {
    let s = &d.settings;
    let output = s.output_points();
    let trim = s.trim_points();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(206., 290.), egui::Sense::hover());
    let k = (184. / output[0]).min(265. / output[1]) as f32;
    let paper = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(output[0] as f32, output[1] as f32) * k,
    );
    let t = paper.shrink(s.surround() as f32 * k);
    let p = ui.painter();
    p.rect_filled(
        t.translate(egui::vec2(2., 3.)),
        0,
        egui::Color32::from_black_alpha(90),
    );
    if s.bleed > 0. {
        p.rect_filled(
            t.expand(s.bleed as f32 * k),
            0,
            egui::Color32::from_rgb(211, 98, 98),
        );
    }
    p.rect_filled(t, 0, egui::Color32::WHITE);
    let body = egui::Rect::from_min_max(
        t.min + egui::vec2(s.margins[2] as f32, s.margins[0] as f32) * k,
        t.max - egui::vec2(s.margins[3] as f32, s.margins[1] as f32) * k,
    );
    for (a, b) in [
        (body.left_top(), body.right_top()),
        (body.right_top(), body.right_bottom()),
        (body.right_bottom(), body.left_bottom()),
        (body.left_bottom(), body.left_top()),
    ] {
        p.add(egui::Shape::dashed_line(
            &[a, b],
            egui::Stroke::new(1., crate::shell::ACCENT),
            3.,
            3.,
        ));
    }
    let inset = body.shrink2(egui::vec2(5., 8.));
    let block_height = ((inset.height() - 22.) / 3.).max(0.);
    for i in 0..3 {
        let block = egui::Rect::from_min_size(
            inset.min + egui::vec2(0., i as f32 * (block_height + 11.)),
            egui::vec2(inset.width().max(0.), block_height),
        );
        p.rect_filled(block, 2, egui::Color32::from_rgb(219, 234, 240));
        p.rect_filled(
            egui::Rect::from_min_size(
                block.left_bottom() + egui::vec2(7., -9.),
                egui::vec2(block.width() * 0.65, 3.),
            ),
            0,
            egui::Color32::from_rgb(145, 172, 184),
        );
    }
    for (content, r, align) in s.running_text(d.current, d.count) {
        let x = r[0]
            + match align {
                Alignment::Left => 0.,
                Alignment::Center => r[2] * 0.5,
                Alignment::Right => r[2],
            };
        let anchor = match align {
            Alignment::Left => egui::Align2::LEFT_TOP,
            Alignment::Center => egui::Align2::CENTER_TOP,
            Alignment::Right => egui::Align2::RIGHT_TOP,
        };
        let clip = egui::Rect::from_min_size(
            t.min + egui::vec2(r[0] as f32, r[1] as f32) * k,
            egui::vec2(r[2] as f32, r[3] as f32) * k,
        );
        p.with_clip_rect(clip.intersect(ui.clip_rect())).text(
            t.min + egui::vec2(x as f32, r[1] as f32) * k,
            anchor,
            content,
            egui::FontId::proportional((11. * k).max(4.)),
            egui::Color32::DARK_GRAY,
        );
    }
    if s.crop_marks {
        for x in [t.left(), t.right()] {
            for (y, dir) in [(t.top(), -1.), (t.bottom(), 1.)] {
                p.line_segment(
                    [
                        egui::pos2(x, y + dir * s.mark_offset as f32 * k),
                        egui::pos2(x, y + dir * (s.mark_offset + s.mark_length) as f32 * k),
                    ],
                    egui::Stroke::new(1., egui::Color32::BLACK),
                );
            }
        }
        for y in [t.top(), t.bottom()] {
            for (x, dir) in [(t.left(), -1.), (t.right(), 1.)] {
                p.line_segment(
                    [
                        egui::pos2(x + dir * s.mark_offset as f32 * k, y),
                        egui::pos2(x + dir * (s.mark_offset + s.mark_length) as f32 * k, y),
                    ],
                    egui::Stroke::new(1., egui::Color32::BLACK),
                );
            }
        }
    }
    let factor = if s.centimeters { 72. / 2.54 } else { 72. };
    let unit = if s.centimeters { "cm" } else { "in" };
    egui::Grid::new("page-preview-measures")
        .min_row_height(14.)
        .num_columns(2)
        .spacing([6., 6.])
        .show(ui, |ui| {
            for (label, value) in [
                (
                    "Trim",
                    format!("{:.2} × {:.2} {unit}", trim[0] / factor, trim[1] / factor),
                ),
                (
                    "Output",
                    format!(
                        "{:.2} × {:.2} {unit}",
                        output[0] / factor,
                        output[1] / factor
                    ),
                ),
                ("Pixels", format!("{} × {}", s.size[0], s.size[1])),
                ("Resolution", format!("{} PPI", s.ppi)),
            ] {
                ui.weak(label);
                ui.label(egui::RichText::new(value).size(12.));
                ui.end_row();
            }
        });
    ui.add_space(3.);
    ui.horizontal(|ui| {
        for (label, color, dashed) in [
            ("Margins", crate::shell::ACCENT, true),
            ("Bleed", egui::Color32::from_rgb(211, 98, 98), false),
        ] {
            let (r, _) = ui.allocate_exact_size(egui::vec2(10., 10.), egui::Sense::hover());
            if dashed {
                for (a, b) in [
                    (r.left_top(), r.right_top()),
                    (r.right_top(), r.right_bottom()),
                    (r.right_bottom(), r.left_bottom()),
                    (r.left_bottom(), r.left_top()),
                ] {
                    ui.painter().add(egui::Shape::dashed_line(
                        &[a, b],
                        egui::Stroke::new(1., color),
                        2.,
                        2.,
                    ));
                }
            } else {
                ui.painter().rect_stroke(
                    r,
                    0,
                    egui::Stroke::new(1., color),
                    egui::StrokeKind::Inside,
                );
            }
            ui.weak(label);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    fn frame(
        ctx: &egui::Context,
        e: &mut Editor,
        width: f32,
        events: Vec<egui::Event>,
    ) -> PaintedOutput {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 780.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |_| {
                e.show_page_setup(ctx);
                crate::guided::paint(ctx);
            },
        ))
    }
    #[test]
    fn full_window_tabs_scope_apply_and_cancel_are_atomic_and_fit_the_viewport() {
        for scale in [1., 1.25, 1.5, 2.] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
            ctx.set_pixels_per_point(scale);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.session = capture_core::Session::new(
                capture_core::templates::build(
                    "two-steps",
                    "Page review",
                    &[],
                    capture_core::templates::Scale::Fill,
                )
                .unwrap(),
            )
            .unwrap();
            let original = serde_json::to_vec(&e.session.document).unwrap();
            e.open_page_setup(&ctx);
            let mut atlas = PaintedOutput(egui::FullOutput::default());
            for width in [960., 640.] {
                let mut preview_bounds = None;
                for tab in 0..4 {
                    ctx.data_mut(|data| {
                        let d = data.get_temp_mut_or::<Draft>(key(), unreachable_draft());
                        d.tab = tab;
                        d.settings.numbers = true;
                        d.settings.header.enabled = true;
                        d.settings.header.text = "Operations guide".into();
                        d.settings.crop_marks = true;
                        d.settings.bleed = 9.;
                    });
                    let first = frame(&ctx, &mut e, width, vec![]);
                    let output = frame(&ctx, &mut e, width, vec![]);
                    let pane = ctx
                        .data(|data| {
                            data.get_temp::<egui::Rect>(egui::Id::unique("page-setup-preview-pane"))
                        })
                        .unwrap();
                    let first_pane = preview_bounds.get_or_insert(pane);
                    assert!(
                        (pane.min - first_pane.min).length() < 0.5
                            && (pane.size() - first_pane.size()).length() < 0.5,
                        "Preview moved with tab {tab}: {pane:?} {first_pane:?}"
                    );
                    atlas.0.textures_delta.append(first.textures_delta.clone());
                    atlas.0.textures_delta.append(output.textures_delta.clone());
                    for label in ["Apply", "Cancel", "Page Setup"] {
                        let rect = crate::color_widgets::tests::text_rect(&output.shapes, label)
                            .unwrap_or_else(|| panic!("missing {label} {width} {tab}"));
                        assert!(rect.top() >= 0. && rect.bottom() <= 780., "{rect:?}");
                    }
                    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
                    if scale == 1.
                        && let Ok(dir) = std::env::var("CLIPDESIGNER_PAGE_SETUP_UI")
                    {
                        let mut captured = first.clone();
                        captured.textures_delta.clear();
                        captured.textures_delta = atlas.textures_delta.clone();
                        dump_meshes(
                            &ctx,
                            &dir,
                            &format!("page-setup-{width}-{tab}"),
                            PaintedOutput(captured),
                            output,
                        );
                    }
                }
            }
            ctx.data_mut(|data| {
                let mut d = data.get_temp::<Draft>(key()).unwrap();
                d.tab = 0;
                d.settings = PageSetup::from_pixels([1000, 1400]);
                d.settings.numbers = true;
                data.insert_temp(key(), d);
            });
            frame(&ctx, &mut e, 960., vec![]);
            let output = frame(&ctx, &mut e, 960., vec![]);
            let at = crate::color_widgets::tests::text_rect(&output.shapes, "Apply")
                .unwrap()
                .center();
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut e,
                    960.,
                    vec![
                        egui::Event::PointerMoved(at),
                        egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(ctx.data(|data| data.get_temp::<Draft>(key())).is_none());
            assert!(
                e.session
                    .document
                    .guide
                    .as_ref()
                    .unwrap()
                    .page_setup(0)
                    .unwrap()
                    .numbers
            );
            e.execute(Command::Undo);
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
            e.open_page_setup(&ctx);
            let first = frame(&ctx, &mut e, 960., vec![]);
            let _ = first;
            frame(
                &ctx,
                &mut e,
                960.,
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            assert!(ctx.data(|data| data.get_temp::<Draft>(key())).is_none());
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        }
    }
    fn unreachable_draft() -> Draft {
        Draft {
            settings: PageSetup::default(),
            tab: 0,
            scope: 0,
            current: 0,
            count: 1,
            range: String::new(),
            scale: false,
            automatic: true,
            distribute: false,
            error: String::new(),
            guidance: None,
        }
    }
    /// The approved guided-errors example: a narrower page with Keep sizes
    /// leaves an object off the page. Apply goes to Layout, rings Resize
    /// behavior and says to choose Scale proportionally; choosing it clears
    /// the ring and Apply works.
    #[test]
    fn apply_error_takes_you_to_resize_behavior_and_clears_when_fixed() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = crate::Session::new(crate::Document::blank(1000, 600).unwrap()).unwrap();
        let a = e.new_annotation(crate::Kind::Rectangle, [800., 100.], [950., 200.]);
        e.execute(Command::Add { annotation: a });
        e.open_page_setup(&ctx);
        frame(&ctx, &mut e, 960., vec![]);
        ctx.data_mut(|data| {
            let mut d = data.get_temp::<Draft>(key()).unwrap();
            d.settings.size = [500, 600];
            d.settings.physical_points = None;
            data.insert_temp(key(), d);
        });
        let output = frame(&ctx, &mut e, 960., vec![]);
        let apply = crate::color_widgets::tests::text_rect(&output.shapes, "Apply").unwrap();
        let click = |pressed| egui::Event::PointerButton {
            pos: apply.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut e,
            960.,
            vec![egui::Event::PointerMoved(apply.center())],
        );
        frame(&ctx, &mut e, 960., vec![click(true)]);
        frame(&ctx, &mut e, 960., vec![click(false)]);
        let output = frame(&ctx, &mut e, 960., vec![]);
        let d = ctx
            .data(|data| data.get_temp::<Draft>(key()))
            .expect("still open");
        let g = d.guidance.as_ref().expect("guided");
        assert_eq!(d.tab, 1, "Layout is shown");
        assert_eq!(g.row, "page-setup:Resize behavior");
        assert_eq!(
            crate::guided::active(&ctx).as_deref(),
            Some("page-setup:Resize behavior")
        );
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, g.message).is_some());
        assert_eq!(e.session.document.width, 1000, "nothing changed yet");
        // The note sits beside the ringed control, never on it, and goes
        // away once the pointer reaches the control to fix it (the ring
        // stays until the value is right).
        frame(&ctx, &mut e, 960., vec![]);
        let (ring, note): (egui::Rect, egui::Rect) = ctx
            .data(|data| data.get_temp(egui::Id::unique("guided-note")))
            .expect("the note shows");
        assert!(!note.intersects(ring), "{note:?} covers {ring:?}");
        assert!(note.right() <= 960. && note.bottom() <= 780.);
        frame(
            &ctx,
            &mut e,
            960.,
            vec![egui::Event::PointerMoved(ring.center())],
        );
        frame(&ctx, &mut e, 960., vec![]);
        assert!(
            ctx.data(
                |data| data.get_temp::<(egui::Rect, egui::Rect)>(egui::Id::unique("guided-note"))
            )
            .is_none(),
            "the note is put away once you start the fix"
        );
        assert_eq!(
            crate::guided::active(&ctx).as_deref(),
            Some("page-setup:Resize behavior")
        );
        // Choosing Scale proportionally clears the ring; the message turns green.
        ctx.data_mut(|data| {
            let mut d = data.get_temp::<Draft>(key()).unwrap();
            d.scale = true;
            data.insert_temp(key(), d);
        });
        let output = frame(&ctx, &mut e, 960., vec![]);
        assert_eq!(crate::guided::active(&ctx), None);
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, g.ready).is_some());
    }
    #[test]
    fn scope_is_adjacent_and_actionable_errors_stay_inside_the_fixed_footer() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.open_page_setup(&ctx);
        let first = frame(&ctx, &mut e, 640., vec![]);
        let output = frame(&ctx, &mut e, 640., vec![]);
        let text = |o: &PaintedOutput, s: &str| {
            crate::color_widgets::tests::text_rect(&o.shapes, s).unwrap()
        };
        let label = text(&output, "Apply to");
        let scope = text(&output, "All pages");
        assert!(scope.left() - label.right() < 24.);
        assert!((scope.center().y - label.center().y).abs() < 2.);
        let pane = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::unique("page-setup-preview-pane")))
            .unwrap();
        ctx.data_mut(|data| {
            let mut d = data.get_temp::<Draft>(key()).unwrap();
            d.settings.margins = [300.; 4];
            data.insert_temp(key(), d);
        });
        let output = frame(&ctx, &mut e, 640., vec![]);
        let error = ctx
            .data(|data| data.get_temp::<Draft>(key()))
            .unwrap()
            .settings
            .validate()
            .unwrap_err()
            .to_string();
        assert!(error.contains("reduce Top/Bottom margins"));
        let footer = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::unique("page-setup-footer")))
            .unwrap();
        assert!(footer.contains_rect(text(&output, &error)));
        assert!(footer.contains_rect(text(&output, "Apply")));
        // BZ4: the message is centered on the buttons, not top-aligned.
        let message = ctx
            .data(|data| data.get_temp::<egui::Rect>(egui::Id::unique("guided-dialog-message")))
            .unwrap();
        let apply = text(&output, "Apply");
        assert!(
            (message.center().y - apply.center().y).abs() <= 1.5,
            "{message:?} beside {apply:?}"
        );
        assert!(footer.bottom() <= 780.);
        assert_eq!(
            pane,
            ctx.data(
                |data| data.get_temp::<egui::Rect>(egui::Id::unique("page-setup-preview-pane"))
            )
            .unwrap()
        );
        if let Ok(dir) = std::env::var("CLIPDESIGNER_PAGE_SETUP_UI") {
            dump_meshes(&ctx, &dir, "page-setup-error", first, output);
        }
        assert!(!e.session.can_undo());
    }
    #[test]
    fn page_ranges_reject_zero_reverse_and_out_of_document_values() {
        let mut d = unreachable_draft();
        d.scope = 2;
        d.count = 5;
        for range in ["0", "6", "3-1", "1-2-3", ""] {
            d.range = range.into();
            assert!(pages(&d).is_err());
        }
        d.range = "1,3-5,3".into();
        assert_eq!(pages(&d).unwrap(), vec![0, 2, 3, 4]);
    }
    #[test]
    fn large_page_setup_precedes_all_other_advanced_controls() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.ribbon_tab = crate::ribbon::RibbonTab::Advanced;
        for _ in 0..3 {
            let output = PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1920., 900.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    e.show_ribbon(ui);
                },
            ));
            assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Step Guide").is_none());
        }
        let rect = ctx.data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique("page-setup-ribbon"))
                .unwrap()
        });
        assert!(
            rect.height() >= 64. && rect.left() < 20. && rect.right() < 130.,
            "{rect:?}"
        );
    }
}
