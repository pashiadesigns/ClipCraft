use super::*;

// Winit 0.30's Windows OLE DragOver does not forward cursor coordinates.
// Read only the pointer (never screen pixels) while a file drag is active.
#[cfg(windows)]
pub(crate) fn native_drop_pointer(owner: Option<isize>, ppp: f32) -> Option<egui::Pos2> {
    use windows_sys::Win32::{
        Foundation::{HWND, POINT},
        Graphics::Gdi::ScreenToClient,
        UI::WindowsAndMessaging::GetCursorPos,
    };
    let hwnd = owner? as HWND;
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut point) == 0 || ScreenToClient(hwnd, &mut point) == 0 {
            return None;
        }
    }
    Some(egui::pos2(point.x as f32 / ppp, point.y as f32 / ppp))
}

pub(crate) fn content_mode(ctx: &egui::Context, id: u64) -> bool {
    ctx.data(|d| d.get_temp::<u64>(egui::Id::unique("frame-content-mode")) == Some(id))
}

impl Editor {
    pub(crate) fn convert_selected_frame(&mut self, frame: bool) {
        self.commit_property_preview();
        if let Some(a) = self.selected_annotation() {
            match a.converted_frame(frame) {
                Ok(annotation) => self.execute(Command::Update { annotation }),
                Err(e) => self.fail(e),
            }
        }
    }

    pub(crate) fn frame_style_choices(&mut self, ui: &mut egui::Ui) {
        let Some(a) = self.selected_annotation().filter(|a| a.is_image_frame()) else {
            return;
        };
        ui.horizontal(|ui| {
            let width = ((ui.available_width() - 2. * ui.spacing().item_spacing.x) / 3.).min(100.);
            for (label, kind, radius) in [
                ("Square", Kind::Rectangle, 0.0_f32),
                ("Rounded", Kind::Rectangle, 20.),
                ("Circle", Kind::Ellipse, 0.),
            ] {
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, 68.), egui::Sense::click());
                ui.painter().rect_filled(
                    rect,
                    3.,
                    if response.hovered() {
                        egui::Color32::from_rgb(65, 79, 85)
                    } else {
                        egui::Color32::from_rgb(42, 47, 50)
                    },
                );
                let icon = egui::Rect::from_center_size(
                    rect.center() - egui::vec2(0., 8.),
                    egui::vec2(38., 30.),
                );
                let stroke = egui::Stroke::new(
                    2.,
                    if response.hovered() {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().text_color()
                    },
                );
                if kind == Kind::Ellipse {
                    ui.painter().circle_stroke(icon.center(), 15., stroke);
                } else {
                    ui.painter().rect_stroke(
                        icon,
                        if radius > 0. { 8 } else { 0 },
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                ui.painter().text(
                    rect.center_bottom() - egui::vec2(0., 5.),
                    egui::Align2::CENTER_BOTTOM,
                    label,
                    egui::FontId::proportional(12.),
                    ui.visuals().text_color(),
                );
                if a.kind == kind && ((a.corner_radius > 0.) == (radius > 0.)) {
                    ui.painter().rect_stroke(
                        rect,
                        3.,
                        egui::Stroke::new(2., crate::shell::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.tip(format!("{label} image frame")).clicked() {
                    let mut annotation = a.clone();
                    annotation.kind = kind;
                    annotation.points.clear();
                    if label != "Rounded" {
                        let (lo, hi) = annotation.body_bounds();
                        let edge = (hi[0] - lo[0]).min(hi[1] - lo[1]);
                        annotation.start = lo;
                        annotation.end = [lo[0] + edge, lo[1] + edge];
                    }
                    annotation.corner_radius = radius.min(annotation.max_corner_radius());
                    self.execute(Command::Update { annotation });
                }
            }
        });
    }

    pub(crate) fn frame_conversion_button(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let Some(a) = self
            .selected_annotation()
            .filter(|a| a.is_shape() || a.is_image_frame())
        else {
            return;
        };
        let frame = !a.is_image_frame();
        let label = if frame {
            "Convert to Image Frame"
        } else {
            "Convert to Basic Shape"
        };
        let response = ui.put(rect, egui::Button::new(label));
        let left = rect.left() + 8.;
        let icon = egui::Rect::from_center_size(
            egui::pos2(left + 6., rect.center().y),
            egui::vec2(10., 10.),
        );
        let stroke = egui::Stroke::new(1., ui.visuals().text_color());
        ui.painter()
            .rect_stroke(icon, 1, stroke, egui::StrokeKind::Inside);
        ui.painter().arrow(
            egui::pos2(left + 13., rect.center().y),
            egui::vec2(7., 0.),
            stroke,
        );
        let icon = icon.translate(egui::vec2(24., 0.));
        ui.painter()
            .rect_stroke(icon, 1, stroke, egui::StrokeKind::Inside);
        ui.painter()
            .line_segment([icon.left_bottom(), icon.center()], stroke);
        ui.painter()
            .line_segment([icon.center(), icon.right_bottom()], stroke);
        if response.clicked() {
            self.convert_selected_frame(frame);
        }
    }

    /// A file dropped on an image project's canvas, away from any image
    /// frame, becomes its own image object at the drop point (BI8).
    fn canvas_file_drop(
        &mut self,
        ui: &mut egui::Ui,
        transform: crate::canvas_view::Transform,
        p: egui::Pos2,
        files: &[std::sync::Arc<dyn egui::DroppedFile + Send + Sync>],
        key: egui::Id,
    ) {
        if self.is_video() || !transform.image.contains(p) {
            return;
        }
        let hint = egui::Rect::from_center_size(p + egui::vec2(0., -28.), egui::vec2(150., 28.));
        ui.painter()
            .rect_filled(hint, 4, egui::Color32::from_rgb(0, 115, 160));
        ui.painter().text(
            hint.center(),
            egui::Align2::CENTER_CENTER,
            "Drop to add image",
            egui::FontId::proportional(13.),
            egui::Color32::WHITE,
        );
        if files.is_empty() {
            return;
        }
        ui.ctx().data_mut(|d| {
            d.remove::<egui::Pos2>(key);
        });
        const IMAGES: [&str; 10] = [
            "png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "svg", "eps",
        ];
        let center = transform.image_point(p);
        let mut placed = 0;
        for file in files {
            let path = file.path();
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| IMAGES.iter().any(|x| e.eq_ignore_ascii_case(x)))
            {
                let shift = placed as f32 * 24.;
                self.insert_image_file_at(path, Some([center[0] + shift, center[1] + shift]));
                placed += 1;
            }
        }
        if placed == 0 {
            self.status = "Only image files can be dropped on the canvas.".into();
        }
    }

    pub(crate) fn frame_file_drop(
        &mut self,
        ui: &mut egui::Ui,
        transform: crate::canvas_view::Transform,
    ) {
        let (hovering, files, pointer) = ui.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw.dropped_files.clone(),
                i.pointer.hover_pos(),
            )
        });
        let key = egui::Id::unique("frame-drop-position");
        #[cfg(windows)]
        let pointer = if hovering || !files.is_empty() {
            native_drop_pointer(self.hotkey_owner, ui.ctx().pixels_per_point()).or(pointer)
        } else {
            pointer
        };
        if hovering {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        if hovering && let Some(p) = pointer {
            ui.ctx().data_mut(|d| d.insert_temp(key, p));
        }
        if !hovering && files.is_empty() {
            ui.ctx().data_mut(|d| {
                d.remove::<egui::Pos2>(key);
            });
            return;
        }
        let Some(p) = pointer.or_else(|| ui.ctx().data(|d| d.get_temp::<egui::Pos2>(key))) else {
            return;
        };
        if !transform.viewport.contains(p) {
            return;
        }
        let target = self
            .session
            .document
            .annotations
            .iter()
            .rev()
            .find(|a| {
                a.is_image_frame()
                    && self.session.document.object_editable(a.id)
                    && a.hit_test(transform.image_point(p), 0.)
            })
            .cloned();
        let Some(a) = target else {
            self.canvas_file_drop(ui, transform, p, &files, key);
            return;
        };
        let points = a
            .frame_boundary()
            .into_iter()
            .map(|p| transform.screen_point(a.rotation().point(p)))
            .collect();
        ui.painter()
            .with_clip_rect(transform.viewport)
            .add(egui::Shape::closed_line(
                points,
                egui::Stroke::new(3., egui::Color32::from_rgb(0, 180, 235)),
            ));
        let box_rect =
            egui::Rect::from_center_size(p + egui::vec2(0., -28.), egui::vec2(176., 28.));
        ui.painter()
            .rect_filled(box_rect, 4, egui::Color32::from_rgb(0, 115, 160));
        ui.painter().text(
            box_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Drop image to fill frame",
            egui::FontId::proportional(13.),
            egui::Color32::WHITE,
        );
        if !files.is_empty() {
            ui.ctx().data_mut(|d| {
                d.remove::<egui::Pos2>(key);
            });
            let result = (|| -> Result<(), String> {
                if files.len() != 1 {
                    return Err("Drop one image into a frame at a time.".into());
                }
                let path = files[0].path();
                if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
                {
                    // An SVG stays a sharp drawing inside the frame (EDIT-084).
                    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                    let bitmap =
                        capture_core::bitmap::Bitmap::from_svg(&text).map_err(|e| e.to_string())?;
                    self.replace_frame_bitmap(a.id, std::sync::Arc::new(bitmap))?;
                    self.select_only(Some(a.id));
                    return Ok(());
                }
                let document =
                    capture_core::Document::open_image(path).map_err(|e| e.to_string())?;
                let pixels = document.render().map_err(|e| e.to_string())?;
                let payload = capture_core::clipboard_payload::ImagePayload::from_rgba(
                    pixels.width(),
                    pixels.height(),
                    pixels.into_raw(),
                )
                .map_err(|e| e.to_string())?;
                let bitmap =
                    capture_core::bitmap::Bitmap::new(payload.image).map_err(|e| e.to_string())?;
                self.replace_frame_bitmap(a.id, std::sync::Arc::new(bitmap))?;
                self.select_only(Some(a.id));
                Ok(())
            })();
            self.status = result.map_or_else(
                |e| e,
                |_| "Image placed in frame. Undo restores its previous contents.".into(),
            );
        }
    }
}
