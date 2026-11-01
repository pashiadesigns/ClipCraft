//! Redact shares click-repeat placement, while a drag defines new bounds.
use crate::{Command, Editor, Kind, egui};
impl Editor {
    pub(super) fn redact_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
        interactive: bool,
    ) -> bool {
        if self.tool != Some(Kind::Redact)
            || !interactive
            || self.view.panning
            || ui.input(|i| i.key_down(egui::Key::Space))
        {
            return false;
        }
        if let Some(a) = self
            .selected_annotation()
            .filter(|a| a.kind == Kind::Redact)
        {
            self.stamps.redact = Some(a);
        }
        let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else {
            return false;
        };
        let point = transform.image_point(pointer);
        let visible = transform.image.intersect(transform.viewport);
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        let hit = self.tool_hit(point, 7. / transform.scale);
        if self.stamps.redact_drag.is_none()
            && (self.edit_drag.is_some()
                || self
                    .selected_annotation()
                    .is_some_and(|a| a.hit_handle(point, 7. / transform.scale).is_some())
                || hit.is_some())
        {
            return false;
        }
        let mut template = self
            .stamps
            .redact
            .clone()
            .unwrap_or_else(|| self.new_annotation(Kind::Redact, [0., 0.], [100., 28.]));
        template.id = self.session.next_id();
        template.group = None;
        let size = [
            (template.end[0] - template.start[0]).abs(),
            (template.end[1] - template.start[1]).abs(),
        ];
        template.start = [point[0] - size[0] * 0.5, point[1] - size[1] * 0.5];
        template.end = [point[0] + size[0] * 0.5, point[1] + size[1] * 0.5];
        if pressed && visible.contains(pointer) {
            self.select_only(None);
            self.stamps.redact_drag = Some((point, template.clone()));
        }
        if let Some((origin, preview)) = &mut self.stamps.redact_drag {
            if egui::vec2(point[0] - origin[0], point[1] - origin[1]).length() * transform.scale
                > 4.
            {
                template.start = [origin[0].min(point[0]), origin[1].min(point[1])];
                template.end = [origin[0].max(point[0]), origin[1].max(point[1])];
            }
            *preview = template.clone();
        }
        if (response.hovered() && visible.contains(pointer) && self.selected.is_none())
            || self.stamps.redact_drag.is_some()
        {
            let rect = egui::Rect::from_min_max(
                transform.screen_point(template.start),
                transform.screen_point(template.end),
            );
            ui.painter().with_clip_rect(visible).rect_filled(
                rect,
                template.corner_radius * transform.scale,
                egui::Color32::from_rgba_unmultiplied(
                    template.color[0],
                    template.color[1],
                    template.color[2],
                    template.color[3],
                ),
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            for delta in [egui::vec2(4., 0.), egui::vec2(0., 4.)] {
                ui.painter().line_segment(
                    [pointer - delta, pointer + delta],
                    egui::Stroke::new(2., egui::Color32::WHITE),
                );
                ui.painter().line_segment(
                    [pointer - delta, pointer + delta],
                    egui::Stroke::new(1., egui::Color32::BLACK),
                );
            }
        }
        if ui.input(|i| i.pointer.primary_released())
            && let Some((_, mut a)) = self.stamps.redact_drag.take()
        {
            let canvas = [self.session.document.width, self.session.document.height];
            for (axis, limit) in canvas.into_iter().enumerate() {
                a.start[axis] = a.start[axis].clamp(0., limit as f32);
                a.end[axis] = a.end[axis].clamp(0., limit as f32);
            }
            if a.end[0] - a.start[0] >= 1. && a.end[1] - a.start[1] >= 1. {
                self.execute(Command::Add { annotation: a });
                self.select_only(None);
            }
        }
        self.selected.is_none() || self.stamps.redact_drag.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn click_repeats_selected_redaction_drag_resizes_and_escape_returns_move() {
        use crate::property_layout::tests::PaintedOutput;
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(Kind::Redact);
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640., 400.));
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(area),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    e.show_canvas(ui);
                },
            ))
        };
        frame(&mut e, vec![]);
        frame(&mut e, vec![]);
        let t = e.view.current_transform().unwrap();
        let button = |p, pressed| egui::Event::PointerButton {
            pos: t.screen_point(p),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(t.screen_point([100., 80.])),
                button([100., 80.], true),
            ],
        );
        assert!(e.session.document.annotations.is_empty());
        frame(&mut e, vec![button([100., 80.], false)]);
        let a = e.session.document.annotations[0].clone();
        assert_eq!(a.end[0] - a.start[0], 100.);
        let mut edited = a.clone();
        edited.color = [22, 32, 42, 255];
        edited.end = [a.start[0] + 70., a.start[1] + 22.];
        e.select_only(Some(a.id));
        e.execute(Command::Update { annotation: edited });
        e.select_only(None);
        frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(t.screen_point([300., 80.])),
                button([300., 80.], true),
            ],
        );
        frame(&mut e, vec![button([300., 80.], false)]);
        assert_eq!(e.session.document.annotations[1].color, [22, 32, 42, 255]);
        assert_eq!(
            e.session.document.annotations[1].end[0] - e.session.document.annotations[1].start[0],
            70.
        );
        frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(t.screen_point([100., 220.])),
                button([100., 220.], true),
            ],
        );
        frame(
            &mut e,
            vec![egui::Event::PointerMoved(t.screen_point([230., 260.]))],
        );
        assert_eq!(e.session.document.annotations.len(), 2);
        frame(&mut e, vec![button([230., 260.], false)]);
        assert_eq!(e.session.document.annotations[2].end, [230., 260.]);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), 2);
        frame(
            &mut e,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(e.tool.is_none());
        assert!(!e.stamps.active);
    }
}
