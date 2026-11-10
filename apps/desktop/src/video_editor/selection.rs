use super::*;
use capture_core::video::editing::Item;
use std::collections::BTreeSet;
pub(super) struct Move {
    pub original: Project,
    pub from: egui::Pos2,
    pub items: BTreeSet<Item>,
    /// The items stay where they are and copies land at the drop point.
    pub duplicate: bool,
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
                let focus = if self.video.selection.contains(&item) {
                    Some(id)
                } else {
                    self.timeline_selected_objects().last().copied()
                };
                self.select_only(focus);
                self.video.selected = Selection::Objects;
                self.adopt_timeline_selection();
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
                self.video.zoom_selected = if self.video.selection.contains(&item) {
                    Some(id)
                } else {
                    self.video.selection.iter().rev().find_map(|i| match i {
                        Item::Zoom(z) => Some(*z),
                        _ => None,
                    })
                };
            }
        }
        self.tool = None;
        self.icon_library.active = false;
    }
    /// What is selected on the canvas shows on the timeline too, so both ways of
    /// selecting are the same selection.
    pub(super) fn sync_timeline_with_canvas(&mut self) {
        if self.selected.is_none()
            || self.video.multi_drag.is_some()
            || self.video.marquee.is_some()
        {
            return;
        }
        let objects: BTreeSet<Item> = self
            .selected_objects()
            .iter()
            .map(|a| Item::Annotation(a.id))
            .collect();
        if objects.is_empty() {
            return;
        }
        let keep = |item: &Item| match item {
            Item::Annotation(id) => self.session.document.object_state(*id).locked,
            _ => false,
        };
        let others: BTreeSet<Item> = self
            .video
            .selection
            .iter()
            .filter(|i| keep(i))
            .copied()
            .collect();
        self.video.selection = others.into_iter().chain(objects).collect();
    }
    /// Objects selected together on the timeline are the editor's multi-selection,
    /// so every property edit reaches all of them.
    fn adopt_timeline_selection(&mut self) {
        let ids = self.timeline_selected_objects();
        if ids.len() < 2 {
            return;
        }
        let Some(focus) = self.selected.filter(|id| ids.contains(id)) else {
            return;
        };
        for id in ids {
            if id != focus && self.extra_selected.insert(id) {
                self.selection_order.push(id);
            }
        }
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
            duplicate: false,
        });
        self.video.pause();
    }
    /// A frame preset chosen with several clips selected places each in full.
    pub(super) fn spread_clip_preset(&self, v: &mut Project, preset: usize, ratio: f32) {
        for item in &self.video.selection {
            if let Item::Media(id) = item
                && let Some(c) = v.segments.iter_mut().find(|c| c.id == *id)
                && c.kind() != capture_core::video::editor::MediaKind::Audio
            {
                super::frames::apply_clip_preset(c, preset, ratio);
            }
        }
    }
    /// Gives the other selected clips what an edit changed on clip `index`.
    pub(super) fn spread_clip_edit(&self, v: &mut Project, original: &Project, index: usize) {
        let (Some(before), Some(after)) =
            (original.segments.get(index), v.segments.get(index).cloned())
        else {
            return;
        };
        if *before == after {
            return;
        }
        let others: Vec<usize> = self
            .video
            .selection
            .iter()
            .filter_map(|item| match item {
                Item::Media(id) if *id != before.id => v.segments.iter().position(|c| c.id == *id),
                _ => None,
            })
            .collect();
        for i in others {
            v.segments[i].copy_change(before, &after);
            if after.speed != before.speed {
                // A clip whose source can't take this speed keeps its own.
                let _ = v.speed(i, after.speed);
            }
        }
    }
    /// Gives the other selected objects what an edit changed on object `id`.
    pub(super) fn spread_timing(
        &self,
        v: &mut Project,
        id: u64,
        before: &capture_core::video::editor::Timing,
        after: &capture_core::video::editor::Timing,
    ) {
        if after == before {
            return;
        }
        for other in self.selected_objects() {
            if other.id != id
                && let Some(mut t) = v.objects.get(&other.id).cloned()
            {
                t.copy_change(before, after, &other);
                v.place(other.id, t);
            }
        }
    }
    /// Alt+drag: copies of the selection follow the pointer.
    pub(super) fn begin_timeline_duplicate(&mut self, item: Item, pos: egui::Pos2) {
        self.begin_timeline_move(item, pos);
        if let Some(d) = &mut self.video.multi_drag {
            d.duplicate = true;
        }
    }
    /// Adds copies of `items` at their places in `moved` as one Undo step.
    pub(super) fn commit_timeline_copies(&mut self, moved: &Project, items: &BTreeSet<Item>) {
        let Some(mut project) = self.session.document.video.clone() else {
            return;
        };
        let mut annotations = self.session.document.annotations.clone();
        let mut next_id = annotations.iter().map(|a| a.id).max().unwrap_or(0);
        let mut next_group = annotations
            .iter()
            .filter_map(|a| a.group)
            .max()
            .unwrap_or(0);
        let mut groups = std::collections::BTreeMap::new();
        let mut copies = BTreeSet::new();
        for item in items {
            match *item {
                Item::Annotation(id) => {
                    let original = annotations.iter().find(|a| a.id == id).cloned();
                    if let (Some(mut copy), Some(timing)) = (original, moved.objects.get(&id)) {
                        next_id += 1;
                        copy.id = next_id;
                        copy.group = copy.group.map(|g| {
                            *groups.entry(g).or_insert_with(|| {
                                next_group += 1;
                                next_group
                            })
                        });
                        annotations.push(copy);
                        project.place(next_id, timing.clone());
                        copies.insert(Item::Annotation(next_id));
                    }
                }
                Item::Media(id) => {
                    if let Some(mut copy) = moved.segments.iter().find(|c| c.id == id).cloned() {
                        copy.id = project.next_media_id();
                        copy.link = None;
                        copies.insert(Item::Media(copy.id));
                        project.segments.push(copy);
                    }
                }
                Item::Zoom(id) => {
                    if let Some(mut copy) = moved.zooms.iter().find(|z| z.id == id).cloned() {
                        // Zooms on one track cannot overlap: a copy that would
                        // gets a track of its own.
                        let track = copy.track();
                        copy.id = project.zooms.iter().map(|z| z.id).max().unwrap_or(0) + 1;
                        copy.lane =
                            if project.zooms.iter().any(|o| {
                                o.track() == track && o.start < copy.end && copy.start < o.end
                            }) {
                                0
                            } else {
                                track
                            };
                        copies.insert(Item::Zoom(copy.id));
                        project.zooms.push(copy);
                    }
                }
            }
        }
        self.execute(Command::VideoComposition {
            project,
            annotations,
        });
        if let Some(id) = copies.iter().find_map(|i| match i {
            Item::Annotation(id) => Some(*id),
            _ => None,
        }) {
            self.video.selection = copies;
            self.select_only(Some(id));
            self.video.selected = Selection::Objects;
            self.adopt_timeline_selection();
        } else {
            self.video.selection = copies;
        }
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
        let key_rows = self.video.keyframes.rows;
        let rect_for = |p: &Project, item: Item| {
            p.item_range(item)
                .filter(|(_, _, layer)| !super::keyframes::row_hidden(p.layer_index(*layer)))
                .map(|(start, end, layer)| {
                    egui::Rect::from_min_max(
                        egui::pos2(
                            left + start as f32 * scale,
                            super::keyframes::row_y(top, p.layer_index(layer), key_rows),
                        ),
                        egui::pos2(
                            left + end as f32 * scale,
                            super::keyframes::row_y(top, p.layer_index(layer), key_rows) + 33.,
                        ),
                    )
                })
        };
        // BA8: a click on empty timeline space deselects (Ctrl/Shift keep the selection).
        if background.clicked_by(egui::PointerButton::Primary)
            && !ui.input(|i| i.modifiers.ctrl || i.modifiers.shift)
            && let Some(at) = background.interact_pointer_pos()
            && !p
                .items()
                .into_iter()
                .any(|id| rect_for(p, id).is_some_and(|r| r.contains(at)))
        {
            self.clear_timeline_selection();
            self.select_only(None);
        }
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
                if let Some(last) = self.timeline_selected_objects().last().copied() {
                    self.select_only(Some(last));
                    self.video.selected = Selection::Objects;
                    self.adopt_timeline_selection();
                }
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
            let rows = super::keyframes::row_at(top, pos.y, key_rows) as i32
                - super::keyframes::row_at(top, d.from.y, key_rows) as i32;
            let moved = if vertical_only {
                d.original.move_items(&d.items, 0, rows)
            } else {
                d.original.move_items_near(&d.items, delta, rows)
            };
            match moved {
                Ok(next) if d.duplicate => {
                    for item in &d.items {
                        if let Some(rect) = rect_for(&next, *item) {
                            ui.painter().rect_filled(
                                rect,
                                4.,
                                egui::Color32::from_rgba_unmultiplied(25, 154, 195, 150),
                            );
                            ui.painter().rect_stroke(
                                rect,
                                4.,
                                egui::Stroke::new(2., crate::shell::ACCENT),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "+ Copy",
                                egui::FontId::proportional(12.),
                                egui::Color32::WHITE,
                            );
                        }
                    }
                    self.video.copy_preview = Some(next);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
                }
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
                let copies = self.video.copy_preview.take();
                if let Some(d) = self.video.multi_drag.take() {
                    if let Some(moved) = copies.filter(|_| d.duplicate) {
                        self.commit_timeline_copies(&moved, &d.items);
                    } else if let Some(next) = next {
                        self.commit_video(next);
                    }
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
            self.video.copy_preview = None;
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
    /// Unlocked objects in the timeline multi-selection, in layer order.
    pub(crate) fn timeline_selected_objects(&self) -> Vec<u64> {
        let d = &self.session.document;
        d.annotations
            .iter()
            .map(|a| a.id)
            .filter(|id| {
                self.video.selection.contains(&Item::Annotation(*id)) && !d.object_state(*id).locked
            })
            .collect()
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
        if self.group_preset_ids().len() > 1
            && crate::controls::menu_action(ui, "Save as block…").clicked()
        {
            self.save_group_preset();
            ui.close();
        }
    }
}
