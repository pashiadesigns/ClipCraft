use crate::tips::Tip as _;
use crate::{Editor, egui};
use capture_core::export::office::{DocumentOptions, Each, Paper, SlideOptions, SlideSize};
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
    pub office: OfficeChoice,
}
/// PowerPoint and Word output (approved beta 6 tab 2), remembered for the
/// session like the image options.
#[derive(Clone, Default)]
pub(crate) struct OfficeChoice {
    /// `None` exports the image format in `Options.format`.
    pub kind: Option<Office>,
    pub slides: SlideOptions,
    pub word: DocumentOptions,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Office {
    PowerPoint,
    Word,
}
impl Office {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Office::PowerPoint => "PowerPoint",
            Office::Word => "Word",
        }
    }
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Office::PowerPoint => "pptx",
            Office::Word => "docx",
        }
    }
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
    /// For a linked part: export every part of its set into one file
    /// (approved beta 6 tab 7). On by default.
    all_parts: bool,
    office: OfficeChoice,
    /// The preview was drawn for an Office format (no watermark).
    texture_office: bool,
}

impl Editor {
    /// Writes a PowerPoint or Word file from this guide, or from every part
    /// of its set when `all` is on. Redaction labels are drawn as chosen;
    /// redactions are always flattened.
    pub(crate) fn export_office(
        &self,
        current: &capture_core::Document,
        options: &Options,
        office: &OfficeChoice,
        kind: Office,
        all: bool,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let labels = options.redaction_labels;
        let decorate = move |d: capture_core::Result<capture_core::Document>| {
            d.and_then(|d| capture_core::redaction_labels::decorate(d, labels))
        };
        let mut ids: Vec<Option<String>> = vec![None];
        if all && let Some(part) = current.part.clone() {
            let siblings = self.part_siblings(&part);
            ids = (1..=part.total)
                .map(|index| {
                    if index == part.index {
                        return Ok(None);
                    }
                    siblings
                        .iter()
                        .find(|e| e.part.as_ref().is_some_and(|p| p.index == index))
                        .map(|e| Some(e.id.clone()))
                        .ok_or_else(|| {
                            format!(
                                "Part {index} of {} isn't in the Library, so the parts can't be joined.",
                                part.total
                            )
                        })
                })
                .collect::<Result<_, _>>()?;
        }
        let store = self.history.store.as_ref();
        let documents = ids.iter().map(|id| {
            decorate(match id {
                None => Ok(current.clone()),
                Some(id) => store
                    .ok_or_else(|| {
                        capture_core::Error::Invalid("The Library isn't available.".into())
                    })
                    .and_then(|s| s.load(id)),
            })
        });
        let bytes = match kind {
            Office::PowerPoint => capture_core::export::office::pptx(documents, &office.slides),
            Office::Word => {
                let mut word = office.word.clone();
                word.title = current
                    .part
                    .as_ref()
                    .map_or_else(|| word.title.clone(), |p| p.title.clone());
                capture_core::export::office::docx(documents, &word)
            }
        }
        .map_err(|e| e.to_string())?;
        capture_core::export::write(path, &bytes).map_err(|e| e.to_string())
    }
    /// The Library items of a part's set, in part order.
    pub(crate) fn part_siblings(
        &self,
        part: &capture_core::pdf_import::PartOf,
    ) -> Vec<capture_core::history::Entry> {
        let mut list: Vec<_> = self
            .history
            .entries()
            .into_iter()
            .filter(|e| e.part.as_ref().is_some_and(|p| p.set == part.set))
            .collect();
        list.sort_by_key(|e| e.part.as_ref().map_or(0, |p| p.index));
        list.dedup_by_key(|e| e.part.as_ref().map_or(0, |p| p.index));
        list
    }
    /// Exports every part of `current`'s set, in order (approved beta 6 tab
    /// 7): this part as edited, the others loaded from the Library one at a
    /// time. PDF becomes one file; PNG and JPEG become numbered pictures, one
    /// per page, next to `path`.
    pub(crate) fn export_all_parts(
        &self,
        current: &capture_core::Document,
        options: &Options,
        path: &std::path::Path,
    ) -> Result<(), String> {
        let part = current
            .part
            .clone()
            .ok_or("This guide isn't a linked part.")?;
        let siblings = self.part_siblings(&part);
        let store = self
            .history
            .store
            .as_ref()
            .ok_or("The Library isn't available.")?;
        let mut ids = vec![];
        for index in 1..=part.total {
            if index == part.index {
                ids.push(None);
            } else {
                let entry = siblings
                    .iter()
                    .find(|e| e.part.as_ref().is_some_and(|p| p.index == index))
                    .ok_or_else(|| {
                        format!(
                            "Part {index} of {} isn't in the Library, so the parts can't be joined.",
                            part.total
                        )
                    })?;
                ids.push(Some(entry.id.clone()));
            }
        }
        let load = |id: &Option<String>| match id {
            None => Ok(current.clone()),
            Some(id) => store.load(id),
        };
        if options.format == Format::Pdf {
            let bytes = capture_core::export::encode_parts_pdf(ids.iter().map(load), options)
                .map_err(|e| e.to_string())?;
            return capture_core::export::write(path, &bytes).map_err(|e| e.to_string());
        }
        let stem = path
            .file_stem()
            .map_or_else(|| "export".to_owned(), |s| s.to_string_lossy().into_owned());
        let folder = path.parent().unwrap_or(std::path::Path::new("."));
        let mut number = 0;
        for id in &ids {
            let document = load(id).map_err(|e| e.to_string())?;
            let document =
                capture_core::redaction_labels::decorate(document, options.redaction_labels)
                    .map_err(|e| e.to_string())?;
            let mut pixels = document.render().map_err(|e| e.to_string())?;
            capture_core::watermark::apply(&mut pixels, options.watermark.as_ref())
                .map_err(|e| e.to_string())?;
            let pages = document.guide.as_ref().map_or(1, |g| g.page_count());
            for page in 0..pages {
                let rect = document.page_rect(page);
                number += 1;
                let [x, y, w, h] = rect.map(|v| v.max(0.) as u32);
                let w = w.min(pixels.width().saturating_sub(x)).max(1);
                let h = h.min(pixels.height().saturating_sub(y)).max(1);
                let crop = image::imageops::crop_imm(&pixels, x, y, w, h).to_image();
                let bytes =
                    capture_core::export::encode(&crop, options).map_err(|e| e.to_string())?;
                let file =
                    folder.join(format!("{stem}-{number:03}.{}", options.format.extension()));
                capture_core::export::write(&file, &bytes).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
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
                    all_parts: true,
                    office: OfficeChoice {
                        kind: if print {
                            None
                        } else {
                            self.export_preferences.office.kind
                        },
                        ..self.export_preferences.office.clone()
                    },
                    texture_office: false,
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
                                let on = d.office.kind.is_none() && d.options.format == format;
                                if ui.selectable_label(on, format.label()).clicked() {
                                    d.options.format = format;
                                    d.office.kind = None;
                                }
                            }
                            if !d.print {
                                for (office, tip) in [
                                    (Office::PowerPoint, "PowerPoint · One slide per guide page or step section. Shapes, lines, arrows, Steps and text stay editable in PowerPoint."),
                                    (Office::Word, "Word · A step-by-step document: each step's picture and its text, in Word's built-in styles."),
                                ] {
                                    if ui
                                        .selectable_label(d.office.kind == Some(office), office.label())
                                        .tip(tip)
                                        .clicked()
                                    {
                                        d.office.kind = Some(office);
                                    }
                                }
                            }
                        });
                        const FLATTENED: &str = "Redactions are always flattened in the export, with what they cover removed, as in PDF export.";
                        match d.office.kind {
                            Some(Office::PowerPoint) => {
                                crate::controls::dialog_section(ui, "Slides", Some(FLATTENED));
                                let s = &mut d.office.slides;
                                crate::property_layout::choice(
                                    ui,
                                    "Slide size",
                                    &mut s.size,
                                    &[
                                        (SlideSize::Widescreen, "Widescreen 16:9"),
                                        (SlideSize::Standard, "Standard 4:3"),
                                        (SlideSize::MatchPage, "Match the page"),
                                    ],
                                    "Slide size · Widescreen and Standard fit each page on the slide; Match the page uses the first page's shape.",
                                );
                                crate::property_layout::choice(
                                    ui,
                                    "Each slide",
                                    &mut s.each,
                                    &[
                                        (Each::Page, "One guide page"),
                                        (Each::Section, "One step section"),
                                    ],
                                    "Each slide · One slide for each page, or for each step section of a guide.",
                                );
                                row(ui, "Objects", |ui| {
                                    ui.selectable_value(&mut s.editable, true, "Editable shapes")
                                        .tip("Editable shapes · Shapes, lines, arrows, Steps and text become PowerPoint shapes and text boxes. Effects PowerPoint can't draw the same way stay in the picture.");
                                    ui.selectable_value(&mut s.editable, false, "Pictures only")
                                        .tip("Pictures only · Each page is one picture, so the deck looks exactly like ClipDesigner.");
                                });
                                crate::controls::toggle(
                                    ui,
                                    &mut s.notes,
                                    "Put step text in the speaker notes",
                                )
                                .tip("Speaker notes · Each slide's text, top to bottom, in its speaker notes.");
                            }
                            Some(Office::Word) => {
                                crate::controls::dialog_section(ui, "Document", Some(FLATTENED));
                                let w = &mut d.office.word;
                                crate::property_layout::choice(
                                    ui,
                                    "Page size",
                                    &mut w.paper,
                                    &[(Paper::Letter, "Letter"), (Paper::A4, "A4")],
                                    "Page size · The Word document's paper.",
                                );
                                crate::property_layout::choice(
                                    ui,
                                    "Layout",
                                    &mut w.beside,
                                    &[
                                        (false, "Picture, then step text"),
                                        (true, "Step text beside picture"),
                                    ],
                                    "Layout · Where each step's text goes.",
                                );
                                crate::controls::toggle(
                                    ui,
                                    &mut w.contents,
                                    "Start with a title and contents",
                                )
                                .tip("Title and contents · The guide's name and a table of contents built from the step headings.");
                            }
                            None => {}
                        }
                        let office = d.office.kind.is_some();
                        if office {
                            ui.weak(FLATTENED);
                        }
                        if d.options.format == Format::Pdf && !d.print && !office {
                            crate::controls::toggle(
                                ui,
                                &mut d.options.searchable_text,
                                "Searchable text",
                            )
                            .tip("Searchable text · Keeps the words of PDF pages selectable and searchable. Words under a redaction, blur or pixelation are always removed.");
                        }
                        if let Some(part) = d.document.part.clone()
                            && !d.print
                        {
                            crate::controls::dialog_section(ui, &part.label(), None);
                            crate::controls::toggle(
                                ui,
                                &mut d.all_parts,
                                &format!("Include all {} parts in one file", part.total),
                            )
                            .tip("Include all parts · Every part of this PDF, in order, in one file (PDF, PowerPoint or Word). Parts are drawn one at a time, so long documents export without running out of memory. PNG and JPEG become numbered pictures, one per page.");
                        }
                        if d.options.format == Format::Jpeg && !office {
                            crate::property_layout::slider(
                                ui,
                                "Quality",
                                egui::Slider::new(&mut d.options.quality, 1..=100),
                            );
                        }
                        if d.options.format != Format::Png && !office {
                            row(ui, "Background", |ui| {
                                crate::color_widgets::rgb_preview(
                                    ui,
                                    &mut d.options.background,
                                    crate::color_widgets::Preview::Fill,
                                );
                            });
                        }
                        let paged = d.print || (d.options.format == Format::Pdf && !office);
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
                        // Office files carry editable objects, not a stamped picture.
                        if !office { match crate::watermark_ui::section(
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
                        } }
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
                        if d.texture_office != office {
                            d.texture_office = office;
                            d.texture = None;
                        }
                        let watermark = d.options.watermark.clone().filter(|_| !office);
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
                            "What ClipDesigner does when the file is saved.",
                        );
                        d.after.controls(ui);
                        pages.is_ok()
                    })
                    .inner;
                ui.add_space(4.);
                ui.separator();
                let note = office_note(&d);
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
                                pages_ok && d.office.kind.is_none(),
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
                        if let Some(note) = note {
                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| ui.weak(note),
                            );
                        }
                    },
                );
            });
        self.export_preferences.options = d.options.clone();
        self.export_preferences.after = d.after;
        if !d.print {
            self.export_preferences.office = d.office.clone();
        }
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if save && let Some(kind) = d.office.kind {
            self.suspend_global_capture();
            if let Some(mut path) = rfd::FileDialog::new()
                .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Export))
                .set_file_name(format!("guide.{}", kind.extension()))
                .add_filter(kind.label(), &[kind.extension()])
                .save_file()
            {
                if !path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(kind.extension()))
                {
                    path.set_extension(kind.extension());
                }
                let all = d.all_parts && d.document.part.is_some();
                match self.export_office(&d.document, &d.options, &d.office, kind, all, &path) {
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
                    Err(e) => d.error = e,
                }
            }
            self.export_dialog = Some(d);
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
                let joined = d.all_parts && d.document.part.is_some();
                let result = if joined {
                    self.export_all_parts(&d.document, &d.options, &path)
                        .map_err(capture_core::Error::Invalid)
                } else {
                    capture_core::export::encode_document(&d.document, &d.options)
                        .and_then(|bytes| capture_core::export::write(&path, &bytes))
                };
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

impl Editor {
    /// Opens Export with PowerPoint or Word chosen, for the dialog gallery.
    #[cfg(test)]
    pub(crate) fn begin_office_export_for_test(&mut self, kind: Office) {
        self.begin_export(false);
        if let Some(d) = self.export_dialog.as_mut() {
            d.office.kind = Some(kind);
        }
    }
}

/// The footer's count for PowerPoint and Word, as in the mockup
/// ("3 slides · 2 editable", "3 steps · Letter").
fn office_note(d: &Dialog) -> Option<String> {
    use capture_core::export::office::summary;
    let plural = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let mut note = match d.office.kind? {
        Office::PowerPoint => {
            let s = &d.office.slides;
            let (slides, editable) = summary(&d.document, s.each, s.editable);
            if s.editable {
                format!("{} · {editable} editable", plural(slides, "slide"))
            } else {
                format!("{} · pictures only", plural(slides, "slide"))
            }
        }
        Office::Word => {
            let (steps, _) = summary(&d.document, Each::Section, false);
            let paper = match d.office.word.paper {
                Paper::Letter => "Letter",
                Paper::A4 => "A4",
            };
            format!("{} · {paper}", plural(steps, "step"))
        }
    };
    if d.all_parts
        && let Some(part) = &d.document.part
    {
        note = format!("{note} in this part · all {} parts", part.total);
    }
    Some(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powerpoint_and_word_follow_the_mockup_and_write_files() {
        use crate::color_widgets::tests::text_rect;
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
        let out = frame(&mut e);
        for label in ["PNG", "JPEG", "PDF", "PowerPoint", "Word"] {
            assert!(text_rect(&out.shapes, label).is_some(), "{label}");
        }
        e.export_dialog.as_mut().unwrap().office.kind = Some(Office::PowerPoint);
        frame(&mut e);
        let out = frame(&mut e);
        for label in [
            "SLIDES",
            "Slide size",
            "Each slide",
            "Objects",
            "Editable shapes",
            "Pictures only",
            "Put step text in the speaker notes",
        ] {
            assert!(text_rect(&out.shapes, label).is_some(), "{label}");
        }
        assert!(text_rect(&out.shapes, "Quality").is_none());
        let note = office_note(e.export_dialog.as_ref().unwrap()).unwrap();
        assert!(note.starts_with("1 slide · "), "{note}");
        e.export_dialog.as_mut().unwrap().office.kind = Some(Office::Word);
        frame(&mut e);
        let out = frame(&mut e);
        for label in [
            "DOCUMENT",
            "Page size",
            "Layout",
            "Start with a title and contents",
        ] {
            assert!(text_rect(&out.shapes, label).is_some(), "{label}");
        }
        assert_eq!(e.export_preferences.office.kind, Some(Office::Word));
        let dir = tempfile::tempdir().unwrap();
        let d = e.export_dialog.take().unwrap();
        for kind in [Office::PowerPoint, Office::Word] {
            let path = dir.path().join(format!("guide.{}", kind.extension()));
            e.export_office(&d.document, &d.options, &d.office, kind, true, &path)
                .unwrap();
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(&bytes[..2], b"PK", "{kind:?}");
        }
    }

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
        if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_BETA_DIR") {
            dump_meshes(&ctx, &dir, "Export", first, PaintedOutput(output));
        }
    }
}
