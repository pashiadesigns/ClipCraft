//! Original full-window File workspace, using the user's supplied layout reference.
use crate::{Editor, FileAction, egui};
use egui::{Color32, RichText};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Home,
    New,
    Open,
    Info,
    Export,
    Options,
    Help,
}

impl Editor {
    pub(super) fn open_file_workspace(&mut self) {
        if self.commit_text() {
            self.cancel_gesture();
            self.shell.backstage = Some(Page::Home);
        }
    }
    pub(super) fn show_backstage(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.new_image.is_none()
            && self.pending.is_none()
            && !egui::Popup::is_any_open(&ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.shell.backstage = None;
            ctx.request_repaint();
        }
        let page = self.shell.backstage.unwrap_or(Page::Home);
        egui::Panel::left("file_sidebar")
            .exact_size(158.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(crate::shell::ACCENT)
                    .inner_margin(6),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                if nav(ui, "Back", false).clicked() {
                    self.shell.backstage = None;
                }
                ui.add_space(10.0);
                for (target, label) in [
                    (Page::Home, "Home"),
                    (Page::New, "New"),
                    (Page::Open, "Open"),
                ] {
                    if nav(ui, label, page == target).clicked() {
                        self.shell.backstage = Some(target);
                    }
                }
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                if nav(ui, "Info", page == Page::Info).clicked() {
                    self.shell.backstage = Some(Page::Info);
                }
                if nav(ui, "Save", false).clicked() {
                    self.save(false);
                }
                if nav(ui, "Save As", false).clicked() {
                    self.save(true);
                }
                if nav(ui, "Export", page == Page::Export).clicked() {
                    self.shell.backstage = Some(Page::Export);
                }
                if nav(ui, "Library", false).clicked() && self.commit_text() {
                    self.shell.backstage = None;
                    self.shell.library = true;
                    self.history.refresh();
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if nav(ui, "Help", page == Page::Help).clicked() {
                        self.shell.backstage = Some(Page::Help);
                    }
                    if nav(ui, "Options", page == Page::Options).clicked() {
                        self.shell.backstage = Some(Page::Options);
                    }
                    if nav(ui, "Exit", false).clicked() {
                        self.request_action(FileAction::Close, &ctx);
                    }
                    ui.separator();
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new()
            .fill(Color32::from_rgb(50,50,50)).inner_margin(egui::Margin { left:40, right:6, top:24, bottom:16 }))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("file_page_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    if page != Page::Options { ui.set_max_width(ui.available_width().min(1180.0)); }
                    ui.horizontal(|ui| {
                        let (r,_) = ui.allocate_exact_size(egui::vec2(32.0,32.0),egui::Sense::hover());
                        crate::brand::paint(ui,r);
                        ui.label(RichText::new("ClipCraft").size(26.0).strong());
                    });
                    ui.add_space(22.0);
                    match page {
                        Page::Home | Page::New => {
                            ui.heading("New"); ui.add_space(14.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing=egui::vec2(22.0,14.0);
                                for (label,description,spec) in [
                                    ("Blank image","White canvas",Some([255;4])),
                                    ("Transparent image","Transparent canvas",Some([0;4])),
                                    ("Custom size","Choose dimensions",None),
                                ] {
                                    if card(ui,label,description,spec).clicked() {
                                        self.new_image=Some(crate::new_image::NewImage::with_background(spec.unwrap_or([255;4])));
                                    }
                                }
                                if card(ui,"Capture","Select a screen region",Some([34,48,59,255])).clicked() {
                                    self.shell.backstage=None; self.request_action(FileAction::Capture,&ctx);
                                }
                                if card(ui,"From clipboard","Create a new image",Some([54,64,60,255])).clicked() {
                                    self.new_from_clipboard(&ctx);
                                }
                            });
                            ui.menu_button("Capture delay", |ui|self.capture_delay_menu(ui));
                            ui.add_space(24.0);
                            ui.heading("Templates");
                            ui.label(RichText::new("Image templates are planned. Saved object styles are available in the editor's Quick Styles.").color(Color32::from_gray(185)));
                            if page==Page::Home {ui.add_space(32.0);self.backstage_recent(ui);}
                        }
                        Page::Open => {
                            ui.heading("Open"); ui.add_space(16.0);
                            if ui.add_sized([220.0,36.0],egui::Button::new("Browse images and projects…")).clicked() {
                                self.request_action(FileAction::Open,&ctx);
                            }
                            ui.label("Open an image or an editable ClipCraft project.");
                            ui.add_space(30.0); self.backstage_recent(ui);
                        }
                        Page::Info => {
                            ui.heading("Image information");ui.add_space(18.0);
                            ui.label(self.path.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"Untitled image".into()));
                            ui.label(format!("{} × {} pixels",self.session.document.width,self.session.document.height));
                            ui.label(format!("{} editable annotations",self.session.document.annotations.len()));
                            ui.add_space(16.0);self.autosave_status(ui);
                            ui.label("Editable projects retain the original image and separate annotations. PNG exports combine them into one image.");
                        }
                        Page::Export => {
                            ui.heading("Export");ui.add_space(18.0);
                            if ui.add_sized([220.0,38.0],egui::Button::new("Export PNG / JPEG / PDF…")).clicked(){self.export();}
                            ui.label("Save a flattened image to a location you choose.");ui.add_space(16.0);
                            if ui.add_sized([220.0,38.0],egui::Button::new("Save editable project…")).clicked(){self.save(true);}
                            ui.label("Keep the original image, annotations and formatting editable.");ui.add_space(16.0);
                            if ui.add_sized([220.0,38.0],egui::Button::new("Copy complete image")).clicked(){self.copy_all();}
                            ui.add_space(20.0);if ui.add_sized([220.0,38.0],egui::Button::new("Print preview…")).clicked(){self.begin_export(true);}
                        }
                        Page::Options => self.show_options(ui),
                        Page::Help => self.file_help(ui),
                    }
                    ui.add_space(18.0);
                    ui.label(RichText::new(&self.status).size(12.0).color(Color32::from_gray(180)));
                });
            });
    }
    fn backstage_recent(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Recent");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.shell.recent_query)
                        .hint_text("Search for an image")
                        .desired_width(240.0),
                );
            });
        });
        ui.add_space(10.0);
        let mut entries = self.history.entries();
        entries.retain(|e| e.matches_query(&self.shell.recent_query));
        entries.sort_by_key(|e| std::cmp::Reverse(e.updated_ms.unwrap_or(e.created_ms)));
        if entries.is_empty() {
            ui.label(
                "Your library images appear here. Use Open to browse for an image or project.",
            );
        }
        for entry in entries.iter().take(12) {
            let response = ui.add_sized(
                [ui.available_width(), 42.0],
                egui::Button::new(format!(
                    "{}    ·    {} × {}    ·    {}",
                    entry.label,
                    entry.width,
                    entry.height,
                    entry.created_utc()
                ))
                .wrap_mode(egui::TextWrapMode::Truncate),
            );
            self.history_context(&response, entry, true);
            if response.clicked() {
                self.open_history(&entry.id, &ui.ctx().clone());
            }
        }
    }
    fn file_help(&mut self, ui: &mut egui::Ui) {
        ui.heading("Help & About");
        ui.label("Independent Rust image editor · alpha preview");
        ui.add_space(18.0);
        ui.label("Home: Clipboard and drawing tools\nTool Settings: text formatting, arrangement, image and canvas settings\nExport: PNG and editable projects\nFile → Open: images and editable projects\nCtrl+S: Save · Ctrl+Shift+S: Save As\nCtrl+Z / Ctrl+Y: Undo / Redo\nText: draw a box and type directly; click away to finish\nAlt+drag an object: duplicate · Shift+drag handles: constrain proportions\nCtrl+wheel: zoom at pointer · middle-drag or Space+drag: pan\nBottom bar: zoom percentage, Fit to Screen and Actual Size");
        ui.add_space(18.0);
        if ui.button("Configure capture keybinds…").clicked() {
            self.shell.backstage = Some(Page::Options);
            self.options.tab = crate::options::Tab::Keybinds;
        }
        ui.label(&self.global_capture_status);
        ui.add_space(18.0);
        ui.label("Development labels apply only to the scope in each tooltip. Overdue labels preserve missed targets. Hide them using Badges on the bottom bar or File → Options.");
        ui.add_enabled(
            false,
            egui::Button::new("Full help and tutorials — planned"),
        );
    }
}

fn nav(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (r, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 37.0), egui::Sense::click());
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            r,
            3.0,
            if selected {
                Color32::from_rgb(13, 81, 115)
            } else {
                Color32::from_rgb(38, 166, 211)
            },
        );
        if selected {
            ui.painter().rect_stroke(
                r,
                3.0,
                egui::Stroke::new(1.0, Color32::from_rgb(172, 222, 243)),
                egui::StrokeKind::Inside,
            );
        }
    }
    let c = r.left_center() + egui::vec2(17.0, 0.0);
    let color = Color32::WHITE;
    if label == "Back" {
        ui.painter()
            .circle_stroke(c, 9.0, egui::Stroke::new(1.2, color));
        ui.painter().arrow(
            c + egui::vec2(5.0, 0.0),
            egui::vec2(-9.0, 0.0),
            egui::Stroke::new(1.2, color),
        );
    } else {
        crate::ribbon::draw_icon(
            ui.painter(),
            c,
            match label {
                "Save" => "Save As",
                "Home" => "Editor",
                "New" | "Info" | "Open" => "Copy",
                _ => label,
            },
            color,
        );
    }
    if label != "Back" {
        ui.painter().text(
            r.left_center() + egui::vec2(38.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            color,
        );
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    response.on_hover_text(if label == "Back" {
        "Return to the workspace · Esc"
    } else {
        label
    })
}
fn card(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    color: Option<[u8; 4]>,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(160.0, 126.0), egui::Sense::click());
    let preview = egui::Rect::from_min_size(r.min, egui::vec2(160.0, 90.0));
    let rgba = color.unwrap_or([245, 245, 245, 255]);
    if rgba[3] == 0 {
        for y in 0..9 {
            for x in 0..16 {
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        preview.min + egui::vec2(x as f32 * 10.0, y as f32 * 10.0),
                        egui::vec2(10.0, 10.0),
                    ),
                    0.0,
                    Color32::from_gray(if (x + y) % 2 == 0 { 215 } else { 245 }),
                );
            }
        }
    } else {
        ui.painter().rect_filled(
            preview,
            2.0,
            Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
        );
    }
    if matches!(label, "Capture" | "From clipboard") {
        crate::ribbon::draw_icon(
            ui.painter(),
            preview.center(),
            if label == "Capture" {
                "Capture"
            } else {
                "Paste"
            },
            Color32::from_rgb(136, 211, 230),
        );
    } else if label == "Custom size" {
        ui.painter().text(
            preview.center(),
            egui::Align2::CENTER_CENTER,
            "W × H",
            egui::FontId::proportional(24.0),
            Color32::from_gray(65),
        );
    }
    ui.painter().rect_stroke(
        preview,
        2.0,
        egui::Stroke::new(
            if response.hovered() || response.has_focus() {
                2.0
            } else {
                1.0
            },
            if response.hovered() || response.has_focus() {
                Color32::from_rgb(42, 176, 227)
            } else {
                Color32::from_gray(160)
            },
        ),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        r.center_top() + egui::vec2(0.0, 108.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        Color32::WHITE,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response.on_hover_text(description)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    fn frame(
        ctx: &egui::Context,
        e: &mut Editor,
        width: f32,
        events: Vec<egui::Event>,
    ) -> PaintedOutput {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 720.0),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                e.show_shell(ui);
                e.show_new_image(ui.ctx());
            },
        ))
    }
    fn text_rect(output: &PaintedOutput, label: &str) -> egui::Rect {
        output
            .0
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    (t.galley.text() == label)
                        .then_some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Missing {label}"))
    }
    fn click(ctx: &egui::Context, e: &mut Editor, r: egui::Rect) {
        for pressed in [true, false] {
            frame(
                ctx,
                e,
                1280.0,
                vec![
                    egui::Event::PointerMoved(r.center()),
                    egui::Event::PointerButton {
                        pos: r.center(),
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn file_navigation_and_cancel_preserve_document_and_editor_selection() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(crate::Kind::Step);
        e.add_step([100.0, 100.0]);
        let before = e.session.document.clone();
        let selection = e.selected;
        frame(&ctx, &mut e, 1280.0, vec![]);
        let output = frame(&ctx, &mut e, 1280.0, vec![]);
        click(&ctx, &mut e, text_rect(&output, "File"));
        assert_eq!(e.shell.backstage, Some(Page::Home));
        assert!(!e.shell.canvas_enabled());
        assert!(!e.editor_shortcuts_idle());
        for (label, page) in [
            ("Open", Page::Open),
            ("Info", Page::Info),
            ("Export", Page::Export),
            ("Options", Page::Options),
            ("Help", Page::Help),
            ("New", Page::New),
        ] {
            let output = frame(&ctx, &mut e, 1280.0, vec![]);
            click(&ctx, &mut e, text_rect(&output, label));
            assert_eq!(e.shell.backstage, Some(page));
        }
        let output = frame(&ctx, &mut e, 1280.0, vec![]);
        click(&ctx, &mut e, text_rect(&output, "Transparent image"));
        assert!(e.new_image.is_some());
        let output = frame(&ctx, &mut e, 1280.0, vec![]);
        click(&ctx, &mut e, text_rect(&output, "Cancel"));
        assert!(e.new_image.is_none());
        frame(
            &ctx,
            &mut e,
            1280.0,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(e.shell.backstage.is_none());
        assert!(e.shell.canvas_enabled());
        assert_eq!(e.selected, selection);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        e.execute(crate::Command::Undo);
        assert!(e.session.document.annotations.is_empty());
    }
    #[test]
    fn full_window_file_pages_render_at_compact_and_wide_sizes() {
        for width in [900.0, 1280.0] {
            for page in [
                Page::Home,
                Page::New,
                Page::Open,
                Page::Info,
                Page::Export,
                Page::Options,
                Page::Help,
            ] {
                let ctx = egui::Context::default();
                crate::shell::configure(&ctx);
                let mut e = crate::canvas::tests::editor(&ctx);
                e.shell.backstage = Some(page);
                let before = e.session.document.clone();
                let first = frame(&ctx, &mut e, width, vec![]);
                let output = frame(&ctx, &mut e, width, vec![]);
                assert!(text_rect(&output, "ClipCraft").max.x < width);
                assert_eq!(
                    serde_json::to_value(&before).unwrap(),
                    serde_json::to_value(&e.session.document).unwrap()
                );
                if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_FILE_DIR") {
                    dump_meshes(&ctx, &root, &format!("{page:?}-{width}"), first, output);
                }
            }
        }
    }
}
