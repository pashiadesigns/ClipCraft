use crate::{Command, Editor, egui, shortcuts};
impl Editor {
    pub fn flatten_all(&mut self) {
        let count = self.session.document.annotations.len();
        if count == 0 {
            return;
        }
        self.execute(Command::FlattenAll);
        if self.session.document.annotations.is_empty() {
            self.status = format!(
                "Flattened {count} objects into the image. Undo restores their editability."
            );
        }
    }
    pub fn flatten_shortcut(&mut self, ctx: &egui::Context) {
        let available = self.pending.is_none()
            && self.new_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.crop.is_none()
            && self.text_draft.is_none()
            && self.group_drag.is_none()
            && self.edit_drag.is_none()
            && self.property_preview.is_none()
            && self.drag_start.is_none()
            && !self.capture_requested
            && self.capture_result.is_none()
            && !self.shell.help
            && !self.view.panning
            && self.shell.canvas_enabled()
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx);
        if available
            && ctx.input_mut(|i| {
                shortcuts::consume_exact(
                    i,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::T,
                )
            })
        {
            self.flatten_all();
        }
    }
}
