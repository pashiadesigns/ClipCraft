use crate::tips::Tip as _;
use crate::{Command, Editor, canvas_view::Transform, egui};
use capture_core::{
    Document,
    guide_sections::{Action, SectionKind},
};

mod pages;
mod reorder;
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
    left: f32,
    right: f32,
    bottom: f32,
    resize: egui::Rect,
    add: egui::Rect,
    menu: egui::Rect,
    grips: [egui::Rect; 2],
}
fn controls(document: &Document, transform: Transform) -> Vec<Controls> {
    let Some(layout) = &document.guide else {
        return vec![];
    };
    layout
        .spans()
        .into_iter()
        .map(|(id, top, bottom)| {
            let section = layout.sections.iter().find(|s| s.id == id).unwrap();
            let origin = layout.page_offset(section.page);
            let left = transform.screen_point(origin).x;
            let right = left
                + layout.page_size(section.page, [document.width, document.height])[0] as f32
                    * transform.scale;
            let center = (left + right) * 0.5;
            let y = transform.screen_point([0., bottom])[1];
            let page_rect = pages::page_rect(document, section.page, transform);
            let cy = y.clamp(
                page_rect.top() + 12.,
                (page_rect.bottom() - 12.).max(page_rect.top() + 12.),
            );
            let rail = pages::rail_width(document, transform);
            Controls {
                left,
                right,
                id,
                top,
                bottom,
                grips: [left - rail * 0.5, right + rail * 0.5].map(|x| {
                    egui::Rect::from_center_size(
                        egui::pos2(x, transform.screen_point([0., (top + bottom) * 0.5]).y),
                        egui::vec2(22., 24.),
                    )
                }),
                resize: egui::Rect::from_center_size(egui::pos2(center, y), egui::vec2(40., 14.)),
                add: egui::Rect::from_center_size(
                    egui::pos2(left - rail * 0.5, cy),
                    egui::vec2(24., 24.),
                ),
                menu: egui::Rect::from_center_size(
                    egui::pos2(right + rail * 0.5, cy),
                    egui::vec2(24., 24.),
                ),
            }
        })
        .collect()
}
pub(super) fn visible_buttons(document: &Document, transform: Transform) -> Vec<egui::Rect> {
    let mut rects = pages::button_rects(document, transform);
    rects.extend(
        controls(document, transform)
            .iter()
            .flat_map(|c| [c.add, c.menu]),
    );
    // The optional insert-at-start action must not displace a section's aligned pair.
    if let Some(top) = top_add(document, transform).filter(|top| {
        rects
            .iter()
            .all(|r| !r.expand(2.).intersects(top.expand(2.)))
    }) {
        rects.push(top);
    }
    // Grips yield to page/section commands in short sections; they must not
    // make an otherwise usable Add Page or section menu disappear.
    for grip in controls(document, transform).iter().flat_map(|c| c.grips) {
        if rects
            .iter()
            .all(|r| !r.expand(2.).intersects(grip.expand(2.)))
        {
            rects.push(grip);
        }
    }
    rects.retain(|r| r.width() + 4. <= pages::rail_width(document, transform));
    let visible = crate::canvas_buttons::visible(&rects);
    rects
        .into_iter()
        .zip(visible)
        .filter_map(|(r, shown)| shown.then_some(r))
        .collect()
}
fn top_add(document: &Document, transform: Transform) -> Option<egui::Rect> {
    document.guide.as_ref()?;
    let page = pages::page_rect(document, 0, transform);
    let x = page.left() - pages::rail_width(document, transform) * 0.5;
    Some(egui::Rect::from_center_size(
        egui::pos2(
            x,
            (page.top() + 90.).min((page.bottom() - 12.).max(page.top() + 12.)),
        ),
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
        if reorder::active(ui.ctx()) {
            return true;
        }
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
            visible_buttons(&self.session.document, transform)
                .iter()
                .any(|r| transform.viewport.contains_rect(*r) && r.contains(p))
                || controls(&self.session.document, transform)
                    .iter()
                    .any(|c| c.resize.contains(p))
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
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "bmp", "webp", "svg", "eps"],
            )
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
        let visible = visible_buttons(&self.session.document, transform);
        if self.show_section_reorder(ui, transform, &visible) {
            return;
        }
        self.show_page_controls(ui, transform, &visible);
        let document = self
            .guide_drag
            .as_ref()
            .map_or(&self.session.document, |d| &d.preview);
        let rows = controls(document, transform);
        let layout = document
            .guide
            .clone()
            .unwrap_or(capture_core::guide_sections::Layout {
                pages: None,
                template: None,
                top: 0.,
                sections: vec![],
            });
        let viewport = transform.viewport;
        let mut action = None;
        let mut image_after = None;
        if let Some(rect) = top_add(document, transform)
            .filter(|r| viewport.contains_rect(*r) && visible.contains(r))
        {
            let add = ui
                .put(rect, egui::Button::new("+"))
                .tip("Insert a section at the beginning");
            egui::Popup::menu(&add).show(|ui| {
                for (label, kind) in [
                    ("Header", SectionKind::Header),
                    ("Image Section…", SectionKind::Image),
                    ("Text Section", SectionKind::Text),
                    ("Blank section", SectionKind::Blank),
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
                        egui::pos2(c.left.max(viewport.left()), line_y),
                        egui::pos2(c.right.min(viewport.right()), line_y),
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
                    .tip("Resize section from its bottom edge");
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
                if viewport.contains_rect(c.add) && visible.contains(&c.add) {
                    let add = ui
                        .put(c.add, egui::Button::new("+"))
                        .tip("Insert a section below");
                    egui::Popup::menu(&add).show(|ui| {
                        for (label, kind) in [
                            ("Header", SectionKind::Header),
                            ("Image Section…", SectionKind::Image),
                            ("Text Section", SectionKind::Text),
                            ("Blank section", SectionKind::Blank),
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
            if viewport.contains_rect(c.menu) && visible.contains(&c.menu) {
                let menu = ui.put(c.menu, egui::Button::new("")).tip("Section actions");
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
                    let section = &layout.sections[index];
                    if layout.page_count() > 1 {
                        for (label, copy) in [("Move to", false), ("Copy to", true)] {
                            ui.menu_button(label, |ui| {
                                for page in 0..layout.page_count() {
                                    if page != section.page
                                        && ui.button(format!("Page {}", page + 1)).clicked()
                                    {
                                        action = Some(Action::SectionPage {
                                            id: c.id,
                                            page,
                                            copy,
                                        });
                                        ui.close();
                                    }
                                }
                            });
                        }
                        ui.separator();
                    }
                    if ui
                        .add_enabled(
                            index > 0 && layout.sections[index - 1].page == section.page,
                            egui::Button::new("Move Up"),
                        )
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
                        .add_enabled(
                            index + 1 < rows.len()
                                && layout.sections[index + 1].page == section.page,
                            egui::Button::new("Move Down"),
                        )
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
