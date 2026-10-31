use crate::{Command, Editor, canvas_view::Transform, egui};

pub(super) struct FillTool {
    pub active: bool,
    pub pending: Option<[u32; 2]>,
    color: [u8; 4],
    tolerance: u8,
}

impl Default for FillTool {
    fn default() -> Self {
        Self {
            active: false,
            pending: None,
            color: [239, 85, 55, 255],
            tolerance: 0,
        }
    }
}

impl Editor {
    fn apply_fill(&mut self, [x, y]: [u32; 2]) {
        self.cancel_gesture();
        let result = self.session.execute(Command::FloodFill {
            x,
            y,
            color: self.fill_tool.color,
            tolerance: self.fill_tool.tolerance,
        });
        match result {
            Ok(()) => {
                self.select_only(None);
                self.needs_render = true;
                self.status = "Fill complete. Undo restores the previous image and objects.".into();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn request_fill(&mut self, seed: [u32; 2]) {
        if self.session.document.annotations.is_empty() {
            self.apply_fill(seed);
        } else {
            self.fill_tool.pending = Some(seed);
            self.status = "Fill location selected. Review Flatten and fill in Properties.".into();
        }
    }

    pub(super) fn fill_properties(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Fill Properties",
            "Click a connected region. Tolerance 0 matches exactly; 255 includes every color. Transparent pixels ignore hidden colors. Fill replaces pixels; objects flatten after review. One Undo restores the editable document; original pixels remain in the project. Selection-constrained Fill, Magic Wand, global replace and antialiased fill are planned.",
        );
        crate::shell::badge(ui, self.shell.badges, "EDIT-042");
        ui.horizontal(|ui| {
            crate::property_layout::swatch(ui, "Fill", &mut self.fill_tool.color);
        });
        crate::property_layout::slider(
            ui,
            "Opacity",
            egui::Slider::new(&mut self.fill_tool.color[3], 0..=255),
        );
        crate::property_layout::slider(
            ui,
            "Tolerance",
            egui::Slider::new(&mut self.fill_tool.tolerance, 0..=255),
        );
        if let Some(seed) = self.fill_tool.pending {
            ui.separator();
            ui.label(format!("Fill at {}, {}", seed[0], seed[1]));
            ui.label(format!(
                "This merges {} editable objects into the image.",
                self.session.document.annotations.len()
            ));
            if ui.button("Flatten and fill").clicked() {
                self.apply_fill(seed);
            }
            if ui.button("Cancel fill").clicked() {
                self.fill_tool.pending = None;
                self.status = "Fill cancelled. The image is unchanged.".into();
            }
        }
    }

    pub(super) fn fill_canvas(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        transform: Transform,
        interactive: bool,
    ) {
        if interactive {
            if ui.input(|i| i.key_pressed(egui::Key::Escape) || !i.focused) {
                self.fill_tool.pending = None;
            } else if response.clicked()
                && let Some(pos) = response.interact_pointer_pos()
                && transform.image.contains(pos)
            {
                let p = transform.image_point(pos);
                self.request_fill([
                    (p[0].floor().max(0.0) as u32).min(self.session.document.width - 1),
                    (p[1].floor().max(0.0) as u32).min(self.session.document.height - 1),
                ]);
                ui.ctx().request_repaint();
            }
        }
        if let Some([x, y]) = self.fill_tool.pending {
            ui.painter().circle_stroke(
                transform.screen_point([x as f32, y as f32]),
                6.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(189, 245, 69)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_defers_flattening_until_apply_and_cancel_preserves_objects_and_history() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let doc: crate::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        let original = serde_json::to_vec(&doc).unwrap();
        editor.session = crate::Session::new(doc).unwrap();
        editor.fill_tool.active = true;
        editor.request_fill([0, 0]);
        assert_eq!(editor.fill_tool.pending, Some([0, 0]));
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        editor.cancel_gesture();
        assert!(editor.fill_tool.pending.is_none());
        editor.request_fill([0, 0]);
        editor.apply_fill([0, 0]);
        assert!(editor.session.document.annotations.is_empty());
        assert_eq!(
            editor.session.document.render().unwrap().get_pixel(0, 0).0,
            editor.fill_tool.color
        );
        editor.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        assert!(editor.fill_tool.active);
    }
}
