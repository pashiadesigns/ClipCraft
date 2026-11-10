use crate::tips::Tip as _;
use crate::{Editor, egui};
use capture_core::export::{Format, Options};
#[cfg(windows)]
mod windows;
#[derive(Default)]
pub(super) struct Preferences {
    pub options: Options,
    pub after: AfterExport,
    pub library_format: Option<Format>,
    /// The default watermark preset was applied this session.
    pub watermark_ready: bool,
}
#[derive(Clone, Copy, Default)]
pub(super) struct AfterExport {
    pub folder: bool,
    pub image: bool,
}
impl AfterExport {
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        crate::controls::toggle(ui, &mut self.folder, "Open export folder");
        crate::controls::toggle(ui, &mut self.image, "Open image after export");
    }
    pub fn open_with(
        &self,
        path: &std::path::Path,
        is_image: bool,
        mut open: impl FnMut(&std::path::Path) -> Result<(), String>,
    ) -> Result<(), String> {
        if self.folder
            && let Some(parent) = path.parent()
        {
            open(parent)?;
        }
        if self.image && is_image {
            open(path)?;
        }
        Ok(())
    }
    pub fn open(&self, path: &std::path::Path, is_image: bool) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.open_with(path, is_image, windows::open_export)
        }
        #[cfg(not(windows))]
        {
            let _ = (path, is_image);
            Ok(())
        }
    }
}

/// Opens a finished export or its folder with the registered Windows handler.
pub(crate) fn open_in_shell(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::open_export(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Opening files requires Windows.".into())
    }
}

/// Opens File Explorer with the file selected.
pub(crate) fn show_in_explorer(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Cannot open File Explorer: {e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Showing files requires Windows.".into())
    }
}

pub(super) struct Dialog {
    document: capture_core::Document,
    pixels: image::RgbaImage,
    texture: Option<egui::TextureHandle>,
    options: Options,
    print: bool,
    after: AfterExport,
    page: usize,
    error: String,
}

impl Editor {
    pub(super) fn begin_export(&mut self, print: bool) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        self.cancel_gesture();
        let document = if self.is_video() {
            match self.video_frame_document() {
                Ok(d) => d,
                Err(e) => {
                    self.status = format!("The frame could not be exported: {e}");
                    return;
                }
            }
        } else {
            self.session.document.clone()
        };
        if !self.export_preferences.watermark_ready {
            // The default preset applies from the first export of a session.
            self.export_preferences.watermark_ready = true;
            self.export_preferences.options.watermark = self.quick_controls.default_watermark();
        }
        match capture_core::redaction_labels::decorate(
            document.clone(),
            self.export_preferences.options.redaction_labels,
        )
        .and_then(|d| d.render())
        {
            Ok(pixels) => {
                self.export_dialog = Some(Dialog {
                    document: document.clone(),
                    pixels,
                    texture: None,
                    options: Options {
                        format: if print {
                            Format::Pdf
                        } else {
                            self.export_preferences.options.format
                        },
                        ..self.export_preferences.options.clone()
                    },
                    print,
                    after: self.export_preferences.after,
                    page: 0,
                    error: String::new(),
                })
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub(super) fn show_export(&mut self, ctx: &egui::Context, frame: Option<&eframe::Frame>) {
        let Some(mut d) = self.export_dialog.take() else {
            return;
        };
        let mut save = false;
        let mut print = false;
        let mut cancel = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("export-print"))
            .area(
                egui::Area::new(egui::Id::unique("export-print"))
                    .anchor(egui::Align2::CENTER_TOP, egui::vec2(0., 32.)),
            )
            .show(ctx, |ui| {
                ui.set_width(crate::watermark_ui::DIALOG_WIDTH);
                // A scrollbar that appears must not widen the dialog.
                ui.spacing_mut().scroll.floating_allocated_width = 0.;
                // Dialog audit (BJ): same title, sections and rows as Export video.
                crate::controls::dialog_title(
                    ui,
                    if d.print {
                        "Print preview"
                    } else {
                        "Export image"
                    },
                );
                use crate::watermark_ui::row;
                let body = (ctx.content_rect().height() - 150.).clamp(240., 660.);
                let pages_ok = egui::ScrollArea::vertical()
                    .id_salt("export-body")
                    .max_height(body)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Room for the scrollbar, whether or not it is showing.
                        ui.set_max_width(ui.available_width() - 13.);
                        crate::controls::dialog_section(ui, "Output", None);
                        row(ui, "Format", |ui| {
                            for format in [Format::Png, Format::Jpeg, Format::Pdf] {
                                ui.selectable_value(&mut d.options.format, format, format.label());
                            }
                        });
                        if d.options.format == Format::Jpeg {
                            crate::property_layout::slider(
                                ui,
                                "Quality",
                                egui::Slider::new(&mut d.options.quality, 1..=100),
                            );
                        }
                        if d.options.format != Format::Png {
                            row(ui, "Background", |ui| {
                                crate::color_widgets::rgb_preview(
                                    ui,
                                    &mut d.options.background,
                                    crate::color_widgets::Preview::Fill,
                                );
                            });
                        }
                        let paged = d.print || d.options.format == Format::Pdf;
                        if paged
                            && d.document
                                .guide
                                .as_ref()
                                .and_then(|g| g.pages.as_ref())
                                .is_none_or(|p| p.setup.is_empty())
                        {
                            row(ui, "Paper", |ui| {
                                for (label, size) in
                                    [("Letter", [612., 792.]), ("A4", [595.276, 841.89])]
                                {
                                    let same = d.options.page.size == size;
                                    if ui.selectable_label(same, label).clicked() {
                                        d.options.page.size = size;
                                    }
                                }
                                if ui.button("Rotate page").clicked() {
                                    d.options.page.size.swap(0, 1);
                                }
                            });
                            crate::property_layout::slider(
                                ui,
                                "Margin (pt)",
                                egui::Slider::new(&mut d.options.page.margin, 0.0..=72.0),
                            );
                            row(ui, "Long images", |ui| {
                                ui.selectable_value(
                                    &mut d.options.page.paginate,
                                    false,
                                    "Fit one page",
                                );
                                ui.selectable_value(
                                    &mut d.options.page.paginate,
                                    true,
                                    "Multiple pages",
                                );
                            });
                        }
                        let presets = self.quick_controls.watermarks();
                        match crate::watermark_ui::section(
                            ui,
                            &mut d.options.watermark,
                            &presets.0,
                            &presets.1,
                        ) {
                            crate::watermark_ui::Outcome::None => {}
                            crate::watermark_ui::Outcome::Changed => d.texture = None,
                            crate::watermark_ui::Outcome::Presets(list, default) => {
                                self.quick_controls.remember_watermarks(list, default);
                                d.texture = None;
                            }
                        }
                        if crate::controls::toggle(
                            ui,
                            &mut d.options.redaction_labels,
                            "Include redaction labels",
                        )
                        .tip("Print a small label on each redaction saying what was hidden.")
                        .changed()
                        {
                            match capture_core::redaction_labels::decorate(
                                d.document.clone(),
                                d.options.redaction_labels,
                            )
                            .and_then(|doc| doc.render())
                            {
                                Ok(pixels) => {
                                    d.pixels = pixels;
                                    d.texture = None;
                                }
                                Err(e) => d.error = e.to_string(),
                            }
                        }
                        let watermark = d.options.watermark.clone();
                        let texture = d.texture.get_or_insert_with(|| {
                            let mut thumb = image::imageops::thumbnail(&d.pixels, 1000, 1000);
                            // The watermark scales with the output, so the preview matches.
                            let _ = capture_core::watermark::apply(&mut thumb, watermark.as_ref());
                            ctx.load_texture(
                                "export-preview",
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [thumb.width() as usize, thumb.height() as usize],
                                    thumb.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            )
                        });
                        let pages = d.options.page.document_pages(&d.document);
                        crate::watermark_ui::heading(
                            ui,
                            "Preview",
                            "How the output will look, with the watermark if one is on.",
                        );
                        // Shorter than before so the sectioned dialog still fits a
                        // small window without moving (dialog audit).
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 220.),
                            egui::Sense::hover(),
                        );
                        if paged {
                            if let Ok(pages) = &pages {
                                d.page = d.page.min(pages.len() - 1);
                                let p = &pages[d.page];
                                let scale = (480. / p.media_size[0]).min(200. / p.media_size[1]);
                                let paper = egui::Rect::from_center_size(
                                    rect.center(),
                                    egui::vec2(
                                        (p.media_size[0] * scale) as f32,
                                        (p.media_size[1] * scale) as f32,
                                    ),
                                );
                                ui.painter().rect_filled(paper, 0., egui::Color32::WHITE);
                                let [x, y, w, h] = p.destination;
                                let image = egui::Rect::from_min_size(
                                    paper.min + egui::vec2((x * scale) as f32, (y * scale) as f32),
                                    egui::vec2((w * scale) as f32, (h * scale) as f32),
                                );
                                ui.painter().rect_filled(
                                    image,
                                    0.,
                                    egui::Color32::from_rgb(
                                        d.options.background[0],
                                        d.options.background[1],
                                        d.options.background[2],
                                    ),
                                );
                                ui.painter().image(
                                    texture.id(),
                                    image,
                                    egui::Rect::from_min_max(
                                        egui::pos2(
                                            p.source[0] as f32 / d.pixels.width() as f32,
                                            p.source[1] as f32 / d.pixels.height() as f32,
                                        ),
                                        egui::pos2(
                                            (p.source[0] + p.source[2]) as f32
                                                / d.pixels.width() as f32,
                                            (p.source[1] + p.source[3]) as f32
                                                / d.pixels.height() as f32,
                                        ),
                                    ),
                                    egui::Color32::WHITE,
                                );
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(d.page > 0, egui::Button::new("Previous"))
                                        .clicked()
                                    {
                                        d.page -= 1;
                                    }
                                    ui.label(format!("Page {} of {}", d.page + 1, pages.len()));
                                    if ui
                                        .add_enabled(
                                            d.page + 1 < pages.len(),
                                            egui::Button::new("Next"),
                                        )
                                        .clicked()
                                    {
                                        d.page += 1;
                                    }
                                });
                            }
                        } else {
                            let size =
                                egui::vec2(d.pixels.width() as f32, d.pixels.height() as f32);
                            let image = egui::Rect::from_center_size(
                                rect.center(),
                                size * (480. / size.x).min(200. / size.y),
                            );
                            ui.painter().rect_filled(
                                image,
                                0.,
                                egui::Color32::from_rgb(
                                    d.options.background[0],
                                    d.options.background[1],
                                    d.options.background[2],
                                ),
                            );
                            ui.painter().image(
                                texture.id(),
                                image,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                                egui::Color32::WHITE,
                            );
                        }
                        ui.weak(format!(
                            "{} × {} pixels",
                            d.pixels.width(),
                            d.pixels.height()
                        ));
                        if let Err(e) = &pages {
                            ui.colored_label(egui::Color32::LIGHT_RED, e.to_string());
                        }
                        if !d.error.is_empty() {
                            ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
                        }
                        crate::watermark_ui::heading(
                            ui,
                            "After export",
                            "What ClipCraft does when the file is saved.",
                        );
                        d.after.controls(ui);
                        pages.is_ok()
                    })
                    .inner;
                ui.add_space(4.);
                ui.separator();
                let footer =
                    egui::vec2(ui.available_width(), crate::controls::DIALOG_ACTION_HEIGHT);
                ui.allocate_ui_with_layout(
                    footer,
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        cancel = ui
                            .add(crate::controls::dialog_button("Close", false))
                            .clicked();
                        print = ui
                            .add_enabled(
                                pages_ok,
                                crate::controls::dialog_button("Print…", d.print).fill(
                                    if d.print {
                                        crate::shell::ACCENT
                                    } else {
                                        ui.visuals().widgets.inactive.bg_fill
                                    },
                                ),
                            )
                            .tip("Choose a printer and confirm in the Windows print dialog.")
                            .clicked();
                        save = ui
                            .add(crate::controls::dialog_button("Save", !d.print))
                            .clicked();
                    },
                );
            });
        self.export_preferences.options = d.options.clone();
        self.export_preferences.after = d.after;
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if save {
            self.suspend_global_capture();
            let format = d.options.format;
            if let Some(mut path) = rfd::FileDialog::new()
                .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Export))
                .set_file_name(format!("capture.{}", format.extension()))
                .add_filter(format.label(), &[format.extension()])
                .save_file()
            {
                if path.extension().is_none() {
                    path.set_extension(format.extension());
                }
                if capture_core::export::Format::from_path(&path).ok() != Some(format) {
                    d.error = format!(
                        "Use a .{} filename for {} output.",
                        format.extension(),
                        format.label()
                    );
                    self.export_dialog = Some(d);
                    return;
                }
                let result = capture_core::export::encode_document(&d.document, &d.options)
                    .and_then(|bytes| capture_core::export::write(&path, &bytes));
                match result {
                    Ok(()) => {
                        self.remember_dialog_folder(
                            capture_core::quick_preferences::Folder::Export,
                            &path,
                        );
                        self.status = format!("Exported {}", path.display());
                        if let Err(error) = d.after.open(&path, true) {
                            self.status =
                                format!("Exported {}, but could not open: {error}", path.display());
                        }
                        return;
                    }
                    Err(e) => d.error = e.to_string(),
                }
            }
        }
        if print {
            self.suspend_global_capture();
            #[cfg(windows)]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                let owner =
                    frame
                        .and_then(|f| f.window_handle().ok())
                        .and_then(|w| match w.as_raw() {
                            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
                            _ => None,
                        });
                match owner
                    .ok_or("Cannot access editor window.".into())
                    .and_then(|owner| {
                        let mut pixels = d.pixels.clone();
                        capture_core::watermark::apply(&mut pixels, d.options.watermark.as_ref())
                            .map_err(|e| e.to_string())?;
                        windows::print(owner, &pixels, &d.document, &d.options)
                    }) {
                    Ok(true) => {
                        self.status = "Print job sent.".into();
                        return;
                    }
                    Ok(false) => (),
                    Err(e) => d.error = e,
                }
            }
            #[cfg(not(windows))]
            {
                let _ = frame;
                d.error = "Native printing is available on Windows. Export PDF here.".into();
            }
        }
        self.export_dialog = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_session_memory_stable_format_position_and_post_save_targets() {
        use crate::property_layout::tests::PaintedOutput;
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.begin_export(false);
        let frame = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 850.),
                    )),
                    ..Default::default()
                },
                |ui| e.show_export(ui.ctx(), None),
            ))
        };
        frame(&mut e);
        frame(&mut e);
        let mut y = None;
        for format in [Format::Png, Format::Pdf, Format::Jpeg, Format::Png] {
            e.export_dialog.as_mut().unwrap().options.format = format;
            frame(&mut e);
            let out = frame(&mut e);
            let pos = crate::color_widgets::tests::text_rect(&out.shapes, "Format")
                .unwrap()
                .top();
            assert!(y.is_none_or(|prior: f32| (prior - pos).abs() < 0.1));
            y = Some(pos);
        }
        e.export_dialog.as_mut().unwrap().options.format = Format::Jpeg;
        e.export_dialog.as_mut().unwrap().options.quality = 82;
        e.export_dialog.as_mut().unwrap().after = AfterExport {
            folder: true,
            image: true,
        };
        frame(&mut e);
        e.export_dialog = None;
        e.begin_export(false);
        let d = e.export_dialog.as_ref().unwrap();
        assert_eq!(d.options.format, Format::Jpeg);
        assert_eq!(d.options.quality, 82);
        let mut opened = vec![];
        d.after
            .open_with(std::path::Path::new("exports/result.jpg"), true, |p| {
                opened.push(p.to_path_buf());
                Ok(())
            })
            .unwrap();
        assert_eq!(
            opened,
            vec![
                std::path::PathBuf::from("exports"),
                std::path::PathBuf::from("exports/result.jpg")
            ]
        );
        opened.clear();
        d.after
            .open_with(
                std::path::Path::new("exports/editable.capture.json"),
                false,
                |p| {
                    opened.push(p.to_path_buf());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(
            opened.len(),
            1,
            "editable projects are never opened as images"
        );
    }
    #[test]
    fn export_preview_is_modal_and_keeps_document_unchanged() {
        use crate::property_layout::tests::{PaintedOutput, dump_meshes};
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = capture_core::Session::new(
            capture_core::Document::from_image(image::RgbaImage::from_fn(640, 400, |x, y| {
                if (50..590).contains(&x) && (50..350).contains(&y) {
                    image::Rgba([25, 145, 193, 255])
                } else {
                    image::Rgba([220, 233, 240, 255])
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let before = serde_json::to_vec(&e.session.document).unwrap();
        e.begin_export(false);
        assert!(e.export_dialog.is_some());
        assert!(!e.editor_shortcuts_idle());
        e.export_dialog.as_mut().unwrap().options.format = Format::Pdf;
        let frame = |e: &mut Editor| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 1000.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    e.show_export(ui.ctx(), None);
                },
            )
        };
        let first = PaintedOutput(frame(&mut e));
        let output = frame(&mut e);
        // The page count sits lower in the scrolling body, below the fold here.
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, "OUTPUT").is_some());
        let dialog = e.export_dialog.as_ref().unwrap();
        assert_eq!(
            dialog
                .options
                .page
                .document_pages(&dialog.document)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_BETA_DIR") {
            dump_meshes(&ctx, &dir, "Export", first, PaintedOutput(output));
        }
    }
}
