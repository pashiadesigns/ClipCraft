use crate::{Editor, FileAction, egui};
use capture_core::{
    Document, Session,
    bitmap::Bitmap,
    templates::{self, Scale},
};
use std::sync::Arc;
pub(super) struct Picker {
    selected: usize,
    category: String,
    title: String,
    scale: Scale,
    images: Vec<Arc<Bitmap>>,
    names: Vec<String>,
    thumbs: Vec<egui::TextureHandle>,
    error: String,
}
impl Picker {
    fn new(images: Vec<Arc<Bitmap>>) -> Self {
        Self {
            selected: templates::ALL
                .iter()
                .position(|t| t.slots >= images.len())
                .unwrap_or(0),
            category: String::new(),
            title: "How-to guide".into(),
            scale: Scale::Fit,
            names: (1..=images.len()).map(|i| format!("Image {i}")).collect(),
            images,
            thumbs: vec![],
            error: String::new(),
        }
    }
}
fn bitmap(path: &std::path::Path) -> Result<Arc<Bitmap>, String> {
    let d = Document::open_image(path).map_err(|e| e.to_string())?;
    let pixels = d.render().map_err(|e| e.to_string())?;
    Bitmap::new(
        capture_core::clipboard_payload::ImagePayload::from_rgba(
            pixels.width(),
            pixels.height(),
            pixels.into_raw(),
        )
        .map_err(|e| e.to_string())?
        .image,
    )
    .map(Arc::new)
    .map_err(|e| e.to_string())
}
impl Editor {
    pub(super) fn open_template_picker(&mut self, images: Vec<Arc<Bitmap>>) {
        self.template_picker = Some(Picker::new(images));
    }
    pub(super) fn templates_from_library(&mut self, entries: &[capture_core::history::Entry]) {
        if entries.len() > 5 {
            self.status = "Select up to five images for a starter template.".into();
            return;
        }
        let result = (|| -> Result<Vec<_>, String> {
            let store = self.history.root().ok_or("Library is unavailable")?;
            let h = capture_core::history::History::open(&store).map_err(|e| e.to_string())?;
            entries
                .iter()
                .map(|e| {
                    let pixels = h
                        .load(&e.id)
                        .and_then(|d| d.render())
                        .map_err(|e| e.to_string())?;
                    Bitmap::new(
                        capture_core::clipboard_payload::ImagePayload::from_rgba(
                            pixels.width(),
                            pixels.height(),
                            pixels.into_raw(),
                        )
                        .map_err(|e| e.to_string())?
                        .image,
                    )
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
                })
                .collect()
        })();
        match result {
            Ok(images) => {
                self.open_template_picker(images);
                self.template_picker.as_mut().unwrap().names =
                    entries.iter().map(|e| e.label.clone()).collect();
            }
            Err(e) => self.status = e,
        }
    }
    pub(super) fn show_template_picker(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.template_picker.take() else {
            return;
        };
        if d.thumbs.is_empty() {
            for t in templates::ALL {
                match templates::build(t.id, "Your guide", &[], Scale::Fit).and_then(|doc| {
                    doc.display_list()?
                        .render_viewport(capture_core::display::Viewport {
                            size: [180, 210],
                            origin: [0., 0.],
                            scale: (180. / doc.width as f64).min(210. / doc.height as f64),
                        })
                }) {
                    Ok(pixels) => d.thumbs.push(ctx.load_texture(
                        t.id,
                        egui::ColorImage::from_rgba_unmultiplied(
                            [pixels.width() as usize, pixels.height() as usize],
                            pixels.as_raw(),
                        ),
                        egui::TextureOptions::LINEAR,
                    )),
                    Err(e) => {
                        d.error = e.to_string();
                        break;
                    }
                }
            }
        }
        let mut create = false;
        let mut cancel = false;
        let mut load = false;
        let mut reorder = None;
        let modal = egui::Modal::new(egui::Id::unique("image-from-template")).show(ctx, |ui| {
            ui.set_width(730.);
            crate::property_layout::section_help(ui, "Create Image from Template", "Choose a layout, optionally choose images, and Create. Empty frames are placeholders; select one and use Replace image in Image frame properties. Sections use the same outside controls as Step guides. Fit keeps the whole image; Fill crops nondestructively to cover its frame.");
            ui.horizontal(|ui| {
                ui.label("Category");
                egui::ComboBox::from_id_salt("template-category")
                    .selected_text(if d.category.is_empty() { "All" } else { &d.category })
                    .show_ui(ui, |ui| {
                        for name in ["All", "Steps", "Comparison", "Overview", "Timeline"] {
                            ui.selectable_value(&mut d.category, if name == "All" { String::new() } else { name.into() }, name);
                        }
                    });
                ui.label("Image scale");
                egui::ComboBox::from_id_salt("template-scale")
                    .selected_text(if d.scale == Scale::Fit { "Fit" } else { "Fill" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.scale, Scale::Fit, "Fit");
                        ui.selectable_value(&mut d.scale, Scale::Fill, "Fill");
                    });
            });
            egui::ScrollArea::vertical().id_salt("template-gallery").max_height(365.).show(ui, |ui| {
                egui::Grid::new("template-grid").num_columns(3).spacing(egui::vec2(14., 14.)).show(ui, |ui| {
                    for (n, (i, t)) in templates::ALL.iter().enumerate().filter(|(_, t)| d.category.is_empty() || t.category == d.category).enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(220.);
                            if let Some(texture) = d.thumbs.get(i) {
                                let response = ui.add(egui::Button::image(egui::Image::new(texture).fit_to_exact_size(egui::vec2(180., 210.))).selected(d.selected == i));
                                if response.clicked() { d.selected = i; }
                            }
                            ui.label(t.label);
                        });
                        if (n + 1) % 3 == 0 { ui.end_row(); }
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Title");
                ui.add(egui::TextEdit::singleline(&mut d.title).desired_width(450.).char_limit(120));
            });
            ui.horizontal(|ui| {
                load = ui.button("Choose images…").clicked();
                if ui.button("Clear images").clicked() { d.images.clear(); d.names.clear(); }
                ui.label(format!("{} / {} images", d.images.len(), templates::ALL[d.selected].slots));
            });
            if !d.names.is_empty() {
                egui::ScrollArea::vertical().id_salt("template-image-order").max_height(90.).show(ui, |ui| {
                    for (i, name) in d.names.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} · {}", i + 1, name));
                            if ui.add_enabled(i > 0, egui::Button::new("↑")).on_hover_text("Move image earlier").clicked() {
                                reorder = Some((i, i - 1));
                            }
                            if ui.add_enabled(i + 1 < d.names.len(), egui::Button::new("↓")).on_hover_text("Move image later").clicked() {
                                reorder = Some((i, i + 1));
                            }
                        });
                    }
                });
            }
            let valid = d.images.len() <= templates::ALL[d.selected].slots;
            if !valid { ui.colored_label(egui::Color32::LIGHT_RED, "Choose a layout with enough image frames, or choose fewer images."); }
            if !d.error.is_empty() { ui.colored_label(egui::Color32::LIGHT_RED, &d.error); }
            (create, cancel) = crate::controls::dialog_actions(ui, "Create", "Cancel", valid, false);
        });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if let Some((a, b)) = reorder {
            d.images.swap(a, b);
            d.names.swap(a, b);
        }
        if load {
            self.suspend_global_capture();
            if let Some(paths) = rfd::FileDialog::new()
                .set_directory(self.dialog_directory())
                .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "webp"])
                .pick_files()
            {
                if paths.len() > 5 {
                    d.error = "Choose at most five images.".into();
                } else {
                    match paths
                        .iter()
                        .map(|p| bitmap(p))
                        .collect::<Result<Vec<_>, _>>()
                    {
                        Ok(images) => {
                            d.images = images;
                            d.names = paths
                                .iter()
                                .map(|p| {
                                    p.file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned()
                                })
                                .collect();
                            d.error.clear();
                            if let Some(path) = paths.last() {
                                self.remember_dialog_folder(
                                    capture_core::quick_preferences::Folder::Open,
                                    path,
                                );
                            }
                        }
                        Err(e) => d.error = e,
                    }
                }
            }
        }
        if create {
            match templates::build(templates::ALL[d.selected].id, &d.title, &d.images, d.scale)
                .and_then(Session::unsaved)
            {
                Ok(session) => {
                    self.request_action(
                        FileAction::Template {
                            session,
                            title: d.title,
                        },
                        ctx,
                    );
                    return;
                }
                Err(e) => d.error = e.to_string(),
            }
        }
        self.template_picker = Some(d);
    }
    pub(super) fn replace_frame_image_request(&mut self, ctx: &egui::Context) {
        let Some(id) =
            ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::unique("replace-frame-image")))
        else {
            return;
        };
        self.commit_property_preview();
        self.suspend_global_capture();
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Replace frame image")
            .set_directory(self.dialog_directory())
            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "webp"])
            .pick_file()
        {
            match self.replace_frame_image_from_path(id, &path) {
                Ok(()) => self
                    .remember_dialog_folder(capture_core::quick_preferences::Folder::Open, &path),
                Err(e) => self.status = e,
            }
        }
    }
    fn replace_frame_image_from_path(
        &mut self,
        id: u64,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let old = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id && a.kind == crate::Kind::Image)
            .ok_or("Image frame no longer exists")?;
        let mut a = old.clone();
        let b = bitmap(path)?;
        let fitted = templates::image_in_frame(
            b,
            id,
            [
                a.start[0],
                a.start[1],
                a.end[0] - a.start[0],
                a.end[1] - a.start[1],
            ],
            Scale::Fill,
        )
        .map_err(|e| e.to_string())?;
        a.image = fitted.image;
        self.execute(crate::Command::Update { annotation: a });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn gallery_creates_editable_template_and_replacing_a_frame_is_one_undo() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.open_template_picker(vec![]);
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_template_picker(ui.ctx()),
            ))
        };
        let mut first = frame(&mut e, vec![]);
        let output = frame(&mut e, vec![]);
        assert!(
            e.template_picker.as_ref().unwrap().error.is_empty(),
            "{}",
            e.template_picker.as_ref().unwrap().error
        );
        assert_eq!(
            e.template_picker.as_ref().unwrap().thumbs.len(),
            templates::ALL.len()
        );
        let create = crate::color_widgets::tests::text_rect(&output.shapes, "Create").unwrap();
        assert!(create.bottom() < 800.);
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_TEMPLATE_DIR") {
            first
                .0
                .textures_delta
                .append(output.0.textures_delta.clone());
            dump_meshes(&ctx, &dir, "gallery", first, output);
        }
        let button = |pressed| egui::Event::PointerButton {
            pos: create.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut e,
            vec![egui::Event::PointerMoved(create.center()), button(true)],
        );
        frame(&mut e, vec![button(false)]);
        assert!(e.template_picker.is_none());
        assert!(
            e.session
                .document
                .guide
                .as_ref()
                .unwrap()
                .template
                .is_some()
        );
        let before = e.session.document.clone();
        let image = before
            .annotations
            .iter()
            .find(|a| a.kind == crate::Kind::Image)
            .unwrap();
        let file = root.path().join("replacement.png");
        image::RgbaImage::from_pixel(80, 120, image::Rgba([220, 80, 40, 255]))
            .save(&file)
            .unwrap();
        e.replace_frame_image_from_path(image.id, &file).unwrap();
        let replaced = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == image.id)
            .unwrap();
        assert_eq!(replaced.start, image.start);
        assert_eq!(replaced.end, image.end);
        assert_eq!(replaced.image.as_ref().unwrap().size(), [80, 120]);
        e.execute(crate::Command::Undo);
        assert_eq!(
            serde_json::to_vec(&before).unwrap(),
            serde_json::to_vec(&e.session.document).unwrap()
        );
    }
}
