use crate::{Command, Editor, egui};
use capture_core::ImageTransform;

fn label(transform: ImageTransform) -> &'static str {
    match transform {
        ImageTransform::Clockwise => "Rotate image clockwise",
        ImageTransform::CounterClockwise => "Rotate image counterclockwise",
        ImageTransform::HalfTurn => "Rotate image 180 degrees",
        ImageTransform::FlipHorizontal => "Flip image horizontally",
        ImageTransform::FlipVertical => "Flip image vertically",
    }
}

impl Editor {
    pub(super) fn begin_image_transform(&mut self, transform: ImageTransform) {
        self.cancel_gesture();
        if self.session.document.annotations.is_empty() {
            self.finish_image_transform(transform);
        } else {
            self.image_transform = Some(transform);
        }
    }

    fn finish_image_transform(&mut self, transform: ImageTransform) {
        match self.session.execute(Command::TransformImage { transform }) {
            Ok(()) => {
                self.image_transform = None;
                self.select_only(None);
                self.view = crate::canvas_view::CanvasView::default();
                self.needs_render = true;
                self.status = format!(
                    "{}. Undo restores the previous image and objects.",
                    label(transform)
                );
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    pub(super) fn show_image_transform(&mut self, ctx: &egui::Context) {
        let Some(transform) = self.image_transform else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("image_transform")).show(ctx, |ui| {
            ui.set_width(370.0);
            ui.heading(label(transform));
            ui.separator();
            ui.label("This transforms the complete image, including every object.");
            ui.label("Objects will be flattened. Undo restores their editability.");
            ui.horizontal(|ui| {
                confirm = ui.button("Flatten and transform").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        cancel |= modal.is_top_modal
            && !modal.any_popup_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel {
            self.image_transform = None;
        } else if confirm {
            self.finish_image_transform(transform);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_transform_confirmation_escape_preserves_objects_and_history() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let doc: crate::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        editor.session = crate::Session::new(doc.clone()).unwrap();
        editor.begin_image_transform(ImageTransform::Clockwise);
        assert!(editor.image_transform.is_some());
        assert_eq!(editor.session.document.annotations, doc.annotations);
        for events in [
            vec![],
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |_| editor.show_image_transform(&ctx),
            );
            output.textures_delta.clear();
        }
        assert!(editor.image_transform.is_none());
        assert_eq!(editor.session.document.annotations, doc.annotations);
        assert!(!editor.session.can_undo());
        editor.begin_image_transform(ImageTransform::Clockwise);
        editor.finish_image_transform(ImageTransform::Clockwise);
        assert_eq!(
            [
                editor.session.document.width,
                editor.session.document.height
            ],
            [400, 640]
        );
        assert!(editor.session.document.annotations.is_empty());
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, doc.annotations);
        assert!(!editor.session.can_undo());
    }
}
