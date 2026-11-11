use crate::tips::Tip as _;
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
    /// Gallery group (bi-proposal 2); "My templates" for saved ones.
    group: String,
    /// Built-in template id; "steps" asks for its step count first.
    id: Option<&'static str>,
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
/// The Save as Template modal. The old template chooser was removed (user decision
/// 2026-11-07); File > New's Templates panel creates from templates.
pub(super) struct Picker {
    save: Option<SaveDraft>,
    error: String,
}
impl Picker {
    fn new() -> Self {
        Self {
            save: None,
            error: String::new(),
        }
    }
}
/// Icon and header tint for a built-in image template.
fn image_template_icon(label: &str, category: &str) -> (&'static str, egui::Color32) {
    let rgb = egui::Color32::from_rgb;
    match category {
        "Steps" => ("Template steps", rgb(43, 75, 92)),
        "Comparison" => ("Template comparison", rgb(58, 53, 80)),
        "Overview" => ("Template overview", rgb(46, 74, 74)),
        "Timeline" => ("Template timeline", rgb(75, 58, 43)),
        _ if label.contains("procedure") => ("Template document", rgb(43, 75, 92)),
        _ if label.contains("Troubleshooting") => ("Template wrench", rgb(90, 58, 46)),
        _ if label.contains("Onboarding") => ("Template people", rgb(74, 54, 83)),
        _ if label.contains("Quick reference") => ("Template list", rgb(61, 74, 46)),
        _ if label.contains("Training") => ("Tutorial", rgb(46, 74, 74)),
        "Guide" => ("Template steps", rgb(43, 75, 92)),
        "Slide" => ("Tutorial", rgb(46, 74, 74)),
        "Support" if label.contains("Issue") => ("Template wrench", rgb(90, 58, 46)),
        "Annotated" => ("Template overview", rgb(46, 74, 74)),
        "Reference" => ("Template list", rgb(61, 74, 46)),
        "Support" if label.contains("Checklist") => ("Template list", rgb(61, 74, 46)),
        "Support" => ("Template document", rgb(74, 54, 83)),
        "Announcement" => ("Announcement", rgb(75, 58, 43)),
        _ => ("Template overview", rgb(45, 54, 64)),
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
/// Template lists are built again next time, after a kit added templates.
pub(crate) fn forget_catalog(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Catalog>(egui::Id::unique("template-catalog")));
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
                    group: t.group.into(),
                    id: Some(t.id),
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
                            Ok(doc) if doc.video.is_some() => continue,
                            Ok(doc) => c.choices.push(Choice {
                                entry: Some(entry.clone()),
                                label: entry.label.clone(),
                                category: "My templates".into(),
                                group: "My templates".into(),
                                id: None,
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
    /// File > New Templates panel (approved AZ mockup 4): Image/Video switch,
    /// search, and one grid of built-in and saved templates with icons.
    pub(super) fn file_templates_panel(&mut self, ui: &mut egui::Ui) {
        use capture_core::video::templates::Template;
        enum Pick {
            Image(usize),
            Video(Template),
            SavedVideo(usize),
        }
        struct Tile {
            icon: &'static str,
            label: String,
            description: String,
            tint: egui::Color32,
            mine: bool,
            pick: Pick,
        }
        let ctx = ui.ctx().clone();
        let kind_id = egui::Id::unique("file-templates-video");
        let query_id = egui::Id::unique("file-templates-query");
        let mut video = ctx
            .data(|d| d.get_temp::<bool>(kind_id))
            .unwrap_or_else(|| self.is_video());
        let mut query = ctx
            .data(|d| d.get_temp::<String>(query_id))
            .unwrap_or_default();
        let rgb = egui::Color32::from_rgb;
        let catalog = (!video).then(|| self.template_catalog(&ctx));
        let saved_video = if video {
            self.saved_video_templates()
        } else {
            vec![]
        };
        let tiles: Vec<Tile> = if let Some(catalog) = &catalog {
            catalog
                .choices
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let mine = c.entry.is_some();
                    let (icon, tint) = if mine {
                        ("Template saved", rgb(45, 54, 64))
                    } else {
                        image_template_icon(&c.label, &c.category)
                    };
                    Tile {
                        icon,
                        label: c.label.clone(),
                        description: if mine {
                            "My template".into()
                        } else {
                            c.group.clone()
                        },
                        tint,
                        mine,
                        pick: Pick::Image(i),
                    }
                })
                .collect()
        } else {
            Template::ALL
                .into_iter()
                .skip(1)
                .map(|t| Tile {
                    icon: match t {
                        Template::Walkthrough => "Product walkthrough",
                        Template::Announcement => "Announcement",
                        _ => "Tutorial",
                    },
                    label: t.label().into(),
                    description: t.description().into(),
                    tint: match t {
                        Template::Walkthrough => rgb(58, 53, 80),
                        Template::Announcement => rgb(75, 58, 43),
                        _ => rgb(43, 75, 92),
                    },
                    mine: false,
                    pick: Pick::Video(t),
                })
                .chain(saved_video.iter().enumerate().map(|(i, (label, _))| Tile {
                    icon: "Video template",
                    label: label.clone(),
                    description: "My template".into(),
                    tint: rgb(45, 54, 64),
                    mine: true,
                    pick: Pick::SavedVideo(i),
                }))
                .collect()
        };
        let group_id = egui::Id::unique("file-templates-group");
        let mut group = ctx.data(|d| d.get_temp::<usize>(group_id)).unwrap_or(0);
        let needle = query.trim().to_lowercase();
        let tiles: Vec<Tile> = tiles
            .into_iter()
            .filter(|t| {
                (video || group == 0 || t.mine || t.description == templates::GROUPS[group - 1])
                    && (needle.is_empty()
                        || t.label.to_lowercase().contains(&needle)
                        || t.description.to_lowercase().contains(&needle))
            })
            .collect();
        ui.spacing_mut().item_spacing.y = 10.;
        let count = format!("· {}", tiles.len());
        crate::backstage::heading(ui, "", "Templates", Some(&count));
        if let Some(choice) = crate::backstage::segmented(ui, ["Image", "Video"], video) {
            video = choice;
        }
        if !video {
            // Template groups (bi-proposal 2): All, Guides, Support, Announce & share.
            crate::controls::segments(ui, |ui| {
                for (i, name) in std::iter::once("All").chain(templates::GROUPS).enumerate() {
                    if ui
                        .selectable_label(group == i, name)
                        .tip_title(format!("Show {} templates", name.to_lowercase()))
                        .clicked()
                    {
                        group = i;
                    }
                }
            });
        }
        ui.add(
            crate::unicode_input::TextEdit::singleline(&mut query)
                .hint_text("Search templates")
                .desired_width(f32::INFINITY),
        );
        let mut picked = None;
        // bj-proposal 2: Steps asks for its step count and layout first.
        let steps_id = egui::Id::unique("file-templates-steps");
        let mut steps: Option<(usize, bool)> = ctx.data(|d| d.get_temp(steps_id));
        let mut create_steps = None;
        if let Some((count, grid)) = steps.as_mut() {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(35, 65, 74))
                .corner_radius(8)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    // Two rows: the buttons need the panel's width to themselves.
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Steps").strong());
                        if ui
                            .add_enabled(*count > 2, egui::Button::new("−"))
                            .tip("Fewer steps")
                            .clicked()
                        {
                            *count -= 1;
                        }
                        ui.label(format!("{count} steps"));
                        if ui
                            .add_enabled(*count < 8, egui::Button::new("+"))
                            .tip("More steps")
                            .clicked()
                        {
                            *count += 1;
                        }
                    });
                    ui.horizontal(|ui| {
                        crate::controls::segments(ui, |ui| {
                            ui.selectable_value(grid, false, "Stacked")
                                .tip("One step per row, with notes beside each screenshot");
                            ui.selectable_value(grid, true, "Grid")
                                .tip("Two steps per row");
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                create_steps = Some(None);
                            }
                            if ui
                                .add(crate::controls::dialog_button("Create", true))
                                .clicked()
                            {
                                create_steps = Some(Some((*count, *grid)));
                            }
                        });
                    });
                });
        }
        egui::ScrollArea::vertical()
            .id_salt("file-templates-grid")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if tiles.is_empty() {
                    ui.label(egui::RichText::new("No templates match.").color(egui::Color32::GRAY));
                }
                // Leave room for the floating scrollbar beside the second column.
                let width = (ui.available_width() - 20.) / 2.;
                for row in tiles.chunks(2) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.;
                        for t in row {
                            let response = crate::backstage::template_tile(
                                ui,
                                width,
                                t.icon,
                                &t.label,
                                &t.description,
                                t.tint,
                                t.mine,
                            );
                            if let (Pick::Image(i), Some(catalog)) = (&t.pick, &catalog) {
                                self.template_context(&response, &catalog.choices[*i]);
                            }
                            if response.clicked() {
                                picked = Some(match t.pick {
                                    Pick::Image(i) => Pick::Image(i),
                                    Pick::Video(v) => Pick::Video(v),
                                    Pick::SavedVideo(i) => Pick::SavedVideo(i),
                                });
                            }
                        }
                    });
                }
                if let Some(catalog) = &catalog
                    && !catalog.error.is_empty()
                {
                    ui.colored_label(egui::Color32::LIGHT_RED, &catalog.error);
                }
            });
        if let Some(Some((count, grid))) = create_steps {
            steps = None;
            match templates::steps(count, grid)
                .and_then(|t| templates::build_template(&t, "Your guide", &[], Scale::Fit))
                .and_then(|doc| templates::library::instantiate(&doc, "Steps", &[], Scale::Fit))
                .and_then(Session::unsaved)
            {
                Ok(session) => self.request_action(
                    FileAction::Template {
                        session,
                        title: "Steps".into(),
                    },
                    &ctx,
                ),
                Err(e) => self.status = e.to_string(),
            }
        } else if create_steps.is_some() {
            steps = None;
        }
        if let Some(Pick::Image(i)) = picked
            && catalog
                .as_ref()
                .is_some_and(|c| c.choices[i].id == Some("steps"))
        {
            steps = Some(steps.unwrap_or((4, false)));
            picked = None;
        }
        ctx.data_mut(|d| {
            d.insert_temp(kind_id, video);
            d.insert_temp(query_id, query);
            d.insert_temp(group_id, group);
            match steps {
                Some(s) => {
                    d.insert_temp(steps_id, s);
                }
                None => {
                    d.remove::<(usize, bool)>(steps_id);
                }
            }
        });
        match picked {
            Some(Pick::Image(i)) => {
                let t = &catalog.as_ref().unwrap().choices[i];
                match templates::library::instantiate(&t.document, &t.label, &[], Scale::Fit)
                    .and_then(Session::unsaved)
                {
                    Ok(session) => self.request_action(
                        FileAction::Template {
                            session,
                            title: t.label.clone(),
                        },
                        &ctx,
                    ),
                    Err(e) => self.status = e.to_string(),
                }
            }
            Some(Pick::Video(t)) => self.new_video(t),
            Some(Pick::SavedVideo(i)) => {
                let (label, doc) = saved_video[i].clone();
                match Session::unsaved(doc) {
                    Ok(session) => self.request_action(
                        FileAction::Template {
                            session,
                            title: label,
                        },
                        &ctx,
                    ),
                    Err(e) => self.status = e.to_string(),
                }
            }
            None => {}
        }
    }
    pub(super) fn begin_save_template(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        let mut picker = Picker::new();
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
            ui.set_width(400.);crate::controls::dialog_title(ui,"Save as Template");crate::controls::dialog_section(ui,"Name",Some("Saves an independent editable copy on this PC. Text, shapes, icons and layout are preserved. Creating an image from this template never changes the saved template or original project."));
            let input=ui.add(crate::unicode_input::TextEdit::singleline(&mut d.name).char_limit(120).desired_width(f32::INFINITY));
            crate::controls::focus_once(&input,&mut d.focus);
            if d.snapshot.video.is_none(){crate::controls::toggle(ui,&mut d.empty_images,"Replace photos with empty frames");}
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
    pub(super) fn show_template_picker(&mut self, ctx: &egui::Context) {
        if let Some(d) = self.template_picker.take()
            && d.save.is_some()
        {
            self.show_save_template(ctx, d);
        }
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
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "bmp", "webp", "svg", "eps"],
            )
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
        let mut picker = Picker::new();
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
        if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_TEMPLATE_DIR") {
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
    fn template_creates_editable_guide_and_replacing_a_frame_is_one_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.session = Session::unsaved(
            templates::library::instantiate(
                &templates::build("two-steps", "Guide", &[], Scale::Fit).unwrap(),
                "Guide",
                &[],
                Scale::Fit,
            )
            .unwrap(),
        )
        .unwrap();
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
        if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_TEMPLATE_DIR") {
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
