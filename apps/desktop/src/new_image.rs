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
    transparent: bool,
    focus_pending: bool,
}
impl Default for NewImage {
    fn default() -> Self {
        Self {
            width: "1920".into(),
            height: "1080".into(),
            color: [255; 4],
            transparent: false,
            focus_pending: true,
        }
    }
}
impl NewImage {
    /// A new draft with the background last used for a blank image.
    pub(super) fn remembered(blank: capture_core::quick_preferences::BlankDefaults) -> Self {
        Self {
            color: blank.image_color,
            transparent: blank.transparent,
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
            color: if self.transparent { [0; 4] } else { self.color },
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
        let modal = crate::controls::Modal::new(egui::Id::unique("new_image")).show(ctx, |ui| {
            ui.set_width(346.0);
            ui.set_max_width(346.0);
            crate::controls::dialog_title(ui, "New Image");
            crate::controls::dialog_section(ui, "Size", None);
            egui::Grid::new("canvas_dimensions")
                .min_col_width(64.0)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    let width_label = ui.label("Width");
                    let input = ui
                        .add(
                            crate::unicode_input::TextEdit::singleline(&mut draft.width)
                                .desired_width(72.0)
                                .min_size(egui::vec2(72.0, 20.0))
                                .align(egui::Align2::RIGHT_CENTER),
                        )
                        .labelled_by(width_label.id);
                    crate::controls::focus_once(&input, &mut draft.focus_pending);
                    ui.label("px");
                    ui.end_row();
                    let height_label = ui.label("Height");
                    ui.add(
                        crate::unicode_input::TextEdit::singleline(&mut draft.height)
                            .desired_width(72.0)
                            .min_size(egui::vec2(72.0, 20.0))
                            .align(egui::Align2::RIGHT_CENTER),
                    )
                    .labelled_by(height_label.id);
                    ui.label("px");
                    ui.end_row();
                });
            let current = [
                draft.width.trim().parse().unwrap_or(0),
                draft.height.trim().parse().unwrap_or(0),
            ];
            if let Some([w, h]) =
                crate::blank_dialog::presets(ui, &crate::blank_dialog::IMAGE_PRESETS, current)
            {
                draft.width = w.to_string();
                draft.height = h.to_string();
            }
            crate::controls::dialog_section(ui, "Background", None);
            crate::controls::toggle(ui, &mut draft.transparent, "Transparent");
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!draft.transparent, |ui| {
                    ui.add_sized([64.0, 20.0], egui::Label::new("Canvas"));
                    crate::color_widgets::canvas_rgba(ui, &mut draft.color);
                });
                if draft.transparent {
                    ui.weak("Off while transparent");
                }
            });
            let spec = draft.specification();
            if let Err(error) = &spec {
                ui.colored_label(egui::Color32::from_rgb(255, 170, 160), error);
            }
            crate::blank_dialog::preview(
                ui,
                spec.as_ref().ok().map(|s| [s.width, s.height]),
                (!draft.transparent).then_some(draft.color),
            );
            ui.add_space(4.0);
            let (submit, closed) =
                crate::controls::dialog_actions(ui, "Create", "Cancel", spec.is_ok(), false);
            if submit {
                create = spec.ok();
            }
            cancel |= closed;
        });
        // A nested color popup owns its dismissal, including ui.close().
        cancel |= crate::text_edit::cancel_on_escape(&modal);
        if cancel {
            self.new_image = None;
        } else if let Some(spec) = create {
            let mut blank = self.quick_controls.blank_defaults();
            if let Some(d) = &self.new_image {
                blank.transparent = d.transparent;
                blank.image_color = d.color;
            }
            self.quick_controls.remember_blank(blank);
            self.new_image = None;
            self.request_action(FileAction::CreateCanvas(spec), ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_transparent_switch_clears_the_canvas_and_keeps_the_color() {
        let mut draft = NewImage {
            color: [10, 20, 30, 255],
            ..Default::default()
        };
        assert_eq!(draft.specification().unwrap().color, [10, 20, 30, 255]);
        draft.transparent = true;
        assert_eq!(draft.specification().unwrap().color, [0; 4]);
        draft.transparent = false;
        assert_eq!(draft.specification().unwrap().color, [10, 20, 30, 255]);
    }
    #[test]
    fn a_new_draft_remembers_the_last_background() {
        let draft = NewImage::remembered(capture_core::quick_preferences::BlankDefaults {
            transparent: true,
            image_color: [1, 2, 3, 255],
            video_color: [255; 4],
        });
        assert!(draft.transparent);
        assert_eq!(draft.color, [1, 2, 3, 255]);
    }
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
