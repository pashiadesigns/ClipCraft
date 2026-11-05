//! Scope mouse gestures to the hovered surface before its normal scrolling runs.
use crate::{Editor, egui};
/// A captured drag keeps its cursor until release, including outside the original handle.
pub(crate) fn latch_cursor(ctx: &egui::Context) {
    let key = egui::Id::unique("captured-drag-cursor");
    let (pressed, down, focused) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.focused,
        )
    });
    if !down || !focused {
        ctx.data_mut(|d| d.remove::<egui::CursorIcon>(key));
        return;
    }
    if pressed {
        let icon = ctx.output(|o| o.cursor_icon);
        ctx.data_mut(|d| d.insert_temp(key, icon));
    }
    if let Some(icon) = ctx.data(|d| d.get_temp::<egui::CursorIcon>(key)) {
        ctx.set_cursor_icon(icon);
    }
}
pub(crate) use capture_core::editor_keybinds::gestures::Gesture;
fn mask(m: egui::Modifiers) -> u8 {
    u8::from(m.alt) | (u8::from(m.ctrl) * 2) | (u8::from(m.shift) * 4) | (u8::from(m.mac_cmd) * 8)
}
impl Editor {
    pub(crate) fn gesture_matches(&self, g: Gesture, m: egui::Modifiers) -> bool {
        self.quick_controls.keybinds().editor.gestures.get(g) == Some(mask(m))
    }
    pub(crate) fn gesture_wheel(&self, ui: &egui::Ui, g: Gesture) -> f32 {
        let selected = self.quick_controls.keybinds().editor.gestures.get(g);
        ui.input_mut(|i| {
            let mut total = 0.;
            i.events.retain(|e| {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = e
                    && selected == Some(mask(*modifiers))
                {
                    total += delta.y
                        * match unit {
                            egui::MouseWheelUnit::Point => 1.,
                            egui::MouseWheelUnit::Line => 40.,
                            egui::MouseWheelUnit::Page => 240.,
                        };
                    false
                } else {
                    true
                }
            });
            if total != 0. {
                i.smooth_scroll_delta = egui::Vec2::ZERO;
            }
            total
        })
    }
}
