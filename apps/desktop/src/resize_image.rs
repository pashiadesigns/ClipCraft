//! Resize Image (approved beta 6 tab 4): Pixels, Percent, Inches or
//! Centimeters with a resolution, and a resampling choice. Objects scale with
//! the picture and stay editable; only a resize that can't keep them editable
//! (stretching rotated objects unevenly, strokes past their limit) asks to
//! flatten.
use crate::tips::Tip as _;
use crate::{Command, Editor, egui};
use capture_core::resize::Resampling;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Units {
    Pixels,
    Percent,
    Inches,
    Centimeters,
}
impl Units {
    const ALL: [Self; 4] = [Self::Pixels, Self::Percent, Self::Inches, Self::Centimeters];
    fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Pixels",
            Self::Percent => "Percent",
            Self::Inches => "Inches",
            Self::Centimeters => "Centimeters",
        }
    }
    fn suffix(self) -> &'static str {
        match self {
            Self::Pixels => "px",
            Self::Percent => "%",
            Self::Inches => "in",
            Self::Centimeters => "cm",
        }
    }
    fn physical(self) -> bool {
        matches!(self, Self::Inches | Self::Centimeters)
    }
}

pub struct ResizeImage {
    original: [u32; 2],
    width: String,
    height: String,
    /// Pixels per inch for Inches and Centimeters.
    ppi: String,
    units: Units,
    resampling: Resampling,
    locked: bool,
    focus_pending: bool,
    confirmation: Option<[u32; 2]>,
}
fn trimmed(value: f64, places: usize) -> String {
    format!("{value:.places$}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
impl ResizeImage {
    fn new(width: u32, height: u32, resampling: Resampling) -> Self {
        Self {
            original: [width, height],
            width: width.to_string(),
            height: height.to_string(),
            ppi: "96".into(),
            units: Units::Pixels,
            resampling,
            locked: true,
            focus_pending: true,
            confirmation: None,
        }
    }
    fn ppi(&self) -> Result<f64, String> {
        let ppi = self
            .ppi
            .trim()
            .parse::<f64>()
            .map_err(|_| "Enter a resolution in pixels per inch.".to_string())?;
        if !ppi.is_finite() || !(1.0..=9600.0).contains(&ppi) {
            return Err("Resolution must be 1 to 9,600 pixels per inch.".into());
        }
        Ok(ppi)
    }
    /// Pixels per unit for the current units.
    fn scale(&self, i: usize) -> Result<f64, String> {
        Ok(match self.units {
            Units::Pixels => 1.,
            Units::Percent => self.original[i] as f64 / 100.,
            Units::Inches => self.ppi()?,
            Units::Centimeters => self.ppi()? / 2.54,
        })
    }
    fn dimensions(&self) -> Result<[u32; 2], String> {
        let mut dimensions = [0; 2];
        for (i, value) in [&self.width, &self.height].into_iter().enumerate() {
            dimensions[i] = if self.units == Units::Pixels {
                value
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "Dimensions must be positive whole pixels.")?
            } else {
                let value = value.trim().parse::<f64>().map_err(|_| {
                    format!(
                        "Enter a positive number of {}.",
                        self.units.label().to_lowercase()
                    )
                })?;
                if !value.is_finite() || value <= 0.0 {
                    return Err("Enter a positive, finite size.".into());
                }
                let pixels = (value * self.scale(i)?).round();
                if pixels > 32768.0 {
                    return Err("Each side must be at most 32,768 pixels.".into());
                }
                pixels.max(1.) as u32
            };
        }
        capture_core::validate_dimensions(dimensions[0], dimensions[1])
            .map_err(|e| e.to_string())?;
        Ok(dimensions)
    }
    fn link(&mut self, width_changed: bool) {
        if !self.locked {
            return;
        }
        let (value, from, to) = if width_changed {
            (&self.width, 0, 1)
        } else {
            (&self.height, 1, 0)
        };
        let linked = match self.units {
            Units::Percent => Some(value.clone()),
            Units::Pixels => value.trim().parse::<u32>().ok().map(|value| {
                ((value as f64 * self.original[to] as f64 / self.original[from] as f64).round()
                    as u64)
                    .to_string()
            }),
            Units::Inches | Units::Centimeters => value.trim().parse::<f64>().ok().map(|value| {
                trimmed(
                    value * self.original[to] as f64 / self.original[from] as f64,
                    3,
                )
            }),
        };
        if let Some(linked) = linked {
            if width_changed {
                self.height = linked;
            } else {
                self.width = linked;
            }
        }
    }
    fn set_units(&mut self, units: Units, dimensions: [u32; 2]) {
        self.units = units;
        for (i, pixels) in dimensions.into_iter().enumerate() {
            let text = match self.scale(i) {
                Ok(_) if units == Units::Pixels => pixels.to_string(),
                Ok(scale) => trimmed(
                    pixels as f64 / scale,
                    if units == Units::Percent { 6 } else { 3 },
                ),
                Err(_) => continue,
            };
            if i == 0 {
                self.width = text;
            } else {
                self.height = text;
            }
        }
    }
    /// "1275 × 850 px · prints 8.5 × 5.67 in at 150 ppi"
    fn result(&self, size: [u32; 2]) -> String {
        let ppi = self.ppi().unwrap_or(96.);
        let (w, h, unit) = if self.units == Units::Centimeters {
            (
                size[0] as f64 / ppi * 2.54,
                size[1] as f64 / ppi * 2.54,
                "cm",
            )
        } else {
            (size[0] as f64 / ppi, size[1] as f64 / ppi, "in")
        };
        format!(
            "Result: {} × {} px · prints {} × {} {unit} at {} ppi",
            size[0],
            size[1],
            trimmed(w, 2),
            trimmed(h, 2),
            trimmed(ppi, 1)
        )
    }
}

impl Editor {
    pub fn begin_resize(&mut self) {
        self.cancel_gesture();
        let d = &self.session.document;
        self.resize_image = Some(ResizeImage::new(
            d.width,
            d.height,
            d.canvas_behavior.resampling,
        ));
    }
    pub fn show_resize(&mut self, ctx: &egui::Context) {
        if let Some(size) = self
            .resize_image
            .as_ref()
            .and_then(|draft| draft.confirmation)
        {
            self.show_resize_confirmation(ctx, size);
            return;
        }
        let Some(draft) = &mut self.resize_image else {
            return;
        };
        let mut apply = None;
        let mut cancel = false;
        let anchor = self.shell.resize_anchor.unwrap_or_else(|| {
            egui::Rect::from_min_size(
                ctx.content_rect().center_bottom() - egui::vec2(48.0, 42.0),
                egui::vec2(96.0, 22.0),
            )
        });
        let area = egui::Area::new(egui::Id::unique("resize_image"))
            .kind(egui::UiKind::Modal)
            .order(egui::Order::Foreground)
            .sense(egui::Sense::hover())
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(anchor.center_top() - egui::vec2(0.0, 4.0));
        let modal = crate::controls::Modal::new(egui::Id::unique("resize_image"))
            .area(area)
            .backdrop_color(egui::Color32::TRANSPARENT)
            .show(ctx, |ui| {
                ui.set_width(330.0);
                ui.set_max_width(330.0);
                crate::controls::dialog_title(ui, "Resize Image");
                let old_dimensions = draft.dimensions().ok();
                // Units as one row of segments, like the mockup.
                ui.horizontal(|ui| {
                    ui.add_sized([72., 20.], egui::Label::new("Units"));
                    ui.add_enabled_ui(old_dimensions.is_some(), |ui| crate::controls::segments(ui, |ui| {
                        for units in Units::ALL {
                            let on = draft.units == units;
                            if ui
                                .add(egui::Button::selectable(on, units.label()))
                                .tip(format!("{} · Size the image in {}.", units.label(), units.label().to_lowercase()))
                                .clicked()
                                && !on
                                && let Some(d) = old_dimensions
                            {
                                draft.set_units(units, d);
                            }
                        }
                    }));
                });
                ui.horizontal(|ui| {
                    if lock_toggle(ui, &mut draft.locked).changed() && draft.locked {
                        draft.link(true);
                    }
                    egui::Grid::new("resize_dimensions").show(ui, |ui| {
                        let suffix = draft.units.suffix();
                        let label = ui.label("Width");
                        let input = ui
                            .add(
                                crate::unicode_input::TextEdit::singleline(&mut draft.width)
                                    .desired_width(86.0)
                                    .min_size(egui::vec2(86.0, 20.0))
                                    .align(egui::Align2::RIGHT_CENTER),
                            )
                            .labelled_by(label.id);
                        crate::controls::focus_once(&input, &mut draft.focus_pending);
                        if input.changed() {
                            draft.link(true);
                        }
                        ui.label(suffix);
                        ui.end_row();
                        let label = ui.label("Height");
                        if ui
                            .add(
                                crate::unicode_input::TextEdit::singleline(&mut draft.height)
                                    .desired_width(86.0)
                                    .min_size(egui::vec2(86.0, 20.0))
                                    .align(egui::Align2::RIGHT_CENTER),
                            )
                            .labelled_by(label.id)
                            .changed()
                        {
                            draft.link(false);
                        }
                        ui.label(suffix);
                        ui.end_row();
                    });
                });
                if draft.units.physical() {
                    ui.horizontal(|ui| {
                        let label = ui.add_sized([72., 20.], egui::Label::new("Resolution"));
                        // Keep the printed size; the pixel size follows the resolution.
                        let before = draft.dimensions().ok();
                        let field = ui
                            .add(
                                crate::unicode_input::TextEdit::singleline(&mut draft.ppi)
                                    .desired_width(64.0)
                                    .align(egui::Align2::RIGHT_CENTER),
                            )
                            .labelled_by(label.id);
                        let _ = before;
                        field.tip("Resolution · Pixels per inch. 96 is the usual screen resolution; 300 suits print.");
                        ui.label("pixels per inch");
                    });
                }
                let mut resampling = draft.resampling;
                let options: Vec<(Resampling, &str)> =
                    Resampling::ALL.iter().map(|r| (*r, r.label())).collect();
                crate::property_layout::choice(
                    ui,
                    "Resampling",
                    &mut resampling,
                    &options,
                    "Resampling · How new pixels are made. Automatic suits most pictures; Sharp keeps screenshots and text crisp; Nearest pixel keeps hard pixel edges.",
                );
                draft.resampling = resampling;
                let spec = draft.dimensions();
                match &spec {
                    Ok(size) => {
                        ui.label(
                            egui::RichText::new(draft.result(*size))
                                .size(11.5)
                                .color(egui::Color32::from_rgb(142, 154, 157)),
                        );
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::from_rgb(255, 170, 160), error);
                    }
                }
                let changed = spec.as_ref().is_ok_and(|size| {
                    *size != draft.original
                }) || (spec.is_ok() && draft.resampling != self.session.document.canvas_behavior.resampling);
                let (submit, closed) =
                    crate::controls::dialog_actions(ui, "Resize", "Cancel", changed, false);
                if submit {
                    apply = spec.ok();
                }
                cancel |= closed;
            });
        // Closing a nested popup must not cancel the enclosing draft.
        cancel |= modal.is_top_modal
            && !modal.any_popup_open
            && (modal.backdrop_response.clicked()
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)));
        if cancel {
            self.resize_image = None;
        } else if let Some(size) = apply {
            self.request_resize(size);
        }
    }
    pub(crate) fn request_resize(&mut self, [width, height]: [u32; 2]) {
        let resampling = self
            .resize_image
            .as_ref()
            .map_or(Resampling::Automatic, |d| d.resampling);
        // Objects stay editable whenever they can.
        match self.session.execute(Command::ResizeImageResampled {
            width,
            height,
            resampling,
        }) {
            Ok(()) => {
                self.resize_image = None;
                self.view = crate::canvas_view::CanvasView::default();
                self.needs_render = true;
                self.status = format!(
                    "Resized to {width} × {height} px. Objects stay editable; Undo restores the previous size."
                );
            }
            Err(_) if !self.session.document.annotations.is_empty() => {
                if let Some(draft) = &mut self.resize_image {
                    draft.confirmation = Some([width, height]);
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    fn finish_resize(&mut self, [width, height]: [u32; 2]) {
        match self
            .session
            .execute(Command::ResizeImageFlattened { width, height })
        {
            Ok(()) => {
                self.resize_image = None;
                self.select_only(None);
                self.view = crate::canvas_view::CanvasView::default();
                self.needs_render = true;
                self.status = format!(
                    "Resized to {width} × {height} px. Undo restores the previous image and objects."
                );
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    fn show_resize_confirmation(&mut self, ctx: &egui::Context, size: [u32; 2]) {
        let mut confirm = false;
        let mut cancel = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("resize_flatten_confirmation"))
            .show(ctx, |ui| {
                ui.set_width(406.0);
                crate::controls::dialog_title(ui, "Flatten Objects");
                ui.horizontal(|ui| {
                    let (icon, _) =
                        ui.allocate_exact_size(egui::vec2(34.0, 44.0), egui::Sense::hover());
                    let center = icon.center();
                    ui.painter()
                        .circle_filled(center, 15.0, egui::Color32::from_rgb(28, 121, 171));
                    ui.painter().text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        "i",
                        egui::FontId::proportional(22.0),
                        egui::Color32::WHITE,
                    );
                    ui.vertical(|ui| {
                        ui.strong("This size can't keep the objects editable.");
                        ui.label("Stretching rotated objects unevenly, or making strokes too thick, needs the objects flattened into the picture.");
                        ui.small(format!(
                            "{} × {} px · Undo restores the previous image and objects.",
                            size[0], size[1]
                        ));
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        cancel |= ui
                            .add_sized(
                                [85.0, crate::controls::DIALOG_ACTION_HEIGHT],
                                egui::Button::new("Cancel"),
                            )
                            .clicked();
                        confirm |= ui
                            .add_sized(
                                [102.0, crate::controls::DIALOG_ACTION_HEIGHT],
                                egui::Button::new("Flatten All")
                                    .fill(egui::Color32::from_rgb(0, 118, 181)),
                            )
                            .clicked();
                    });
                });
            });
        cancel |= modal.is_top_modal
            && !modal.any_popup_open
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel {
            self.resize_image = None;
        } else if confirm {
            self.finish_resize(size);
        }
    }
}

fn lock_toggle(ui: &mut egui::Ui, locked: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(22.0, 45.0), egui::Sense::click());
    if response.clicked() {
        *locked = !*locked;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::CheckBox,
            ui.is_enabled(),
            *locked,
            "Lock aspect ratio",
        )
    });
    let center = rect.center();
    let painter = ui.painter();
    if *locked {
        painter.rect_filled(
            egui::Rect::from_center_size(center, egui::vec2(18.0, 20.0)),
            3.0,
            egui::Color32::from_rgb(24, 145, 193),
        );
    }
    let stroke = egui::Stroke::new(1.2, ui.visuals().text_color());
    let body = egui::Rect::from_center_size(center + egui::vec2(0.0, 3.0), egui::vec2(8.0, 7.0));
    painter.rect_stroke(body, 1.0, stroke, egui::StrokeKind::Inside);
    let shackle_x = if *locked { 0.0 } else { 3.0 };
    painter.line_segment(
        [
            center + egui::vec2(-3.0 + shackle_x, -1.0),
            center + egui::vec2(-3.0 + shackle_x, -5.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(-3.0 + shackle_x, -5.0),
            center + egui::vec2(3.0 + shackle_x, -5.0),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + egui::vec2(3.0 + shackle_x, -5.0),
            center + egui::vec2(3.0 + shackle_x, -1.0),
        ],
        stroke,
    );
    response.tip("Lock aspect ratio")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_link_roundtrip_units_and_reject_invalid_values() {
        let mut draft = ResizeImage::new(640, 400, Resampling::Automatic);
        draft.width = "320".into();
        draft.link(true);
        assert_eq!(draft.dimensions().unwrap(), [320, 200]);
        draft.set_units(Units::Percent, [320, 200]);
        assert_eq!(draft.width, "50");
        draft.height = "125".into();
        draft.link(false);
        assert_eq!(draft.dimensions().unwrap(), [800, 500]);
        for bad in ["0", "-1", "NaN", "inf", "abc", "1000000"] {
            draft.width = bad.into();
            assert!(draft.dimensions().is_err());
        }
        draft.set_units(Units::Pixels, [640, 400]);
        draft.width = "1.2".into();
        assert!(draft.dimensions().is_err());
        draft.set_units(Units::Pixels, [640, 400]);
        draft.locked = false;
        draft.width = "500".into();
        draft.link(true);
        assert_eq!(draft.dimensions().unwrap(), [500, 400]);
        let mut large = ResizeImage::new(32768, 8, Resampling::Automatic);
        large.set_units(Units::Percent, [32001, 7]);
        assert_eq!(large.dimensions().unwrap(), [32001, 7]);
    }
    #[test]
    fn print_units_convert_through_the_resolution() {
        let mut draft = ResizeImage::new(1275, 850, Resampling::Automatic);
        draft.ppi = "150".into();
        draft.set_units(Units::Inches, [1275, 850]);
        assert_eq!(draft.width, "8.5");
        assert_eq!(draft.height, "5.667");
        draft.width = "4".into();
        draft.link(true);
        assert_eq!(draft.dimensions().unwrap(), [600, 400]);
        assert_eq!(
            draft.result([600, 400]),
            "Result: 600 × 400 px · prints 4 × 2.67 in at 150 ppi"
        );
        draft.set_units(Units::Centimeters, [600, 400]);
        assert_eq!(draft.width, "10.16");
        draft.ppi = "0".into();
        assert!(draft.dimensions().is_err());
    }
}
