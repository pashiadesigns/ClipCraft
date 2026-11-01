use crate::{Annotation, Command, Editor, Kind, egui, property_layout};
use capture_core::image_frame::ImageView;
mod context;
mod shape_frames;
pub(super) use shape_frames::content_mode;

#[cfg(test)]
mod tests;

fn crop_mode_key() -> egui::Id {
    egui::Id::unique("image-crop-mode")
}
pub(super) fn crop_mode(ctx: &egui::Context, id: u64) -> bool {
    ctx.data(|d| d.get_temp::<u64>(crop_mode_key()) == Some(id))
}

pub(super) fn properties(ui: &mut egui::Ui, a: &mut Annotation) {
    let Some(bitmap) = a.image.as_ref().filter(|b| !b.is_cursor()).cloned() else {
        return;
    };
    if let Some(before) = bitmap.vector() {
        let mut vector = before.clone();
        property_layout::section_help(
            ui,
            "Vector stamp",
            "Edit up to five colors independently. Paths stay crisp at any size; color edits apply only to this stamp and support Undo.",
        );
        let columns = if vector.colors.len() == 5 { 3 } else { 4 };
        egui::Grid::new("stamp-palette")
            .num_columns(columns)
            .spacing(egui::vec2(8., 8.))
            .show(ui, |ui| {
                for (i, color) in vector.colors.iter_mut().enumerate() {
                    property_layout::swatch(ui, &format!("Color {}", i + 1), color);
                    if (i + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        if &vector != before
            && let Ok(bitmap) = capture_core::bitmap::Bitmap::from_vector(vector)
        {
            a.image = Some(std::sync::Arc::new(
                if let Some(name) = a.image.as_ref().and_then(|b| b.stamp_name()) {
                    bitmap.named_stamp(name)
                } else {
                    bitmap
                },
            ));
        }
        return;
    }
    property_layout::section_help(
        ui,
        "Image frame",
        "Crop image enables edge and corner cropping; drag inside to reposition the source. The center crop handle pans the image at any time. Ctrl-drag also crops or pans. Original pixels remain intact. Reset crop restores the entire screenshot.",
    );
    if ui.button("Replace image…").clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("replace-frame-image"), a.id));
    }
    let before = bitmap.view();
    let mut view = before;
    ui.horizontal(|ui| {
        if ui.button("−").on_hover_text("Zoom image out").clicked() {
            view = view.zoomed_about(1. / 1.25, bitmap.focus()).unwrap_or(view);
        }
        if ui.button("+").on_hover_text("Zoom image in").clicked() {
            view = view.zoomed_about(1.25, bitmap.focus()).unwrap_or(view);
        }
    });
    ui.horizontal(|ui| {
        let active = crop_mode(ui.ctx(), a.id);
        let response = ui.selectable_label(
            active,
            if active {
                "Done cropping"
            } else {
                "Crop image"
            },
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("crop-image-button"), response.rect));
        if response.clicked() {
            ui.ctx().data_mut(|d| {
                if active {
                    d.remove::<u64>(crop_mode_key());
                } else {
                    d.insert_temp(crop_mode_key(), a.id);
                }
            });
        }
        if ui.button("Reset crop").clicked() {
            view = ImageView::default();
            ui.ctx().data_mut(|d| d.remove::<u64>(crop_mode_key()));
        }
    });
    let mut zoom = 1. / view.rect[2].max(view.rect[3]);
    let old_zoom = zoom;
    if property_layout::slider(
        ui,
        "Image zoom",
        egui::Slider::new(&mut zoom, 1.0..=16.0).suffix("×"),
    )
    .changed()
    {
        view = view
            .zoomed_about(zoom / old_zoom, bitmap.focus())
            .unwrap_or(view);
    }
    for (axis, label) in [(0, "Horizontal"), (1, "Vertical")] {
        let available = (1. - view.rect[axis + 2]).max(0.);
        let mut percent = if available > 0. {
            100. * view.rect[axis] / available
        } else {
            50.
        };
        ui.add_enabled_ui(available > 0., |ui| {
            if property_layout::slider(
                ui,
                label,
                egui::Slider::new(&mut percent, 0.0..=100.0).suffix("%"),
            )
            .changed()
            {
                view.rect[axis] = available * percent / 100.;
            }
        });
    }
    if view != before
        && let Ok(edited) = a.with_image_view(Some(view))
    {
        *a = edited;
    }
    if matches!(a.kind, Kind::Image | Kind::Rectangle) {
        let max = a.max_corner_radius();
        property_layout::slider(
            ui,
            "Corners",
            egui::Slider::new(&mut a.corner_radius, 0.0..=max),
        );
    }
}

impl Editor {
    fn placeholder_buttons(
        &self,
        transform: crate::canvas_view::Transform,
    ) -> Vec<(u64, egui::Rect)> {
        self.session
            .document
            .annotations
            .iter()
            .filter(|a| {
                a.image.as_ref().is_some_and(|b| b.is_placeholder())
                    && !self.session.document.object_state(a.id).locked
                    && !self.session.document.object_state(a.id).hidden
            })
            .filter_map(|a| {
                let (min, max) = a.body_bounds();
                let rect = egui::Rect::from_two_pos(
                    transform.screen_point(min),
                    transform.screen_point(max),
                );
                let button = egui::Rect::from_center_size(rect.center(), egui::vec2(110., 28.));
                (rect.width() >= 110.
                    && rect.height() >= 28.
                    && transform.viewport.contains_rect(button))
                .then_some((a.id, button))
            })
            .collect()
    }
    pub(super) fn image_pan_handle(
        &self,
        transform: crate::canvas_view::Transform,
    ) -> Option<egui::Rect> {
        if self.selected_objects().len() != 1 {
            return None;
        }
        let a = self.selected_annotation()?;
        if a.kind != Kind::Image
            || a.image.as_ref()?.is_placeholder()
            || a.image.as_ref()?.is_cursor()
            || a.image.as_ref()?.vector().is_some()
            || self.session.document.object_state(a.id).locked
        {
            return None;
        }
        let (min, max) = a.body_bounds();
        let rect =
            egui::Rect::from_two_pos(transform.screen_point(min), transform.screen_point(max));
        (rect.width() >= 50. && rect.height() >= 50.)
            .then(|| egui::Rect::from_center_size(rect.center(), egui::vec2(28., 28.)))
    }
    fn image_frame_buttons(
        &self,
        transform: crate::canvas_view::Transform,
    ) -> Option<[egui::Rect; 2]> {
        if self.selected_objects().len() != 1 {
            return None;
        }
        let a = self.selected_annotation()?;
        if a.kind != Kind::Image
            || a.image.as_ref()?.is_placeholder()
            || a.image.as_ref()?.is_cursor()
            || a.image.as_ref()?.vector().is_some()
            || self.session.document.object_state(a.id).locked
        {
            return None;
        }
        let (min, max) = a.bounds();
        let rect =
            egui::Rect::from_two_pos(transform.screen_point(min), transform.screen_point(max));
        if rect.width() < 70. || rect.height() < 40. {
            return None;
        }
        let right = rect.right_bottom() - egui::vec2(8., 8.);
        let buttons = [
            egui::Rect::from_min_size(right - egui::vec2(52., 24.), egui::vec2(24., 24.)),
            egui::Rect::from_min_size(right - egui::vec2(24., 24.), egui::vec2(24., 24.)),
        ];
        buttons
            .iter()
            .all(|b| transform.viewport.contains_rect(*b))
            .then_some(buttons)
    }
    pub(super) fn image_frame_buttons_hovered(
        &self,
        ui: &egui::Ui,
        transform: crate::canvas_view::Transform,
    ) -> bool {
        if self.edit_drag.is_some()
            || self.group_drag.is_some()
            || self.rotation_drag.is_some()
            || ui.input(|i| i.modifiers.alt || i.modifiers.ctrl)
        {
            return false;
        }
        // A drag started on the image still belongs to it when a fast pointer
        // crosses the small crop buttons before egui reports drag_started.
        ui.input(|i| {
            if i.pointer.primary_down() {
                i.pointer.press_origin()
            } else {
                i.pointer.hover_pos()
            }
        })
        .is_some_and(|p| {
            self.placeholder_buttons(transform)
                .iter()
                .any(|(_, r)| r.contains(p))
                || self
                    .image_frame_buttons(transform)
                    .is_some_and(|buttons| buttons.iter().any(|r| r.contains(p)))
        })
    }
    pub(super) fn show_image_frame_buttons(
        &mut self,
        ui: &mut egui::Ui,
        transform: crate::canvas_view::Transform,
    ) {
        let selected = self
            .selected_annotation()
            .filter(|a| a.kind == Kind::Image || a.is_image_frame())
            .map(|a| a.id);
        ui.ctx().data_mut(|d| {
            if d.get_temp::<u64>(crop_mode_key()) != selected {
                d.remove::<u64>(crop_mode_key());
            }
        });
        if self.shell.raster_preview
            || self.shell.annotations_hidden
            || self.edit_drag.is_some()
            || self.property_preview.is_some()
            || ui.input(|i| i.modifiers.alt || i.modifiers.ctrl)
        {
            return;
        }
        for (id, rect) in self.placeholder_buttons(transform) {
            let response = ui.put(
                rect,
                egui::Button::new("Choose Image").fill(crate::shell::ACCENT),
            );
            self.choose_frame_source(ui, &response, id);
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(("choose-frame-image", id)), rect));
        }
        if let Some(rect) = self.image_pan_handle(transform) {
            let response = ui
                .interact(
                    rect,
                    egui::Id::unique("image-pan-handle"),
                    egui::Sense::hover(),
                )
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text("Drag to reposition image inside its crop");
            let p = ui.painter();
            let c = rect.center();
            let lines = [
                vec![
                    c + egui::vec2(-6., -10.),
                    c + egui::vec2(-6., 6.),
                    c + egui::vec2(10., 6.),
                ],
                vec![
                    c + egui::vec2(-10., -6.),
                    c + egui::vec2(6., -6.),
                    c + egui::vec2(6., 10.),
                ],
            ];
            for line in lines {
                p.add(egui::Shape::line(
                    line.clone(),
                    egui::Stroke::new(4., egui::Color32::from_black_alpha(150)),
                ));
                p.add(egui::Shape::line(
                    line,
                    egui::Stroke::new(
                        2.,
                        if response.hovered() {
                            crate::shell::ACCENT
                        } else {
                            egui::Color32::from_white_alpha(210)
                        },
                    ),
                ));
            }
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("image-pan-handle-rect"), rect));
        }
        let Some(buttons) = self.image_frame_buttons(transform) else {
            return;
        };
        for (index, rect) in buttons.into_iter().enumerate() {
            let response = ui
                .put(rect, egui::Button::new(if index == 0 { "−" } else { "+" }))
                .on_hover_text(if index == 0 {
                    "Zoom image out"
                } else {
                    "Zoom image in"
                });
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(("image-frame-button", index)), rect));
            if response.clicked()
                && let Some(a) = self.selected_annotation()
            {
                let bitmap = a.image.as_ref().unwrap();
                if let Ok(view) = bitmap.view().zoomed_about(
                    if index == 0 { 1. / 1.25 } else { 1.25 },
                    self.session.document.image_focus(&a),
                ) && let Ok(edited) = a.with_image_view(Some(view))
                    && edited != a
                {
                    self.execute(Command::Update { annotation: edited });
                }
            }
        }
    }
}

#[cfg(test)]
mod shape_frames_tests;
