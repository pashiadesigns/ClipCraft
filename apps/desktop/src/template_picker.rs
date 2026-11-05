use crate::{Editor, FileAction, egui};
use capture_core::{
    Document, Session,
    bitmap::Bitmap,
    templates::{self, Scale},
};
use std::sync::Arc;
mod manage;
#[derive(Clone)]
struct Choice {
    entry: Option<capture_core::history::Entry>,
    label: String,
    category: String,
    slots: usize,
    document: Arc<Document>,
}
#[derive(Clone, Default)]
struct Catalog {
    choices: Vec<Choice>,
    thumbs: Vec<egui::TextureHandle>,
    error: String,
}
struct SaveDraft {
    name: String,
    empty_images: bool,
    snapshot: Arc<Document>,
    focus: bool,
}
fn order_button(ui: &mut egui::Ui, enabled: bool, up: bool, _index: usize) -> egui::Response {
    let label = if up {
        "Move image earlier"
    } else {
        "Move image later"
    };
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("").min_size(egui::vec2(28., 24.)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("template-order", _index, up)),
            response.rect,
        )
    });
    let c = response.rect.center();
    let y = if up { -3. } else { 3. };
    ui.painter().add(egui::Shape::line(
        vec![
            c + egui::vec2(-5., -y),
            c + egui::vec2(0., y),
            c + egui::vec2(5., -y),
        ],
        egui::Stroke::new(
            1.7,
            if enabled {
                egui::Color32::WHITE
            } else {
                egui::Color32::GRAY
            },
        ),
    ));
    response.on_hover_text(label)
}
pub(super) struct Picker {
    selected: usize,
    catalog: Option<Catalog>,
    save: Option<SaveDraft>,
    category: String,
    title: String,
    scale: Scale,
    images: Vec<Arc<Bitmap>>,
    names: Vec<String>,
    error: String,
    focus_pending: bool,
}
impl Picker {
    fn new(images: Vec<Arc<Bitmap>>) -> Self {
        Self {
            selected: templates::ALL
                .iter()
                .position(|t| t.slots >= images.len())
                .unwrap_or(0),
            catalog: None,
            save: None,
            category: String::new(),
            title: "How-to guide".into(),
            scale: Scale::Fit,
            names: (1..=images.len()).map(|i| format!("Image {i}")).collect(),
            images,
            error: String::new(),
            focus_pending: true,
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
    fn template_catalog(&self, ctx: &egui::Context) -> Catalog {
        let key = egui::Id::unique("template-catalog");
        if let Some(c) = ctx.data(|d| d.get_temp::<Catalog>(key)) {
            return c;
        }
        let mut c = Catalog::default();
        for t in templates::ALL {
            match templates::build(t.id, "Your guide", &[], Scale::Fit) {
                Ok(doc) => c.choices.push(Choice {
                    entry: None,
                    label: t.label.into(),
                    category: t.category.into(),
                    slots: t.slots,
                    document: Arc::new(doc),
                }),
                Err(e) => c.error = e.to_string(),
            }
        }
        if let Some(root) = self.template_root() {
            match templates::library::Library::open(&root) {
                Ok(store) => {
                    for entry in store.0.entries() {
                        match store.0.load(&entry.id) {
                            Ok(doc) => c.choices.push(Choice {
                                entry: Some(entry.clone()),
                                label: entry.label.clone(),
                                category: "My templates".into(),
                                slots: templates::library::slots(&doc),
                                document: Arc::new(doc),
                            }),
                            Err(e) => c.error = e.to_string(),
                        }
                    }
                }
                Err(e) => c.error = e.to_string(),
            }
        }
        for choice in &c.choices {
            let doc = &choice.document;
            match doc.display_list().and_then(|list| {
                list.render_viewport(capture_core::display::Viewport {
                    size: [180, 210],
                    origin: [0., 0.],
                    scale: (180. / doc.width as f64).min(210. / doc.height as f64),
                })
            }) {
                Ok(pixels) => c.thumbs.push(ctx.load_texture(
                    &choice.label,
                    egui::ColorImage::from_rgba_unmultiplied([180, 210], pixels.as_raw()),
                    egui::TextureOptions::LINEAR,
                )),
                Err(e) => {
                    c.error = e.to_string();
                    break;
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(key, c.clone()));
        c
    }
    pub(super) fn file_template_cards(&mut self, ui: &mut egui::Ui) {
        let catalog = self.template_catalog(ui.ctx());
        let key = egui::Id::unique("file-template-limit");
        let limit = ui.ctx().data(|d| d.get_temp::<usize>(key)).unwrap_or(6);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(22., 14.);
            for (i, t) in catalog.choices.iter().take(limit).enumerate() {
                let response =
                    crate::backstage::card(ui, &t.label, &t.category, Some([244, 246, 247, 255]));
                if let Some(texture) = catalog.thumbs.get(i) {
                    let rect = egui::Rect::from_center_size(
                        response.rect.min + egui::vec2(80., 45.),
                        egui::vec2(72., 84.),
                    );
                    ui.painter().image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                        egui::Color32::WHITE,
                    );
                }
                if response.clicked() {
                    match templates::library::instantiate(&t.document, &t.label, &[], Scale::Fit)
                        .and_then(Session::unsaved)
                    {
                        Ok(session) => self.request_action(
                            FileAction::Template {
                                session,
                                title: t.label.clone(),
                            },
                            ui.ctx(),
                        ),
                        Err(e) => self.status = e.to_string(),
                    }
                }
                self.template_context(&response, t);
            }
        });
        ui.horizontal(|ui| {
            if limit < catalog.choices.len() && ui.button("Show more templates").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(key, limit + 6));
            }
            if ui.button("Save current layout as template…").clicked() {
                self.begin_save_template();
            }
        });
        if !catalog.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &catalog.error);
        }
    }
    pub(super) fn begin_save_template(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        let mut picker = Picker::new(vec![]);
        picker.save = Some(SaveDraft {
            name: "My template".into(),
            empty_images: true,
            snapshot: Arc::new(self.session.document.clone()),
            focus: true,
        });
        self.template_picker = Some(picker);
    }
    fn show_save_template(&mut self, ctx: &egui::Context, mut picker: Picker) {
        let d = picker.save.as_mut().unwrap();
        let mut save = false;
        let mut cancel = false;
        let modal=crate::controls::Modal::new(egui::Id::unique("save-local-template")).show(ctx,|ui|{
            ui.set_width(400.);ui.heading("Save as Template");ui.label("Name");
            let input=ui.add(crate::unicode_input::TextEdit::singleline(&mut d.name).char_limit(120).desired_width(f32::INFINITY));
            crate::controls::focus_once(&input,&mut d.focus);
            crate::controls::toggle(ui,&mut d.empty_images,"Replace photos with empty frames");
            crate::property_layout::section_help(ui,"Local template","Saves an independent editable copy on this PC. Text, shapes, icons and layout are preserved. Creating an image from this template never changes the saved template or original project.");
            if !picker.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&picker.error);}
            (save,cancel)=crate::controls::dialog_actions(ui,"Save","Cancel",!d.name.trim().is_empty(),false);
        });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if save {
            let result = self
                .template_root()
                .ok_or_else(|| "Local storage is unavailable.".to_string())
                .and_then(|root| {
                    templates::library::Library::open(&root)
                        .and_then(|mut store| store.save(&d.snapshot, &d.name, d.empty_images))
                        .map_err(|e| e.to_string())
                });
            match result {
                Ok(_) => {
                    ctx.data_mut(|data| {
                        data.remove::<Catalog>(egui::Id::unique("template-catalog"))
                    });
                    self.status = format!("Saved local template “{}”.", d.name);
                    return;
                }
                Err(e) => picker.error = e,
            }
        }
        self.template_picker = Some(picker);
    }
    pub(super) fn open_template_picker(&mut self, images: Vec<Arc<Bitmap>>) {
        self.template_picker = Some(Picker::new(images));
    }
    pub(super) fn show_template_picker(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.template_picker.take() else {
            return;
        };
        if d.save.is_some() {
            self.show_save_template(ctx, d);
            return;
        }
        if d.catalog.is_none() {
            d.catalog = Some(self.template_catalog(ctx));
        }
        let catalog = d.catalog.as_ref().unwrap();
        let mut create = false;
        let mut cancel = false;
        let mut load = false;
        let mut reorder = None;
        let modal = crate::controls::Modal::new(egui::Id::unique("image-from-template")).show(ctx, |ui| {
            ui.set_width(730.);
            crate::property_layout::section_help(ui, "Create Image from Template", "Choose a layout, optionally choose images, and Create. Empty frames have a Choose Image button. Save your current layout as a reusable local template from File. Sections use the same outside controls as Step guides. Fit keeps the whole image; Fill crops nondestructively to cover its frame.");
            ui.horizontal(|ui| {
                ui.label("Category");
                egui::ComboBox::from_id_salt("template-category")
                    .selected_text(if d.category.is_empty() { "All" } else { &d.category })
                    .show_ui(ui, |ui| {
                        for name in ["All", "Steps", "Comparison", "Overview", "Timeline", "Business", "My templates"] {
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
            let gallery_height=(ctx.content_rect().height()-if d.names.is_empty(){235.}else{350.}).clamp(130.,365.);
            egui::ScrollArea::vertical().id_salt("template-gallery").auto_shrink([false,false]).max_height(gallery_height).show(ui, |ui| {
                egui::Grid::new("template-grid").num_columns(3).spacing(egui::vec2(14., 14.)).show(ui, |ui| {
                    for (n, (i, t)) in catalog.choices.iter().enumerate().filter(|(_, t)| d.category.is_empty() || t.category == d.category).enumerate() {
                        ui.vertical(|ui| {
                            ui.set_width(220.);
                            if let Some(texture) = catalog.thumbs.get(i) {
                                let response = ui.add(egui::Button::image(egui::Image::new(texture).fit_to_exact_size(egui::vec2(180., 210.))).selected(d.selected == i));
                                if response.clicked() { d.selected = i; }
                                self.template_context(&response,t);
                            }
                            ui.label(&t.label);
                        });
                        if (n + 1) % 3 == 0 { ui.end_row(); }
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Title");
                let input=ui.add(crate::unicode_input::TextEdit::singleline(&mut d.title).desired_width(450.).char_limit(120));crate::controls::focus_once(&input,&mut d.focus_pending);
            });
            ui.horizontal(|ui| {
                load = ui.button("Choose images…").clicked();
                if ui.button("Clear images").clicked() { d.images.clear(); d.names.clear(); }
                ui.label(format!("{} / {} images", d.images.len(), catalog.choices[d.selected].slots));
            });
            if !d.names.is_empty() {
                egui::ScrollArea::vertical().id_salt("template-image-order").auto_shrink([false,false]).max_height(90.).show(ui, |ui| {
                    for (i, name) in d.names.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2((ui.available_width()-74.).max(100.),24.),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_width(ui.max_rect().width());ui.add(egui::Label::new(format!("{} · {}",i+1,name)).truncate());});
                            if order_button(ui, i > 0, true, i).clicked() {
                                reorder = Some((i, i - 1));
                            }
                            if order_button(ui, i + 1 < d.names.len(), false, i).clicked() {
                                reorder = Some((i, i + 1));
                            }
                        });
                    }
                });
            }
            let valid = d.images.len() <= catalog.choices[d.selected].slots;
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
                if paths.len() > 256 {
                    d.error = "Choose at most 256 images.".into();
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
            match templates::library::instantiate(
                &catalog.choices[d.selected].document,
                &d.title,
                &d.images,
                d.scale,
            )
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
        let b = bitmap(path)?;
        self.replace_frame_bitmap(id, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn compact_gallery_reorders_images_with_drawn_buttons_and_keeps_footer_visible() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.open_template_picker(vec![templates::placeholder().unwrap(); 3]);
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 580.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_template_picker(ui.ctx()),
            ))
        };
        let mut first = frame(&mut e, vec![]);
        let out = frame(&mut e, vec![]);
        let create = crate::color_widgets::tests::text_rect(&out.shapes, "Create").unwrap();
        assert!(create.bottom() < 580.);
        let up = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("template-order", 1_usize, true))))
            .unwrap();
        let down = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("template-order", 1_usize, false)))
            })
            .unwrap();
        assert!(
            up.left() > 700. && down.right() < 850.,
            "up={up:?}, down={down:?}"
        );
        first.0.textures_delta.append(out.0.textures_delta.clone());
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_TEMPLATE_DIR") {
            dump_meshes(&ctx, &dir, "gallery-images-compact", first, out);
        }
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(up.center()),
                    egui::Event::PointerButton {
                        pos: up.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(
            e.template_picker.as_ref().unwrap().names,
            vec!["Image 2", "Image 1", "Image 3"]
        );
    }
    #[test]
    fn save_template_modal_creates_local_copy_without_changing_open_document() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.session =
            Session::new(templates::build("two-steps", "Source", &[], Scale::Fill).unwrap())
                .unwrap();
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let mut picker = Picker::new(vec![]);
        picker.save = Some(SaveDraft {
            name: "Saved team layout".into(),
            empty_images: true,
            snapshot: Arc::new(e.session.document.clone()),
            focus: true,
        });
        e.template_picker = Some(picker);
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 580.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_template_picker(ui.ctx()),
            ))
        };
        let mut first = frame(&mut e, vec![]);
        let out = frame(&mut e, vec![]);
        assert!(ctx.egui_wants_keyboard_input());
        let save = crate::color_widgets::tests::text_rect(&out.shapes, "Save").unwrap();
        first.0.textures_delta.append(out.0.textures_delta.clone());
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_TEMPLATE_DIR") {
            dump_meshes(&ctx, &dir, "save-template", first, out);
        }
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(save.center()),
                    egui::Event::PointerButton {
                        pos: save.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(e.template_picker.is_none());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        let catalog = e.template_catalog(&ctx);
        assert_eq!(
            catalog.choices[templates::ALL.len()].label,
            "Saved team layout"
        );
        assert_eq!(
            catalog.choices[templates::ALL.len()].category,
            "My templates"
        );
    }
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
            e.template_picker
                .as_ref()
                .unwrap()
                .catalog
                .as_ref()
                .unwrap()
                .thumbs
                .len(),
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
        assert!(!replaced.image.as_ref().unwrap().is_placeholder());
        e.execute(crate::Command::Undo);
        assert_eq!(
            serde_json::to_vec(&before).unwrap(),
            serde_json::to_vec(&e.session.document).unwrap()
        );
    }
    #[test]
    fn empty_frames_offer_choose_image_without_crop_controls_and_catalog_keeps_local_layouts() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let dir = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(dir.path());
        let doc = templates::build("two-steps", "Team procedure", &[], Scale::Fill).unwrap();
        let before = serde_json::to_vec(&doc).unwrap();
        templates::library::Library::open(&dir.path().join("templates"))
            .unwrap()
            .save(&doc, "Team procedure", true)
            .unwrap();
        e.session = Session::new(doc).unwrap();
        let frame_id = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.kind == crate::Kind::Image)
            .unwrap()
            .id;
        e.select_only(Some(frame_id));
        let transform = crate::canvas_view::Transform {
            viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 1000.)),
            image: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 948.)),
            scale: 1.,
            zoom: 1.,
        };
        assert!(e.image_pan_handle(transform).is_none());
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(transform.viewport),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_image_frame_buttons(ui, transform),
            ))
        };
        let mut textures = frame(&mut e, vec![]);
        let out = frame(&mut e, vec![]);
        textures
            .0
            .textures_delta
            .append(out.0.textures_delta.clone());
        assert!(crate::color_widgets::tests::text_rect(&out.shapes, "Choose Image").is_some());
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("choose-frame-image", frame_id))))
            .unwrap();
        for pressed in [true, false] {
            let output = frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            textures
                .0
                .textures_delta
                .append(output.0.textures_delta.clone());
        }
        let menu = frame(&mut e, vec![]);
        for label in ["From File…", "From Clipboard", "From Library…"] {
            assert!(
                crate::color_widgets::tests::text_rect(&menu.shapes, label).is_some(),
                "missing {label}"
            );
        }
        assert!(
            ctx.data(|d| d.get_temp::<u64>(egui::Id::unique("replace-frame-image")))
                .is_none()
        );
        let pos = crate::color_widgets::tests::text_rect(&menu.shapes, "From File…")
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(
            ctx.data_mut(|d| d.remove_temp::<u64>(egui::Id::unique("replace-frame-image"))),
            Some(frame_id)
        );
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        let catalog = e.template_catalog(&ctx);
        assert_eq!(catalog.choices.len(), templates::ALL.len() + 1);
        assert_eq!(
            catalog.choices[templates::ALL.len()].label,
            "Team procedure"
        );
        assert_eq!(catalog.choices[templates::ALL.len()].slots, 2);
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_TEMPLATE_DIR") {
            e.refresh_canvas_display(&ctx);
            let canvas = |e: &mut Editor| {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(transform.viewport),
                        focused: true,
                        ..Default::default()
                    },
                    |ui| e.show_canvas(ui),
                ))
            };
            let first = canvas(&mut e);
            let out = canvas(&mut e);
            textures
                .0
                .textures_delta
                .append(first.0.textures_delta.clone());
            textures
                .0
                .textures_delta
                .append(out.0.textures_delta.clone());
            dump_meshes(&ctx, &dir, "empty-frames", textures, out);
        }
    }
}
