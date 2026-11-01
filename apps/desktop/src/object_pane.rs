use crate::{Annotation, Command, Editor, Kind, egui};
use egui::{Color32, Rect, Stroke};
mod guide_tree;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
struct LayerDrag(u64);

pub(super) fn delete_button(
    ui: &mut egui::Ui,
    enabled: bool,
    label: &str,
    width: f32,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 30.), egui::Sense::click());
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
        let fill = if response.hovered() || response.is_pointer_button_down_on() {
            Color32::from_rgb(178, 48, 57)
        } else {
            Color32::from_rgb(55, 62, 65)
        };
        let color = if ui.is_enabled() {
            Color32::WHITE
        } else {
            Color32::from_gray(130)
        };
        let p = ui.painter();
        p.rect(
            rect,
            3,
            fill,
            Stroke::new(
                1.,
                if response.has_focus() {
                    Color32::from_rgb(38, 155, 194)
                } else {
                    Color32::from_gray(170)
                },
            ),
            egui::StrokeKind::Inside,
        );
        let galley = p.layout_no_wrap(label.into(), egui::FontId::proportional(13.), color);
        let left = rect.center().x - (galley.size().x + 22.) * 0.5;
        let center = egui::pos2(left + 8., rect.center().y);
        let stroke = Stroke::new(1.3, color);
        p.line_segment(
            [center + egui::vec2(-6., -4.), center + egui::vec2(6., -4.)],
            stroke,
        );
        p.rect_stroke(
            Rect::from_center_size(center + egui::vec2(0., 1.5), egui::vec2(9., 10.)),
            1,
            stroke,
            egui::StrokeKind::Inside,
        );
        p.line_segment(
            [center + egui::vec2(-2., -6.), center + egui::vec2(2., -6.)],
            stroke,
        );
        for x in [-1.5, 1.5] {
            p.line_segment(
                [center + egui::vec2(x, -1.5), center + egui::vec2(x, 4.)],
                stroke,
            );
        }
        p.galley(
            egui::pos2(left + 22., rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("delete-object-button"), (rect, fill)));
        response
    })
    .inner
}

fn preview(a: &Annotation) -> String {
    let text = a.text.as_ref().map_or("", |t| t.content.as_str());
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut short: String = compact.chars().take(30).collect();
    if compact.chars().count() > 30 {
        short.push('…');
    }
    short
}
fn color_name(c: [u8; 4]) -> &'static str {
    if c[3] == 0 {
        return "Transparent";
    }
    let rgb = &c[..3];
    let lo = *rgb.iter().min().unwrap();
    let hi = *rgb.iter().max().unwrap();
    if hi - lo < 28 {
        return if hi < 50 {
            "Black"
        } else if lo > 215 {
            "White"
        } else {
            "Gray"
        };
    }
    // Use hue so pale fills retain their color name instead of matching an
    // unrelated darker swatch by RGB distance.
    let [r, g, b] = [c[0], c[1], c[2]].map(f32::from);
    let delta = f32::from(hi - lo);
    let hue = (60.
        * if c[0] == hi {
            (g - b) / delta
        } else if c[1] == hi {
            2. + (b - r) / delta
        } else {
            4. + (r - g) / delta
        })
    .rem_euclid(360.);
    if (15. ..50.).contains(&hue) && hi < 170 {
        "Brown"
    } else if !(15. ..345.).contains(&hue) {
        "Red"
    } else if hue < 45. {
        "Orange"
    } else if hue < 70. {
        "Yellow"
    } else if hue < 165. {
        "Green"
    } else if hue < 195. {
        "Cyan"
    } else if hue < 265. {
        "Blue"
    } else if hue < 320. {
        "Purple"
    } else {
        "Pink"
    }
}
fn label(a: &Annotation) -> String {
    if a.is_shape() && a.is_image_frame() {
        return format!(
            "{} image frame",
            match a.kind {
                Kind::Ellipse => "Ellipse",
                Kind::Polygon => "Polygon",
                _ => "Rectangle",
            }
        );
    }
    match a.kind {
        Kind::Image => {
            if let Some(b) = a.image.as_ref().filter(|b| b.is_stamp()) {
                return format!("Stamp: {}", b.stamp_name().unwrap_or("Imported stamp"));
            }
            let size = a.image.as_ref().map(|b| b.size()).unwrap_or([0; 2]);
            format!(
                "{} ({} × {})",
                if a.image.as_ref().is_some_and(|b| b.is_cursor()) {
                    "Cursor"
                } else {
                    "Image"
                },
                size[0],
                size[1]
            )
        }
        Kind::Text => format!("Text: \"{}\"", preview(a)),
        Kind::Callout => format!("Callout: \"{}\"", preview(a)),
        Kind::Step => format!("Step {}", preview(a)),
        Kind::Rectangle | Kind::Ellipse | Kind::Polygon => {
            let (lo, hi) = a.body_bounds();
            let name = match a.kind {
                Kind::Rectangle if a.corner_radius > 0.0 => "Rounded rectangle",
                Kind::Rectangle => "Rectangle",
                Kind::Ellipse if ((hi[0] - lo[0]) - (hi[1] - lo[1])).abs() < 0.5 => "Circle",
                Kind::Ellipse => "Ellipse",
                _ => match a.points.len() {
                    3 => "Triangle",
                    5 => "Pentagon",
                    6 => "Hexagon",
                    _ => "Polygon",
                },
            };
            format!(
                "{name} ({}, {})",
                color_name(a.fill.unwrap_or([0; 4])),
                color_name(a.color)
            )
        }
        Kind::Blur | Kind::Pixelate => format!("{:?} ({})", a.kind, a.width),
        _ => format!("{:?} ({})", a.kind, color_name(a.color)),
    }
}

fn control(ui: &mut egui::Ui, rect: Rect, id: egui::Id, eye: bool, active: bool) -> egui::Response {
    let response = ui.interact(rect, id, egui::Sense::click());
    let name = if eye {
        if active { "Hide object" } else { "Show object" }
    } else if active {
        "Unlock object"
    } else {
        "Lock object"
    };
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), name));
    if response.hovered() {
        ui.painter().rect_filled(rect, 3, Color32::from_gray(75));
    }
    let color = if active {
        Color32::from_rgb(175, 211, 225)
    } else {
        Color32::from_gray(147)
    };
    let p = ui.painter();
    let center = rect.center();
    let s = Stroke::new(1.25, color);
    if eye {
        let points = (0..=24)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 24.0;
                center + egui::vec2(a.cos() * 6.5, a.sin() * 3.7)
            })
            .collect();
        p.add(egui::Shape::line(points, s));
        p.circle_filled(center, 1.8, color);
        if !active {
            p.line_segment(
                [center + egui::vec2(-7., 6.), center + egui::vec2(7., -6.)],
                s,
            );
        }
    } else {
        p.rect_stroke(
            Rect::from_center_size(center + egui::vec2(0., 3.), egui::vec2(9., 8.)),
            1,
            s,
            egui::StrokeKind::Inside,
        );
        let offset = if active { 0. } else { 3. };
        let points = (0..=12)
            .map(|i| {
                let a = std::f32::consts::PI + i as f32 * std::f32::consts::PI / 12.;
                center + egui::vec2(offset + a.cos() * 3.5, -1. + a.sin() * 5.)
            })
            .collect();
        p.add(egui::Shape::line(points, s));
    }
    response.on_hover_text(name)
}

impl Editor {
    pub(super) fn show_objects(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Objects",
            "Frontmost first. Drag a row to arrange it; Shift-click to select multiple objects. The eye hides an object and the lock protects it from editing. Show All and Hide All affect visibility; Select type selects visible, unlocked objects matching the current object.",
        );
        let ids: Vec<_> = self
            .session
            .document
            .annotations
            .iter()
            .map(|a| a.id)
            .collect();
        let mut command = None;
        ui.horizontal(|ui| {
            for (name, visible) in [("Show All", true), ("Hide All", false)] {
                let r = ui.add_enabled(!ids.is_empty(), egui::Button::new(name));
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::unique(("object-all", name)), r.rect));
                if r.clicked() {
                    command = Some(Command::SetObjectsVisible {
                        ids: ids.clone(),
                        visible,
                    });
                }
            }
            let kind = self.selected_annotation().map(|a| a.kind);
            let r = ui
                .add_enabled(kind.is_some(), egui::Button::new("Select type"))
                .on_hover_text("Select all visible, unlocked objects of the same type");
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("object-select-type"), r.rect));
            if r.clicked()
                && let Some(kind) = kind
                && self.commit_text()
            {
                let ids: Vec<_> = self
                    .session
                    .document
                    .annotations
                    .iter()
                    .filter(|a| a.kind == kind && self.session.document.object_editable(a.id))
                    .map(|a| a.id)
                    .collect();
                self.select_only(ids.first().copied());
                self.extra_selected.extend(ids.iter().skip(1).copied());
                self.selection_order = ids;
            }
        });
        if self.shell.annotations_hidden {
            ui.small("Global annotation preview is hidden.");
        }
        let objects: Vec<_> = self
            .session
            .document
            .annotations
            .iter()
            .rev()
            .cloned()
            .collect();
        let mut reorder = None;
        let mut selected = None;
        // Present group children together without rewriting document stacking.
        let mut rows = Vec::new();
        for a in &objects {
            if rows.iter().any(|old: &&Annotation| old.id == a.id) {
                continue;
            }
            rows.push(a);
            if let Some(group) = a.group {
                rows.extend(
                    objects
                        .iter()
                        .filter(|b| b.group == Some(group) && b.id != a.id),
                );
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let rows = guide_tree::rows(ui, &self.session.document, rows);
        for row in rows {
            let a = match row {
                guide_tree::Row::Object(a) => a,
                guide_tree::Row::Header {
                    key,
                    label,
                    depth,
                    page,
                } => {
                    if let Some(action) = guide_tree::header(ui, key, &label, depth, page) {
                        command = Some(Command::Guide { action });
                    }
                    continue;
                }
            };
            if let Some(group) = a.group {
                let key = egui::Id::unique(("object-group-expanded", group));
                let automatic = objects
                    .iter()
                    .any(|a| a.group == Some(group) && a.analysis.is_some());
                let mut expanded = ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(key))
                    .unwrap_or(!automatic);
                if seen.insert(group) {
                    let members = self.session.document.group_members(a.id);
                    let hidden = members
                        .iter()
                        .all(|id| !self.session.document.object_visible(*id));
                    let locked = members
                        .iter()
                        .all(|id| self.session.document.object_state(*id).locked);
                    let (rect, r) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 31.),
                        egui::Sense::click_and_drag(),
                    );
                    ui.painter().rect_filled(
                        rect,
                        3,
                        if self.is_selected(a.id) {
                            Color32::from_rgb(51, 82, 96)
                        } else {
                            Color32::from_gray(52)
                        },
                    );
                    let disclosure = Rect::from_min_size(rect.min, egui::vec2(22., 31.));
                    let d = ui.interact(disclosure, r.id.with("disclosure"), egui::Sense::click());
                    let c = disclosure.center();
                    let points = if expanded {
                        vec![
                            c + egui::vec2(-4., -2.),
                            c + egui::vec2(0., 2.),
                            c + egui::vec2(4., -2.),
                        ]
                    } else {
                        vec![
                            c + egui::vec2(-2., -4.),
                            c + egui::vec2(2., 0.),
                            c + egui::vec2(-2., 4.),
                        ]
                    };
                    ui.painter()
                        .add(egui::Shape::line(points, Stroke::new(1.5, Color32::WHITE)));
                    d.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            true,
                            if expanded {
                                "Collapse group"
                            } else {
                                "Expand group"
                            },
                        )
                    });
                    if d.clicked() {
                        expanded = !expanded;
                        ui.ctx().data_mut(|data| data.insert_temp(key, expanded));
                    }
                    let lock = Rect::from_min_max(
                        egui::pos2(rect.right() - 24., rect.top() + 2.),
                        rect.max - egui::vec2(0., 2.),
                    );
                    let eye = lock.translate(egui::vec2(-24., 0.));
                    let name = if automatic {
                        format!("Simplify · {} blocks", members.len().saturating_sub(1))
                    } else {
                        format!("Group · {} objects", members.len())
                    };
                    let label = Rect::from_min_max(
                        rect.min + egui::vec2(25., 0.),
                        egui::pos2(eye.left() - 3., rect.bottom()),
                    );
                    ui.painter().with_clip_rect(label).text(
                        label.left_center(),
                        egui::Align2::LEFT_CENTER,
                        &name,
                        egui::FontId::proportional(12.),
                        Color32::WHITE,
                    );
                    if control(ui, eye, r.id.with("eye"), true, !hidden).clicked() {
                        command = Some(Command::SetObjectsVisible {
                            ids: members.clone(),
                            visible: hidden,
                        });
                    }
                    if control(ui, lock, r.id.with("lock"), false, locked).clicked() {
                        command = Some(Command::SetObjectsLocked {
                            ids: members.clone(),
                            locked: !locked,
                        });
                    }
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
                    r.dnd_set_drag_payload(LayerDrag(a.id));
                    if let Some(drag) = r.dnd_release_payload::<LayerDrag>() {
                        reorder = Some((
                            drag.0,
                            a.id,
                            ui.input(|i| {
                                i.pointer.hover_pos().is_some_and(|p| p.y > rect.center().y)
                            }),
                        ));
                    }
                    if r.clicked() {
                        selected = Some((a.id, false));
                    }
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::unique(("object-group", group)),
                            (rect, disclosure, eye, lock),
                        )
                    });
                }
                if !expanded {
                    continue;
                }
            }
            if a.analysis.is_some() {
                continue;
            }
            ui.push_id(("object-row", a.id), |ui| {
                let state = self.session.document.object_state(a.id);
                let (mut rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 29.),
                    egui::Sense::click_and_drag(),
                );
                if self.session.document.guide.is_some() {
                    rect.min.x += 24.;
                }
                let selected = self.is_selected(a.id);
                if selected || response.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        3,
                        if selected {
                            Color32::from_rgb(51, 82, 96)
                        } else {
                            Color32::from_gray(63)
                        },
                    );
                }
                if selected {
                    ui.painter().rect_stroke(
                        rect,
                        3,
                        Stroke::new(1., Color32::from_rgb(38, 155, 194)),
                        egui::StrokeKind::Inside,
                    );
                }
                for x in [6., 10.] {
                    for y in [-4., 0., 4.] {
                        ui.painter().circle_filled(
                            egui::pos2(
                                rect.left() + x + if a.group.is_some() { 14. } else { 0. },
                                rect.center().y + y,
                            ),
                            1.,
                            Color32::from_gray(140),
                        );
                    }
                }
                let lock_rect = Rect::from_min_max(
                    egui::pos2(rect.right() - 24., rect.top() + 2.),
                    rect.max - egui::vec2(0., 2.),
                );
                let eye_rect = lock_rect.translate(egui::vec2(-24., 0.));
                let text_rect = Rect::from_min_max(
                    rect.min + egui::vec2(if a.group.is_some() { 32. } else { 18. }, 0.),
                    egui::pos2(eye_rect.left() - 2., rect.bottom()),
                );
                let name = label(a);
                let color = if state.hidden {
                    Color32::from_gray(135)
                } else {
                    Color32::from_gray(235)
                };
                let mut job = egui::text::LayoutJob::simple(
                    name.clone(),
                    egui::FontId::proportional(12.),
                    color,
                    text_rect.width(),
                );
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                let galley = ui.painter().layout_job(job);
                ui.painter().with_clip_rect(text_rect).galley(
                    text_rect.left_center() - egui::vec2(0., galley.size().y * 0.5),
                    galley,
                    color,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &name)
                });
                response
                    .clone()
                    .on_hover_text(format!("{name}\nDrag to change layer order"));
                let eye = control(ui, eye_rect, response.id.with("eye"), true, !state.hidden);
                let lock = control(ui, lock_rect, response.id.with("lock"), false, state.locked);
                if eye.clicked() {
                    command = Some(Command::SetObjectsVisible {
                        ids: vec![a.id],
                        visible: state.hidden,
                    });
                }
                if lock.clicked() {
                    command = Some(Command::SetObjectsLocked {
                        ids: vec![a.id],
                        locked: !state.locked,
                    });
                }
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::unique(("object-row", a.id)),
                        (rect, eye_rect, lock_rect, name.clone()),
                    )
                });
                response.dnd_set_drag_payload(LayerDrag(a.id));
                if response.dnd_hover_payload::<LayerDrag>().is_some() {
                    let after =
                        ui.input(|i| i.pointer.hover_pos().is_some_and(|p| p.y > rect.center().y));
                    let y = if after { rect.bottom() } else { rect.top() };
                    ui.painter().line_segment(
                        [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                        Stroke::new(2., Color32::from_rgb(38, 155, 194)),
                    );
                    if let Some(drag) = response.dnd_release_payload::<LayerDrag>() {
                        reorder = Some((drag.0, a.id, after));
                    }
                }
                // Child eye/lock controls own their input; no selection side effect.
                response
            })
            .inner
            .clicked()
            .then(|| {
                selected = Some((a.id, a.group.is_some()));
            });
        }
        if let Some((from, target, after)) = reorder
            && from != target
        {
            let moving = self.session.document.group_members(from);
            let target_members = self.session.document.group_members(target);
            if !moving.contains(&target) {
                let front_to_back: Vec<_> = self
                    .session
                    .document
                    .annotations
                    .iter()
                    .rev()
                    .map(|a| a.id)
                    .collect();
                let mut order: Vec<_> = front_to_back
                    .iter()
                    .copied()
                    .filter(|id| !moving.contains(id))
                    .collect();
                let at = if after {
                    order
                        .iter()
                        .rposition(|id| target_members.contains(id))
                        .map(|i| i + 1)
                } else {
                    order.iter().position(|id| target_members.contains(id))
                };
                if let Some(at) = at {
                    order.splice(
                        at..at,
                        front_to_back.into_iter().filter(|id| moving.contains(id)),
                    );
                    order.reverse();
                    command = Some(Command::ReorderObjects { ids: order });
                }
            }
        }
        if let Some(command) = command {
            if self.commit_text() {
                self.commit_property_preview();
                self.execute(command);
            }
        } else if let Some((id, child)) = selected
            && self.commit_text()
        {
            if child {
                self.enter_group_child(id);
                self.select_only(Some(id));
            } else {
                self.editing_group = None;
                self.click_selection(
                    Some(id),
                    ui.input(|i| i.modifiers.shift || i.modifiers.command),
                );
            }
            self.auto_activate_selected_tool();
        }
    }
}
