//! PowerPoint and Word export (approved beta 6 tab 2).
//!
//! Each guide page (or step section) becomes a slide, or a step in a Word
//! document. On slides, shapes, lines, arrows, text and Steps become native
//! PowerPoint objects that can be moved and retyped, unless a later object
//! that can't be drawn natively overlaps them: then they are drawn into the
//! slide's picture with everything else, so redactions always cover what
//! they cover. Redactions are always flattened into the picture.
use crate::{Annotation, Document, Error, Kind, Result};
use image::RgbaImage;
use std::fmt::Write as _;

/// PowerPoint slide size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SlideSize {
    #[default]
    Widescreen,
    Standard,
    MatchPage,
}
/// What one slide (or one Word step) shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Each {
    #[default]
    Page,
    Section,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlideOptions {
    pub size: SlideSize,
    pub each: Each,
    /// Shapes and text as native objects; off puts each page in as one picture.
    pub editable: bool,
    /// Each slide's text in its speaker notes.
    pub notes: bool,
}
impl Default for SlideOptions {
    fn default() -> Self {
        Self {
            size: SlideSize::Widescreen,
            each: Each::Page,
            editable: true,
            notes: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Paper {
    #[default]
    Letter,
    A4,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentOptions {
    pub paper: Paper,
    /// Step text beside the picture instead of below it.
    pub beside: bool,
    /// A title and a table of contents first.
    pub contents: bool,
    pub title: String,
}
impl Default for DocumentOptions {
    fn default() -> Self {
        Self {
            paper: Paper::Letter,
            beside: false,
            contents: true,
            title: "Guide".into(),
        }
    }
}

// ---------------------------------------------------------------- zip

/// Office packages are ZIP files built in memory.
struct Zip(crate::zip::Writer<Vec<u8>>);
impl Zip {
    fn new() -> Self {
        Self(crate::zip::Writer::new(Vec::new()))
    }
    fn add(&mut self, name: &str, data: &[u8]) -> Result<()> {
        self.0.bytes(name, data)
    }
    fn finish(self) -> Result<Vec<u8>> {
        self.0.finish()
    }
}

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}
fn png(pixels: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(vec![]);
    pixels.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}
fn hex(c: [u8; 4]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

// ---------------------------------------------------------------- regions

/// One slide or Word step: a part of the canvas, its picture, the objects
/// drawn natively on top, and its text.
struct Region {
    rect: [f32; 4],
    picture: RgbaImage,
    shapes: Vec<Annotation>,
    text: Vec<String>,
}

/// Objects PowerPoint can draw as its own editable shapes.
fn native(a: &Annotation) -> bool {
    let plain = a.image.is_none()
        && a.focus.is_none()
        && a.shadow.is_none()
        && a.line_style.is_none()
        && a.callout.is_none()
        && a.step_outline.is_none();
    plain
        && match a.kind {
            Kind::Rectangle | Kind::Ellipse => true,
            Kind::Line | Kind::Arrow => a.points.is_empty(),
            Kind::Text | Kind::Step => a
                .text
                .as_ref()
                .is_some_and(|t| t.rich.is_none() && t.outline.is_none()),
            _ => false,
        }
}
fn overlap(a: &Annotation, b: &Annotation) -> bool {
    let (a0, a1) = a.bounds();
    let (b0, b1) = b.bounds();
    a0[0] < b1[0] && b0[0] < a1[0] && a0[1] < b1[1] && b0[1] < a1[1]
}
fn inside(a: &Annotation, rect: [f32; 4]) -> bool {
    let (lo, hi) = a.body_bounds();
    let c = [(lo[0] + hi[0]) / 2., (lo[1] + hi[1]) / 2.];
    c[0] >= rect[0] && c[0] < rect[0] + rect[2] && c[1] >= rect[1] && c[1] < rect[1] + rect[3]
}

fn visible(document: &Document) -> Vec<&Annotation> {
    document
        .annotations
        .iter()
        .filter(|a| document.object_visible(a.id))
        .collect()
}
/// Objects that stay native: an object stays native only if nothing drawn
/// later and non-native overlaps it (a redaction, blur, Focus, picture...).
fn natives(visible: &[&Annotation]) -> Vec<u64> {
    visible
        .iter()
        .enumerate()
        .filter(|(i, a)| native(a) && !visible[i + 1..].iter().any(|b| !native(b) && overlap(a, b)))
        .map(|(_, a)| a.id)
        .collect()
}
/// How many slides (or Word steps) a document makes, and how many of its
/// objects stay editable. Nothing is drawn, so it is cheap to call often.
pub fn summary(document: &Document, each: Each, editable: bool) -> (usize, usize) {
    let editable = if editable {
        natives(&visible(document)).len()
    } else {
        0
    };
    (rects(document, each).len(), editable)
}
fn rects(document: &Document, each: Each) -> Vec<[f32; 4]> {
    match (&document.guide, each) {
        (Some(g), Each::Section) if !g.sections.is_empty() => {
            let spans = g.spans();
            spans
                .iter()
                .zip(&g.sections)
                .map(|((_, top, bottom), s)| {
                    let page = document.page_rect(s.page);
                    [page[0], *top, page[2], bottom - top]
                })
                .collect()
        }
        (Some(g), _) if g.pages.is_some() => {
            (0..g.page_count()).map(|i| document.page_rect(i)).collect()
        }
        _ => vec![[0., 0., document.width as f32, document.height as f32]],
    }
}

fn regions(document: &Document, each: Each, editable: bool) -> Result<Vec<Region>> {
    let visible = visible(document);
    let natives = if editable { natives(&visible) } else { vec![] };
    let mut background = document.clone();
    background.annotations.retain(|a| !natives.contains(&a.id));
    let pixels = background.render()?;
    let mut out = vec![];
    for rect in rects(document, each) {
        let [x, y, w, h] = rect.map(|v| v.max(0.) as u32);
        let w = w.min(pixels.width().saturating_sub(x)).max(1);
        let h = h.min(pixels.height().saturating_sub(y)).max(1);
        let picture = image::imageops::crop_imm(&pixels, x, y, w, h).to_image();
        let shapes = visible
            .iter()
            .filter(|a| natives.contains(&a.id) && inside(a, rect))
            .map(|a| (*a).clone())
            .collect();
        let mut texts: Vec<(f32, f32, String)> = visible
            .iter()
            .filter(|a| inside(a, rect) && matches!(a.kind, Kind::Text | Kind::Callout))
            .filter_map(|a| {
                let t = a.text.as_ref()?.content.trim().to_owned();
                (!t.is_empty()).then(|| (a.start[1].min(a.end[1]), a.start[0].min(a.end[0]), t))
            })
            .collect();
        texts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        out.push(Region {
            rect,
            picture,
            shapes,
            text: texts.into_iter().map(|t| t.2).collect(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------- PowerPoint

const EMU_PER_PX: f64 = 9525.; // 914400 EMU per inch at 96 px per inch

/// Where a region lands on a slide: EMU per document pixel and the offset.
struct Place {
    scale: f64,
    left: f64,
    top: f64,
    rect: [f32; 4],
}
impl Place {
    fn x(&self, x: f32) -> i64 {
        (self.left + f64::from(x - self.rect[0]) * self.scale).round() as i64
    }
    fn y(&self, y: f32) -> i64 {
        (self.top + f64::from(y - self.rect[1]) * self.scale).round() as i64
    }
    fn len(&self, v: f32) -> i64 {
        (f64::from(v) * self.scale).round().max(1.) as i64
    }
}

fn shape_xml(a: &Annotation, place: &Place, id: u32) -> String {
    let (lo, hi) = a.body_bounds();
    let (x, y) = (place.x(lo[0]), place.y(lo[1]));
    let (cx, cy) = (place.len(hi[0] - lo[0]), place.len(hi[1] - lo[1]));
    let rot = (f64::from(a.rotation_degrees) * 60000.).round() as i64;
    let alpha = |c: [u8; 4]| {
        if c[3] == 255 {
            String::new()
        } else {
            format!("<a:alpha val=\"{}\"/>", u32::from(c[3]) * 100_000 / 255)
        }
    };
    let line_w = place.len(a.width as f32);
    match a.kind {
        Kind::Line | Kind::Arrow => {
            let flip_h = a.end[0] < a.start[0];
            let flip_v = a.end[1] < a.start[1];
            let x0 = place.x(a.start[0].min(a.end[0]));
            let y0 = place.y(a.start[1].min(a.end[1]));
            let w = place.len((a.end[0] - a.start[0]).abs()).max(1);
            let h = place.len((a.end[1] - a.start[1]).abs()).max(1);
            let head = if a.kind == Kind::Arrow {
                "<a:tailEnd type=\"triangle\" w=\"med\" len=\"med\"/>"
            } else {
                ""
            };
            format!(
                "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"{} {id}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr><a:xfrm{}{}><a:off x=\"{x0}\" y=\"{y0}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom><a:ln w=\"{line_w}\" cap=\"rnd\"><a:solidFill><a:srgbClr val=\"{}\">{}</a:srgbClr></a:solidFill>{head}</a:ln></p:spPr></p:cxnSp>",
                if a.kind == Kind::Arrow {
                    "Arrow"
                } else {
                    "Line"
                },
                if flip_h { " flipH=\"1\"" } else { "" },
                if flip_v { " flipV=\"1\"" } else { "" },
                hex(a.color),
                alpha(a.color)
            )
        }
        _ => {
            let geom = match a.kind {
                Kind::Ellipse | Kind::Step => "ellipse",
                Kind::Rectangle if a.corner_radius > 0. => "roundRect",
                _ => "rect",
            };
            let fill = match (a.kind, a.fill) {
                (Kind::Step, _) => format!(
                    "<a:solidFill><a:srgbClr val=\"{}\">{}</a:srgbClr></a:solidFill>",
                    hex(a.color),
                    alpha(a.color)
                ),
                (_, Some(f)) if f[3] > 0 => format!(
                    "<a:solidFill><a:srgbClr val=\"{}\">{}</a:srgbClr></a:solidFill>",
                    hex(f),
                    alpha(f)
                ),
                _ => "<a:noFill/>".into(),
            };
            let line = if a.kind == Kind::Text || a.kind == Kind::Step || a.width == 0 {
                "<a:ln><a:noFill/></a:ln>".to_owned()
            } else {
                format!(
                    "<a:ln w=\"{line_w}\"><a:solidFill><a:srgbClr val=\"{}\">{}</a:srgbClr></a:solidFill></a:ln>",
                    hex(a.color),
                    alpha(a.color)
                )
            };
            let text = match &a.text {
                Some(t) if !t.content.is_empty() => {
                    let color = if a.kind == Kind::Step {
                        t.color.unwrap_or([255; 4])
                    } else {
                        t.color.unwrap_or(a.color)
                    };
                    // Text sizes are points at 96 pixels per inch.
                    let px = f64::from(t.size_points) * 96. / 72.;
                    let pt = px * place.scale / 12700.;
                    let size = (pt * 100.).round().clamp(100., 400_000.) as i64;
                    let algn = if a.kind == Kind::Step {
                        "ctr"
                    } else {
                        match t.horizontal {
                            crate::text::HorizontalAlignment::Center => "ctr",
                            crate::text::HorizontalAlignment::Right => "r",
                            _ => "l",
                        }
                    };
                    let paragraphs: String = t
                        .content
                        .split('\n')
                        .map(|line| {
                            format!(
                                "<a:p><a:pPr algn=\"{algn}\"/><a:r><a:rPr lang=\"en-US\" sz=\"{size}\"{}{}{}><a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill></a:rPr><a:t>{}</a:t></a:r></a:p>",
                                if t.bold { " b=\"1\"" } else { "" },
                                if t.italic { " i=\"1\"" } else { "" },
                                if t.underline { " u=\"sng\"" } else { "" },
                                hex(color),
                                esc(line)
                            )
                        })
                        .collect();
                    format!(
                        "<p:txBody><a:bodyPr wrap=\"square\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"{}\"/><a:lstStyle/>{paragraphs}</p:txBody>",
                        if a.kind == Kind::Step { "ctr" } else { "t" }
                    )
                }
                _ => String::new(),
            };
            format!(
                "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{:?} {id}\"/><p:cNvSpPr{}/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm rot=\"{rot}\"><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"{geom}\"><a:avLst/></a:prstGeom>{fill}{line}</p:spPr>{text}</p:sp>",
                a.kind,
                if a.kind == Kind::Text {
                    " txBox=\"1\""
                } else {
                    ""
                },
            )
        }
    }
}

const DRAWING_NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn theme() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"ClipDesigner\"><a:themeElements><a:clrScheme name=\"ClipDesigner\"><a:dk1><a:srgbClr val=\"222222\"/></a:dk1><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1><a:dk2><a:srgbClr val=\"2B3235\"/></a:dk2><a:lt2><a:srgbClr val=\"EEF1F3\"/></a:lt2><a:accent1><a:srgbClr val=\"1891C1\"/></a:accent1><a:accent2><a:srgbClr val=\"F2B134\"/></a:accent2><a:accent3><a:srgbClr val=\"4FC28A\"/></a:accent3><a:accent4><a:srgbClr val=\"E06C6C\"/></a:accent4><a:accent5><a:srgbClr val=\"7D5BA6\"/></a:accent5><a:accent6><a:srgbClr val=\"7F8B8F\"/></a:accent6><a:hlink><a:srgbClr val=\"1891C1\"/></a:hlink><a:folHlink><a:srgbClr val=\"7D5BA6\"/></a:folHlink></a:clrScheme><a:fontScheme name=\"ClipDesigner\"><a:majorFont><a:latin typeface=\"Segoe UI\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"Segoe UI\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"ClipDesigner\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{ln}{ln}{ln}</a:lnStyleLst><a:effectStyleLst>{eff}{eff}{eff}</a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>",
        fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>",
        ln = "<a:ln w=\"9525\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:ln>",
        eff = "<a:effectStyle><a:effectLst/></a:effectStyle>",
    )
}

/// A PowerPoint file from documents (one, or every linked part in order).
pub fn pptx(
    documents: impl Iterator<Item = Result<Document>>,
    options: &SlideOptions,
) -> Result<Vec<u8>> {
    let mut zip = Zip::new();
    let mut slides: Vec<(String, String, Option<String>)> = vec![]; // xml, rels, notes
    let mut size: Option<(i64, i64)> = None;
    let mut media = 0usize;
    for document in documents {
        let document = document?;
        for region in regions(&document, options.each, options.editable)? {
            let (sw, sh) = *size.get_or_insert_with(|| match options.size {
                SlideSize::Widescreen => (12_192_000, 6_858_000),
                SlideSize::Standard => (9_144_000, 6_858_000),
                SlideSize::MatchPage => {
                    let w = (f64::from(region.rect[2]) * EMU_PER_PX).round() as i64;
                    let h = (f64::from(region.rect[3]) * EMU_PER_PX).round() as i64;
                    let k = (51_206_400. / w.max(h) as f64).min(1.);
                    ((w as f64 * k) as i64, (h as f64 * k) as i64)
                }
            });
            let scale =
                (sw as f64 / f64::from(region.rect[2])).min(sh as f64 / f64::from(region.rect[3]));
            let place = Place {
                scale,
                left: (sw as f64 - f64::from(region.rect[2]) * scale) / 2.,
                top: (sh as f64 - f64::from(region.rect[3]) * scale) / 2.,
                rect: region.rect,
            };
            media += 1;
            zip.add(
                &format!("ppt/media/image{media}.png"),
                &png(&region.picture)?,
            )?;
            let mut body = format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Page\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
                place.left.round() as i64,
                place.top.round() as i64,
                (f64::from(region.rect[2]) * scale).round() as i64,
                (f64::from(region.rect[3]) * scale).round() as i64
            );
            for (k, shape) in region.shapes.iter().enumerate() {
                body.push_str(&shape_xml(shape, &place, k as u32 + 3));
            }
            let xml = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld {DRAWING_NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"
            );
            let n = slides.len() + 1;
            let notes = (options.notes && !region.text.is_empty()).then(|| region.text.join("\n"));
            let rels = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"{REL}/image\" Target=\"../media/image{media}.png\"/>{}</Relationships>",
                if notes.is_some() {
                    format!(
                        "<Relationship Id=\"rId3\" Type=\"{REL}/notesSlide\" Target=\"../notesSlides/notesSlide{n}.xml\"/>"
                    )
                } else {
                    String::new()
                }
            );
            slides.push((xml, rels, notes));
        }
    }
    let Some(size) = size else {
        return Err(Error::Invalid("Nothing to export".into()));
    };
    package(zip, &slides, size, "")
}

/// A one-slide PowerPoint file playing a video project's export: an MP4
/// with its first frame as the poster (click to play), or an animated GIF.
pub fn pptx_movie(movie: &[u8], gif: bool, poster: &RgbaImage, size: SlideSize) -> Result<Vec<u8>> {
    let mut zip = Zip::new();
    let (w, h) = (poster.width().max(1) as f64, poster.height().max(1) as f64);
    let (sw, sh) = match size {
        SlideSize::Widescreen => (12_192_000, 6_858_000),
        SlideSize::Standard => (9_144_000, 6_858_000),
        SlideSize::MatchPage => {
            let k = (51_206_400. / (w.max(h) * EMU_PER_PX)).min(1.);
            ((w * EMU_PER_PX * k) as i64, (h * EMU_PER_PX * k) as i64)
        }
    };
    let scale = (sw as f64 / w).min(sh as f64 / h);
    let (cx, cy) = ((w * scale) as i64, (h * scale) as i64);
    let (x, y) = ((sw - cx) / 2, (sh - cy) / 2);
    let xfrm = format!(
        "<p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr>"
    );
    let (body, rels, timing) = if gif {
        zip.add("ppt/media/image1.gif", movie)?;
        (
            format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Recording\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>{xfrm}</p:pic>"
            ),
            format!(
                "<Relationship Id=\"rId2\" Type=\"{REL}/image\" Target=\"../media/image1.gif\"/>"
            ),
            "",
        )
    } else {
        zip.add("ppt/media/image1.png", &png(poster)?)?;
        zip.add("ppt/media/media1.mp4", movie)?;
        (
            format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Recording\"><a:hlinkClick r:id=\"\" action=\"ppaction://media\"/></p:cNvPr><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr><a:videoFile r:link=\"rId3\"/><p:extLst><p:ext uri=\"{{DAA4B4D4-6D71-4841-9C94-3DE7FCFB9230}}\"><p14:media xmlns:p14=\"http://schemas.microsoft.com/office/powerpoint/2010/main\" r:embed=\"rId4\"/></p:ext></p:extLst></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>{xfrm}</p:pic>"
            ),
            format!(
                "<Relationship Id=\"rId2\" Type=\"{REL}/image\" Target=\"../media/image1.png\"/><Relationship Id=\"rId3\" Type=\"{REL}/video\" Target=\"../media/media1.mp4\"/><Relationship Id=\"rId4\" Type=\"http://schemas.microsoft.com/office/2007/relationships/media\" Target=\"../media/media1.mp4\"/>"
            ),
            "<p:timing><p:tnLst><p:par><p:cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><p:childTnLst><p:video><p:cMediaNode vol=\"80000\"><p:cTn id=\"2\" fill=\"hold\" display=\"0\"><p:stCondLst><p:cond delay=\"indefinite\"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid=\"2\"/></p:tgtEl></p:cMediaNode></p:video></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>",
        )
    };
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld {DRAWING_NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{timing}</p:sld>"
    );
    let rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>{rels}</Relationships>"
    );
    let defaults = if gif {
        "<Default Extension=\"gif\" ContentType=\"image/gif\"/>"
    } else {
        "<Default Extension=\"mp4\" ContentType=\"video/mp4\"/>"
    };
    package(zip, &[(xml, rels, None)], (sw, sh), defaults)
}

/// The parts every PowerPoint file needs around its slides.
fn package(
    mut zip: Zip,
    slides: &[(String, String, Option<String>)],
    (sw, sh): (i64, i64),
    defaults: &str,
) -> Result<Vec<u8>> {
    let any_notes = slides.iter().any(|s| s.2.is_some());
    let mut types = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/>{defaults}<Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>",
    );
    if any_notes {
        types.push_str("<Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\"/><Override PartName=\"/ppt/theme/theme2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>");
    }
    for (i, s) in slides.iter().enumerate() {
        let _ = write!(
            types,
            "<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>",
            i + 1
        );
        if s.2.is_some() {
            let _ = write!(
                types,
                "<Override PartName=\"/ppt/notesSlides/notesSlide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>",
                i + 1
            );
        }
    }
    types.push_str("</Types>");
    zip.add("[Content_Types].xml", types.as_bytes())?;
    zip.add(
        "_rels/.rels",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>").as_bytes(),
    )?;
    let ids: String = (0..slides.len())
        .map(|i| format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 256 + i, i + 10))
        .collect();
    zip.add(
        "ppt/presentation.xml",
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation {DRAWING_NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst>{}<p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{sw}\" cy=\"{sh}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
            if any_notes { "<p:notesMasterIdLst><p:notesMasterId r:id=\"rId3\"/></p:notesMasterIdLst>" } else { "" }
        )
        .as_bytes(),
    )?;
    let slide_rels: String = (0..slides.len())
        .map(|i| {
            format!(
                "<Relationship Id=\"rId{}\" Type=\"{REL}/slide\" Target=\"slides/slide{}.xml\"/>",
                i + 10,
                i + 1
            )
        })
        .collect();
    zip.add(
        "ppt/_rels/presentation.xml.rels",
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/><Relationship Id=\"rId2\" Type=\"{REL}/theme\" Target=\"theme/theme1.xml\"/>{}{slide_rels}</Relationships>",
            if any_notes { format!("<Relationship Id=\"rId3\" Type=\"{REL}/notesMaster\" Target=\"notesMasters/notesMaster1.xml\"/>") } else { String::new() }
        )
        .as_bytes(),
    )?;
    let empty_tree = "<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>";
    zip.add(
        "ppt/slideMasters/slideMaster1.xml",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldMaster {DRAWING_NS}>{empty_tree}<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>").as_bytes(),
    )?;
    zip.add(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"{REL}/theme\" Target=\"../theme/theme1.xml\"/></Relationships>").as_bytes(),
    )?;
    zip.add(
        "ppt/slideLayouts/slideLayout1.xml",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldLayout {DRAWING_NS} type=\"blank\" preserve=\"1\">{empty_tree}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>").as_bytes(),
    )?;
    zip.add(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>").as_bytes(),
    )?;
    zip.add("ppt/theme/theme1.xml", theme().as_bytes())?;
    if any_notes {
        zip.add("ppt/theme/theme2.xml", theme().as_bytes())?;
        zip.add(
            "ppt/notesMasters/notesMaster1.xml",
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notesMaster {DRAWING_NS}>{empty_tree}<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/></p:notesMaster>").as_bytes(),
        )?;
        zip.add(
            "ppt/notesMasters/_rels/notesMaster1.xml.rels",
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/theme\" Target=\"../theme/theme2.xml\"/></Relationships>").as_bytes(),
        )?;
    }
    for (i, (xml, rels, notes)) in slides.iter().enumerate() {
        let n = i + 1;
        zip.add(&format!("ppt/slides/slide{n}.xml"), xml.as_bytes())?;
        zip.add(
            &format!("ppt/slides/_rels/slide{n}.xml.rels"),
            rels.as_bytes(),
        )?;
        if let Some(text) = notes {
            let paragraphs: String = text
                .split('\n')
                .map(|l| {
                    format!(
                        "<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p>",
                        esc(l)
                    )
                })
                .collect();
            zip.add(
                &format!("ppt/notesSlides/notesSlide{n}.xml"),
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notes {DRAWING_NS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Notes\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>").as_bytes(),
            )?;
            zip.add(
                &format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/notesMaster\" Target=\"../notesMasters/notesMaster1.xml\"/><Relationship Id=\"rId2\" Type=\"{REL}/slide\" Target=\"../slides/slide{n}.xml\"/></Relationships>").as_bytes(),
            )?;
        }
    }
    zip.finish()
}

// ---------------------------------------------------------------- Word

/// A Word document from documents: each page or section is a step with its
/// picture and its text, in Word's own Title, Heading 1 and Normal styles.
pub fn docx(
    documents: impl Iterator<Item = Result<Document>>,
    options: &DocumentOptions,
) -> Result<Vec<u8>> {
    let mut zip = Zip::new();
    let (page_w, page_h) = match options.paper {
        Paper::Letter => (12_240, 15_840),
        Paper::A4 => (11_906, 16_838),
    };
    let margin = 1_440;
    // Text width in EMU (1 twip = 635 EMU).
    let text_w = i64::from(page_w - 2 * margin) * 635;
    let mut body = String::new();
    if options.contents {
        let _ = write!(
            body,
            "<w:p><w:pPr><w:pStyle w:val=\"Title\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p><w:p><w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-1\" \\h \\z \\u </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>Right-click and choose Update Field to show the contents.</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>",
            esc(&options.title)
        );
    }
    let mut rels = String::new();
    let mut media = 0usize;
    let mut step = 0usize;
    for document in documents {
        let document = document?;
        for region in regions(&document, Each::Section, false)? {
            step += 1;
            media += 1;
            zip.add(
                &format!("word/media/image{media}.png"),
                &png(&region.picture)?,
            )?;
            let rid = format!("rId{}", media + 10);
            let _ = write!(
                rels,
                "<Relationship Id=\"{rid}\" Type=\"{REL}/image\" Target=\"media/image{media}.png\"/>"
            );
            let width = if options.beside {
                text_w * 3 / 5
            } else {
                text_w
            };
            let height = (width as f64 * f64::from(region.rect[3])
                / f64::from(region.rect[2].max(1.))) as i64;
            let picture = format!(
                "<w:p><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{width}\" cy=\"{height}\"/><wp:docPr id=\"{media}\" name=\"Step {step}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{media}\" name=\"Step {step}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
            );
            // The first line of the step's text is its heading; the rest is
            // its body.
            let mut lines = region.text.iter().flat_map(|t| t.lines());
            let heading = lines
                .next()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Step {step}"));
            let _ = write!(
                body,
                "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{step}. {}</w:t></w:r></w:p>",
                esc(&heading)
            );
            let text: String = lines
                .map(|l| {
                    format!(
                        "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                        esc(l)
                    )
                })
                .collect();
            if options.beside {
                let _ = write!(
                    body,
                    "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"{}\"/><w:gridCol w:w=\"{}\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/></w:tcPr>{picture}</w:tc><w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/></w:tcPr>{}</w:tc></w:tr></w:tbl><w:p/>",
                    (page_w - 2 * margin) * 3 / 5,
                    (page_w - 2 * margin) * 2 / 5,
                    (page_w - 2 * margin) * 3 / 5,
                    (page_w - 2 * margin) * 2 / 5,
                    if text.is_empty() {
                        "<w:p/>".to_owned()
                    } else {
                        text
                    }
                );
            } else {
                body.push_str(&picture);
                body.push_str(&text);
            }
        }
    }
    if step == 0 {
        return Err(Error::Invalid("Nothing to export".into()));
    }
    let _ = write!(
        body,
        "<w:sectPr><w:pgSz w:w=\"{page_w}\" w:h=\"{page_h}\"/><w:pgMar w:top=\"{margin}\" w:right=\"{margin}\" w:bottom=\"{margin}\" w:left=\"{margin}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr>"
    );
    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"{REL}\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><w:body>{body}</w:body></w:document>"
    );
    zip.add(
        "[Content_Types].xml",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>",
    )?;
    zip.add(
        "_rels/.rels",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/officeDocument\" Target=\"word/document.xml\"/></Relationships>").as_bytes(),
    )?;
    zip.add("word/document.xml", document_xml.as_bytes())?;
    zip.add(
        "word/_rels/document.xml.rels",
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"{REL_NS}\"><Relationship Id=\"rId1\" Type=\"{REL}/styles\" Target=\"styles.xml\"/>{rels}</Relationships>").as_bytes(),
    )?;
    // Word's own style names, so a company template restyles the document.
    zip.add(
        "word/styles.xml",
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"160\"/></w:pPr><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"240\"/></w:pPr><w:rPr><w:sz w:val=\"56\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:pPr><w:keepNext/><w:spacing w:before=\"360\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style></w:styles>",
    )?;
    zip.finish()
}

#[cfg(test)]
#[path = "office_tests.rs"]
mod tests;
