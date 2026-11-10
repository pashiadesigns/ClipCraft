use crate::tips::Tip as _;
use crate::{Command, Editor, egui};
use capture_core::Placement;

pub struct ResizeCanvas {
    original: [u32; 2],
    width: String,
    height: String,
    relative: bool,
    focus_pending: bool,
    placement: Placement,
    checked: Option<([u32; 2], Placement, Result<(), String>)>,
}
impl ResizeCanvas {
    fn new(original: [u32; 2], relative: bool) -> Self {
        Self {
            original,
            width: if relative {
                "0".into()
            } else {
                original[0].to_string()
            },
            height: if relative {
                "0".into()
            } else {
                original[1].to_string()
            },
            relative,

            focus_pending: true,
            placement: Placement::Center,
            checked: None,
        }
    }
    fn dimensions(&self) -> Result<[u32; 2], String> {
        let mut size = [0; 2];
        for (i, text) in [&self.width, &self.height].into_iter().enumerate() {
            let value = text
                .trim()
                .parse::<i64>()
                .map_err(|_| "Enter whole pixel dimensions.")?;
            let pixels = if self.relative {
                value
                    .checked_add(self.original[i] as i64)
                    .ok_or("Dimensions are too large.")?
            } else {
                value
            };
            size[i] = u32::try_from(pixels)
                .map_err(|_| "Resulting dimensions must be positive whole pixels.")?;
        }
        capture_core::validate_dimensions(size[0], size[1]).map_err(|e| e.to_string())?;
        Ok(size)
    }
    fn specification(&mut self, document: &capture_core::Document) -> Result<[u32; 2], String> {
        let size = self.dimensions()?;
        if self
            .checked
            .as_ref()
            .is_none_or(|(old, placement, _)| *old != size || *placement != self.placement)
        {
            self.checked = Some((
                size,
                self.placement,
                document
                    .canvas_annotations(size[0], size[1], self.placement)
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
            ));
        }
        self.checked.as_ref().unwrap().2.clone()?;
        Ok(size)
    }
}
impl Editor {
    pub fn begin_resize_canvas(&mut self) {
        self.cancel_gesture();
        self.resize_canvas = Some(ResizeCanvas::new(
            [self.session.document.width, self.session.document.height],
            self.shell.canvas_relative,
        ));
    }
    pub fn show_resize_canvas(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.resize_canvas else {
            return;
        };
        let mut apply = None;
        let mut cancel = false;
        let modal =
            crate::controls::Modal::new(egui::Id::unique("resize_canvas")).show(ctx, |ui| {
                ui.set_width(320.0);
                crate::controls::dialog_title(ui, "Resize Canvas");
                egui::Grid::new("canvas_resize_dimensions")
                    .min_col_width(64.0)
                    .spacing([8.0, 8.0])
                    .show(ui, |ui| {
                        for (label, value) in
                            [("Width", &mut draft.width), ("Height", &mut draft.height)]
                        {
                            let label = ui.label(label);
                            let input = ui
                                .add(
                                    crate::unicode_input::TextEdit::singleline(value)
                                        .desired_width(75.0)
                                        .min_size(egui::vec2(75.0, 20.0))
                                        .align(egui::Align2::RIGHT_CENTER),
                                )
                                .labelled_by(label.id);
                            crate::controls::focus_once(&input, &mut draft.focus_pending);
                            ui.label("px");
                            ui.end_row();
                        }
                    });
                if crate::controls::toggle(ui, &mut draft.relative, "Use relative dimensions")
                    .changed()
                {
                    draft.width = if draft.relative {
                        "0".into()
                    } else {
                        draft.original[0].to_string()
                    };
                    draft.height = if draft.relative {
                        "0".into()
                    } else {
                        draft.original[1].to_string()
                    };
                }
                crate::controls::dialog_section(
                    ui,
                    "Placement",
                    Some("Where the image sits on the resized canvas. Added space is transparent."),
                );
                egui::Grid::new("canvas_placement")
                    .min_col_width(0.0)
                    .spacing([2.0, 2.0])
                    .show(ui, |ui| {
                        for (index, placement) in Placement::ALL.into_iter().enumerate() {
                            let button = ui
                                .add_sized(
                                    [23.0, 23.0],
                                    egui::Button::selectable(draft.placement == placement, " "),
                                )
                                .tip(placement.label());
                            button.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::RadioButton,
                                    true,
                                    draft.placement == placement,
                                    placement.label(),
                                )
                            });
                            ui.painter().rect_stroke(
                                button.rect,
                                0.0,
                                egui::Stroke::new(1.0, egui::Color32::from_gray(105)),
                                egui::StrokeKind::Inside,
                            );
                            if button.clicked() {
                                draft.placement = placement;
                            }
                            if index % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(4.0);
                let spec = draft.specification(&self.session.document);
                match &spec {
                    Ok([w, h]) => {
                        ui.weak(format!("Result: {w} × {h} px"));
                    }
                    Err(error) => {
                        ui.colored_label(egui::Color32::from_rgb(255, 170, 160), error);
                    }
                }
                let (submit, closed) = crate::controls::dialog_actions(
                    ui,
                    "Apply",
                    "Cancel",
                    spec.as_ref().is_ok_and(|size| *size != draft.original),
                    false,
                );
                if submit {
                    apply = spec.ok();
                }
                cancel |= closed;
            });
        self.shell.canvas_relative = draft.relative;
        let placement = draft.placement;
        if cancel || modal.should_close() {
            self.resize_canvas = None;
        } else if let Some([width, height]) = apply {
            match self.session.execute(Command::ResizeCanvas {
                width,
                height,
                placement,
            }) {
                Ok(()) => {
                    self.resize_canvas = None;
                    self.view = crate::canvas_view::CanvasView::default();
                    self.needs_render = true;
                    self.status = format!(
                        "Canvas resized to {width} × {height} px. Original pixels are retained for Revert."
                    );
                }
                Err(error) => self.status = error.to_string(),
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relative_dimensions_accept_signed_deltas_and_reject_invalid_or_overflowing_results() {
        let mut draft = ResizeCanvas::new([640, 400], true);
        assert_eq!(draft.dimensions().unwrap(), [640, 400]);
        draft.width = "+100".into();
        draft.height = "-50".into();
        assert_eq!(draft.dimensions().unwrap(), [740, 350]);
        for value in ["-640", "-999", "1.5", "NaN", "9223372036854775807", "32768"] {
            draft.width = value.into();
            assert!(draft.dimensions().is_err());
        }
        let mut absolute = ResizeCanvas::new([640, 400], false);
        absolute.width = "0".into();
        assert!(absolute.dimensions().is_err());
    }
}
