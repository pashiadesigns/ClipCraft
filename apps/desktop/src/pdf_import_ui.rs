//! New from PDF (approved `artifacts/pdf-import-proposal`, engine chosen in
//! `artifacts/pdf-engine-spike`). File › New › From PDF, File › Open of a PDF
//! and a PDF dropped on the window all come here.
//!
//! The PDF is opened, drawn and converted off the window thread; the dialog
//! only shows pictures it is sent.
use crate::tips::Tip as _;
use crate::{Editor, FileAction, egui};
use capture_core::pdf_import::{self, ImportAs, OpenError, PdfFile, PictureReason};
use std::path::PathBuf;
use std::sync::{Arc, mpsc};

const WIDTH: f32 = 760.;
const THUMB: u32 = 132;
const PREVIEW: u32 = 640;
const DPI_CHOICES: [u32; 5] = [300, 200, 150, 100, 72];
/// Pictures for pages that can't be drawn faithfully, before fitting the budget.
const FALLBACK_DPI: u32 = 150;

enum Stage {
    Password {
        text: String,
        wrong: bool,
        focus: bool,
    },
    Opening(mpsc::Receiver<Result<PdfFile, OpenError>>),
    Ready,
    Importing {
        rx: mpsc::Receiver<Progress>,
        done: usize,
        total: usize,
    },
    Failed(String),
}
/// The built guides (one, or linked parts), the pages that came in as
/// pictures, and their resolution.
type Imported = (
    Vec<(capture_core::Document, Vec<usize>)>,
    Vec<(usize, PictureReason)>,
    u32,
);
enum Progress {
    Page,
    Done(Result<Imported, String>),
    /// Insert mode: the prepared pages and their PDF sizes.
    Pages(Result<Vec<(capture_core::bitmap::Bitmap, [f64; 2])>, String>),
}

/// Insert › PDF pages (approved beta 6 tab 6): where the pages go in the
/// open guide and how they are sized.
#[derive(Clone)]
pub(crate) struct InsertInto {
    /// The guide page in view (0-based) and the guide's page sizes.
    current: usize,
    sizes: Vec<[u32; 2]>,
    place: Place,
    fit: bool,
}
#[derive(Clone, Copy, PartialEq)]
enum Place {
    After,
    Start,
    End,
}
impl InsertInto {
    fn at(&self) -> usize {
        match self.place {
            Place::After => (self.current + 1).min(self.sizes.len()),
            Place::Start => 0,
            Place::End => self.sizes.len(),
        }
    }
}
/// A picture the worker drew: (page, is the large preview, pixels).
type Drawn = (usize, bool, image::RgbaImage);

pub(crate) struct PdfImport {
    path: PathBuf,
    stage: Stage,
    file: Option<Arc<PdfFile>>,
    selected: Vec<bool>,
    current: usize,
    range: String,
    range_error: bool,
    pictures: bool,
    dpi: Option<u32>,
    scan: bool,
    options: capture_core::privacy::Options,
    phrases: String,
    cover: crate::sensitive_ui::Cover,
    thumbs: Vec<Option<egui::TextureHandle>>,
    preview: Option<(usize, egui::TextureHandle)>,
    drawn: Option<mpsc::Receiver<Drawn>>,
    want_preview: Option<mpsc::Sender<usize>>,
    start: bool,
    /// Set for Insert › PDF pages.
    insert: Option<InsertInto>,
}

/// Paper names for common page sizes in points.
fn paper(points: [f64; 2]) -> String {
    let [w, h] = [points[0].min(points[1]), points[0].max(points[1])];
    for (name, size) in [
        ("Letter", [612., 792.]),
        ("Legal", [612., 1008.]),
        ("Tabloid", [792., 1224.]),
        ("A3", [841.89, 1190.55]),
        ("A4", [595.28, 841.89]),
        ("A5", [419.53, 595.28]),
    ] {
        if (w - size[0]).abs() < 3. && (h - size[1]).abs() < 3. {
            return name.into();
        }
    }
    format!("{:.1} × {:.1} in", points[0] / 72., points[1] / 72.)
}

/// Parses "1-3, 6" (one-based) into a selection of `count` pages.
pub(crate) fn parse_range(text: &str, count: usize) -> Option<Vec<bool>> {
    let mut selected = vec![false; count];
    let mut any = false;
    for part in text.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (a, b) = match part.split_once(['-', '–']) {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: usize = a.parse().ok()?;
        let b: usize = if b.is_empty() { count } else { b.parse().ok()? };
        if a == 0 || b == 0 || a > b || b > count {
            return None;
        }
        for s in &mut selected[a - 1..b] {
            *s = true;
        }
        any = true;
    }
    any.then_some(selected)
}
/// Writes a selection back as a short range like "1-3, 6".
fn describe(selected: &[bool]) -> String {
    let mut parts = vec![];
    let mut i = 0;
    while i < selected.len() {
        if selected[i] {
            let start = i;
            while i + 1 < selected.len() && selected[i + 1] {
                i += 1;
            }
            parts.push(if start == i {
                format!("{}", start + 1)
            } else {
                format!("{}-{}", start + 1, i + 1)
            });
        }
        i += 1;
    }
    parts.join(", ")
}

impl PdfImport {
    pub(crate) fn new(path: PathBuf) -> Self {
        let mut value = Self {
            path,
            stage: Stage::Failed(String::new()),
            file: None,
            selected: vec![],
            current: 0,
            range: String::new(),
            range_error: false,
            pictures: false,
            dpi: None,
            scan: false,
            options: Default::default(),
            phrases: String::new(),
            cover: Default::default(),
            thumbs: vec![],
            preview: None,
            drawn: None,
            want_preview: None,
            start: false,
            insert: None,
        };
        value.open("");
        value
    }
    fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "PDF".into())
    }
    fn open(&mut self, password: &str) {
        let path = self.path.clone();
        let password = password.to_owned();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = match std::fs::metadata(&path) {
                Ok(m) if m.len() > pdf_import::MAX_FILE_BYTES as u64 => Err(OpenError::TooLarge(
                    "PDF files must be under 256 MB.".into(),
                )),
                Ok(_) => match std::fs::read(&path) {
                    Ok(bytes) => pdf_import::open(bytes, &password),
                    Err(e) => Err(OpenError::Damaged(format!(
                        "The file couldn't be read: {e}"
                    ))),
                },
                Err(e) => Err(OpenError::Damaged(format!(
                    "The file couldn't be read: {e}"
                ))),
            };
            let _ = tx.send(result);
        });
        self.stage = Stage::Opening(rx);
    }
    fn opened(&mut self, file: PdfFile, ctx: &egui::Context) {
        let count = file.page_count();
        let file = Arc::new(file);
        self.selected = vec![true; count];
        self.range = describe(&self.selected);
        self.current = 0;
        self.thumbs = vec![None; count];
        self.preview = None;
        // One worker draws the large preview first, then the thumbnails.
        let (tx, rx) = mpsc::channel::<Drawn>();
        let (want_tx, want_rx) = mpsc::channel::<usize>();
        let worker = file.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut next_thumb = 0;
            loop {
                let mut wanted = None;
                while let Ok(i) = want_rx.try_recv() {
                    wanted = Some(i);
                }
                if let Some(i) = wanted {
                    let pixels = worker.thumbnail(i, PREVIEW);
                    if tx.send((i, true, pixels)).is_err() {
                        return;
                    }
                    repaint.request_repaint();
                    continue;
                }
                if next_thumb < count {
                    let pixels = worker.thumbnail(next_thumb, THUMB);
                    if tx.send((next_thumb, false, pixels)).is_err() {
                        return;
                    }
                    next_thumb += 1;
                    repaint.request_repaint();
                    continue;
                }
                match want_rx.recv() {
                    Ok(i) => {
                        let pixels = worker.thumbnail(i, PREVIEW);
                        if tx.send((i, true, pixels)).is_err() {
                            return;
                        }
                        repaint.request_repaint();
                    }
                    Err(_) => return,
                }
            }
        });
        let _ = want_tx.send(0);
        self.drawn = Some(rx);
        self.want_preview = Some(want_tx);
        self.file = Some(file);
        self.stage = Stage::Ready;
        self.dpi = None;
    }
    fn show_page(&mut self, page: usize) {
        if page < self.selected.len() && page != self.current {
            self.current = page;
            if let Some(tx) = &self.want_preview {
                let _ = tx.send(page);
            }
        }
    }
    fn chosen(&self) -> Vec<usize> {
        (0..self.selected.len())
            .filter(|i| self.selected[*i])
            .collect()
    }
    /// The chosen pages split into linked parts that each fit one guide
    /// (approved beta 6 tab 7); one run when they all fit.
    fn parts(&self) -> Vec<std::ops::Range<usize>> {
        let Some(file) = &self.file else {
            return vec![];
        };
        let sizes: Vec<_> = self.chosen().iter().map(|i| file.page_pixels(*i)).collect();
        pdf_import::plan_parts(&sizes)
    }
    /// Picture resolutions that fit one part's picture budget.
    fn dpi_fits(&self, dpi: u32) -> bool {
        let Some(file) = &self.file else { return false };
        let chosen = self.chosen();
        let first = self.parts().first().cloned().unwrap_or(0..0);
        let pixels: f64 = chosen[first]
            .iter()
            .map(|i| {
                let [w, h] = file.page_points(*i);
                w * h * (f64::from(dpi) / 72.).powi(2)
            })
            .sum();
        pixels <= capture_core::MAX_PIXELS as f64
    }
    fn best_dpi(&self) -> u32 {
        DPI_CHOICES
            .into_iter()
            .find(|d| self.dpi_fits(*d))
            .unwrap_or(72)
    }
    fn title(&self) -> &'static str {
        if self.insert.is_some() {
            "Insert PDF pages"
        } else {
            "New from PDF"
        }
    }
    /// Insert mode: prepares the chosen pages without building a guide.
    fn import_pages(&mut self, ctx: &egui::Context) {
        let Some(file) = self.file.clone() else {
            return;
        };
        let pages = self.chosen();
        // Pictures share the open document's picture budget, so they come in
        // at screen resolution.
        let how = if self.pictures {
            ImportAs::Pictures { dpi: 96 }
        } else {
            ImportAs::Editable { fallback_dpi: 96 }
        };
        let total = pages.len();
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let mut out = vec![];
            let mut result = Ok(());
            for i in &pages {
                match file.prepare(*i, how) {
                    Ok(p) => out.push((p.bitmap, p.points)),
                    Err(e) => {
                        result = Err(e.to_string());
                        break;
                    }
                }
                let _ = tx.send(Progress::Page);
                repaint.request_repaint();
            }
            let _ = tx.send(Progress::Pages(result.map(|()| out)));
            repaint.request_repaint();
        });
        self.stage = Stage::Importing { rx, done: 0, total };
    }
    fn import(&mut self, ctx: &egui::Context) {
        if self.insert.is_some() {
            self.import_pages(ctx);
            return;
        }
        let Some(file) = self.file.clone() else {
            return;
        };
        let pages = self.chosen();
        let how = if self.pictures {
            ImportAs::Pictures {
                dpi: self
                    .dpi
                    .filter(|d| self.dpi_fits(*d))
                    .unwrap_or(self.best_dpi()),
            }
        } else {
            ImportAs::Editable {
                fallback_dpi: FALLBACK_DPI,
            }
        };
        let total = pages.len();
        let title = std::path::Path::new(&self.name())
            .file_stem()
            .map_or_else(|| "PDF".to_owned(), |s| s.to_string_lossy().into_owned());
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let sizes: Vec<_> = pages.iter().map(|i| file.page_pixels(*i)).collect();
            let runs = pdf_import::plan_parts(&sizes);
            let set = pdf_import::new_set_id();
            let total_parts = runs.len() as u32;
            let mut parts = vec![];
            let mut all_notes = vec![];
            let mut lowest = u32::MAX;
            let mut result: Result<(), String> = Ok(());
            for (k, run) in runs.into_iter().enumerate() {
                let pages: Vec<usize> = pages[run].to_vec();
                let built = (|| {
                    let mut prepared = vec![];
                    for i in &pages {
                        prepared.push(file.prepare(*i, how).map_err(|e| e.to_string())?);
                        let _ = tx.send(Progress::Page);
                        repaint.request_repaint();
                    }
                    // Pictures share the document's picture budget with the drawings;
                    // lower their resolution until everything fits.
                    let mut dpi = match how {
                        ImportAs::Pictures { dpi } => dpi,
                        ImportAs::Editable { fallback_dpi } => fallback_dpi,
                    };
                    loop {
                        let used: u64 = prepared
                            .iter()
                            .map(|p| {
                                let [w, h] = p.bitmap.size();
                                u64::from(w) * u64::from(h)
                            })
                            .sum();
                        if used <= capture_core::MAX_PIXELS || dpi <= 48 {
                            break;
                        }
                        dpi = (dpi as f64
                            * (capture_core::MAX_PIXELS as f64 / used as f64).sqrt()
                            * 0.97)
                            .floor()
                            .max(48.) as u32;
                        for page in &mut prepared {
                            if page.picture.is_some() {
                                let again = match how {
                                    ImportAs::Pictures { .. } => ImportAs::Pictures { dpi },
                                    ImportAs::Editable { .. } => {
                                        ImportAs::Editable { fallback_dpi: dpi }
                                    }
                                };
                                let reason = page.picture;
                                let mut redone =
                                    file.prepare(page.index, again).map_err(|e| e.to_string())?;
                                if redone.picture.is_none() {
                                    redone.picture = reason;
                                }
                                *page = redone;
                            }
                        }
                    }
                    let notes: Vec<_> = prepared
                        .iter()
                        .filter_map(|p| p.picture.map(|r| (p.index, r)))
                        .collect();
                    let mut document =
                        pdf_import::build_guide(prepared).map_err(|e| e.to_string())?;
                    if total_parts > 1 {
                        document.part = Some(pdf_import::PartOf {
                            set: set.clone(),
                            index: k as u32 + 1,
                            total: total_parts,
                            title: title.clone(),
                        });
                    }
                    Ok::<_, String>((document, notes, dpi))
                })();
                match built {
                    Ok((document, notes, dpi)) => {
                        parts.push((document, pages));
                        all_notes.extend(notes);
                        lowest = lowest.min(dpi);
                    }
                    Err(e) => {
                        result = Err(e);
                        break;
                    }
                }
            }
            let result = result.map(|()| (parts, all_notes, lowest.min(300)));
            let _ = tx.send(Progress::Done(result));
            repaint.request_repaint();
        });
        self.stage = Stage::Importing { rx, done: 0, total };
    }
}

/// The note shown once after import for pages that came in as pictures.
fn picture_note(notes: &[(usize, PictureReason)], dpi: u32) -> Option<String> {
    let list = |reason| -> Vec<String> {
        notes
            .iter()
            .filter(|(_, r)| *r == reason)
            .map(|(i, _)| (i + 1).to_string())
            .collect()
    };
    let (scans, effects) = (list(PictureReason::Scan), list(PictureReason::Effects));
    let mut parts = vec![];
    match scans.len() {
        0 => {}
        1 => parts.push(format!(
            "Page {} is scanned and came in as a picture; Find Sensitive Info reads it with OCR.",
            scans[0]
        )),
        _ => parts.push(format!(
            "Pages {} are scanned and came in as pictures; Find Sensitive Info reads them with OCR.",
            scans.join(", ")
        )),
    }
    match effects.len() {
        0 => {}
        1 => parts.push(format!(
            "Page {} uses effects ClipDesigner can't rebuild exactly, so it came in as a {dpi} DPI picture.",
            effects[0]
        )),
        _ => parts.push(format!(
            "Pages {} use effects ClipDesigner can't rebuild exactly, so they came in as {dpi} DPI pictures.",
            effects.join(", ")
        )),
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}
impl Editor {
    /// Insert › PDF pages into the open guide (approved beta 6 tab 6).
    pub(crate) fn insert_pdf_pages(&mut self) {
        let d = &self.session.document;
        let Some(guide) = d.guide.as_ref().filter(|g| g.pages.is_some()) else {
            self.status =
                "Insert PDF pages works in a guide with pages. Use File › New › From PDF for a new guide."
                    .into();
            return;
        };
        let count = guide.page_count();
        let sizes: Vec<[u32; 2]> = (0..count)
            .map(|i| guide.page_size(i, [d.width, d.height]))
            .collect();
        let point = self
            .selected_annotation()
            .map(|a| a.start)
            .or_else(|| {
                self.view
                    .current_transform()
                    .map(|t| t.image_point(t.viewport.center()))
            })
            .unwrap_or([0.; 2]);
        let current = guide.page_at(point);
        self.suspend_global_capture();
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Open))
            .add_filter("PDF documents", &["pdf"])
            .pick_file()
        {
            self.remember_dialog_folder(capture_core::quick_preferences::Folder::Open, &path);
            let mut draft = PdfImport::new(path);
            draft.insert = Some(InsertInto {
                current,
                sizes,
                place: Place::After,
                fit: false,
            });
            self.pdf_import = Some(draft);
        }
    }
    fn finish_insert(
        &mut self,
        draft: PdfImport,
        pages: Vec<(capture_core::bitmap::Bitmap, [f64; 2])>,
    ) {
        let Some(into) = draft.insert else { return };
        let count = pages.len();
        let pages: Vec<_> = pages
            .into_iter()
            .map(|(b, p)| (std::sync::Arc::new(b), p))
            .collect();
        match self.session.execute(capture_core::Command::InsertPdfPages {
            pages,
            at: into.at(),
            fit: into.fit,
        }) {
            Ok(()) => {
                self.needs_render = true;
                self.status = format!(
                    "Inserted {count} PDF {}. Undo removes {}.",
                    if count == 1 { "page" } else { "pages" },
                    if count == 1 { "it" } else { "them" }
                );
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    /// File › New › From PDF: Windows' picker filtered to PDF files.
    pub(crate) fn new_from_pdf(&mut self) {
        self.suspend_global_capture();
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Open))
            .add_filter("PDF documents", &["pdf"])
            .pick_file()
        {
            self.remember_dialog_folder(capture_core::quick_preferences::Folder::Open, &path);
            self.pdf_import = Some(PdfImport::new(path));
        }
    }
    /// A PDF dropped anywhere on an image window opens the same dialog.
    pub(crate) fn take_dropped_pdf(&mut self, ctx: &egui::Context) {
        if self.is_video()
            || self.pdf_import.is_some()
            || ctx.memory(|m| m.top_modal_layer().is_some())
        {
            return;
        }
        let path = ctx.input_mut(|i| {
            let at = i.raw.dropped_files.iter().position(|f| is_pdf(f.path()))?;
            Some(i.raw.dropped_files.remove(at).path().to_path_buf())
        });
        if let Some(path) = path {
            self.pdf_import = Some(PdfImport::new(path));
        }
    }
    pub(crate) fn show_pdf_import(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.pdf_import else {
            return;
        };
        draft.options.custom = self.quick_controls.sensitive_types();
        // Pictures the worker finished.
        if let Some(rx) = &draft.drawn {
            while let Ok((page, large, pixels)) = rx.try_recv() {
                let texture = ctx.load_texture(
                    format!("pdf-{}-{page}", if large { "preview" } else { "thumb" }),
                    egui::ColorImage::from_rgba_unmultiplied(
                        [pixels.width() as usize, pixels.height() as usize],
                        pixels.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                );
                if large {
                    draft.preview = Some((page, texture));
                } else if let Some(slot) = draft.thumbs.get_mut(page) {
                    *slot = Some(texture);
                }
            }
        }
        let mut done = None;
        let mut inserted = None;
        match &mut draft.stage {
            Stage::Opening(rx) => match rx.try_recv() {
                Ok(Ok(file)) => draft.opened(file, ctx),
                Ok(Err(OpenError::NeedsPassword)) => {
                    draft.stage = Stage::Password {
                        text: String::new(),
                        wrong: false,
                        focus: true,
                    }
                }
                Ok(Err(OpenError::WrongPassword)) => {
                    draft.stage = Stage::Password {
                        text: String::new(),
                        wrong: true,
                        focus: true,
                    }
                }
                Ok(Err(e)) => draft.stage = Stage::Failed(e.to_string()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    draft.stage = Stage::Failed("The PDF couldn't be opened.".into())
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint(),
            },
            Stage::Importing {
                rx, done: count, ..
            } => loop {
                match rx.try_recv() {
                    Ok(Progress::Page) => *count += 1,
                    Ok(Progress::Done(result)) => {
                        done = Some(result);
                        break;
                    }
                    Ok(Progress::Pages(result)) => {
                        inserted = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        done = Some(Err("The import stopped. Try again.".into()));
                        break;
                    }
                }
            },
            _ => {}
        }
        if let Some(result) = inserted {
            match result {
                Ok(pages) => {
                    let draft = self.pdf_import.take().unwrap();
                    self.finish_insert(draft, pages);
                }
                Err(e) => {
                    if let Some(d) = &mut self.pdf_import {
                        d.stage = Stage::Failed(e);
                    }
                }
            }
            return;
        }
        if let Some(result) = done {
            match result {
                Ok((parts, notes, dpi)) => {
                    let draft = self.pdf_import.take().unwrap();
                    self.finish_pdf_import(draft, parts, &notes, dpi, ctx);
                }
                Err(e) => {
                    if let Some(d) = &mut self.pdf_import {
                        d.stage = Stage::Failed(e);
                    }
                }
            }
            return;
        }
        let draft = self.pdf_import.as_mut().unwrap();
        let mut close = false;
        let mut change_file = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("new_from_pdf")).show(ctx, |ui| {
            let width = WIDTH.min((ctx.content_rect().width() - 48.).max(360.));
            let title = draft.title();
            match &mut draft.stage {
                Stage::Password { text, wrong, focus } => {
                    ui.set_width(380.);
                    crate::controls::dialog_title(ui, "Password-protected PDF");
                    ui.label(format!(
                        "\"{}\" is password-protected. Enter its password to open it. ClipDesigner doesn't keep the password, and the guide you create is not protected.",
                        draft
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    ));
                    crate::controls::dialog_section(ui, "Password", None);
                    let field = ui.add(
                        egui::TextEdit::singleline(text)
                            .password(true)
                            .desired_width(f32::INFINITY),
                    );
                    crate::controls::focus_once(&field, focus);
                    if *wrong {
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 170, 160),
                            "That password isn't right.",
                        );
                    }
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let (open, cancel) = crate::controls::dialog_actions(
                        ui,
                        "Open",
                        "Cancel",
                        !text.is_empty(),
                        false,
                    );
                    close |= cancel;
                    if (open || enter) && !text.is_empty() {
                        let password = std::mem::take(text);
                        draft.open(&password);
                    }
                }
                Stage::Opening(_) => {
                    ui.set_width(380.);
                    crate::controls::dialog_title(ui, title);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Opening {}…", draft.name()));
                    });
                    close |= ui
                        .add(crate::controls::dialog_button("Cancel", false))
                        .clicked();
                }
                Stage::Failed(message) => {
                    ui.set_width(380.);
                    crate::controls::dialog_title(ui, title);
                    let message = if message.is_empty() {
                        "The PDF couldn't be opened.".to_owned()
                    } else {
                        message.clone()
                    };
                    let mut chars = message.chars();
                    let message = match chars.next() {
                        Some(first) => first.to_lowercase().chain(chars).collect::<String>(),
                        None => message,
                    };
                    ui.label(format!("\"{}\" couldn't be opened: {message}", draft.name()));
                    ui.add_space(6.);
                    let (ok, other) =
                        crate::controls::dialog_actions(ui, "OK", "Change file…", true, false);
                    close |= ok;
                    change_file |= other;
                }
                Stage::Importing { done, total, .. } => {
                    ui.set_width(380.);
                    crate::controls::dialog_title(ui, title);
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!(
                            "Preparing page {} of {total}…",
                            (*done + 1).min(*total)
                        ));
                    });
                    ui.add(egui::ProgressBar::new(*done as f32 / (*total).max(1) as f32));
                }
                Stage::Ready => {
                    ui.set_width(width);
                    let (c, f) = ready(ui, draft);
                    close |= c;
                    change_file |= f;
                }
            }
        });
        if !matches!(draft.stage, Stage::Importing { .. }) {
            close |= crate::text_edit::cancel_on_escape(&modal);
        }

        if change_file {
            self.pdf_import = None;
            self.new_from_pdf();
        } else if close {
            self.pdf_import = None;
        } else if self
            .pdf_import
            .as_ref()
            .is_some_and(|d| matches!(d.stage, Stage::Ready) && d.start)
        {
            let d = self.pdf_import.as_mut().unwrap();
            d.start = false;
            d.import(ctx);
        }
    }
    fn finish_pdf_import(
        &mut self,
        draft: PdfImport,
        parts: Vec<(capture_core::Document, Vec<usize>)>,
        notes: &[(usize, PictureReason)],
        dpi: u32,
        ctx: &egui::Context,
    ) {
        // "Vendor packet.pdf · pages 1–3, 6", or for a part
        // "Vendor packet · part 2 of 10 · pages 25–48".
        let label = |document: &capture_core::Document, pages: &[usize]| {
            let mut selected = vec![false; draft.selected.len()];
            for p in pages {
                selected[*p] = true;
            }
            let range = describe(&selected).replace('-', "–");
            let pages_word = if pages.len() == 1 { "page" } else { "pages" };
            match &document.part {
                Some(part) => format!("{} · {pages_word} {range}", part.label()),
                None => format!("{} · {pages_word} {range}", draft.name()),
            }
        };
        let mut parts = parts.into_iter();
        let Some((first, first_pages)) = parts.next() else {
            return;
        };
        let first_label = label(&first, &first_pages);
        let more: Vec<_> = parts
            .map(|(d, p)| {
                let l = label(&d, &p);
                (d, l)
            })
            .collect();
        let session = match capture_core::Session::unsaved(first) {
            Ok(s) => s,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        let mut note = picture_note(notes, dpi);
        if !more.is_empty() {
            let parts_note = format!(
                "Made {} linked parts; the others are in the Library. Export can put every part into one file.",
                more.len() + 1
            );
            note = Some(match note {
                Some(n) => format!("{parts_note} {n}"),
                None => parts_note,
            });
        }
        let label = first_label;
        let scan = draft
            .scan
            .then(|| (draft.options.clone(), draft.phrases.clone(), draft.cover));
        self.request_action(
            FileAction::FromPdf {
                session,
                label,
                note,
                scan,
                more,
            },
            ctx,
        );
    }
}

fn is_pdf(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}
pub(crate) fn is_pdf_path(path: &std::path::Path) -> bool {
    is_pdf(path)
}

/// The main view: file row, page picker and preview, Import as, Guide and
/// the shared scan controls. Returns (close, change file).
fn ready(ui: &mut egui::Ui, draft: &mut PdfImport) -> (bool, bool) {
    let mut close = false;
    let mut change_file = false;
    let Some(file) = draft.file.clone() else {
        return (true, false);
    };
    let count = file.page_count();
    crate::controls::dialog_title(ui, draft.title());
    // File row.
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(draft.name()).strong());
        ui.label(
            egui::RichText::new(format!(
                "{count} {} · {}",
                if count == 1 { "page" } else { "pages" },
                {
                    let first = paper(file.page_points(0));
                    if (1..count.min(400)).all(|i| paper(file.page_points(i)) == first) {
                        first
                    } else {
                        "mixed sizes".into()
                    }
                }
            ))
            .color(egui::Color32::from_rgb(142, 154, 157)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            change_file = ui
                .button("Change file…")
                .tip("Change file · Choose a different PDF.")
                .clicked();
        });
    });
    ui.columns(2, |cols| {
        // Pages.
        let ui = &mut cols[0];
        crate::controls::dialog_section(
            ui,
            "Pages",
            Some("Click a page to include or leave it out; it also shows in the preview. Type a range such as 1-3, 6."),
        );
        ui.horizontal(|ui| {
            if ui.small_button("All").tip("All · Include every page.").clicked() {
                draft.selected.fill(true);
                draft.range = describe(&draft.selected);
                draft.range_error = false;
            }
            if ui.small_button("None").tip("None · Leave out every page.").clicked() {
                draft.selected.fill(false);
                draft.range.clear();
                draft.range_error = false;
            }
            let field = ui.add(
                crate::unicode_input::TextEdit::singleline(&mut draft.range)
                    .hint_text("1-3, 6")
                    .desired_width(ui.available_width()),
            );
            field.clone().tip("Select · Pages to include, such as 1-3, 6.");
            if field.changed() {
                match parse_range(&draft.range, count) {
                    Some(sel) => {
                        draft.selected = sel;
                        draft.range_error = false;
                    }
                    None => draft.range_error = !draft.range.trim().is_empty(),
                }
            }
        });
        if draft.range_error {
            ui.colored_label(
                egui::Color32::from_rgb(255, 170, 160),
                format!("Use page numbers from 1 to {count}, such as 1-3, 6."),
            );
        }
        let columns = 4usize;
        let cell = ((ui.available_width() - 8. * (columns as f32 - 1.)) / columns as f32).max(60.);
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .id_salt("pdf-pages")
            .max_height(300.)
            .auto_shrink([false, true])
            .show_rows(ui, cell * 1.36 + 8., count.div_ceil(columns), |ui, rows| {
                for row in rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;
                        for i in row * columns..((row + 1) * columns).min(count) {
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(cell, cell * 1.36),
                                egui::Sense::click(),
                            );
                            let on = draft.selected[i];
                            let p = ui.painter();
                            p.rect_filled(rect, 6., egui::Color32::from_rgb(30, 37, 40));
                            let inner = egui::Rect::from_min_max(
                                rect.min + egui::vec2(6., 6.),
                                rect.max - egui::vec2(6., 20.),
                            );
                            if let Some(t) = &draft.thumbs[i] {
                                let s = t.size_vec2();
                                let scale = (inner.width() / s.x).min(inner.height() / s.y);
                                let r = egui::Rect::from_center_size(inner.center(), s * scale);
                                p.image(
                                    t.id(),
                                    r,
                                    egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                                    if on {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::from_gray(110)
                                    },
                                );
                            } else {
                                p.rect_filled(inner.shrink(10.), 2., egui::Color32::from_rgb(52, 61, 64));
                            }
                            p.text(
                                rect.center_bottom() - egui::vec2(0., 9.),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", i + 1),
                                egui::FontId::proportional(11.),
                                if on {
                                    egui::Color32::WHITE
                                } else {
                                    egui::Color32::from_rgb(142, 154, 157)
                                },
                            );
                            let stroke = if i == draft.current {
                                egui::Stroke::new(2., crate::shell::ACCENT)
                            } else if on {
                                egui::Stroke::new(1., egui::Color32::from_rgb(86, 160, 190))
                            } else {
                                egui::Stroke::new(1., egui::Color32::from_rgb(60, 70, 74))
                            };
                            p.rect_stroke(rect, 6., stroke, egui::StrokeKind::Inside);
                            if on {
                                let badge = egui::Rect::from_center_size(
                                    rect.right_top() + egui::vec2(-12., 12.),
                                    egui::vec2(14., 14.),
                                );
                                p.rect_filled(badge, 4., crate::shell::ACCENT);
                                p.text(
                                    badge.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "✓",
                                    egui::FontId::proportional(10.),
                                    egui::Color32::WHITE,
                                );
                            }
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::Role::CheckBox,
                                    true,
                                    on,
                                    format!("Page {}", i + 1),
                                )
                            });
                            if response.clicked() {
                                clicked = Some(i);
                            }
                        }
                    });
                    ui.add_space(8.);
                }
            });
        if let Some(i) = clicked {
            if draft.current == i || !draft.selected[i] {
                draft.selected[i] = !draft.selected[i];
            }
            draft.show_page(i);
            draft.range = describe(&draft.selected);
            draft.range_error = false;
        }
        // Preview.
        let ui = &mut cols[1];
        crate::controls::dialog_section(ui, "Preview", None);
        let height = 330.;
        let (area, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
        ui.painter()
            .rect_filled(area, 6., egui::Color32::from_rgb(21, 26, 28));
        if let Some((page, t)) = &draft.preview
            && *page == draft.current
        {
            let s = t.size_vec2();
            let room = area.shrink(10.);
            let scale = (room.width() / s.x).min(room.height() / s.y);
            ui.painter().image(
                t.id(),
                egui::Rect::from_center_size(room.center(), s * scale),
                egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        } else {
            ui.put(area, egui::Spinner::new());
        }
        ui.horizontal(|ui| {
            let prev = ui
                .add_enabled(draft.current > 0, egui::Button::new("‹"))
                .tip("Previous page · Left arrow");
            ui.label(format!("Page {} of {count}", draft.current + 1));
            let next = ui
                .add_enabled(draft.current + 1 < count, egui::Button::new("›"))
                .tip("Next page · Right arrow");
            let typing = ui.ctx().memory(|m| m.focused().is_some());
            let (left, right) = ui.input(|i| {
                (
                    !typing && i.key_pressed(egui::Key::ArrowLeft),
                    !typing && i.key_pressed(egui::Key::ArrowRight),
                )
            });
            if (prev.clicked() || left) && draft.current > 0 {
                draft.show_page(draft.current - 1);
            }
            if (next.clicked() || right) && draft.current + 1 < count {
                draft.show_page(draft.current + 1);
            }
        });
    });
    ui.add_space(4.);
    ui.columns(2, |cols| {
        let ui = &mut cols[0];
        crate::controls::dialog_section(ui, "Import as", None);
        for (pictures, title, detail) in [
            (
                false,
                "Editable pages",
                "Sharp at any zoom. Uses the PDF's own text, so Find Sensitive Info is exact.",
            ),
            (
                true,
                "Pictures",
                "Each page becomes one image at the resolution below. Text is read with OCR.",
            ),
        ] {
            let on = draft.pictures == pictures;
            let response = ui
                .add(egui::Button::selectable(
                    on,
                    egui::RichText::new(format!("{title}\n"))
                        .strong()
                        .append_detail(detail),
                ))
                .tip(format!("{title} · {detail}"));
            if response.clicked() {
                draft.pictures = pictures;
            }
        }
        if draft.pictures {
            let best = draft.best_dpi();
            let mut dpi = draft.dpi.filter(|d| draft.dpi_fits(*d)).unwrap_or(best);
            let labels: Vec<(u32, String)> = DPI_CHOICES
                .into_iter()
                .filter(|d| draft.dpi_fits(*d))
                .map(|d| (d, format!("{d} DPI")))
                .collect();
            let options: Vec<(u32, &str)> =
                labels.iter().map(|(d, l)| (*d, l.as_str())).collect();
            if crate::property_layout::choice(
                ui,
                "Resolution",
                &mut dpi,
                &options,
                "Resolution · How many pixels each inch of the page gets. Resolutions too large for these pages aren't offered.",
            ) {
                draft.dpi = Some(dpi);
            }
        }
        let ui = &mut cols[1];
        if let Some(into) = &mut draft.insert {
            crate::controls::dialog_section(ui, "Into this guide", None);
            let after = format!("After page {} (current)", into.current + 1);
            crate::property_layout::choice(
                ui,
                "Place pages",
                &mut into.place,
                &[
                    (Place::After, after.as_str()),
                    (Place::Start, "At the start"),
                    (Place::End, "At the end"),
                ],
                "Place pages · Where the new pages go in this guide.",
            );
            ui.horizontal(|ui| {
                ui.add_sized([86., 20.], egui::Label::new("Page size"));
                for (fit, label, tip) in [
                    (false, "Match PDF", "Match PDF · Each new page is the size of its PDF page."),
                    (true, "Fit this guide's pages", "Fit this guide's pages · New pages take the size of the guide page where they go."),
                ] {
                    if ui
                        .add(egui::Button::selectable(into.fit == fit, label))
                        .tip(tip)
                        .clicked()
                    {
                        into.fit = fit;
                    }
                }
            });
            return;
        }
        crate::controls::dialog_section(ui, "Guide", None);
        ui.horizontal(|ui| {
            ui.add_sized([86., 20.], egui::Label::new("Page size"));
            ui.label(format!("Match PDF ({})", paper(file.page_points(draft.current))))
                .tip("Page size · Each guide page is the size of its PDF page.");
        });
        crate::controls::dialog_section(ui, "Scan for sensitive info", None);
        crate::controls::toggle(ui, &mut draft.scan, "Scan after import")
            .tip("Scan for sensitive info · Runs on this PC after import. Findings open for review; nothing is covered until you choose.");
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Runs on this PC after import. Findings open for review; nothing is covered until you choose.",
                    )
                    .size(11.5)
                    .color(egui::Color32::from_rgb(142, 154, 157)),
                )
                .wrap(),
            );
        });
    });
    if draft.scan {
        crate::panel_ui::divider(ui);
        let _ = crate::sensitive_ui::look_for(ui, &mut draft.options, &mut draft.phrases);
        crate::sensitive_ui::cover(ui, &mut draft.cover);
    }
    // Footer: what will happen, and the buttons.
    crate::panel_ui::divider(ui);
    if let Some(into) = &draft.insert {
        let chosen: Vec<usize> = draft.chosen();
        let at = into.at();
        let mut sizes = into.sizes.clone();
        let reference = sizes[at.min(sizes.len().saturating_sub(1))];
        let added: Vec<[u32; 2]> = chosen
            .iter()
            .map(|i| {
                if into.fit {
                    reference
                } else {
                    file.page_pixels(*i)
                }
            })
            .collect();
        sizes.splice(at..at, added);
        let fits = pdf_import::pages_that_fit(&sizes) == sizes.len();
        let n = chosen.len();
        let (summary, ok) = if n == 0 {
            ("Choose at least one page.".to_owned(), false)
        } else if !fits {
            (
                "These pages don't fit in this guide. Insert fewer pages, or use File › New › From PDF to make linked parts.".to_owned(),
                false,
            )
        } else {
            (
                format!(
                    "Adds {n} {} · guide becomes {} pages",
                    if n == 1 { "page" } else { "pages" },
                    sizes.len()
                ),
                true,
            )
        };
        ui.label(egui::RichText::new(summary).color(if ok {
            egui::Color32::from_rgb(142, 154, 157)
        } else {
            egui::Color32::from_rgb(255, 170, 160)
        }));
        let action = format!("Insert {n} {}", if n == 1 { "page" } else { "pages" });
        let (create, cancel) = crate::controls::dialog_actions(ui, &action, "Cancel", ok, false);
        close |= cancel;
        if create && ok {
            draft.start = true;
        }
        return (close, change_file);
    }
    let chosen = draft.chosen().len();
    let parts = draft.parts().len();
    let (summary, ok) = if chosen == 0 {
        ("Choose at least one page.".to_owned(), false)
    } else if parts > 1 {
        (
            format!(
                "These {chosen} pages come in as {parts} linked parts of up to {} pages, grouped in the Library. Export can put every part back into one file.",
                draft.parts().iter().map(|r| r.len()).max().unwrap_or(0)
            ),
            true,
        )
    } else {
        (
            format!(
                "{chosen} of {count} {} · {}",
                if count == 1 { "page" } else { "pages" },
                if draft.pictures {
                    "pictures"
                } else {
                    "editable pages"
                }
            ),
            true,
        )
    };
    ui.label(egui::RichText::new(summary).color(if ok {
        egui::Color32::from_rgb(142, 154, 157)
    } else {
        egui::Color32::from_rgb(255, 170, 160)
    }));
    let action = if parts > 1 {
        format!("Create {parts} linked parts")
    } else {
        format!(
            "Create guide with {chosen} {}",
            if chosen == 1 { "page" } else { "pages" }
        )
    };
    let (create, cancel) = crate::controls::dialog_actions(ui, &action, "Cancel", ok, false);
    close |= cancel;
    if create && ok {
        draft.start = true;
    }
    (close, change_file)
}

/// Rich text with a smaller, muted second part.
trait Detail {
    fn append_detail(self, detail: &str) -> egui::WidgetText;
}
impl Detail for egui::RichText {
    fn append_detail(self, detail: &str) -> egui::WidgetText {
        let mut job = egui::text::LayoutJob::default();
        let style = egui::Style::default();
        self.append_to(
            &mut job,
            &style,
            egui::FontSelection::Default,
            egui::Align::Min,
        );
        egui::RichText::new(detail)
            .size(11.5)
            .color(egui::Color32::from_rgb(170, 182, 186))
            .append_to(
                &mut job,
                &style,
                egui::FontSelection::Default,
                egui::Align::Min,
            );
        job.wrap.max_width = 330.;
        job.into()
    }
}

/// The sample PDF the tests and dialog gallery use (generated content only).
#[cfg(test)]
pub(crate) fn sample_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/core/tests/fixtures/pdf/sample-8-pages.pdf")
}
#[cfg(test)]
impl PdfImport {
    /// Opens a PDF and waits for its preview and thumbnails, for headless tests.
    pub(crate) fn ready_now(path: PathBuf, ctx: &egui::Context) -> Self {
        let mut value = Self::new(path.clone());
        let file = pdf_import::open(std::fs::read(&path).unwrap(), "").unwrap();
        let count = file.page_count();
        value.opened(file, ctx);
        let rx = value.drawn.take().unwrap();
        let mut thumbs = 0;
        let mut preview = false;
        while thumbs < count || !preview {
            let (page, large, pixels) = rx.recv().unwrap();
            let texture = ctx.load_texture(
                format!("pdf-test-{large}-{page}"),
                egui::ColorImage::from_rgba_unmultiplied(
                    [pixels.width() as usize, pixels.height() as usize],
                    pixels.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            if large {
                value.preview = Some((page, texture));
                preview = true;
            } else {
                value.thumbs[page] = Some(texture);
                thumbs += 1;
            }
        }
        value.drawn = Some(rx);
        value
    }
    pub(crate) fn with_password_prompt(mut self) -> Self {
        self.stage = Stage::Password {
            text: String::new(),
            wrong: false,
            focus: true,
        };
        self
    }
    pub(crate) fn with_failure(mut self) -> Self {
        self.stage =
            Stage::Failed("The file is damaged or isn't a PDF. Nothing was imported.".into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_parse_and_print_back() {
        assert_eq!(
            parse_range("1-3, 6", 8).unwrap(),
            vec![true, true, true, false, false, true, false, false]
        );
        assert_eq!(parse_range("2-", 4).unwrap(), vec![false, true, true, true]);
        assert!(parse_range("0", 4).is_none());
        assert!(parse_range("3-1", 4).is_none());
        assert!(parse_range("9", 4).is_none());
        assert!(parse_range("x", 4).is_none());
        assert!(parse_range("", 4).is_none());
        assert_eq!(describe(&[true, true, true, false, false, true]), "1-3, 6");
        assert_eq!(paper([612., 792.]), "Letter");
        assert_eq!(paper([842., 595.]), "A4");
        assert_eq!(paper([300., 200.]), "4.2 × 2.8 in");
    }
    #[test]
    fn importing_the_sample_builds_a_guide_with_one_page_per_pdf_page() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut draft = PdfImport::ready_now(sample_path(), &ctx);
        // Pages 1-3 and 6 (the scan) only, as typed in the range box.
        let dump = std::env::var_os("CLIPDESIGNER_PDF_IMPORT_DEBUG");
        draft.selected = parse_range(if dump.is_some() { "1-8" } else { "1-3, 6" }, 8).unwrap();
        draft.import(&ctx);
        e.pdf_import = Some(draft);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while e.pdf_import.is_some() {
            assert!(std::time::Instant::now() < deadline, "import finished");
            let mut output = ctx.run_ui(Default::default(), |ui| e.show_pdf_import(ui.ctx()));
            output.textures_delta.clear();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let d = &e.session.document;
        if let Some(dir) = dump {
            let dir = PathBuf::from(dir);
            d.render().unwrap().save(dir.join("guide.png")).unwrap();
            let bytes = capture_core::export::encode_document(
                d,
                &capture_core::export::Options {
                    format: capture_core::export::Format::Pdf,
                    ..Default::default()
                },
            )
            .unwrap();
            std::fs::write(dir.join("guide.pdf"), bytes).unwrap();
            std::fs::write(dir.join("status.txt"), &e.status).unwrap();
            return;
        }
        let pages = d.guide.as_ref().unwrap().pages.as_ref().unwrap();
        assert_eq!(pages.count, 4);
        assert_eq!(d.annotations.len(), 4);
        let bitmaps: Vec<_> = d
            .annotations
            .iter()
            .map(|a| a.image.clone().unwrap())
            .collect();
        assert!(bitmaps[0].is_svg() && bitmaps[0].text_layer().is_some());
        assert!(!bitmaps[3].is_svg(), "the scan comes in as a picture");
        assert!(e.status.contains("Page 6 is scanned"), "{}", e.status);
        let words = capture_core::pdf_import::document_words(d).0;
        assert!(
            words
                .iter()
                .any(|w| w.text.contains("dana.reyes@example.com"))
        );
        d.validate().unwrap();
    }
    #[test]
    fn a_long_import_makes_linked_parts_that_export_as_one_pdf() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        let long = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/core/tests/fixtures/pdf/long-100-pages.pdf");
        let mut draft = PdfImport::ready_now(long, &ctx);
        draft.selected = parse_range("1-30", 100).unwrap();
        // 30 Letter pages don't fit one guide: two linked parts.
        assert_eq!(draft.parts().len(), 2);
        draft.import(&ctx);
        e.pdf_import = Some(draft);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
        while e.pdf_import.is_some() {
            assert!(std::time::Instant::now() < deadline, "import finished");
            let mut output = ctx.run_ui(Default::default(), |ui| e.show_pdf_import(ui.ctx()));
            output.textures_delta.clear();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let part = e
            .session
            .document
            .part
            .clone()
            .expect("the first part is open");
        assert_eq!((part.index, part.total), (1, 2));
        assert!(e.status.contains("2 linked parts"), "{}", e.status);
        let siblings = e.part_siblings(&part);
        assert_eq!(siblings.len(), 1, "the second part is in the Library");
        assert!(
            siblings[0].label.contains("part 2 of 2"),
            "{}",
            siblings[0].label
        );
        // Export every part into one PDF.
        let out = tempfile::tempdir().unwrap();
        let path = out.path().join("joined.pdf");
        e.export_all_parts(
            &e.session.document.clone(),
            &capture_core::export::Options {
                format: capture_core::export::Format::Pdf,
                ..Default::default()
            },
            &path,
        )
        .unwrap();
        let joined = pdf_import::open(std::fs::read(&path).unwrap(), "").unwrap();
        assert_eq!(joined.page_count(), 30);
    }
    #[test]
    fn inserting_pdf_pages_from_the_dialog_adds_them_after_the_current_page() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let file = pdf_import::open(std::fs::read(sample_path()).unwrap(), "").unwrap();
        let pages: Vec<_> = (0..2)
            .map(|i| file.prepare(i, ImportAs::Pictures { dpi: 48 }).unwrap())
            .collect();
        e.session = capture_core::Session::new(pdf_import::build_guide(pages).unwrap()).unwrap();
        let mut draft = PdfImport::ready_now(sample_path(), &ctx);
        let d = &e.session.document;
        let g = d.guide.as_ref().unwrap();
        draft.insert = Some(InsertInto {
            current: 0,
            sizes: (0..2)
                .map(|i| g.page_size(i, [d.width, d.height]))
                .collect(),
            place: Place::After,
            fit: true,
        });
        draft.selected = parse_range("4", 8).unwrap();
        draft.import(&ctx);
        e.pdf_import = Some(draft);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while e.pdf_import.is_some() {
            assert!(std::time::Instant::now() < deadline, "insert finished");
            let mut output = ctx.run_ui(Default::default(), |ui| e.show_pdf_import(ui.ctx()));
            output.textures_delta.clear();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(e.status.contains("Inserted 1 PDF page"), "{}", e.status);
        assert_eq!(e.session.document.guide.as_ref().unwrap().page_count(), 3);
        e.execute(capture_core::Command::Undo);
        assert_eq!(e.session.document.guide.as_ref().unwrap().page_count(), 2);
    }
    #[test]
    fn the_note_names_scans_and_effects_once() {
        assert_eq!(picture_note(&[], 150), None);
        let note = picture_note(
            &[(5, PictureReason::Scan), (8, PictureReason::Effects)],
            150,
        )
        .unwrap();
        assert!(note.contains("Page 6 is scanned"), "{note}");
        assert!(note.contains("Page 9 uses effects"), "{note}");
        assert!(note.contains("150 DPI"), "{note}");
    }
}
