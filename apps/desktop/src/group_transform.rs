use crate::{Annotation, Editor, canvas_view::Transform, egui};
use capture_core::{
    groups,
    rotation::{Rotation, RotationGesture},
};
pub(super) enum TransformDrag {
    Scale { anchor: [f32; 2], vector: [f32; 2] },
    Rotate(RotationGesture),
}
fn nodes(objects: &[Annotation], t: Transform) -> Option<[egui::Pos2; 5]> {
    let (min, max) = groups::bounds(objects).ok()?;
    let r = egui::Rect::from_min_max(t.screen_point(min), t.screen_point(max)).expand(5.);
    Some([
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.center_top() - egui::vec2(0., 24.),
    ])
}
impl Editor {
    fn group_transform_hit(&self, pos: egui::Pos2, t: Transform) -> Option<usize> {
        if self.text_draft.is_some()
            || self.property_preview.is_some()
            || self.edit_drag.is_some()
            || self.rotation_drag.is_some()
        {
            return None;
        }
        let objects = self.selected_objects();
        let nodes = nodes(&objects, t)?;
        nodes
            .iter()
            .enumerate()
            .find(|(i, p)| {
                p.distance(pos) <= 8. && (*i != 4 || objects.iter().all(|a| a.can_rotate()))
            })
            .map(|(i, _)| i)
    }
    /// A selection-rotation drag is in progress.
    pub(super) fn group_rotating(&self) -> bool {
        self.group_drag
            .as_ref()
            .is_some_and(|d| matches!(d.transform, Some(TransformDrag::Rotate(_))))
    }
    pub(super) fn group_transform_cursor(
        &self,
        pos: egui::Pos2,
        t: Transform,
    ) -> Option<egui::CursorIcon> {
        if self
            .group_drag
            .as_ref()
            .is_some_and(|d| d.transform.is_some())
        {
            return Some(egui::CursorIcon::Grabbing);
        }
        self.group_transform_hit(pos, t).map(|n| match n {
            0 | 2 => egui::CursorIcon::ResizeNwSe,
            1 | 3 => egui::CursorIcon::ResizeNeSw,
            _ => egui::CursorIcon::Grab,
        })
    }
    pub(super) fn group_transform_input(
        &mut self,
        ui: &egui::Ui,
        t: Transform,
        enabled: bool,
    ) -> bool {
        let active = self
            .group_drag
            .as_ref()
            .is_some_and(|d| d.transform.is_some());
        if !enabled || !ui.input(|i| i.focused) {
            if active {
                self.group_drag = None;
                self.needs_render = true;
            }
            return active;
        }
        if !active
            && self.group_drag.is_none()
            && !self.view.panning
            && !ui.input(|i| {
                i.key_down(egui::Key::Space)
                    || self
                        .gesture_matches(crate::gesture_input::Gesture::DuplicateDrag, i.modifiers)
            })
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && let Some(node) = self.group_transform_hit(pos, t)
        {
            let originals = self.selected_objects();
            let (min, max) = groups::bounds(&originals).unwrap();
            let point = t.image_point(pos);
            let operation = if node == 4 {
                let center = [0, 1].map(|i| (min[i] + max[i]) * 0.5);
                let Ok(gesture) = RotationGesture::begin(Rotation::new(center, 0.).unwrap(), point)
                else {
                    return false;
                };
                TransformDrag::Rotate(gesture)
            } else {
                let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
                let anchor = corners[(node + 2) % 4];

                TransformDrag::Scale {
                    anchor,
                    vector: [point[0] - anchor[0], point[1] - anchor[1]],
                }
            };
            self.group_drag = Some(crate::multi_select::GroupDrag {
                preview: originals.clone(),
                originals,
                from: point,
                duplicate: false,
                transform: Some(operation),
            });
        }
        let Some(drag) = self.group_drag.as_mut().filter(|d| d.transform.is_some()) else {
            return false;
        };
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let point = t.image_point(pos);
            let next = match drag.transform.as_mut().unwrap() {
                TransformDrag::Scale { anchor, vector } => {
                    let d = [point[0] - anchor[0], point[1] - anchor[1]];
                    let scale = ((d[0] * vector[0] + d[1] * vector[1])
                        / (vector[0] * vector[0] + vector[1] * vector[1]).max(0.001))
                    .clamp(0.01, 100.);
                    groups::scaled(&drag.originals, *anchor, scale)
                }
                TransformDrag::Rotate(gesture) => gesture
                    .update(point, ui.input(|i| i.modifiers.shift))
                    .and_then(|r| groups::rotated(&drag.originals, r.center, r.degrees)),
            }
            .and_then(|objects| {
                self.session.document.plan_expanding_move(&objects, false)?;
                Ok(objects)
            });
            match next {
                Ok(objects) if objects != drag.preview => {
                    drag.preview = objects;
                    self.needs_render = true;
                    ui.ctx().request_repaint();
                }
                Err(error) => self.status = error.to_string(),
                _ => {}
            }
        }
        if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
            let drag = self.group_drag.take().unwrap();
            if drag.preview != drag.originals {
                self.commit_expanding_move(drag.preview, false, t);
                self.status = "Selection transformed. Undo restores the whole gesture.".into();
            }
            self.needs_render = true;
        }
        true
    }
    pub(super) fn paint_group_handles(&self, ui: &egui::Ui, t: Transform) {
        if self.text_draft.is_some() {
            return;
        }
        let objects = self
            .group_drag
            .as_ref()
            .map(|d| d.preview.clone())
            .unwrap_or_else(|| self.selected_objects());
        let Some(nodes) = nodes(&objects, t) else {
            return;
        };
        let p = ui.painter().with_clip_rect(t.viewport);
        let ink = egui::Color32::from_rgb(189, 245, 69);
        for n in &nodes[..4] {
            let r = egui::Rect::from_center_size(*n, egui::vec2(9., 9.));
            p.rect_filled(r, 1, egui::Color32::from_gray(25));
            p.rect_stroke(r, 1, egui::Stroke::new(1.5, ink), egui::StrokeKind::Inside);
        }
        if objects.iter().all(|a| a.can_rotate()) {
            let n = nodes[4];
            p.line_segment(
                [n + egui::vec2(0., 8.), n + egui::vec2(0., 24.)],
                egui::Stroke::new(1., ink),
            );
            p.circle_filled(n, 8., egui::Color32::from_rgb(35, 119, 172));
            let arc = (0..=18)
                .map(|i| {
                    let a = (50. + i as f32 * 15.).to_radians();
                    n + egui::vec2(a.cos(), a.sin()) * 4.5
                })
                .collect();
            p.add(egui::Shape::line(
                arc,
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            ));
            p.add(egui::Shape::convex_polygon(
                vec![
                    n + egui::vec2(5., -4.),
                    n + egui::vec2(5., 0.5),
                    n + egui::vec2(1., -2.5),
                ],
                egui::Color32::WHITE,
                egui::Stroke::NONE,
            ));
        }
        if let Some(n) = ui
            .input(|i| i.pointer.hover_pos())
            .and_then(|p| self.group_transform_hit(p, t))
        {
            let text = if n == 4 {
                "Rotate selection · Shift: 45°"
            } else {
                "Resize selection · proportions locked"
            };
            p.text(
                nodes[n] + egui::vec2(13., 0.),
                egui::Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(11.),
                egui::Color32::WHITE,
            );
        }
    }
}
#[cfg(test)]
mod tests;
