use crate::{Annotation, Command, Editor, egui};

pub(super) struct GroupDrag {
    pub originals: Vec<Annotation>,
    pub preview: Vec<Annotation>,
    pub from: [f32; 2],
    pub duplicate: bool,
    pub transform: Option<crate::group_transform::TransformDrag>,
}

impl Editor {
    pub(super) fn enter_group_child(&mut self, id: u64) -> bool {
        let Some(a) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
        else {
            return false;
        };
        let Some(group) = a.group else { return false };
        if !self.session.document.object_editable(id)
            || (self.editing_group == Some(group) && self.selected == Some(id))
        {
            return false;
        }
        self.editing_group = Some(group);
        self.select_only(Some(id));
        self.activate_selected_tool();
        true
    }

    pub(super) fn auto_activate_selected_tool(&mut self) {
        if self.quick_controls.auto_switch_tool() {
            self.activate_selected_tool();
        }
    }
    pub(super) fn activate_selected_tool(&mut self) {
        if let Some(a) = self.selected_annotation() {
            self.shell.focus.active = a.focus.is_some();
            self.shell.focus.remember(&a);
            self.tool = (a.kind != crate::Kind::Image).then_some(a.kind);
            self.restore_drawing_defaults(self.tool);

            self.shell.properties = true;
            self.sync_step_sequence();
        }
    }
    pub(super) fn select_only(&mut self, id: Option<u64>) {
        if let Some(id) = id {
            self.sync_video_object_selection(id);
        }
        self.video.zoom_selected = None;
        if !id.is_some_and(|id| {
            self.session
                .document
                .annotations
                .iter()
                .any(|a| a.id == id && a.group.is_some() && a.group == self.editing_group)
        }) {
            self.editing_group = None;
        }
        if let Some(a) = id
            .and_then(|id| {
                self.session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == id)
            })
            .cloned()
        {
            self.quick_controls.touch_snap(a.id);
            self.remember_placement_template(&a);
        }
        self.selected = id;
        self.extra_selected.clear();
        self.selection_order = id.into_iter().collect();
        if let Some(id) = id.filter(|_| self.editing_group.is_none()) {
            for member in self.session.document.group_members(id) {
                if member != id {
                    self.extra_selected.insert(member);
                    self.selection_order.push(member);
                }
            }
        }
    }

    pub(super) fn selection_ordered_ids(&self) -> Vec<u64> {
        let objects = self.selected_objects();
        let mut ids = self
            .selection_order
            .iter()
            .copied()
            .filter(|id| objects.iter().any(|a| a.id == *id))
            .collect::<Vec<_>>();
        for a in objects {
            if !ids.contains(&a.id) {
                ids.push(a.id);
            }
        }
        ids
    }

    pub(super) fn is_selected(&self, id: u64) -> bool {
        self.selected == Some(id) || (self.selected.is_some() && self.extra_selected.contains(&id))
    }

    pub(super) fn selected_objects(&self) -> Vec<Annotation> {
        self.session
            .document
            .annotations
            .iter()
            .filter(|a| self.is_selected(a.id) && !self.session.document.object_state(a.id).locked)
            .map(|a| {
                self.text_draft
                    .as_ref()
                    .filter(|d| d.preview.id == a.id)
                    .map_or_else(|| self.keyframe_annotation(a), |d| d.preview.clone())
            })
            .collect()
    }

    pub(super) fn selection_same_kind(&self) -> bool {
        let objects = self.selected_objects();
        objects
            .first()
            .is_none_or(|first| objects.iter().all(|a| a.kind == first.kind))
    }

    pub(super) fn click_selection(&mut self, hit: Option<u64>, additive: bool) {
        self.clear_timeline_selection();
        if !additive {
            self.select_only(hit);
            return;
        }
        let Some(id) = hit else {
            return;
        };
        let in_group = self.editing_group.is_some()
            && self
                .session
                .document
                .annotations
                .iter()
                .any(|a| a.id == id && a.group == self.editing_group);
        if !in_group {
            self.editing_group = None;
        }
        let unit = if in_group {
            vec![id]
        } else {
            self.session.document.group_members(id)
        };
        let remove = unit.iter().any(|id| self.is_selected(*id));
        let mut order = self.selection_ordered_ids();
        if remove {
            order.retain(|id| !unit.contains(id));
        } else {
            order.extend(unit);
        }
        self.selected = order.last().copied();
        self.extra_selected = order
            .iter()
            .copied()
            .filter(|id| Some(*id) != self.selected)
            .collect();
        self.selection_order = order;
    }

    pub(super) fn move_selection(&mut self, delta: [f32; 2]) {
        match capture_core::multi_selection::translated(
            &self.selected_objects(),
            delta,
            [self.session.document.width, self.session.document.height],
        ) {
            Ok(annotations) => self.execute(Command::UpdateMany { annotations }),
            Err(error) => self.status = error.to_string(),
        }
    }

    pub(super) fn delete_selection(&mut self) {
        if self.is_video() && self.delete_video_selection() {
            return;
        }
        let ids = self.selected_objects().iter().map(|a| a.id).collect();
        self.execute(Command::RemoveMany { ids });
    }

    pub(super) fn select_all_matching(&mut self) {
        let ids: Vec<_> = self
            .session
            .document
            .annotations
            .iter()
            .filter(|a| {
                self.session.document.object_editable(a.id) && Self::tool_matches(self.tool, a.kind)
            })
            .map(|a| a.id)
            .collect();
        self.select_only(None);
        for id in ids {
            if !self.is_selected(id) {
                self.click_selection(Some(id), true);
            }
        }
    }

    pub(super) fn selection_outlines(
        &self,
        painter: &egui::Painter,
        transform: crate::canvas_view::Transform,
    ) {
        let objects = self
            .group_drag
            .as_ref()
            .map(|d| d.preview.clone())
            .unwrap_or_else(|| self.selected_objects());
        if objects.len() < 2 && !self.group_drag.as_ref().is_some_and(|d| d.duplicate) {
            return;
        }
        let color = egui::Color32::from_rgb(189, 245, 69);
        let mut union = egui::Rect::NOTHING;
        for a in objects {
            let transform = self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.objects.get(&a.id))
                .map_or(transform, |timing| {
                    self.video_layer_transform(
                        transform,
                        capture_core::video::links::Layer::Annotation(timing.track),
                    )
                });
            crate::line_properties::paint_selection(painter, &a, transform, color);
            let (min, max) = a.bounds();
            let rect =
                egui::Rect::from_two_pos(transform.screen_point(min), transform.screen_point(max));
            union = union.union(rect);
            painter.rect_stroke(
                rect.expand(2.0),
                0.0,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Outside,
            );
        }
        painter.rect_stroke(
            union.expand(5.0),
            0.0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Outside,
        );
    }
}
