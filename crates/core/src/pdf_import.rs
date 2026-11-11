//! New from PDF (approved `artifacts/pdf-import-proposal`). Pages are read
//! with hayro, the pure-Rust engine chosen by the head-to-head test
//! (`artifacts/pdf-engine-spike`).
//!
//! An editable page becomes a drawing (hayro-svg) that stays sharp at any
//! zoom, plus a text layer of the page's own words for Find Sensitive Info.
//! A page that the drawing cannot show faithfully (a scan, or effects such as
//! blend modes) comes in as a picture, and the import says so once.
//!
//! PDFs are untrusted input: the file size is capped, every compressed stream
//! is measured before anything is drawn, and nesting and cycles are rejected
//! by the parser.
use crate::{Document, Error, Result, bitmap::Bitmap};
use hayro::hayro_interpret::font::{FontQuery, Glyph, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::{BfString, CidFamily};
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::object::Object;
use hayro::hayro_syntax::{Filter, LoadPdfError, Pdf};
use hayro::kurbo::{Affine, BezPath, Point, Rect};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::sync::Arc;

/// Largest PDF file ClipDesigner opens.
pub const MAX_FILE_BYTES: usize = 256 * 1024 * 1024;
/// Largest decompressed picture stream (about 10,000 x 10,000 color pixels).
const MAX_IMAGE_STREAM: u64 = 320 * 1024 * 1024;
/// Largest decompressed stream of any other kind (page content, fonts).
const MAX_OTHER_STREAM: u64 = 64 * 1024 * 1024;
/// All decompressed streams together.
const MAX_TOTAL_STREAMS: u64 = 2 * 1024 * 1024 * 1024;
/// Guide pages are laid out at 96 pixels per inch, like New Image.
pub const GUIDE_PPI: f64 = 96.;
/// Share of pixels a drawing may differ from the page before it imports as a picture.
const FAITHFUL_SHARE: f64 = 0.01;
/// A page whose pictures cover this share of it and that has no text is a scan.
const SCAN_COVER: f64 = 0.6;

/// Why a PDF could not be opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The file needs a password (none was given).
    NeedsPassword,
    /// The password given was not right.
    WrongPassword,
    /// The file is damaged, isn't a PDF, or uses something unsupported.
    Damaged(String),
    /// The file is over a safety limit.
    TooLarge(String),
}
impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsPassword => f.write_str("This PDF is password-protected."),
            Self::WrongPassword => f.write_str("That password isn't right."),
            Self::Damaged(m) | Self::TooLarge(m) => f.write_str(m),
        }
    }
}

/// One word of a page's text layer. `rect` is x, y, width, height as
/// fractions of the page, so it follows the page wherever it is placed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerWord {
    pub text: String,
    pub line: u32,
    pub rect: [f32; 4],
}
/// The words of one PDF page.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextLayer {
    pub words: Vec<LayerWord>,
}
impl TextLayer {
    pub const MAX_WORDS: usize = 40_000;
    pub fn validate(&self) -> Result<()> {
        if self.words.len() > Self::MAX_WORDS
            || self.words.iter().any(|w| {
                w.text.is_empty()
                    || w.text.chars().count() > 256
                    || w.text.chars().any(char::is_control)
                    || !w
                        .rect
                        .iter()
                        .all(|v| v.is_finite() && (-0.5..=1.5).contains(v))
                    || w.rect[2] < 0.
                    || w.rect[3] < 0.
            })
        {
            return Err(Error::Invalid("Invalid PDF text layer".into()));
        }
        Ok(())
    }
}

/// How the pages come in (the dialog's Import as).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImportAs {
    /// Drawings with a text layer; pages that can't be drawn faithfully
    /// become pictures at `fallback_dpi`.
    Editable { fallback_dpi: u32 },
    /// Every page is a picture at `dpi`; its text is read with OCR later.
    Pictures { dpi: u32 },
}

/// Why a page came in as a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureReason {
    /// Pictures were asked for.
    Chosen,
    /// The page is a scan: no text, mostly one picture.
    Scan,
    /// The drawing didn't match the page closely enough.
    Effects,
}

/// One page ready to place in a guide.
pub struct PreparedPage {
    pub index: usize,
    /// Page size in PDF points (1/72 inch), after the page's own rotation.
    pub points: [f64; 2],
    pub bitmap: Bitmap,
    /// `None` for an editable drawing.
    pub picture: Option<PictureReason>,
}

/// An open PDF.
pub struct PdfFile {
    pdf: Pdf,
    settings: InterpreterSettings,
}

fn settings() -> InterpreterSettings {
    let mut settings = InterpreterSettings::default();
    let standard = settings.font_resolver.clone();
    // Fonts the PDF doesn't carry: CJK text uses the Windows fonts for its
    // script (the engine test showed nothing drawn without them).
    settings.font_resolver = Arc::new(move |query: &FontQuery| {
        if let FontQuery::Fallback(f) = query
            && let Some(family) = f.character_collection.as_ref().map(|c| &c.family)
        {
            let files: &[&str] = match family {
                CidFamily::AdobeJapan1 if f.is_serif => &["msmincho.ttc", "yumin.ttf"],
                CidFamily::AdobeJapan1 => &["msgothic.ttc", "YuGothR.ttc"],
                CidFamily::AdobeGB1 => &["simsun.ttc", "msyh.ttc"],
                CidFamily::AdobeCNS1 => &["mingliub.ttc", "msjh.ttc"],
                CidFamily::AdobeKorea1 => &["malgun.ttf", "batang.ttc"],
                _ => &[],
            };
            let dir = std::env::var_os("WINDIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| "C:\\Windows".into())
                .join("Fonts");
            for file in files {
                if let Ok(bytes) = std::fs::read(dir.join(file)) {
                    let data: hayro::hayro_interpret::font::FontData = Arc::new(bytes);
                    return Some((data, 0));
                }
            }
        }
        standard(query)
    });
    settings
}

/// Measures every compressed stream before anything is drawn, so a small
/// file that inflates to gigabytes is refused instead of exhausting memory.
fn check_streams(pdf: &Pdf) -> std::result::Result<(), OpenError> {
    let mut total = 0u64;
    for object in pdf.objects() {
        let Object::Stream(stream) = object else {
            continue;
        };
        let filters = stream.filters();
        if filters.first() != Some(&Filter::FlateDecode) {
            continue;
        }
        let image = stream
            .dict()
            .get::<hayro::hayro_syntax::object::Name<'_>>(b"Subtype".as_slice())
            .is_some_and(|n| n.as_str() == "Image");
        let limit = if image {
            MAX_IMAGE_STREAM
        } else {
            MAX_OTHER_STREAM
        };
        let raw = stream.raw_data();
        let mut decoder = flate2::read::ZlibDecoder::new(&raw[..]).take(limit + 1);
        let size = std::io::copy(&mut decoder, &mut std::io::sink()).unwrap_or(0);
        if size > limit {
            return Err(OpenError::TooLarge(
                "This PDF contains a part that expands far beyond its file size, so it wasn't opened.".into(),
            ));
        }
        total += size;
        if total > MAX_TOTAL_STREAMS {
            return Err(OpenError::TooLarge(
                "This PDF expands to more than 2 GB, so it wasn't opened.".into(),
            ));
        }
    }
    Ok(())
}

/// Opens a PDF from its bytes. `password` is used only to open it; it is not kept.
pub fn open(bytes: Vec<u8>, password: &str) -> std::result::Result<PdfFile, OpenError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(OpenError::TooLarge(
            "PDF files must be under 256 MB.".into(),
        ));
    }
    let damaged =
        || OpenError::Damaged("The file is damaged or isn't a PDF. Nothing was imported.".into());
    if !bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|w| w == b"%PDF-")
    {
        return Err(damaged());
    }
    let pdf = match Pdf::new_with_password(bytes, password) {
        Ok(pdf) => pdf,
        Err(LoadPdfError::Decryption(e)) => {
            use hayro::hayro_syntax::DecryptionError as D;
            return Err(match e {
                D::PasswordProtected if password.is_empty() => OpenError::NeedsPassword,
                D::PasswordProtected => OpenError::WrongPassword,
                D::UnsupportedAlgorithm => OpenError::Damaged(
                    "This PDF uses a kind of protection ClipDesigner can't open.".into(),
                ),
                _ => damaged(),
            });
        }
        Err(LoadPdfError::Invalid) => return Err(damaged()),
    };
    if pdf.pages().is_empty() {
        return Err(OpenError::Damaged("This PDF has no pages.".into()));
    }
    check_streams(&pdf)?;
    Ok(PdfFile {
        pdf,
        settings: settings(),
    })
}

impl PdfFile {
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }
    /// Page size in points after the page's rotation.
    pub fn page_points(&self, index: usize) -> [f64; 2] {
        let (w, h) = self.pdf.pages()[index].render_dimensions();
        [f64::from(w).max(1.), f64::from(h).max(1.)]
    }
    /// The page's size in guide pixels (96 per inch).
    pub fn page_pixels(&self, index: usize) -> [u32; 2] {
        guide_pixels(self.page_points(index))
    }
    /// Draws a page with its longest side `max_side` pixels, on white.
    pub fn thumbnail(&self, index: usize, max_side: u32) -> RgbaImage {
        let [w, h] = self.page_points(index);
        self.render(index, f64::from(max_side) / w.max(h))
    }
    /// Draws a page at `scale` pixels per point, on white.
    pub fn render(&self, index: usize, scale: f64) -> RgbaImage {
        use hayro::vello_cpu::color::palette::css::WHITE;
        let page = &self.pdf.pages()[index];
        let [w, h] = self.page_points(index);
        // Keep a picture within the document's size limits.
        let scale = scale
            .min(16384. / w.max(h))
            .min((crate::MAX_PIXELS as f64 / (w * h)).sqrt());
        let pixmap = hayro::render(
            page,
            &hayro::RenderCache::new(),
            &self.settings,
            &hayro::RenderSettings::default(),
            // The engine truncates the size; nudge so 792 pt at 150 DPI is 1650 px.
            &hayro::PixmapSettings {
                x_scale: (scale * (1. + 1e-5)) as f32,
                y_scale: (scale * (1. + 1e-5)) as f32,
                bg_color: WHITE,
            },
        );
        let (pw, ph) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
        let mut out = RgbaImage::from_raw(pw, ph, pixmap.data_as_u8_slice().to_vec())
            .unwrap_or_else(|| RgbaImage::new(1, 1));
        // vello_cpu keeps premultiplied pixels; the page is opaque white below,
        // so only partly covered edge pixels need care.
        for p in out.pixels_mut() {
            if p[3] != 255 && p[3] != 0 {
                let a = f32::from(p[3]) / 255.;
                for c in 0..3 {
                    p[c] = (f32::from(p[c]) / a).round().min(255.) as u8;
                }
            }
        }
        out
    }
    /// Reads the page's words and what its pictures cover.
    fn probe(&self, index: usize) -> Probe {
        let page = &self.pdf.pages()[index];
        let [w, h] = self.page_points(index);
        let cache = InterpreterCache::new();
        let mut ctx = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0., 0., w, h),
            &cache,
            page.xref(),
            self.settings.clone(),
        );
        let mut probe = Probe::default();
        interpret_page(page, &mut ctx, &mut probe);
        probe
    }
    /// The page's words as a text layer.
    pub fn text_layer(&self, index: usize) -> TextLayer {
        let [w, h] = self.page_points(index);
        layer(&self.probe(index).glyphs, [w, h])
    }
    /// Converts one page as asked.
    pub fn prepare(&self, index: usize, how: ImportAs) -> Result<PreparedPage> {
        let points = self.page_points(index);
        let size = guide_pixels(points);
        let picture = |dpi: u32, text: Option<TextLayer>, reason| -> Result<PreparedPage> {
            let pixels = self.render(index, f64::from(dpi) / 72.);
            Ok(PreparedPage {
                index,
                points,
                bitmap: Bitmap::from_pdf_picture(&pixels, text)?,
                picture: Some(reason),
            })
        };
        let fallback_dpi = match how {
            ImportAs::Pictures { dpi } => return picture(dpi, None, PictureReason::Chosen),
            ImportAs::Editable { fallback_dpi } => fallback_dpi,
        };
        let probe = self.probe(index);
        let words = layer(&probe.glyphs, points);
        if words.words.is_empty() && probe.image_cover >= SCAN_COVER * points[0] * points[1] {
            return picture(fallback_dpi, None, PictureReason::Scan);
        }
        let text = (!words.words.is_empty()).then_some(words);
        let svg = hayro_svg::convert(
            &self.pdf.pages()[index],
            &hayro_svg::RenderCache::new(),
            &self.settings,
            &hayro_svg::SvgRenderSettings::default(),
        );
        if svg.len() > crate::vector_import::MAX_PDF_PAGE_SVG_BYTES || !self.faithful(index, &svg) {
            return picture(fallback_dpi, text, PictureReason::Effects);
        }
        Ok(PreparedPage {
            index,
            points,
            bitmap: Bitmap::from_pdf_page(svg, size, text)?,
            picture: None,
        })
    }
    /// Whether the drawing shows the page as the engine draws it.
    fn faithful(&self, index: usize, svg: &str) -> bool {
        let [w, h] = self.page_points(index);
        let scale = 900. / w.max(h);
        let reference = self.render(index, scale);
        let Ok(mut drawn) =
            crate::vector_import::render_pdf_page(svg, [reference.width(), reference.height()])
        else {
            return false;
        };
        for p in drawn.pixels_mut() {
            // Flatten on white like the reference.
            let a = u16::from(p[3]);
            for c in 0..3 {
                p[c] = ((u16::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
            }
            p[3] = 255;
        }
        share_different(&reference, &drawn) <= FAITHFUL_SHARE
    }
}

/// Share of pixels that differ by more than 40 levels after a light blur,
/// so anti-aliasing differences don't count.
fn share_different(a: &RgbaImage, b: &RgbaImage) -> f64 {
    let a = image::imageops::blur(a, 1.);
    let b = image::imageops::blur(b, 1.);
    let mut off = 0usize;
    for (p, q) in a.pixels().zip(b.pixels()) {
        if (0..3).any(|c| (i16::from(p[c]) - i16::from(q[c])).abs() > 40) {
            off += 1;
        }
    }
    off as f64 / (a.width() as f64 * a.height() as f64).max(1.)
}

/// A page's size in guide pixels.
pub fn guide_pixels(points: [f64; 2]) -> [u32; 2] {
    points.map(|v| ((v * GUIDE_PPI / 72.).round() as u32).clamp(128, 32768))
}

#[derive(Default)]
struct Probe {
    /// Character and box (left, top, right, bottom in points from the top left).
    glyphs: Vec<(String, [f64; 4])>,
    image_cover: f64,
    last: Option<(usize, Affine)>,
}
impl<'a> Device<'a> for Probe {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        let glyphs = run.glyphs();
        let Some(first) = glyphs.first() else { return };
        let key = (glyphs.len(), props.transform * first.transform());
        // Fill-and-stroke text arrives twice; keep it once.
        if matches!(mode, DrawMode::Stroke(_)) && self.last == Some(key) {
            return;
        }
        self.last = Some(key);
        if self.glyphs.len() > TextLayer::MAX_WORDS * 8 {
            return;
        }
        for g in glyphs {
            let t = props.transform * g.transform();
            let advance = match &**g {
                Glyph::Outline(o) => f64::from(o.advance_width().unwrap_or(500.)),
                Glyph::Type3(_) => 600.,
            }
            .clamp(1., 3000.);
            let corners = [
                Point::new(0., -200.),
                Point::new(advance, -200.),
                Point::new(0., 800.),
                Point::new(advance, 800.),
            ]
            .map(|p| t * p);
            let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
            for p in corners {
                b = [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)];
            }
            if !b.iter().all(|v| v.is_finite()) {
                continue;
            }
            let text = match g.as_unicode() {
                Some(BfString::Char(c)) => c.to_string(),
                Some(BfString::String(s)) => s,
                None => continue,
            };
            self.glyphs.push((text, b));
        }
    }
    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        let (w, h) = (f64::from(image.width()), f64::from(image.height()));
        let quad = [
            Point::new(0., 0.),
            Point::new(w, 0.),
            Point::new(w, h),
            Point::new(0., h),
        ]
        .map(|p| props.transform * p);
        let mut area = 0.;
        for i in 0..4 {
            let (p, q) = (quad[i], quad[(i + 1) % 4]);
            area += p.x * q.y - q.x * p.y;
        }
        self.image_cover += (area / 2.).abs();
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// Groups characters into words and lines, as fractions of the page.
fn layer(glyphs: &[(String, [f64; 4])], page: [f64; 2]) -> TextLayer {
    let mut words: Vec<LayerWord> = vec![];
    let mut current: Option<(String, [f64; 4])> = None;
    let mut line = 0u32;
    let mut previous: Option<[f64; 4]> = None;
    let mut flush = |current: &mut Option<(String, [f64; 4])>, line: u32| {
        if let Some((text, b)) = current.take() {
            let text = text.trim().to_owned();
            if !text.is_empty() && words.len() < TextLayer::MAX_WORDS {
                words.push(LayerWord {
                    text: text.chars().take(256).collect(),
                    line,
                    rect: [
                        (b[0] / page[0]) as f32,
                        (b[1] / page[1]) as f32,
                        ((b[2] - b[0]) / page[0]) as f32,
                        ((b[3] - b[1]) / page[1]) as f32,
                    ],
                });
            }
        }
    };
    for (text, b) in glyphs {
        let height = (b[3] - b[1]).max(0.1);
        if let Some(p) = previous {
            let ph = (p[3] - p[1]).max(0.1);
            let same_line = (b[3] - p[3]).abs() < 0.5 * height.min(ph) && b[0] > p[0] - 0.5 * ph;
            if !same_line {
                flush(&mut current, line);
                line += 1;
            } else if b[0] - p[2] > 0.25 * height.min(ph) {
                flush(&mut current, line);
            }
        }
        previous = Some(*b);
        if text.chars().all(char::is_whitespace) {
            flush(&mut current, line);
            continue;
        }
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        match &mut current {
            Some((t, r)) => {
                t.push_str(&text);
                *r = [
                    r[0].min(b[0]),
                    r[1].min(b[1]),
                    r[2].max(b[2]),
                    r[3].max(b[3]),
                ];
            }
            None => current = Some((text, *b)),
        }
    }
    flush(&mut current, line);
    TextLayer { words }
}

/// The text layers of a document's PDF pages as words in document pixels,
/// for Find Sensitive Info. Only upright, unmirrored, uncropped pages are
/// included (their regions are returned so OCR can skip them); anything else
/// is read with OCR as usual.
pub fn document_words(document: &Document) -> (Vec<crate::privacy::Word>, Vec<[f32; 4]>) {
    let mut words = vec![];
    let mut regions = vec![];
    let mut line_base = 0u32;
    for a in &document.annotations {
        let Some(layer) = a.image.as_ref().and_then(|b| b.text_layer()) else {
            continue;
        };
        let bitmap = a.image.as_ref().unwrap();
        if !document.object_visible(a.id)
            || a.rotation_degrees != 0.
            || a.mirror_x
            || bitmap.perspective().is_some()
            || bitmap.view().rect != [0., 0., 1., 1.]
        {
            continue;
        }
        let (lo, hi) = a.body_bounds();
        let size = [hi[0] - lo[0], hi[1] - lo[1]];
        regions.push([lo[0], lo[1], size[0], size[1]]);
        let mut last_line = 0;
        for w in &layer.words {
            last_line = last_line.max(w.line);
            words.push(crate::privacy::Word {
                text: w.text.clone(),
                line: line_base + w.line,
                bounds: [
                    lo[0] + w.rect[0] * size[0],
                    lo[1] + w.rect[1] * size[1],
                    w.rect[2] * size[0],
                    w.rect[3] * size[1],
                ],
            });
        }
        line_base += last_line + 2;
    }
    (words, regions)
}

/// The document without the PDF pages whose words [`document_words`] uses,
/// so what is drawn on top of them can still be read with OCR. `None` when
/// there are no such pages.
pub fn without_pdf_pages(document: &Document) -> Option<Document> {
    let mut rest = document.clone();
    let before = rest.annotations.len();
    rest.annotations.retain(|a| {
        !a.image.as_ref().is_some_and(|b| {
            b.text_layer().is_some()
                && a.rotation_degrees == 0.
                && !a.mirror_x
                && b.perspective().is_none()
                && b.view().rect == [0., 0., 1., 1.]
        })
    });
    if rest.annotations.len() == before {
        return None;
    }
    rest.prune_canvas_behavior();
    rest.reconcile_guide();
    Some(rest)
}

/// How many of `sizes` (guide pixels) one guide can hold, in order.
pub fn pages_that_fit(sizes: &[[u32; 2]]) -> usize {
    let mut width = 0u64;
    let mut height = 0u64;
    let mut pixels = 0u64;
    for (i, s) in sizes.iter().enumerate() {
        let gap = if i == 0 {
            0
        } else {
            u64::from(crate::guide_sections::PAGE_GAP)
        };
        let next_width = width + gap + u64::from(s[0]);
        let next_height = height.max(u64::from(s[1]));
        let next_pixels = pixels + u64::from(s[0]) * u64::from(s[1]);
        if i >= 128
            || next_width > 32768
            || next_width * next_height > crate::MAX_PIXELS
            || next_pixels > crate::MAX_PIXELS
        {
            return i;
        }
        width = next_width;
        height = next_height;
        pixels = next_pixels;
    }
    sizes.len()
}

/// Builds a guide with one prepared page per guide page, each guide page
/// sized to its PDF page.
pub fn build_guide(pages: Vec<PreparedPage>) -> Result<Document> {
    use crate::guide_sections::{Layout, PAGE_GAP, PageLayout, PageSetup};
    if pages.is_empty() {
        return Err(Error::Invalid("Choose at least one page.".into()));
    }
    let sizes: Vec<_> = pages.iter().map(|p| guide_pixels(p.points)).collect();
    if pages_that_fit(&sizes) < sizes.len() {
        return Err(Error::Invalid(
            "These pages don't fit in one guide. Choose fewer pages.".into(),
        ));
    }
    let width = sizes.iter().map(|s| s[0]).sum::<u32>() + PAGE_GAP * (sizes.len() as u32 - 1);
    let height = sizes.iter().map(|s| s[1]).max().unwrap();
    let mut document = Document::blank(width, height)?;
    let setup: Vec<PageSetup> = pages
        .iter()
        .zip(&sizes)
        .map(|(p, s)| PageSetup {
            size: *s,
            ppi: GUIDE_PPI,
            physical_points: Some(p.points),
            margins: [0.; 4],
            ..PageSetup::default()
        })
        .collect();
    document.guide = Some(Layout {
        pages: Some(PageLayout {
            gap: PAGE_GAP,
            setup,
            horizontal: true,
            size: sizes[0],
            count: pages.len(),
            auto_overflow: false,
        }),
        template: None,
        top: 0.,
        sections: vec![],
    });
    let mut x = 0.;
    for (i, (page, size)) in pages.into_iter().zip(&sizes).enumerate() {
        let mut a = Arc::new(page.bitmap).annotation(i as u64 + 1);
        a.start = [x, 0.];
        a.end = [x + size[0] as f32, size[1] as f32];
        document.annotations.push(a);
        x += (size[0] + PAGE_GAP) as f32;
    }
    document.validate()?;
    Ok(document)
}

#[cfg(test)]
#[path = "pdf_import_tests.rs"]
mod tests;
