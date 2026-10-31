use crate::{Annotation, Command, Editor, canvas_view::Transform, egui};
use capture_core::rotation::RotationGesture;

pub(super) struct RotateDrag {
    pub preview: Annotation,
    gesture: RotationGesture,
}

impl Editor {
    fn rotatable_selection(&self) -> Option<Annotation> {
        (self.selected_objects().len() == 1 && self.text_draft.is_none())
            .then(|| self.selected_annotation())
            .flatten()
            .filter(|a| a.can_rotate() && Self::tool_matches(self.tool, a.kind))
    }

    pub(super) fn rotation_hit(&self, pos: egui::Pos2, transform: Transform) -> bool {
        self.rotatable_selection().is_some_and(|a| {
            let (min, max) = a.body_bounds();
            a.rotation()
                .handle(min, max, transform.scale)
                .is_ok_and(|p| transform.screen_point(p).distance(pos) <= 9.0)
        })
    }

    pub(super) fn rotation_input(
        &mut self,
        ui: &egui::Ui,
        transform: Transform,
        enabled: bool,
    ) -> bool {
        let (focused, pressed, released, pointer, shift) = ui.input(|i| {
            (
                i.focused,
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.interact_pos(),
                i.modifiers.shift,
            )
        });
        if !focused || !enabled {
            let handled = self.rotation_drag.take().is_some();
            if handled {
                self.needs_render = true;
            }
            return handled;
        }
        if self.rotation_drag.is_none()
            && pressed
            && self.edit_drag.is_none()
            && self.group_drag.is_none()
            && self.step_placement.is_none()
            && self.drag_start.is_none()
            && !self.view.panning
            && let Some(pos) = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
            })
            && transform.viewport.contains(pos)
            && self.rotation_hit(pos, transform)
            && let Some(a) = self.rotatable_selection()
        {
            match RotationGesture::begin(a.rotation(), transform.image_point(pos)) {
                Ok(gesture) => {
                    self.rotation_drag = Some(RotateDrag {
                        preview: a,
                        gesture,
                    })
                }
                Err(error) => self.status = error.to_string(),
            }
        }
        let Some(drag) = &mut self.rotation_drag else {
            return false;
        };
        if let Some(pos) = pointer {
            match drag.gesture.update(transform.image_point(pos), shift) {
                Ok(rotation) => {
                    if rotation.degrees != drag.preview.rotation_degrees {
                        drag.preview.rotation_degrees = rotation.degrees;
                        self.needs_render = true;
                        ui.ctx().request_repaint();
                    }
                }
                Err(error) => self.status = error.to_string(),
            }
        }
        if released {
            let drag = self.rotation_drag.take().unwrap();
            self.execute(Command::RotateAnnotation {
                id: drag.preview.id,
                degrees: drag.preview.rotation_degrees,
            });
            self.needs_render = true;
            ui.ctx().request_repaint();
        }
        true
    }

    pub(super) fn paint_rotation_node(&self, ui: &egui::Ui, transform: Transform) {
        let Some(a) = self.rotatable_selection() else {
            return;
        };
        let (min, max) = a.body_bounds();
        let Ok(handle) = a.rotation().handle(min, max, transform.scale) else {
            return;
        };
        let pos = transform.screen_point(handle);
        let top = transform.screen_point(a.rotation().point([(min[0] + max[0]) * 0.5, min[1]]));
        let painter = ui.painter().with_clip_rect(transform.viewport);
        let stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
        painter.line_segment(
            [top, pos],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(189, 245, 69)),
        );
        painter.circle_filled(pos, 8.0, egui::Color32::from_rgb(35, 119, 172));
        let arc = (0..=18)
            .map(|i| {
                let angle = (50.0 + i as f32 * 15.0).to_radians();
                pos + egui::vec2(angle.cos(), angle.sin()) * 4.5
            })
            .collect();
        painter.add(egui::Shape::line(arc, stroke));
        painter.add(egui::Shape::convex_polygon(
            vec![
                pos + egui::vec2(5.0, -4.0),
                pos + egui::vec2(5.0, 0.5),
                pos + egui::vec2(1.0, -2.5),
            ],
            egui::Color32::WHITE,
            egui::Stroke::NONE,
        ));
        let hover = ui
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| p.distance(pos) <= 9.0);
        if self.rotation_drag.is_some() || hover {
            ui.ctx().set_cursor_icon(if self.rotation_drag.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
            painter.text(
                pos + egui::vec2(13.0, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{:.0}° · Shift:45°", a.rotation_degrees),
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton};

    fn frame(ctx: &egui::Context, e: &mut Editor, events: Vec<Event>, focused: bool) -> Transform {
        let mut viewport = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                focused,
                ..Default::default()
            },
            |ui| {
                viewport = ui.available_rect_before_wrap();
                e.show_canvas(ui);
            },
        );
        output.textures_delta.clear();
        e.view
            .transform(viewport, egui::vec2(640.0, 400.0), ctx.pixels_per_point())
    }
    fn button(pos: egui::Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        }
    }
    #[test]
    fn rotation_node_previews_actual_objects_then_commits_one_undo_at_multiple_zooms() {
        use crate::Kind;
        for kind in [
            Kind::Step,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Text,
            Kind::Callout,
            Kind::Line,
            Kind::Arrow,
        ] {
            for zoom in [0.5, 1.0, 2.0] {
                let ctx = egui::Context::default();
                let mut e = crate::canvas::tests::editor(&ctx);
                let mut a = capture_core::steps::marker(
                    1,
                    [320.0, 200.0],
                    80,
                    [30, 70, 100, 255],
                    (8, false),
                    [640, 400],
                )
                .unwrap();
                a.kind = kind;
                if kind == Kind::Callout {
                    a.callout = Some(capture_core::callout::CalloutStyle::for_box(
                        a.start,
                        a.end,
                        [640, 400],
                    ));
                }
                if matches!(kind, Kind::Line | Kind::Arrow) {
                    a.text = None;
                }
                e.session.document.annotations.push(a.clone());
                e.tool = Some(kind);
                e.select_only(Some(1));
                e.view.apply(
                    crate::canvas_view::ZoomAction::Set(crate::canvas_view::ZoomMode::Manual(zoom)),
                    None,
                );
                let t = frame(&ctx, &mut e, vec![], true);
                let (min, max) = a.body_bounds();
                let p = t.screen_point(a.rotation().handle(min, max, t.scale).unwrap());
                let center = t.screen_point(a.rotation().center);
                let distance = p.distance(center);
                let target = |angle: f32| {
                    center
                        + egui::vec2(angle.to_radians().sin(), -angle.to_radians().cos()) * distance
                };
                frame(
                    &ctx,
                    &mut e,
                    vec![Event::PointerMoved(p), button(p, true)],
                    true,
                );
                assert!(e.rotation_drag.is_some(), "{kind:?} at {zoom}");
                assert!(e.step_placement.is_none());
                frame(&ctx, &mut e, vec![Event::PointerMoved(target(37.0))], true);
                assert!(
                    (e.rotation_drag.as_ref().unwrap().preview.rotation_degrees - 37.0).abs()
                        < 0.01
                );
                assert_eq!(e.session.document.annotations, vec![a.clone()]);
                assert!(!e.session.can_undo());
                let preview = e.preview_document();
                assert_eq!(preview.version, 3);
                assert_ne!(
                    preview.render().unwrap(),
                    e.session.document.render().unwrap()
                );
                frame(
                    &ctx,
                    &mut e,
                    vec![
                        Event::ModifiersChanged(Modifiers::SHIFT),
                        Event::PointerMoved(target(55.0)),
                    ],
                    true,
                );
                assert!(
                    (e.rotation_drag.as_ref().unwrap().preview.rotation_degrees - 45.0).abs()
                        < 0.01
                );
                frame(
                    &ctx,
                    &mut e,
                    vec![
                        Event::ModifiersChanged(Modifiers::NONE),
                        Event::PointerMoved(target(90.0)),
                    ],
                    true,
                );
                let preview = e.preview_document().render().unwrap();
                frame(&ctx, &mut e, vec![button(target(90.0), false)], true);
                assert!(e.rotation_drag.is_none());
                assert_eq!(
                    e.session.document.annotations.len(),
                    1,
                    "Step rotation must not place another marker"
                );
                assert_eq!(e.selected, Some(1));
                assert_eq!(e.session.document.render().unwrap(), preview);
                assert!(e.session.can_undo());
                e.execute(Command::Undo);
                assert_eq!(e.session.document.annotations, vec![a]);
                assert!(!e.session.can_undo());
                e.execute(Command::Redo);
                assert_eq!(e.session.document.render().unwrap(), preview);
            }
        }
    }

    #[test]
    fn rotation_focus_loss_keeps_document_and_redo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let a = capture_core::steps::marker(
            1,
            [320.0, 200.0],
            80,
            [30, 70, 100, 255],
            (8, false),
            [640, 400],
        )
        .unwrap();
        e.session.document.annotations.push(a.clone());
        e.execute(Command::RotateAnnotation {
            id: 1,
            degrees: 20.0,
        });
        e.execute(Command::Undo);
        e.select_only(Some(1));
        e.tool = None;
        let t = frame(&ctx, &mut e, vec![], true);
        let (min, max) = a.body_bounds();
        let p = t.screen_point(a.rotation().handle(min, max, t.scale).unwrap());
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(p), button(p, true)],
            true,
        );
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(p + egui::vec2(40.0, 10.0))],
            true,
        );
        assert!(e.rotation_drag.is_some());
        frame(&ctx, &mut e, vec![], false);
        assert!(e.rotation_drag.is_none());
        assert_eq!(e.session.document.annotations, vec![a]);
        assert!(!e.session.can_undo());
        assert!(e.session.can_redo());
    }
}
