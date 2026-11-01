use crate::{Command, Editor, egui};

#[derive(Clone, Copy, PartialEq)]
enum Units {
    Pixels,
    Percent,
}
impl Units {
    fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Pixels",
            Self::Percent => "Percent",
        }
    }
}

pub struct ResizeImage {
    original: [u32; 2],
    width: String,
    height: String,
    units: Units,
    locked: bool,
    focus_pending: bool,
    advanced: bool,
    confirmation: Option<[u32; 2]>,
}
impl ResizeImage {
    fn new(width: u32, height: u32) -> Self {
        Self {
            original: [width, height],
            width: width.to_string(),
            height: height.to_string(),
            units: Units::Pixels,
            locked: true,
            focus_pending: true,
            advanced: false,
            confirmation: None,
        }
    }
    fn dimensions(&self) -> Result<[u32; 2], String> {
        let mut dimensions = [0; 2];
        for (i, value) in [&self.width, &self.height].into_iter().enumerate() {
            dimensions[i] = match self.units {
                Units::Pixels => value
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "Dimensions must be positive whole pixels.")?,
                Units::Percent => {
                    let value = value
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| "Enter a positive percentage.")?;
                    if !value.is_finite() || value <= 0.0 {
                        return Err("Enter a positive, finite percentage.".into());
                    }
                    let pixels = (value * self.original[i] as f64 / 100.0).round();
                    if pixels > 32768.0 {
                        return Err("Each side must be at most 32,768 pixels.".into());
                    }
                    pixels as u32
                }
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
        let value = |i: usize| match units {
            Units::Pixels => dimensions[i].to_string(),
            Units::Percent => format!(
                "{:.6}",
                dimensions[i] as f64 / self.original[i] as f64 * 100.0
            )
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string(),
        };
        self.width = value(0);
        self.height = value(1);
    }
}

impl Editor {
    pub fn begin_resize(&mut self) {
        self.cancel_gesture();
        let mut draft = ResizeImage::new(self.session.document.width, self.session.document.height);
        draft.advanced = self.shell.resize_advanced;
        self.resize_image = Some(draft);
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
        let modal=egui::Modal::new(egui::Id::unique("resize_image"))
            .area(area)
            .backdrop_color(egui::Color32::TRANSPARENT)
            .show(ctx,|ui| {
            ui.set_width(196.0);
            ui.set_max_width(196.0);
            ui.style_mut().override_font_id = Some(egui::FontId::proportional(12.0));
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 7.0);
            let old_dimensions=draft.dimensions().ok();
            let mut units=draft.units;
            ui.add_enabled_ui(old_dimensions.is_some(),|ui| {
                ui.horizontal(|ui| {
                    let label=ui.label("Units:");
                    egui::ComboBox::from_id_salt("resize_units").width(145.0).selected_text(units.label()).show_ui(ui,|ui| {
                        ui.selectable_value(&mut units,Units::Pixels,"Pixels");
                        for name in ["Inches", "Centimeters"] {
                            ui.horizontal(|ui| {
                                ui.add_enabled(false, egui::Button::selectable(false,name)).on_disabled_hover_text("Print units and DPI are planned.");
                                crate::shell::badge(ui,self.shell.badges,"EDIT-028");
                            });
                        }
                        ui.selectable_value(&mut units,Units::Percent,"Percent");
                    }).response.labelled_by(label.id);
                });
            });
            if units!=draft.units && let Some(dimensions)=old_dimensions {draft.set_units(units,dimensions);}
            ui.horizontal(|ui| {
                if lock_toggle(ui,&mut draft.locked).changed() && draft.locked {draft.link(true);}
                egui::Grid::new("resize_dimensions").show(ui,|ui| {
                    let suffix=if draft.units==Units::Pixels {"px"} else {"%"};
                    let label=ui.label("Width:");
                    let input=ui.add(egui::TextEdit::singleline(&mut draft.width).desired_width(86.0).min_size(egui::vec2(86.0,20.0)).align(egui::Align2::RIGHT_CENTER)).labelled_by(label.id);crate::controls::focus_once(&input,&mut draft.focus_pending);if input.changed(){draft.link(true);}
                    ui.label(suffix);
                    ui.end_row();
                    let label=ui.label("Height:");
                    if ui.add(egui::TextEdit::singleline(&mut draft.height).desired_width(86.0).min_size(egui::vec2(86.0,20.0)).align(egui::Align2::RIGHT_CENTER)).labelled_by(label.id).changed() {draft.link(false);}
                    ui.label(suffix);
                    ui.end_row();
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                let advanced=ui.add_sized([91.0,22.0],egui::Button::new("Advanced").frame(false));
                if advanced.clicked() {draft.advanced = !draft.advanced;}
                let center=advanced.rect.right_center()-egui::vec2(7.0,0.0);
                let direction=if draft.advanced {-1.0} else {1.0};
                ui.painter().add(egui::Shape::convex_polygon(vec![center+egui::vec2(-3.0,-2.0*direction),center+egui::vec2(3.0,-2.0*direction),center+egui::vec2(0.0,2.0*direction)],ui.visuals().text_color(),egui::Stroke::NONE));
                crate::shell::badge(ui,self.shell.badges,"EDIT-028");
            });
            if draft.advanced {
                ui.add_enabled_ui(false,|ui| {
                    ui.horizontal(|ui| {
                        ui.label("Resolution:");
                        let mut planned_resolution="96".to_owned();
                        ui.add(egui::TextEdit::singleline(&mut planned_resolution).desired_width(65.0));
                        ui.label("dpi");
                    });
                    crate::controls::toggle(ui, &mut true, "Use resampling");
                }).response.on_disabled_hover_text("Resolution metadata and the resampling toggle are planned. Current resizing always resamples pixels; editable objects require flattening confirmation.");
            }
            let spec=draft.dimensions();
            if let Err(error)=&spec {
                ui.colored_label(egui::Color32::from_rgb(255,170,160),error);
            }
            let (submit,closed)=crate::controls::dialog_actions(ui,"Apply","Cancel",spec.as_ref().is_ok_and(|size|*size!=draft.original),false);
            if submit {apply=spec.ok();}
            cancel |= closed;
            if self.shell.badges { ui.horizontal(|ui| {ui.small("Image size");crate::shell::badge(ui,true,"EDIT-027");}); }
        });
        // Closing a nested units popup must not cancel the enclosing draft.
        cancel |= modal.is_top_modal
            && !modal.any_popup_open
            && (modal.backdrop_response.clicked()
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)));
        self.shell.resize_advanced = draft.advanced;
        if cancel {
            self.resize_image = None;
        } else if let Some([width, height]) = apply {
            self.request_resize([width, height]);
        }
    }
    pub(crate) fn request_resize(&mut self, size: [u32; 2]) {
        if !self.session.document.annotations.is_empty() {
            if let Some(draft) = &mut self.resize_image {
                draft.confirmation = Some(size);
            }
        } else {
            self.finish_resize(size);
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
        let modal =
            egui::Modal::new(egui::Id::unique("resize_flatten_confirmation")).show(ctx, |ui| {
                ui.set_width(406.0);
                ui.vertical_centered(|ui| {
                    ui.heading("Flatten Objects");
                });
                let close = egui::Rect::from_min_size(
                    ui.min_rect().right_top() - egui::vec2(22.0, 0.0),
                    egui::vec2(22.0, 22.0),
                );
                cancel |= ui
                    .put(close, egui::Button::new("×").frame(false))
                    .on_hover_text("Cancel resize")
                    .clicked();
                ui.separator();
                ui.add_space(8.0);
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
                        ui.strong("Flatten all objects to resize this image.");
                        ui.label("Flattened objects can no longer be moved or edited.");
                        ui.small(format!(
                            "{} × {} px · Undo restores the previous image and objects.",
                            size[0], size[1]
                        ));
                    });
                });
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    crate::shell::badge(ui, self.shell.badges, "EDIT-029");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        cancel |= ui
                            .add_sized([85.0, 24.0], egui::Button::new("Cancel"))
                            .clicked();
                        confirm |= ui
                            .add_sized(
                                [102.0, 24.0],
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
    response.on_hover_text("Lock aspect ratio")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_link_roundtrip_units_and_reject_invalid_values() {
        let mut draft = ResizeImage::new(640, 400);
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
        let mut large = ResizeImage::new(32768, 8);
        large.set_units(Units::Percent, [32001, 7]);
        assert_eq!(large.dimensions().unwrap(), [32001, 7]);
    }
}
