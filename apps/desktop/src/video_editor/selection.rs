use super::*;
use capture_core::video::editing::Item;
use std::collections::BTreeSet;
pub(super) struct Move {
    pub original: Project,
    pub from: egui::Pos2,
    pub items: BTreeSet<Item>,
}
pub(super) struct Marquee {
    pub from: egui::Pos2,
    pub prior: BTreeSet<Item>,
}
impl Editor {
    pub(super) fn timeline_focus(&mut self, item: Item, add: bool) {
        self.tool = None;
        self.icon_library.active = false;
        self.video.zoom_selected = None;
        self.video.zoom_edit = false;
        self.video.zoom_focus_drag = None;
        if add {
            if !self.video.selection.insert(item) {
                self.video.selection.remove(&item);
            }
        } else if !self.video.selection.contains(&item) {
            self.video.selection.clear();
            self.video.selection.insert(item);
        }
        match item {
            Item::Annotation(id) => {
                self.select_only(Some(id));
                self.video.selected = Selection::Objects;
            }
            Item::Media(id) => {
                self.select_only(None);
                if let Some(i) = self
                    .session
                    .document
                    .video
                    .as_ref()
                    .and_then(|p| p.segments.iter().position(|c| c.id == id))
                {
                    self.video.selected = Selection::Recording(i);
                }
            }
            Item::Zoom(id) => {
                self.select_only(None);
                self.video.selected = Selection::Objects;
                self.video.zoom_selected = Some(id);
            }
        }
        self.tool = None;
        self.icon_library.active = false;
    }
    pub(super) fn begin_timeline_move(&mut self, item: Item, pos: egui::Pos2) {
        self.timeline_focus(item, false);
        let original = self.session.document.video.clone().unwrap();
        let items = original.linked_items(&self.video.selection);
        self.video.selection = items.clone();
        self.video.multi_drag = Some(Move {
            original,
            from: pos,
            items,
        });
        self.video.pause();
    }
    pub(super) fn timeline_selection_input(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        background: egui::Response,
        left: f32,
        top: f32,
        scale: f32,
    ) {
        let rect_for = |p: &Project, item: Item| {
            p.item_range(item).map(|(start, end, layer)| {
                egui::Rect::from_min_max(
                    egui::pos2(
                        left + start as f32 * scale,
                        top + p.layer_index(layer) as f32 * 39.,
                    ),
                    egui::pos2(
                        left + end as f32 * scale,
                        top + p.layer_index(layer) as f32 * 39. + 33.,
                    ),
                )
            })
        };
        if background.drag_started_by(egui::PointerButton::Primary)
            && self.video.multi_drag.is_none()
            && self.video.drag.is_none()
            && self.video.record_drag.is_none()
            && self.video.zoom_drag.is_none()
            && let Some(from) = ui.input(|i| i.pointer.press_origin())
            && ui.clip_rect().contains(from)
            && !p
                .items()
                .into_iter()
                .any(|id| rect_for(p, id).is_some_and(|r| r.contains(from)))
        {
            let prior = if ui.input(|i| i.modifiers.ctrl || i.modifiers.shift) {
                self.video.selection.clone()
            } else {
                BTreeSet::new()
            };
            self.video.marquee = Some(Marquee { from, prior });
            self.tool = None;
            self.icon_library.active = false;
        }
        if let Some(m) = &self.video.marquee
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let rect = egui::Rect::from_two_pos(m.from, pos);
            self.video.selection = m
                .prior
                .iter()
                .copied()
                .chain(
                    p.items()
                        .into_iter()
                        .filter(|id| rect_for(p, *id).is_some_and(|r| r.intersects(rect))),
                )
                .collect();
            ui.painter().rect(
                rect,
                2.,
                egui::Color32::from_rgba_unmultiplied(25, 158, 200, 30),
                egui::Stroke::new(1., crate::shell::ACCENT),
                egui::StrokeKind::Inside,
            );
            if ui.input(|i| i.pointer.any_released()) {
                self.video.marquee = None;
            }
        }
        if let Some(d) = &self.video.multi_drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let vertical_only = ui.input(|i| i.modifiers.shift);
            let mut delta = if vertical_only {
                0
            } else {
                ((pos.x - d.from.x) / scale).round() as i64
            };
            if self.video.snap && !vertical_only {
                delta = d
                    .original
                    .snap_move(&d.items, delta, (8. / scale).round() as u64);
            }
            let rows =
                ((pos.y - top) / 39.).floor() as i32 - ((d.from.y - top) / 39.).floor() as i32;
            let moved = if vertical_only {
                d.original.move_items(&d.items, 0, rows)
            } else {
                d.original.move_items_near(&d.items, delta, rows)
            };
            match moved {
                Ok(next) => {
                    self.video.timing_preview = Some(next);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                Err(e) => {
                    self.status = e.to_string();
                    ui.ctx().set_cursor_icon(egui::CursorIcon::NotAllowed);
                }
            }
            if ui.input(|i| i.pointer.any_released()) {
                let next = self.video.timing_preview.take();
                self.video.multi_drag = None;
                if let Some(next) = next {
                    self.commit_video(next);
                }
            }
        }
        for item in &self.video.selection {
            if let Some(rect) = rect_for(self.video.timing_preview.as_ref().unwrap_or(p), *item) {
                ui.painter().rect_stroke(
                    rect,
                    4.,
                    egui::Stroke::new(2., crate::shell::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.marquee = None;
            self.video.multi_drag = None;
            self.video.timing_preview = None;
        }
        if background.clicked()
            && let Some(pos) = background.interact_pointer_pos()
            && ui.clip_rect().contains(pos)
            && !p
                .items()
                .into_iter()
                .any(|id| rect_for(p, id).is_some_and(|r| r.contains(pos)))
        {
            self.video.selection.clear();
        }
    }
    pub(super) fn timeline_link_menu(&mut self, ui: &mut egui::Ui) {
        if self.video.selection.len() > 1
            && crate::controls::menu_action(ui, "Link selected clips").clicked()
        {
            let mut p = self.session.document.video.clone().unwrap();
            match p.link_items(&self.video.selection) {
                Ok(()) => self.commit_video(p),
                Err(e) => self.status = e.to_string(),
            };
            ui.close();
        }
        if crate::controls::menu_action(ui, "Unlink selected clips").clicked() {
            let mut p = self.session.document.video.clone().unwrap();
            p.unlink_items(&self.video.selection);
            self.commit_video(p);
            ui.close();
        }
    }
}
