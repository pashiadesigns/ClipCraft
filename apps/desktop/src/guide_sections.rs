use crate::{Command, Editor, canvas_view::Transform, egui};
use capture_core::{
    Document,
    guide_sections::{Action, SectionKind},
};

#[cfg(test)]
mod tests;

pub(super) struct SectionDrag {
    pub preview: Document,
    transform: Transform,
    id: u64,
    start_y: f32,
    original_height: f32,
    height: f32,
}

struct Controls {
    id: u64,
    top: f32,
    bottom: f32,
    resize: egui::Rect,
    add: egui::Rect,
    menu: egui::Rect,
}
fn controls(document: &Document, transform: Transform) -> Vec<Controls> {
    let Some(layout) = &document.guide else {
        return vec![];
    };
    let left = transform.image.left();
    let right = transform.image.right();
    let center = (left + right) * 0.5;
    layout
        .spans()
        .into_iter()
        .map(|(id, top, bottom)| {
            let y = transform.screen_point([0., bottom])[1];
            Controls {
                id,
                top,
                bottom,
                resize: egui::Rect::from_center_size(egui::pos2(center, y), egui::vec2(40., 14.)),
                add: egui::Rect::from_center_size(egui::pos2(left - 18., y), egui::vec2(24., 24.)),
                menu: egui::Rect::from_center_size(
                    egui::pos2(right + 18., transform.screen_point([0., top + 16.])[1]),
                    egui::vec2(24., 24.),
                ),
            }
        })
        .collect()
}
fn top_add(document: &Document, transform: Transform) -> Option<egui::Rect> {
    let top = document.guide.as_ref()?.top;
    let x = transform.image.left() - 18.;
    Some(egui::Rect::from_center_size(
        egui::pos2(x, transform.screen_point([0., top]).y),
        egui::vec2(24., 24.),
    ))
}
impl Editor {
    pub(super) fn guide_transform(&self, mut transform: Transform) -> Transform {
        if let Some(drag) = &self.guide_drag {
            transform = drag.transform;
            transform.image = egui::Rect::from_min_size(
                transform.image.min,
                egui::vec2(drag.preview.width as f32, drag.preview.height as f32) * transform.scale,
            );
        }
        transform
    }
    pub(super) fn guide_controls_hovered(&self, ui: &egui::Ui, transform: Transform) -> bool {
        if self.guide_drag.is_some() {
            return true;
        }
        if self.edit_drag.is_some() || self.group_drag.is_some() || self.rotation_drag.is_some() {
            return false;
        }
        ui.input(|i| {
            if i.pointer.primary_down() {
                i.pointer.press_origin()
            } else {
                i.pointer.hover_pos()
            }
        })
        .is_some_and(|p| {
            top_add(&self.session.document, transform).is_some_and(|r| r.contains(p))
                || controls(&self.session.document, transform)
                    .iter()
                    .any(|c| [c.resize, c.add, c.menu].iter().any(|r| r.contains(p)))
        })
    }
    fn apply_section(&mut self, action: Action, transform: Transform) {
        if !self.commit_text() {
            return;
        }
        let old = self.session.document.guide.clone();
        self.execute(Command::Guide { action });
        if old != self.session.document.guide {
            self.view.retain_expanded_position(
                transform,
                [0, 0],
                [self.session.document.width, self.session.document.height],
            );
            self.status = "Guide section updated. Undo restores the previous layout.".into();
        }
    }
    fn insert_image_section(&mut self, after: Option<u64>, transform: Transform) {
        self.suspend_global_capture();
        let path = rfd::FileDialog::new()
            .set_title("Add image section")
            .set_directory(&self.last_directory)
            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "webp"])
            .pick_file();
        let Some(path) = path else {
            return;
        };
        self.insert_image_section_from_path(after, transform, &path);
    }
    fn insert_image_section_from_path(
        &mut self,
        after: Option<u64>,
        transform: Transform,
        path: &std::path::Path,
    ) {
        let result = (|| -> Result<_, String> {
            let d = Document::open_image(path).map_err(|e| e.to_string())?;
            let pixels = d.render().map_err(|e| e.to_string())?;
            let payload = capture_core::clipboard_payload::ImagePayload::from_rgba(
                pixels.width(),
                pixels.height(),
                pixels.into_raw(),
            )
            .map_err(|e| e.to_string())?;
            capture_core::bitmap::Bitmap::new(payload.image)
                .map(std::sync::Arc::new)
                .map_err(|e| e.to_string())
        })();
        match result {
            Ok(image) => self.apply_section(Action::InsertImage { after, image }, transform),
            Err(e) => self.status = e,
        }
    }
    pub(super) fn show_guide_controls(&mut self, ui: &mut egui::Ui, transform: Transform) {
        if self.text_draft.is_some()
            || self.edit_drag.is_some()
            || self.rotation_drag.is_some()
            || self.group_drag.is_some()
            || self.drag_start.is_some()
            || self.property_preview.is_some()
        {
            return;
        }
        let document = self
            .guide_drag
            .as_ref()
            .map_or(&self.session.document, |d| &d.preview);
        let rows = controls(document, transform);
        let viewport = transform.viewport;
        let mut action = None;
        let mut image_after = None;
        if let Some(rect) = top_add(document, transform).filter(|r| viewport.contains_rect(*r)) {
            let add = ui
                .put(rect, egui::Button::new("+"))
                .on_hover_text("Insert a section at the beginning");
            egui::Popup::menu(&add).show(|ui| {
                for (label, kind) in [
                    ("Header", SectionKind::Header),
                    ("Image Section…", SectionKind::Image),
                    ("Text Section", SectionKind::Text),
                ] {
                    if ui.button(label).clicked() {
                        if kind == SectionKind::Image {
                            image_after = Some(None);
                        } else {
                            action = Some(Action::Insert { after: None, kind });
                        }
                        ui.close();
                    }
                }
            });
        }
        for (index, c) in rows.iter().enumerate() {
            let line_y = transform.screen_point([0., c.bottom]).y;
            if line_y >= viewport.top() && line_y <= viewport.bottom() {
                let painter = ui.painter().with_clip_rect(viewport);
                painter.line_segment(
                    [
                        egui::pos2(transform.image.left().max(viewport.left()), line_y),
                        egui::pos2(transform.image.right().min(viewport.right()), line_y),
                    ],
                    egui::Stroke::new(1., egui::Color32::from_gray(145)),
                );
                let resize = ui
                    .interact(
                        c.resize,
                        egui::Id::unique(("guide-resize", c.id)),
                        egui::Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                    .on_hover_text("Resize section from its bottom edge");
                painter.rect_filled(
                    egui::Rect::from_center_size(c.resize.center(), egui::vec2(30., 3.)),
                    2.,
                    egui::Color32::from_rgb(45, 175, 225),
                );
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique(("guide-resize-test", c.id)), c.resize)
                });
                if resize.drag_started() && self.guide_drag.is_none() {
                    let start_y = ui
                        .input(|i| i.pointer.press_origin())
                        .unwrap_or(c.resize.center())
                        .y;
                    self.guide_drag = Some(SectionDrag {
                        preview: self.session.document.clone(),
                        transform,
                        id: c.id,
                        start_y,
                        original_height: c.bottom - c.top,
                        height: c.bottom - c.top,
                    });
                }
                if viewport.contains_rect(c.add) {
                    let add = ui
                        .put(c.add, egui::Button::new("+"))
                        .on_hover_text("Insert a section below");
                    egui::Popup::menu(&add).show(|ui| {
                        for (label, kind) in [
                            ("Header", SectionKind::Header),
                            ("Image Section…", SectionKind::Image),
                            ("Text Section", SectionKind::Text),
                        ] {
                            let choice = ui.button(label);
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::unique(("guide-insert-choice", c.id, label)),
                                    choice.rect,
                                )
                            });
                            if choice.clicked() {
                                if kind == SectionKind::Image {
                                    image_after = Some(Some(c.id));
                                } else {
                                    action = Some(Action::Insert {
                                        after: Some(c.id),
                                        kind,
                                    });
                                }
                                ui.close();
                            }
                        }
                    });
                }
            }
            if viewport.contains_rect(c.menu) {
                let menu = ui
                    .put(c.menu, egui::Button::new(""))
                    .on_hover_text("Section actions");
                menu.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, "Section actions")
                });
                for dx in [-5., 0., 5.] {
                    ui.painter().circle_filled(
                        c.menu.center() + egui::vec2(dx, 0.),
                        1.5,
                        ui.visuals().text_color(),
                    );
                }
                egui::Popup::menu(&menu).show(|ui| {
                    if ui
                        .add_enabled(index > 0, egui::Button::new("Move Up"))
                        .clicked()
                    {
                        action = Some(Action::Move {
                            id: c.id,
                            index: index - 1,
                        });
                        ui.close();
                    }
                    if ui.button("Duplicate").clicked() {
                        action = Some(Action::Duplicate { id: c.id });
                        ui.close();
                    }
                    if ui
                        .add_enabled(index + 1 < rows.len(), egui::Button::new("Move Down"))
                        .clicked()
                    {
                        action = Some(Action::Move {
                            id: c.id,
                            index: index + 1,
                        });
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete Section").clicked() {
                        action = Some(Action::Remove { id: c.id });
                        ui.close();
                    }
                });
            }
        }
        if let Some(drag) = &mut self.guide_drag {
            if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                let height = (drag.original_height
                    + (pointer.y - drag.start_y) / drag.transform.scale)
                    .clamp(48., 32768.);
                if (height - drag.height).abs() > 0.01 {
                    match self.session.document.edit_guide(&Action::Resize {
                        id: drag.id,
                        height,
                    }) {
                        Ok(preview) => {
                            drag.height = height;
                            drag.preview = preview;
                            self.needs_render = true;
                            ui.ctx().request_repaint();
                        }
                        Err(error) => self.status = error.to_string(),
                    }
                }
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                let drag = self.guide_drag.take().unwrap();
                if drag.preview.guide != self.session.document.guide {
                    action = Some(Action::Resize {
                        id: drag.id,
                        height: drag.height,
                    });
                }
                self.needs_render = true;
            }
        }
        if let Some(action) = action {
            self.apply_section(action, transform);
        }
        if let Some(after) = image_after {
            self.insert_image_section(after, transform);
        }
    }
}
