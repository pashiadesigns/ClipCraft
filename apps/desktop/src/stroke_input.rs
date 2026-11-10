//! Wheel adjustments use raw events once; smoothed scroll tails must not pan.
use crate::{Editor, Kind, egui};

#[derive(Clone, Default)]
struct Wheel {
    consumed: bool,
    remainder: f32,
}

impl Editor {
    pub(super) fn stroke_wheel(&mut self, ui: &egui::Ui, hovered: bool, enabled: bool) -> bool {
        let id = egui::Id::unique("stroke-width-wheel");
        let mut state = ui
            .ctx()
            .data_mut(|d| d.get_temp::<Wheel>(id).unwrap_or_default());
        // Live placement of an icon or step sizes the same way (BJ10).
        let placing_icon = self.icon_library.active && self.icon_library.is_armed();
        let placing_step = !self.icon_library.active && self.tool == Some(Kind::Step);
        let active = enabled
            && hovered
            && (placing_icon
                || placing_step
                || (!self.icon_library.active
                    && matches!(
                        self.tool,
                        Some(
                            Kind::Pen | Kind::Highlighter | Kind::Eraser | Kind::Line | Kind::Arrow
                        )
                    )
                    && !(self.tool == Some(Kind::Highlighter)
                        && self.drawing_rectangle_highlighter())));
        let mut steps = 0;
        ui.input(|input| {
            if !input.is_scrolling() {
                state.consumed = false;
            }
            for event in &input.events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = event
                {
                    state.consumed = active
                        && input.focused
                        && self
                            .gesture_matches(crate::gesture_input::Gesture::StrokeSize, *modifiers);
                    if state.consumed {
                        state.remainder += delta.y
                            * match unit {
                                egui::MouseWheelUnit::Line => 1.,
                                egui::MouseWheelUnit::Point => 1. / 40.,
                                egui::MouseWheelUnit::Page => 8.,
                            };
                        steps += state.remainder.trunc() as i32;
                        state.remainder = state.remainder.fract();
                    } else {
                        state.remainder = 0.;
                    }
                }
            }
        });
        if state.consumed {
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
        if steps != 0 && placing_icon {
            let canvas = [self.session.document.width, self.session.document.height];
            if let Some(size) = self.icon_library.resize_armed(steps, canvas) {
                self.status = format!("Icon size: {size:.0} px");
                ui.ctx().request_repaint();
            }
        } else if steps != 0 && placing_step {
            let next = ((self.step_size as f32 * 1.08f32.powi(steps)).round() as i64
                + i64::from(steps.signum()))
            .clamp(1, 16384) as u32;
            if next != self.step_size {
                self.step_size = next;
                self.status = format!("Step size: {next} px");
                ui.ctx().request_repaint();
            }
        } else if steps != 0 {
            let next = (self.width as i64 + i64::from(steps)).clamp(1, 32) as u32;
            if next != self.width {
                self.width = next;
                self.status = format!("Stroke width: {next} px");
                if self.tool == Some(Kind::Eraser) && self.drag_start.is_some() {
                    self.needs_render = true;
                }
                ui.ctx().request_repaint();
            }
        }
        let consumed = state.consumed;
        ui.ctx().data_mut(|d| d.insert_temp(id, state));
        consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::tests::{editor, frame};
    fn wheel(delta: f32, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            delta: egui::vec2(0., delta),
            modifiers,
        }
    }
    #[test]
    fn alt_wheel_changes_active_strokes_once_without_panning_or_editing_objects() {
        for kind in [
            Kind::Pen,
            Kind::Highlighter,
            Kind::Eraser,
            Kind::Line,
            Kind::Arrow,
        ] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.tool = Some(kind);
            e.width = 8;
            let before = frame(&ctx, &mut e, vec![]);
            let pos = before.image.center();
            frame(&ctx, &mut e, vec![egui::Event::PointerMoved(pos)]);
            for (delta, expected) in [(1., 9), (-1., 8), (100., 32), (-100., 1)] {
                let after = frame(&ctx, &mut e, vec![wheel(delta, egui::Modifiers::ALT)]);
                assert_eq!(e.width, expected, "{kind:?}");
                assert_eq!(after.image, before.image);
                assert_eq!(
                    e.new_annotation(kind, [10., 10.], [40., 40.]).width,
                    expected
                );
                for _ in 0..12 {
                    let tail = frame(&ctx, &mut e, vec![]);
                    assert_eq!(tail.image, before.image, "wheel tail moved view");
                    assert_eq!(e.width, expected, "smoothed event adjusted width again");
                }
            }
            assert!(e.session.document.annotations.is_empty());
            assert!(!e.session.can_undo());
            let zoomed = frame(
                &ctx,
                &mut e,
                vec![wheel(3., egui::Modifiers::CTRL | egui::Modifiers::COMMAND)],
            );
            assert!(zoomed.zoom > before.zoom);
            assert_eq!(e.width, 1);
        }
    }
    #[test]
    fn wheel_ignores_other_tools_and_edits_live_draft_in_one_undo() {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.tool = Some(Kind::Text);
        e.width = 8;
        let view = frame(&ctx, &mut e, vec![]);
        let pos = view.screen_point([100., 100.]);
        frame(
            &ctx,
            &mut e,
            vec![
                egui::Event::PointerMoved(pos),
                wheel(1., egui::Modifiers::ALT),
            ],
        );
        assert_eq!(e.width, 8);
        e.tool = Some(Kind::Pen);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, &mut e, vec![button(pos, true)]);
        let end = pos + egui::vec2(50., 25.);
        frame(
            &ctx,
            &mut e,
            vec![
                egui::Event::PointerMoved(end),
                wheel(2., egui::Modifiers::ALT),
            ],
        );
        assert_eq!(e.creation.annotation.as_ref().unwrap().width, 10);
        assert!(!e.session.can_undo());
        frame(&ctx, &mut e, vec![button(end, false)]);
        assert_eq!(e.session.document.annotations[0].width, 10);
        e.execute(crate::Command::Undo);
        assert!(e.session.document.annotations.is_empty());
    }
}
