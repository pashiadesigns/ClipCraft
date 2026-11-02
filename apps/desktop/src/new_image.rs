use crate::{Editor, FileAction, egui};

#[derive(Clone, Copy)]
pub struct CanvasSpec {
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
}
pub struct NewImage {
    width: String,
    height: String,
    color: [u8; 4],
    focus_pending: bool,
}
impl Default for NewImage {
    fn default() -> Self {
        Self {
            width: "300".into(),
            height: "200".into(),
            color: [255; 4],
            focus_pending: true,
        }
    }
}
impl NewImage {
    pub(super) fn with_background(color: [u8; 4]) -> Self {
        Self {
            color,
            ..Default::default()
        }
    }
    fn specification(&self) -> Result<CanvasSpec, String> {
        let width = self
            .width
            .trim()
            .parse::<u32>()
            .map_err(|_| "Width must be a positive whole number.")?;
        let height = self
            .height
            .trim()
            .parse::<u32>()
            .map_err(|_| "Height must be a positive whole number.")?;
        capture_core::validate_dimensions(width, height).map_err(|e| e.to_string())?;
        Ok(CanvasSpec {
            width,
            height,
            color: self.color,
        })
    }
}
impl Editor {
    pub fn show_new_image(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.new_image else {
            return;
        };
        let mut create = None;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::unique("new_image")).show(ctx, |ui| {
            ui.set_width(266.0);
            ui.set_max_width(266.0);
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(12.0));
            ui.vertical_centered(|ui| { ui.label(egui::RichText::new("New Image").strong().size(14.0)); });
            let close_rect = egui::Rect::from_min_size(ui.min_rect().right_top()-egui::vec2(22.0,0.0),egui::vec2(22.0,22.0));
                    let close = ui.put(close_rect,egui::Button::new("×").frame(false)).on_hover_text("Close New Image");
                    close.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, "Close New Image")
                    });
                    cancel |= close.clicked();
            ui.separator();
            ui.add_space(12.0);
            ui.horizontal(|ui| {
            ui.add_space(72.0);
            egui::Grid::new("canvas_dimensions")
                .min_col_width(0.0)
                .spacing([6.0, 8.0])
                .show(ui, |ui| {
                    let width_label = ui.label("Width:");
                    let input = ui.add(egui::TextEdit::singleline(&mut draft.width).desired_width(48.0).min_size(egui::vec2(48.0,20.0)).align(egui::Align2::RIGHT_CENTER))
                        .labelled_by(width_label.id);
                    crate::controls::focus_once(&input, &mut draft.focus_pending);
                    ui.label("px");
                    ui.end_row();
                    let height_label = ui.label("Height:");
                    ui.add(egui::TextEdit::singleline(&mut draft.height).desired_width(48.0).min_size(egui::vec2(48.0,20.0)).align(egui::Align2::RIGHT_CENTER))
                        .labelled_by(height_label.id);
                    ui.label("px");
                    ui.end_row();
                    ui.label("Canvas:");
                    crate::color_widgets::canvas_rgba(ui, &mut draft.color);
                    ui.label("");
                    ui.end_row();
                });
            });
            let spec = draft.specification();
            if let Err(error) = &spec {
                ui.colored_label(egui::Color32::from_rgb(255, 170, 160), error);
            }
            ui.add_space(12.0);
            ui.separator();
            ui.horizontal(|ui| {
                crate::shell::badge(ui,self.shell.badges,"FILE-010");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                if ui.add_sized([78.0,crate::controls::DIALOG_ACTION_HEIGHT],egui::Button::new("Cancel")).clicked() {cancel=true;}
                if ui
                    .add_enabled(spec.is_ok(), egui::Button::new("Create").min_size(egui::vec2(78.0,crate::controls::DIALOG_ACTION_HEIGHT)).fill(egui::Color32::from_rgb(0,118,181)))
                    .on_hover_text("Create an unsaved image. Up to 25 megapixels; each side at most 32,768 px.")
                    .clicked()
                {
                    create = spec.ok();
                }
                });
            });
        });
        // A nested color popup owns its dismissal, including ui.close().
        cancel |= crate::text_edit::cancel_on_escape(&modal);
        if cancel {
            self.new_image = None;
        } else if let Some(spec) = create {
            self.new_image = None;
            self.request_action(FileAction::CreateCanvas(spec), ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_reject_invalid_and_excessive_allocations() {
        let mut draft = NewImage::default();
        for (w, h) in [
            ("0", "20"),
            ("-1", "20"),
            ("abc", "20"),
            ("4294967296", "20"),
            ("32769", "1"),
            ("5001", "5000"),
        ] {
            draft.width = w.into();
            draft.height = h.into();
            assert!(draft.specification().is_err());
        }
        draft.width = " 512 ".into();
        draft.height = "256".into();
        draft.color = [0; 4];
        let spec = draft.specification().unwrap();
        assert_eq!((spec.width, spec.height, spec.color), (512, 256, [0; 4]));
    }
}
