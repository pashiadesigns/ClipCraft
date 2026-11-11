//! Rotate Image by any angle (approved beta 6 tab 5): a slider and a typed
//! angle, Grow to fit or Keep size, transparent or canvas-colored corners and
//! a live preview. Objects rotate with the picture and stay editable; one
//! Undo step.
use crate::tips::Tip as _;
use crate::{Command, Editor, egui};

const PREVIEW_SIDE: u32 = 200;

pub(crate) struct RotateImage {
    degrees: f32,
    text: String,
    grow: bool,
    canvas_color: bool,
    /// The picture reduced for the preview, rendered once.
    small: Option<image::RgbaImage>,
    shown: Option<(f32, bool, bool, egui::TextureHandle)>,
}
impl RotateImage {
    pub(crate) fn new() -> Self {
        Self {
            degrees: 0.,
            text: "0°".into(),
            grow: true,
            canvas_color: false,
            small: None,
            shown: None,
        }
    }
    fn parse(text: &str) -> Option<f32> {
        let value = text
            .trim()
            .trim_end_matches('°')
            .trim()
            .parse::<f32>()
            .ok()?;
        (value.is_finite() && (-360.0..=360.0).contains(&value)).then_some(value)
    }
}

impl Editor {
    pub(crate) fn begin_rotate_image(&mut self) {
        self.cancel_gesture();
        self.rotate_image = Some(RotateImage::new());
    }
    fn corner_color(&self, canvas_color: bool) -> Option<[u8; 4]> {
        canvas_color
            .then(|| self.session.document.base_image().ok())
            .flatten()
            .map(|b| b.get_pixel(0, 0).0)
    }
    pub(crate) fn show_rotate_image(&mut self, ctx: &egui::Context) {
        if self.rotate_image.is_none() {
            return;
        }
        let corners = {
            let canvas_color = self.rotate_image.as_ref().unwrap().canvas_color;
            self.corner_color(canvas_color)
        };
        let draft = self.rotate_image.as_mut().unwrap();
        if draft.small.is_none() {
            let full = self
                .session
                .document
                .render()
                .unwrap_or_else(|_| image::RgbaImage::new(1, 1));
            let scale = f64::from(PREVIEW_SIDE) / f64::from(full.width().max(full.height()).max(1));
            draft.small = Some(image::imageops::thumbnail(
                &full,
                ((f64::from(full.width()) * scale).round() as u32).max(1),
                ((f64::from(full.height()) * scale).round() as u32).max(1),
            ));
        }
        if draft.shown.as_ref().is_none_or(|(d, g, c, _)| {
            *d != draft.degrees || *g != draft.grow || *c != draft.canvas_color
        }) {
            let rotated = capture_core::image_transform::rotate_preview(
                draft.small.as_ref().unwrap(),
                draft.degrees,
                draft.grow,
                corners.unwrap_or([0; 4]),
            );
            let texture = ctx.load_texture(
                "rotate-image-preview",
                egui::ColorImage::from_rgba_unmultiplied(
                    [rotated.width() as usize, rotated.height() as usize],
                    rotated.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            draft.shown = Some((draft.degrees, draft.grow, draft.canvas_color, texture));
        }
        let mut apply = false;
        let mut cancel = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("rotate_image")).show(ctx, |ui| {
            ui.set_width(400.);
            crate::controls::dialog_title(ui, "Rotate Image");
            ui.horizontal(|ui| {
                ui.add_sized([72., 20.], egui::Label::new("Angle"));
                let before = draft.degrees;
                crate::controls::slider(
                    ui,
                    egui::Slider::new(&mut draft.degrees, -180.0..=180.0)
                        .show_value(false)
                        .step_by(0.5),
                )
                .tip("Angle · Drag to turn the picture; clockwise is positive.");
                if draft.degrees != before {
                    draft.text = format!("{}°", draft.degrees);
                }
                let field = ui.add(
                    crate::unicode_input::TextEdit::singleline(&mut draft.text)
                        .desired_width(64.)
                        .align(egui::Align2::RIGHT_CENTER),
                );
                if field.changed()
                    && let Some(value) = RotateImage::parse(&draft.text)
                {
                    draft.degrees = value;
                }
            });
            ui.horizontal(|ui| {
                ui.add_sized([72., 20.], egui::Label::new("Canvas"));
                crate::controls::segments(ui, |ui| {
                    for (grow, label, tip) in [
                        (
                            true,
                            "Grow to fit",
                            "Grow to fit · The canvas grows so no corner is cut off.",
                        ),
                        (
                            false,
                            "Keep size",
                            "Keep size · The canvas stays the same and the corners are cropped.",
                        ),
                    ] {
                        if ui
                            .add(egui::Button::selectable(draft.grow == grow, label))
                            .tip(tip)
                            .clicked()
                        {
                            draft.grow = grow;
                        }
                    }
                });
            });
            let mut canvas_color = draft.canvas_color;
            crate::property_layout::choice(
                ui,
                "Corners",
                &mut canvas_color,
                &[(false, "Transparent"), (true, "Canvas color")],
                "Corners · What fills the new corners around the turned picture.",
            );
            draft.canvas_color = canvas_color;
            let (area, _) = ui
                .allocate_exact_size(egui::vec2(ui.available_width(), 170.), egui::Sense::hover());
            let p = ui.painter();
            p.rect_filled(area, 6., egui::Color32::from_rgb(21, 26, 28));
            if let Some((_, _, _, t)) = &draft.shown {
                let s = t.size_vec2();
                let room = area.shrink(10.);
                let scale = (room.width() / s.x).min(room.height() / s.y).min(1.);
                let r = egui::Rect::from_center_size(room.center(), s * scale);
                p.rect_stroke(
                    r,
                    0.,
                    egui::Stroke::new(1., egui::Color32::from_rgb(107, 119, 124)),
                    egui::StrokeKind::Outside,
                );
                p.image(
                    t.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
            let valid = RotateImage::parse(&draft.text).is_some();
            ui.label(
                egui::RichText::new(if !valid {
                    "Type an angle from -360 to 360 degrees."
                } else if draft.grow {
                    "Grow to fit: the canvas grows so no corner is cut off."
                } else {
                    "Keep size: the canvas stays the same and the corners are cropped."
                })
                .size(11.5)
                .color(if valid {
                    egui::Color32::from_rgb(142, 154, 157)
                } else {
                    egui::Color32::from_rgb(255, 170, 160)
                }),
            );
            let (ok, closed) = crate::controls::dialog_actions(
                ui,
                "Rotate",
                "Cancel",
                valid && draft.degrees.rem_euclid(360.) != 0.,
                false,
            );
            apply = ok;
            cancel = closed;
        });
        cancel |= crate::text_edit::cancel_on_escape(&modal);
        if cancel {
            self.rotate_image = None;
        } else if apply {
            let (degrees, grow) = {
                let d = self.rotate_image.as_ref().unwrap();
                (d.degrees, d.grow)
            };
            match self.session.execute(Command::RotateImage {
                degrees,
                grow,
                corners,
            }) {
                Ok(()) => {
                    self.rotate_image = None;
                    self.view = crate::canvas_view::CanvasView::default();
                    self.needs_render = true;
                    self.status = format!(
                        "Rotated the image {degrees}°. Objects stay editable; Undo restores it."
                    );
                }
                Err(e) => self.status = e.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn angles_parse_with_or_without_the_degree_sign() {
        assert_eq!(RotateImage::parse("12°"), Some(12.));
        assert_eq!(RotateImage::parse(" -45.5 "), Some(-45.5));
        assert_eq!(RotateImage::parse("361"), None);
        assert_eq!(RotateImage::parse("abc"), None);
    }
    #[test]
    fn rotating_from_the_dialog_keeps_objects_and_is_one_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = crate::Session::new(crate::Document::blank(300, 200).unwrap()).unwrap();
        let a = e.new_annotation(crate::Kind::Rectangle, [40., 40.], [120., 100.]);
        e.execute(Command::Add { annotation: a });
        let before = e.session.document.clone();
        e.begin_rotate_image();
        e.rotate_image.as_mut().unwrap().degrees = 30.;
        e.rotate_image.as_mut().unwrap().text = "30°".into();
        let mut output = ctx.run_ui(Default::default(), |ui| e.show_rotate_image(ui.ctx()));
        output.textures_delta.clear();
        assert!(e.rotate_image.is_some());
        // Rotate as the dialog does on its main button.
        e.session
            .execute(Command::RotateImage {
                degrees: 30.,
                grow: true,
                corners: None,
            })
            .unwrap();
        assert_eq!(e.session.document.annotations.len(), 1);
        assert!((e.session.document.annotations[0].rotation_degrees - 30.).abs() < 0.01);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before.annotations);
    }
}
