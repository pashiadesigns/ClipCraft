//! Editing state is session-only; property keys belong to the saved project.
use super::*;
use crate::tips::Tip as _;
use capture_core::video::{
    editing::Item,
    keyframes::{Curve, Property, Set, Value},
    motion_path::{Path as MotionPath, Purpose},
};
use egui::{Color32, Rect, Sense, Stroke};
pub(crate) struct State {
    pub auto_key: bool,
    pub lanes: bool,
    pub path: bool,
    pub rows: Option<(usize, usize)>,
    property: Property,
    drag: Option<Drag>,
    path_drag: Option<Drag>,
    /// Open the Keyframes section on its next display (keyframing just turned on).
    pub(super) expand: bool,
    /// A key copied from the ⋯ menu, pasted at the playhead for the same property.
    copied: Option<(Property, Value)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            auto_key: false,
            lanes: false,
            path: true,
            rows: None,
            property: Property::Position,
            drag: None,
            path_drag: None,
            expand: false,
            copied: None,
        }
    }
}
/// The recorded cursor path of an object, with its clip start and duration.
fn cursor_path(p: &Project, id: u64) -> Option<(&MotionPath, u64, u64)> {
    let t = p.objects.get(&id)?;
    let path = t.motion_path.as_deref()?;
    (path.purpose == Purpose::Cursor).then_some((path, t.start, t.end - t.start))
}
/// Minimum on-screen spacing between drawn cursor keys; denser keys are thinned for display only.
const PATH_KEY_SPACING: f32 = 6.;
struct Drag {
    original: Project,
    item: Item,
    from: u64,
    property: Option<Property>,
    duplicate: bool,
}
fn keys(p: &Project, item: Item) -> Option<(&Set, u64, u64)> {
    match item {
        Item::Annotation(id) => p
            .objects
            .get(&id)
            .map(|t| (&t.keyframes, t.start, t.end - t.start)),
        Item::Media(id) => p
            .segments
            .iter()
            .find(|c| c.id == id)
            .map(|c| (&c.keyframes, c.at, c.duration())),
        _ => None,
    }
}
fn keys_mut(p: &mut Project, item: Item) -> Option<&mut Set> {
    match item {
        Item::Annotation(id) => p.objects.get_mut(&id).map(|t| &mut t.keyframes),
        Item::Media(id) => p
            .segments
            .iter_mut()
            .find(|c| c.id == id)
            .map(|c| &mut c.keyframes),
        _ => None,
    }
}
/// A keyframe lane's name, pinned to the visible left edge while the timeline scrolls (BA6).
fn lane_label(ui: &egui::Ui, row: Rect, label: &str) {
    let x = ui.clip_rect().left().max(row.left()) + 4.;
    let galley = ui.painter().layout_no_wrap(
        label.into(),
        egui::FontId::proportional(10.),
        Color32::from_gray(200),
    );
    let pill = Rect::from_min_size(
        egui::pos2(x, row.top() + 3.),
        galley.size() + egui::vec2(8., 2.),
    );
    ui.painter()
        .rect_filled(pill, 3., Color32::from_rgba_unmultiplied(20, 26, 28, 230));
    ui.painter().galley(
        pill.min + egui::vec2(4., 1.),
        galley,
        Color32::from_gray(200),
    );
}
thread_local! {
    /// Timeline rows hidden by the Hide tracks menu this frame; they take no height.
    static HIDDEN_ROWS: std::cell::RefCell<Vec<bool>> = const { std::cell::RefCell::new(Vec::new()) };
}
pub(super) fn set_hidden_rows(mask: Vec<bool>) {
    HIDDEN_ROWS.with(|h| *h.borrow_mut() = mask);
}
pub(super) fn row_hidden(row: usize) -> bool {
    HIDDEN_ROWS.with(|h| h.borrow().get(row).copied().unwrap_or(false))
}
fn hidden_before(row: usize) -> usize {
    HIDDEN_ROWS.with(|h| h.borrow().iter().take(row).filter(|x| **x).count())
}
pub(super) fn row_y(top: f32, row: usize, expanded: Option<(usize, usize)>) -> f32 {
    top + (row - hidden_before(row)) as f32 * 39.
        + expanded
            .filter(|(r, _)| row > *r)
            .map_or(0., |(_, n)| n as f32 * 25.)
}
pub(super) fn row_at(top: f32, y: f32, expanded: Option<(usize, usize)>) -> usize {
    let visible = visible_row_at(top, y, expanded);
    // Map the n-th visible row back to its layer row.
    HIDDEN_ROWS.with(|h| {
        let h = h.borrow();
        let mut seen = 0;
        for (row, hidden) in h.iter().enumerate() {
            if !hidden {
                if seen == visible {
                    return row;
                }
                seen += 1;
            }
        }
        h.len() + visible - seen
    })
}
fn visible_row_at(top: f32, y: f32, expanded: Option<(usize, usize)>) -> usize {
    let expanded = expanded.map(|(row, n)| (row - hidden_before(row), n));
    let mut value = y - top;
    if let Some((row, n)) = expanded {
        let edge = (row + 1) as f32 * 39.;
        if value >= edge {
            if value < edge + n as f32 * 25. {
                return row;
            }
            value -= n as f32 * 25.;
        }
    }
    (value / 39.).floor().max(0.) as usize
}
impl Editor {
    pub(super) fn keyframe_rows(&mut self, p: &Project) {
        // While a clip or track is dragged, keep the lanes where they were when
        // the drag began. Following the dragged clip moves the rows under the
        // pointer, which moved the clip back and forth every frame (bg-repair).
        if self.timeline_dragging() {
            return;
        }
        self.video.keyframes.rows = None;
        if !self.video.keyframes.lanes {
            return;
        }
        let item = match self.video.selected {
            Selection::Recording(i) => p.segments.get(i).map(|c| Item::Media(c.id)),
            _ => self.selected.map(Item::Annotation),
        };
        if let Some(item) = item
            && let Some((set, _, _)) = keys(p, item)
            && let Some((_, _, layer)) = p.item_range(item)
        {
            let cursor = match item {
                Item::Annotation(id) => usize::from(cursor_path(p, id).is_some()),
                _ => 0,
            };
            let lanes = set.channels.len() + cursor;
            if lanes > 0 {
                self.video.keyframes.rows = Some((p.layer_index(layer), lanes));
            }
        }
    }
    pub(crate) fn keyframe_annotation(&self, a: &crate::Annotation) -> crate::Annotation {
        self.session
            .document
            .video
            .as_ref()
            .and_then(|p| {
                p.objects.get(&a.id).map(|t| {
                    // Outside its own range a split or trimmed part holds its first
                    // or last pose instead of following keys that belong to the
                    // other part (BB10).
                    let last = t.end.saturating_sub(t.start).saturating_sub(1);
                    t.keyframes
                        .editable_annotation(a, p.cursor.saturating_sub(t.start).min(last))
                })
            })
            .unwrap_or_else(|| a.clone())
    }
    pub(crate) fn keyframe_command(&self, command: Command) -> capture_core::Result<Command> {
        let Some(p) = &self.session.document.video else {
            return Ok(command);
        };
        // A handle rotation is an edit of the value at the playhead, like a move.
        let rotated;
        let edits = match &command {
            Command::RotateAnnotation { id, degrees } => {
                let Some(base) = self
                    .session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == *id)
                else {
                    return Ok(command);
                };
                let mut a = self.keyframe_annotation(base);
                a.rotation_degrees = *degrees;
                rotated = [a];
                &rotated[..]
            }
            Command::Update { annotation } => std::slice::from_ref(annotation),
            Command::UpdateMany { annotations } | Command::MoveAndExpand { annotations, .. } => {
                annotations.as_slice()
            }
            _ => return Ok(command),
        };
        // Placing a new object (for example a Step marker) is not a keyframe
        // edit; routing it here silently dropped it while Keyframes was on (BB9).
        // Switching an object's shape (rectangle, ellipse, callout…) changes the
        // whole clip; key channels cannot hold it, so it was dropped too (BB8).
        if edits.iter().any(|a| {
            self.session
                .document
                .annotations
                .iter()
                .find(|b| b.id == a.id)
                .is_none_or(|b| b.kind != a.kind || b.callout.is_some() != a.callout.is_some())
        }) {
            return Ok(command);
        }
        if !self.video.keyframes.auto_key
            && !edits.iter().any(|a| {
                p.objects
                    .get(&a.id)
                    .is_some_and(|t| !t.keyframes.is_empty())
            })
        {
            return Ok(command);
        }
        if edits
            .iter()
            .any(|a| self.session.document.object_state(a.id).locked)
        {
            return Err(capture_core::Error::Invalid(
                "This object is locked. Unlock it to animate it.".into(),
            ));
        }
        let mut doc = self.session.document.clone();
        capture_core::video::keyframes::edit_annotations(
            &mut doc,
            edits,
            self.video.keyframes.auto_key,
        )?;
        Ok(Command::VideoComposition {
            project: doc.video.unwrap(),
            annotations: doc.annotations,
        })
    }
    /// The Properties tab follows the Keyframes switch for what is selected.
    pub(super) fn keyframes_tab(&mut self, on: bool) {
        if self.selected.is_none() && !matches!(self.video.selected, Selection::Recording(_)) {
            return;
        }
        let tab = if on {
            PropertyTab::Motion
        } else {
            PropertyTab::Style
        };
        self.video.tab = tab;
        self.video.object_tab = tab;
        self.video.text_return = None;
    }
    pub(super) fn keyframe_toggle(&mut self, ui: &mut egui::Ui) {
        let on = self.video.keyframes.auto_key;
        if super::ui::button(ui,"Keyframes",if on{"Keyframes ON"}else{"Keyframes OFF"},126.,on).tip("On: changes create keys at the playhead. Off: edit the whole clip while retaining its animation.").clicked(){
            self.commit_text();self.commit_property_preview();self.video.keyframes.auto_key = !on;self.video.keyframes.expand = !on;self.video.keyframes.lanes = !on;self.video.keyframes.path = !on;self.keyframes_tab(!on);
        }
    }
    /// The Keyframes section (approved mockup 2): the ON/OFF switch sits in the
    /// section header; the section opens when keyframing turns on and stays collapsible.
    pub(super) fn keyframe_controls(
        &mut self,
        ui: &mut egui::Ui,
        set: &mut Set,
        bounds: (u64, u64),
        cursor: &mut u64,
        values: &[(Property, Value)],
        privacy: bool,
    ) {
        let on = self.video.keyframes.auto_key;
        let expand = std::mem::take(&mut self.video.keyframes.expand);
        let count = set.times(bounds.1.saturating_sub(bounds.0)).len();
        let id = ui.make_persistent_id(("property-group", "Keyframes"));
        let (manual, automatic) = ui.ctx().data_mut(|d| {
            if expand || on && d.get_temp::<bool>(id).is_none() {
                d.insert_temp(id, true);
            }
            (
                d.get_temp::<bool>(id),
                d.get_temp::<bool>(egui::Id::unique("properties-auto-open"))
                    .unwrap_or(true),
            )
        });
        let mut open = manual.unwrap_or(automatic);
        let (r, header) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.), Sense::click());
        let switch = Rect::from_center_size(
            egui::pos2(r.right() - 22., r.center().y),
            egui::vec2(34., 22.),
        );
        let mut state = on;
        let toggled = ui
            .scope_builder(egui::UiBuilder::new().max_rect(switch), |ui| {
                crate::controls::toggle(ui, &mut state, "")
            })
            .inner
            .tip(
                "On: changes create keys at the playhead. Off: edit the whole clip while keeping its animation.",
            )
            .changed();
        if toggled {
            self.commit_text();
            self.commit_property_preview();
            self.video.keyframes.auto_key = state;
            // BA8: lanes and the canvas path follow the Keyframes switch.
            self.video.keyframes.lanes = state;
            self.video.keyframes.path = state;
            self.keyframes_tab(state);
            if state {
                open = true;
                ui.ctx().data_mut(|d| d.insert_temp(id, true));
            }
        }
        if header.clicked() {
            open = !open;
            ui.ctx().data_mut(|d| d.insert_temp(id, open));
        }
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique(("property-group-state", "Keyframes")),
                (r, open),
            );
            d.insert_temp(egui::Id::unique("keyframes-switch"), switch);
        });
        let p = ui.painter();
        if header.hovered() {
            p.rect_filled(r, 4., Color32::from_white_alpha(5));
        }
        super::ui::icon(
            p,
            Rect::from_center_size(
                egui::pos2(r.left() + 9., r.center().y),
                egui::vec2(16., 16.),
            ),
            "Keyframes",
            Color32::from_rgb(169, 196, 207),
        );
        p.text(
            egui::pos2(r.left() + 27., r.center().y),
            egui::Align2::LEFT_CENTER,
            "Keyframes",
            crate::panel_ui::font(13.),
            Color32::WHITE,
        );
        p.text(
            egui::pos2(switch.left() - 8., r.center().y),
            egui::Align2::RIGHT_CENTER,
            format!(
                "{} · {count} key{}",
                if state { "On" } else { "Off" },
                if count == 1 { "" } else { "s" }
            ),
            egui::FontId::proportional(11.),
            Color32::from_rgb(181, 194, 199),
        );
        if open {
            ui.push_id(id, |ui| {
                self.keyframe_body(ui, set, bounds, cursor, values, privacy)
            });
            ui.add_space(8.);
        }
    }
    /// A 28 px icon button in the key bar.
    fn key_button(
        ui: &mut egui::Ui,
        icon: &str,
        tip: &str,
        enabled: bool,
        on: bool,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(28., 28.),
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let p = ui.painter();
        let fill = if on {
            Color32::from_rgb(35, 65, 74)
        } else if response.hovered() && enabled {
            Color32::from_rgb(52, 61, 64)
        } else {
            Color32::from_rgb(36, 43, 46)
        };
        p.rect_filled(rect, 6., fill);
        if on {
            p.rect_stroke(
                rect,
                6.,
                Stroke::new(1., crate::shell::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        super::ui::icon(
            p,
            rect.shrink(6.),
            icon,
            if !enabled {
                Color32::from_gray(95)
            } else if on {
                Color32::from_rgb(125, 214, 237)
            } else {
                Color32::from_gray(215)
            },
        );
        response.tip(tip)
    }
    fn keyframe_body(
        &mut self,
        ui: &mut egui::Ui,
        set: &mut Set,
        bounds: (u64, u64),
        cursor: &mut u64,
        values: &[(Property, Value)],
        privacy: bool,
    ) {
        let (start, end) = bounds;
        let duration = end.saturating_sub(start).max(1);
        if !values
            .iter()
            .any(|(p, _)| *p == self.video.keyframes.property)
            || (privacy && self.video.keyframes.property == Property::Opacity)
        {
            self.video.keyframes.property = Property::Position;
        }
        ui.horizontal(|ui| {
            ui.add_sized(
                [62., 22.],
                egui::Label::new(egui::RichText::new("Property").weak()),
            );
            egui::ComboBox::from_id_salt("key-property")
                .width(ui.available_width())
                .selected_text(self.video.keyframes.property.label())
                .show_ui(ui, |ui| {
                    for (p, _) in values
                        .iter()
                        .filter(|(p, _)| !privacy || *p != Property::Opacity)
                    {
                        ui.selectable_value(&mut self.video.keyframes.property, *p, p.label());
                    }
                });
        });
        let property = self.video.keyframes.property;
        // Only video clips have Crop among their properties.
        let media = values.iter().any(|(p, _)| *p == Property::Crop);
        let node = property.node(media);
        let age = cursor.saturating_sub(start);
        let active = *cursor >= start && *cursor < end;
        let times = set
            .channels
            .get(&property)
            .map(|c| {
                c.keys
                    .iter()
                    .filter_map(|k| k.at.checked_sub(set.offset_ms))
                    .filter(|t| *t < duration)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // Mini lane: this property's keys and the playhead; click to seek.
        let (lane, lane_response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 22.), Sense::click());
        {
            let p = ui.painter();
            p.rect_filled(lane, 5., Color32::from_rgb(21, 26, 28));
            p.rect_stroke(
                lane,
                5.,
                Stroke::new(1., Color32::from_rgb(52, 61, 64)),
                egui::StrokeKind::Inside,
            );
            let x = |t: u64| lane.left() + 6. + t as f32 / duration as f32 * (lane.width() - 12.);
            for t in &times {
                let c = egui::pos2(x(*t), lane.center().y);
                p.add(egui::Shape::convex_polygon(
                    vec![
                        c + egui::vec2(0., -4.5),
                        c + egui::vec2(4.5, 0.),
                        c + egui::vec2(0., 4.5),
                        c + egui::vec2(-4.5, 0.),
                    ],
                    if *t == age {
                        Color32::WHITE
                    } else {
                        crate::shell::ACCENT
                    },
                    Stroke::new(1., Color32::from_rgb(11, 18, 20)),
                ));
            }
            if active {
                let px = x(age);
                p.line_segment(
                    [egui::pos2(px, lane.top()), egui::pos2(px, lane.bottom())],
                    Stroke::new(2., Color32::WHITE),
                );
            }
            if lane_response.clicked()
                && let Some(pos) = lane_response.interact_pointer_pos()
            {
                let t = (((pos.x - lane.left() - 6.) / (lane.width() - 12.)).clamp(0., 1.)
                    * duration as f32) as u64;
                *cursor = start + t.min(duration - 1);
            }
        }
        // Key bar: previous/next, key status, add/remove, duplicate and more.
        let mut duplicate = false;
        let at_key = times.contains(&age);
        egui::Frame::new()
            .fill(Color32::from_rgb(21, 26, 28))
            .stroke(Stroke::new(1., Color32::from_rgb(52, 61, 64)))
            .corner_radius(8)
            .inner_margin(4)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    if Self::key_button(ui, "Chevron left", "Previous key", times.iter().any(|t| *t < age), false)
                        .clicked()
                    {
                        *cursor = start + *times.iter().rfind(|t| **t < age).unwrap();
                    }
                    if Self::key_button(ui, "Chevron right", "Next key", times.iter().any(|t| *t > age), false)
                        .clicked()
                    {
                        *cursor = start + *times.iter().find(|t| **t > age).unwrap();
                    }
                    // Three 28 px buttons, each preceded by one item gap.
                    let right = 3. * (28. + ui.spacing().item_spacing.x);
                    let status = if !active {
                        "Playhead outside clip".to_string()
                    } else if at_key {
                        format!(
                            "Key {} of {} · {:.2} s",
                            times.iter().position(|t| *t == age).unwrap() + 1,
                            times.len(),
                            age as f64 / 1000.
                        )
                    } else {
                        format!("No key here · {:.2} s", age as f64 / 1000.)
                    };
                    ui.add_sized(
                        [(ui.available_width() - right).max(40.), 28.],
                        egui::Label::new(egui::RichText::new(status).size(11.5).weak()).truncate(),
                    );
                    if Self::key_button(
                        ui,
                        if at_key { "Keyframe filled" } else { "Keyframe" },
                        if at_key { "Remove the key at the playhead" } else { "Add a key at the playhead" },
                        active,
                        at_key,
                    )
                    .clicked()
                    {
                        if at_key {
                            for q in node {
                                set.remove(age, Some(*q));
                            }
                        } else {
                            for q in node {
                                if let Some((_, base)) = values.iter().find(|(p, _)| p == q) {
                                    let value = set.sample(*q, age).unwrap_or_else(|| base.clone());
                                    set.insert(*q, age, base.clone(), value);
                                }
                            }
                        }
                    }
                    let earlier = set.times(duration).iter().any(|t| *t < age);
                    duplicate |= Self::key_button(
                        ui,
                        "Duplicate",
                        "Duplicate previous keyframe: copy the nearest earlier keyframe (every property keyed there) to the playhead",
                        active && earlier,
                        false,
                    )
                    .clicked();
                    let more = Self::key_button(ui, "More", "More key actions", true, false);
                    egui::Popup::menu(&more).show(|ui| {
                        if ui
                            .add_enabled(active && earlier, egui::Button::new("Duplicate previous keyframe"))
                            .clicked()
                        {
                            duplicate = true;
                            ui.close();
                        }
                        if ui.add_enabled(at_key, egui::Button::new("Copy this key")).clicked() {
                            self.video.keyframes.copied = set.sample(property, age).map(|v| (property, v));
                            ui.close();
                        }
                        let paste = self
                            .video
                            .keyframes
                            .copied
                            .clone()
                            .filter(|(p, _)| *p == property);
                        if ui
                            .add_enabled(active && paste.is_some(), egui::Button::new("Paste key at playhead"))
                            .clicked()
                            && let Some((p, v)) = paste
                            && let Some((_, base)) = values.iter().find(|(q, _)| *q == p)
                        {
                            set.insert(p, age, base.clone(), v);
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(at_key, egui::Button::new(egui::RichText::new("Delete this key").color(Color32::from_rgb(239, 106, 98))))
                            .clicked()
                        {
                            for q in node {
                                set.remove(age, Some(*q));
                            }
                            ui.close();
                        }
                        if ui
                            .add_enabled(
                                set.channels.contains_key(&property),
                                egui::Button::new(egui::RichText::new(format!("Clear {} keys", property.label())).color(Color32::from_rgb(239, 106, 98))),
                            )
                            .clicked()
                        {
                            set.channels.remove(&property);
                            ui.close();
                        }
                    });
                });
            });
        if duplicate {
            set.duplicate_previous(age, None);
        }
        if let Some((_, base)) = values.iter().find(|(p, _)| *p == property) {
            let mut value = set.sample(property, age).unwrap_or_else(|| base.clone());
            let before = value.clone();
            if let Value::Numbers(v) = &mut value {
                ui.add_enabled_ui(active, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (i, n) in v.iter_mut().enumerate() {
                            let label = match property {
                                Property::Position => ["X", "Y"][i],
                                Property::Size => ["Width", "Height"][i],
                                Property::Crop => ["Left", "Top", "Width", "Height"][i],
                                Property::Opacity => "Opacity",
                                Property::Rotation => "Degrees",
                                _ => "Value",
                            };
                            ui.label(label);
                            let range = match property {
                                Property::Opacity | Property::Crop => 0. ..=1.,
                                // A video clip's frame is a fraction of the canvas.
                                Property::Size if media => 0.01..=1.,
                                Property::Position if media => -8. ..=8.,
                                Property::Size => 0.001..=10000.,
                                Property::Rotation => -360. ..=360.,
                                Property::StrokeWidth => 0. ..=128.,
                                Property::CornerRadius => 0. ..=2048.,
                                _ => -10000. ..=10000.,
                            };
                            ui.add(
                                egui::DragValue::new(n)
                                    .speed(
                                        if property == Property::Opacity
                                            || property == Property::Crop
                                        {
                                            0.01
                                        } else if media
                                            && matches!(
                                                property,
                                                Property::Position | Property::Size
                                            )
                                        {
                                            0.005
                                        } else {
                                            0.5
                                        },
                                    )
                                    .range(range),
                            );
                        }
                    });
                });
            }
            if value != before {
                for q in node.iter().filter(|q| **q != property) {
                    if let Some((_, other)) = values.iter().find(|(p, _)| p == q) {
                        let held = set.sample(*q, age).unwrap_or_else(|| other.clone());
                        set.insert(*q, age, other.clone(), held);
                    }
                }
                set.insert(property, age, base.clone(), value);
            }
        }
        if let Some(c) = set.channels.get_mut(&property) {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [62., 22.],
                    egui::Label::new(egui::RichText::new("Transition").weak()),
                );
                crate::controls::segments(ui, |ui| {
                    for curve in [Curve::Smooth, Curve::Linear, Curve::Hold] {
                        ui.selectable_value(&mut c.curve, curve, format!("{curve:?}"));
                    }
                });
            });
        }
        ui.horizontal(|ui| {
            ui.add_sized([62., 22.], egui::Label::new(egui::RichText::new("Show").weak()));
            let lanes = self.video.keyframes.lanes;
            if ui.selectable_label(lanes, "Lanes").tip("Show property lanes under the selected clip").clicked() {
                self.video.keyframes.lanes = !lanes;
            }
            let path = self.video.keyframes.path;
            if ui.selectable_label(path, "Path").tip("Show the motion path on the canvas").clicked() {
                self.video.keyframes.path = !path;
            }
            ui.label(egui::RichText::new(" ? ").weak()).tip(
                "Position, size and styling can also be edited on the canvas. Text changes use Hold; other properties interpolate.",
            );
        });
        if set.is_empty() {
            set.offset_ms = 0;
        }
    }
    pub(super) fn keyframe_marks(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        item: Item,
        rect: Rect,
        scale: f32,
        property: Option<Property>,
    ) -> bool {
        let Some((set, start, duration)) = keys(p, item) else {
            return false;
        };
        let times = if let Some(property) = property {
            set.channels
                .get(&property)
                .map(|c| {
                    c.keys
                        .iter()
                        .filter_map(|k| k.at.checked_sub(set.offset_ms))
                        .filter(|t| *t < duration)
                        .collect()
                })
                .unwrap_or_default()
        } else {
            set.times(duration)
        };
        let mut captured = self
            .video
            .keyframes
            .drag
            .as_ref()
            .is_some_and(|d| d.item == item && d.property == property);
        for age in times {
            let center = egui::pos2(rect.left() + age as f32 * scale, rect.bottom() - 6.);
            let hit = Rect::from_center_size(center, egui::vec2(12., 12.));
            let r = ui.interact(
                hit,
                egui::Id::unique(("key-diamond", format!("{item:?}"), property, age)),
                Sense::click_and_drag(),
            );
            let selected = p.cursor.abs_diff(start + age) <= 1000 / p.fps as u64;
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(0., -4.),
                    center + egui::vec2(4., 0.),
                    center + egui::vec2(0., 4.),
                    center + egui::vec2(-4., 0.),
                ],
                if selected {
                    Color32::WHITE
                } else {
                    crate::shell::ACCENT
                },
                Stroke::new(1., Color32::from_rgb(20, 35, 39)),
            ));
            captured |= r.hovered() || r.is_pointer_button_down_on();
            if r.clicked() {
                self.video_seek(start + age);
            }
            if r.drag_started_by(egui::PointerButton::Primary) {
                self.video.pause();
                let duplicate = ui.input(|i| {
                    self.gesture_matches(crate::gesture_input::Gesture::DuplicateDrag, i.modifiers)
                });
                self.video.keyframes.drag = Some(Drag {
                    original: self.session.document.video.clone().unwrap(),
                    item,
                    from: age,
                    property,
                    duplicate,
                });
            }
            r.tip(format!(
                "Keyframe {:.3}s · drag to retime, Alt+drag to copy",
                (start + age) as f64 / 1000.
            ))
            .context_menu(|ui| {
                if ui.button("Delete keyframe").clicked() {
                    let mut next = p.clone();
                    keys_mut(&mut next, item).unwrap().remove(age, property);
                    self.commit_video(next);
                    ui.close();
                }
            });
        }
        if captured
            && let Some(d) = &self.video.keyframes.drag
            && d.item == item
            && d.property == property
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let mut next = d.original.clone();
            let fps = p.fps as f64;
            let age = ((pos.x - rect.left()) / scale).max(0.) as f64;
            let at = ((age * fps / 1000.).round() * 1000. / fps).round() as u64;
            let at = at.min(duration.saturating_sub(1));
            let set = keys_mut(&mut next, item).unwrap();
            let edited = if d.duplicate {
                set.copy_key(d.from, at, property)
            } else {
                set.move_key(d.from, at, property)
            };
            if edited.is_ok() {
                self.video.timing_preview = Some(next);
            }
            ui.ctx().set_cursor_icon(if d.duplicate {
                egui::CursorIcon::Copy
            } else {
                egui::CursorIcon::ResizeHorizontal
            });
            if ui.input(|i| i.pointer.any_released()) {
                self.video.keyframes.drag = None;
                if let Some(next) = self.video.timing_preview.take() {
                    self.commit_video(next);
                }
            }
        }
        captured
    }
    pub(super) fn keyframe_lanes(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        left: f32,
        top: f32,
        width: f32,
        scale: f32,
    ) {
        if !self.video.keyframes.lanes {
            return;
        }
        let item = match self.video.selected {
            Selection::Recording(i) => p.segments.get(i).map(|c| Item::Media(c.id)),
            _ => self.selected.map(Item::Annotation),
        };
        let Some(item) = item else { return };
        let Some((set, start, duration)) = keys(p, item) else {
            return;
        };
        for (i, property) in set.channels.keys().copied().enumerate() {
            let y = top + i as f32 * 25.;
            let row = Rect::from_min_size(egui::pos2(left, y), egui::vec2(width, 23.));
            ui.painter()
                .rect_filled(row, 3., Color32::from_rgb(28, 36, 39));
            let rect = Rect::from_min_max(
                egui::pos2(left + start as f32 * scale, y),
                egui::pos2(left + (start + duration) as f32 * scale, y + 23.),
            );
            self.keyframe_marks(ui, p, item, rect, scale, Some(property));
            lane_label(ui, row, property.label());
        }
        if let Item::Annotation(id) = item
            && cursor_path(p, id).is_some()
        {
            let y = top + set.channels.len() as f32 * 25.;
            let row = Rect::from_min_size(egui::pos2(left, y), egui::vec2(width, 23.));
            ui.painter()
                .rect_filled(row, 3., Color32::from_rgb(28, 36, 39));
            let rect = Rect::from_min_max(
                egui::pos2(left + start as f32 * scale, y),
                egui::pos2(left + (start + duration) as f32 * scale, y + 23.),
            );
            self.path_marks(ui, p, id, rect, scale);
            lane_label(ui, row, "Cursor path");
        }
    }
    /// Draws a recorded cursor path's keys on its clip. Click and release anchors
    /// are larger; keys closer than [`PATH_KEY_SPACING`] are thinned on screen only.
    /// Click seeks, drag retimes one key, and the context menu deletes it.
    pub(super) fn path_marks(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        id: u64,
        rect: Rect,
        scale: f32,
    ) -> bool {
        let Some((path, start, duration)) = cursor_path(p, id) else {
            return false;
        };
        let offset = path.offset_ms;
        let mut anchors: Vec<u64> = path
            .inputs
            .iter()
            .flat_map(|i| std::iter::once(i.at).chain(i.release))
            .collect();
        anchors.sort_unstable();
        let mut captured = self
            .video
            .keyframes
            .path_drag
            .as_ref()
            .is_some_and(|d| d.item == Item::Annotation(id));
        let mut last_x = f32::NEG_INFINITY;
        let painter = ui
            .painter()
            .with_clip_rect(rect.expand2(egui::vec2(6., 0.)));
        for node in &path.nodes {
            let Some(age) = node.at.checked_sub(offset).filter(|a| *a < duration) else {
                continue;
            };
            let anchor = anchors.binary_search(&node.at).is_ok();
            let x = rect.left() + age as f32 * scale;
            if !anchor && x - last_x < PATH_KEY_SPACING {
                continue;
            }
            last_x = x;
            let center = egui::pos2(x, rect.bottom() - 6.);
            let half = if anchor { 4.5 } else { 3. };
            let selected = p.cursor.abs_diff(start + age) <= 1000 / p.fps as u64;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(0., -half),
                    center + egui::vec2(half, 0.),
                    center + egui::vec2(0., half),
                    center + egui::vec2(-half, 0.),
                ],
                if selected {
                    Color32::WHITE
                } else if anchor {
                    Color32::from_rgb(125, 214, 237)
                } else {
                    Color32::from_rgb(95, 150, 170)
                },
                Stroke::new(1., Color32::from_rgb(20, 35, 39)),
            ));
            let r = ui.interact(
                Rect::from_center_size(center, egui::vec2(10., 12.)),
                egui::Id::unique(("cursor-key", id, node.at)),
                Sense::click_and_drag(),
            );
            captured |= r.hovered() || r.is_pointer_button_down_on();
            if r.clicked() {
                self.video_seek(start + age);
            }
            if r.drag_started_by(egui::PointerButton::Primary) {
                self.video.pause();
                self.video.keyframes.path_drag = Some(Drag {
                    original: self.session.document.video.clone().unwrap(),
                    item: Item::Annotation(id),
                    from: node.at,
                    property: None,
                    duplicate: false,
                });
            }
            r.tip(format!(
                "{} {:.3}s · drag to retime",
                if anchor { "Click" } else { "Cursor key" },
                (start + age) as f64 / 1000.
            ))
            .context_menu(|ui| {
                if ui
                    .add_enabled(path.nodes.len() > 1, egui::Button::new("Delete cursor key"))
                    .clicked()
                {
                    let mut next = p.clone();
                    if let Some(t) = next.objects.get_mut(&id)
                        && let Some(path) = t.motion_path.as_mut()
                    {
                        std::sync::Arc::make_mut(path)
                            .nodes
                            .retain(|n| n.at != node.at);
                    }
                    self.commit_video(next);
                    ui.close();
                }
            });
        }
        if captured
            && let Some(d) = &self.video.keyframes.path_drag
            && d.item == Item::Annotation(id)
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let mut next = d.original.clone();
            let fps = p.fps as f64;
            let age = ((pos.x - rect.left()) / scale).max(0.) as f64;
            let age = ((age * fps / 1000.).round() * 1000. / fps).round() as u64;
            let to = age.min(duration.saturating_sub(1)) + offset;
            if let Some(t) = next.objects.get_mut(&id)
                && let Some(path) = t.motion_path.as_mut()
            {
                let path = std::sync::Arc::make_mut(path);
                if let Some(i) = path.nodes.iter().position(|n| n.at == d.from) {
                    let lo = i.checked_sub(1).map_or(0, |j| path.nodes[j].at + 1);
                    let hi = path.nodes.get(i + 1).map_or(u64::MAX, |n| n.at - 1);
                    path.nodes[i].at = to.clamp(lo, hi.max(lo));
                    self.video.timing_preview = Some(next);
                }
            }
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            if ui.input(|i| i.pointer.any_released()) {
                self.video.keyframes.path_drag = None;
                if let Some(next) = self.video.timing_preview.take() {
                    self.commit_video(next);
                }
            }
        }
        captured
    }
    pub(super) fn paint_keyframe_path(&self, ui: &egui::Ui, t: crate::canvas_view::Transform) {
        if !self.video.keyframes.path {
            return;
        };
        let (points, line) = self.keyframe_path_geometry();
        let points: Vec<_> = points.into_iter().map(|p| t.screen_point(p)).collect();
        let line: Vec<_> = line.into_iter().map(|p| t.screen_point(p)).collect();
        let painter = ui.painter().with_clip_rect(t.viewport);
        for pair in line.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(1., crate::shell::ACCENT));
        }
        for point in points {
            painter.circle_stroke(point, 3., Stroke::new(1.5, Color32::WHITE));
        }
    }
    /// Canvas-space centers of the selected object's position keys within its
    /// visible range, including an in-progress drag's effect.
    #[cfg(test)]
    pub(super) fn keyframe_path_points(&self) -> Vec<[f32; 2]> {
        self.keyframe_path_geometry().0
    }
    /// Key centers plus the line the object actually travels inside its own
    /// range: a trimmed or split part starts and ends where it really is (BA8),
    /// not at keys that now lie outside the part.
    fn keyframe_path_geometry(&self) -> (Vec<[f32; 2]>, Vec<[f32; 2]>) {
        let none = || (vec![], vec![]);
        let Some(p) = &self.session.document.video else {
            return none();
        };
        let Some(id) = self.selected else {
            return none();
        };
        let Some(timing) = p.objects.get(&id) else {
            return none();
        };
        let Some(base) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
        else {
            return none();
        };
        // While dragging, show the path the release will produce: apply the
        // in-progress edit to a copy of the keys exactly as the commit will.
        let dragged = self
            .edit_drag
            .as_ref()
            .filter(|d| d.preview.id == id)
            .map(|d| d.preview.clone())
            .or_else(|| {
                self.rotation_drag
                    .as_ref()
                    .filter(|d| d.preview.id == id)
                    .map(|d| d.preview.clone())
            })
            .or_else(|| {
                self.group_drag
                    .as_ref()
                    .and_then(|g| g.preview.iter().find(|a| a.id == id).cloned())
            });
        let mut keys = timing.keyframes.clone();
        let mut base = base.clone();
        if let Some(edited) = dragged
            && (self.video.keyframes.auto_key || !keys.is_empty())
        {
            let age = p.cursor.saturating_sub(timing.start);
            base = keys.edit_annotation(&base, &edited, age, self.video.keyframes.auto_key);
        }
        let Some(c) = keys.channels.get(&Property::Position) else {
            return none();
        };
        let mut points = vec![];
        let mut line = vec![];
        let center = |age: u64| {
            let a = keys.editable_annotation(&base, age);
            let (lo, hi) = a.body_bounds();
            [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]
        };
        // Only keys inside this clip's visible range are drawn. A split part keeps
        // the other part's keys (so it can be extended again) without showing them.
        let duration = timing.end.saturating_sub(timing.start);
        for key in &c.keys {
            if let Some(age) = key
                .at
                .checked_sub(keys.offset_ms)
                .filter(|age| *age < duration)
            {
                points.push(center(age));
            }
        }
        if !points.is_empty() {
            let before = c.keys.iter().any(|k| k.at < keys.offset_ms);
            let after = c
                .keys
                .iter()
                .any(|k| k.at.saturating_sub(keys.offset_ms) >= duration);
            if before {
                line.push(center(0));
            }
            line.extend(points.iter().copied());
            if after {
                line.push(center(duration.saturating_sub(1)));
            }
        }
        (points, line)
    }
}
