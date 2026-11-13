//! PDF 1.4: vector coverage and outlined glyphs, with bounded raster effects.
//! Pixel replacements remove covered source data, rather than merely hiding it.
use super::*;
use crate::display::{
    Layer,
    vector::{Op, record},
};
use std::fmt::Write as _;
mod drawing;
#[cfg(test)]
mod tests;
mod text_layer;
use vello_cpu::{
    kurbo::{Affine, BezPath, Cap, Join, PathEl, Point, Rect, Stroke},
    peniko::Compose,
};

#[derive(Default)]
struct Pdf {
    objects: Vec<Vec<u8>>,
    resources: Vec<(String, usize)>,
    states: Vec<(String, usize)>,
    fonts: Vec<(String, usize)>,
}
impl Pdf {
    fn add(&mut self, bytes: Vec<u8>) -> usize {
        self.objects.push(bytes);
        self.objects.len()
    }
    fn stream(&mut self, dict: &str, bytes: &[u8]) -> Result<usize> {
        let mut zip = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        zip.write_all(bytes)?;
        let bytes = zip.finish()?;
        let mut value = format!(
            "<< {dict} /Filter /FlateDecode /Length {} >>\nstream\n",
            bytes.len()
        )
        .into_bytes();
        value.extend(bytes);
        value.extend(b"\nendstream");
        Ok(self.add(value))
    }
    fn resources(&self) -> String {
        let fonts = if self.fonts.is_empty() {
            String::new()
        } else {
            format!(
                " /Font << {} >>",
                self.fonts
                    .iter()
                    .map(|(n, id)| format!("/{n} {id} 0 R"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        format!(
            "<< /XObject << {} >> /ExtGState << {} >>{fonts} >>",
            self.resources
                .iter()
                .map(|(n, id)| format!("/{n} {id} 0 R"))
                .collect::<Vec<_>>()
                .join(" "),
            self.states
                .iter()
                .map(|(n, id)| format!("/{n} {id} 0 R"))
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
    fn state(&mut self, dict: String) -> String {
        let id = self.add(format!("<< /Type /ExtGState {dict} >>").into_bytes());
        let name = format!("G{id}");
        self.states.push((name.clone(), id));
        name
    }
    fn form(&mut self, content: &str, bounds: Rect, gray: bool) -> Result<String> {
        let id=self.stream(&format!("/Type /XObject /Subtype /Form /BBox [{} {} {} {}] /Group << /S /Transparency /I true /CS /{} >> /Resources {}",bounds.x0,bounds.y0,bounds.x1,bounds.y1,if gray{"DeviceGray"}else{"DeviceRGB"},self.resources()),content.as_bytes())?;
        let name = format!("F{id}");
        self.resources.push((name.clone(), id));
        Ok(name)
    }
    fn image(&mut self, pixels: &RgbaImage) -> Result<String> {
        let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
        let mut alpha = Vec::with_capacity(pixels.len() / 4);
        for p in pixels.pixels() {
            rgb.extend(if p[3] == 0 {
                [0; 3]
            } else {
                [p[0], p[1], p[2]]
            });
            alpha.push(p[3]);
        }
        let base = format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /BitsPerComponent 8",
            pixels.width(),
            pixels.height()
        );
        let mask = if alpha.iter().any(|a| *a != 255) {
            format!(
                " /SMask {} 0 R",
                self.stream(&format!("{base} /ColorSpace /DeviceGray"), &alpha)?
            )
        } else {
            String::new()
        };
        let id = self.stream(&format!("{base} /ColorSpace /DeviceRGB{mask}"), &rgb)?;
        let name = format!("I{id}");
        self.resources.push((name.clone(), id));
        Ok(name)
    }
}
fn path(out: &mut String, p: &BezPath) {
    let mut current = Point::ZERO;
    let mut start = Point::ZERO;
    for e in p.elements() {
        match *e {
            PathEl::MoveTo(p) => {
                let _ = writeln!(out, "{:.5} {:.5} m", p.x, p.y);
                current = p;
                start = p;
            }
            PathEl::LineTo(p) => {
                let _ = writeln!(out, "{:.5} {:.5} l", p.x, p.y);
                current = p;
            }
            PathEl::QuadTo(c, p) => {
                let a = current + (c - current) * (2. / 3.);
                let b = p + (c - p) * (2. / 3.);
                let _ = writeln!(
                    out,
                    "{:.5} {:.5} {:.5} {:.5} {:.5} {:.5} c",
                    a.x, a.y, b.x, b.y, p.x, p.y
                );
                current = p;
            }
            PathEl::CurveTo(a, b, p) => {
                let _ = writeln!(
                    out,
                    "{:.5} {:.5} {:.5} {:.5} {:.5} {:.5} c",
                    a.x, a.y, b.x, b.y, p.x, p.y
                );
                current = p;
            }
            PathEl::ClosePath => {
                out.push_str("h\n");
                current = start;
            }
        }
    }
}
fn stroke(out: &mut String, s: &Stroke) {
    let cap = match s.start_cap {
        Cap::Butt => 0,
        Cap::Round => 1,
        Cap::Square => 2,
    };
    let join = match s.join {
        Join::Miter => 0,
        Join::Round => 1,
        Join::Bevel => 2,
    };
    let _ = writeln!(
        out,
        "{} w {cap} J {join} j {} M [{}] {} d",
        s.width,
        s.miter_limit,
        s.dash_pattern
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" "),
        s.dash_offset
    );
}
fn ops(out: &mut String, list: &[Op]) {
    for op in list {
        match op {
            Op::Fill(p) => {
                path(out, p);
                out.push_str("f\n");
            }
            Op::Stroke(p, s) => {
                stroke(out, s);
                path(out, p);
                out.push_str("S\n");
            }
            Op::Clip(p) => {
                out.push_str("q\n");
                path(out, p);
                out.push_str("W n\n");
            }
            Op::PopClip => out.push_str("Q\n"),
            Op::Layer(_) => out.push_str("q\n"),
            Op::PopLayer => out.push_str("Q\n"),
        }
    }
}
fn matrix(out: &mut String, a: Affine) {
    let [a, b, c, d, e, f] = a.as_coeffs();
    let _ = writeln!(out, "{a:.7} {b:.7} {c:.7} {d:.7} {e:.7} {f:.7} cm");
}
fn rotation(a: &crate::Annotation) -> Affine {
    let r = a.rotation();
    Affine::translate((r.center[0] as f64, r.center[1] as f64))
        * Affine::rotate((r.degrees as f64).to_radians())
        * Affine::scale_non_uniform(if a.mirror_x { -1. } else { 1. }, 1.)
        * Affine::translate((-r.center[0] as f64, -r.center[1] as f64))
}
fn vector(pdf: &mut Pdf, out: &mut String, a: &crate::Annotation, bounds: Rect) -> Result<()> {
    let bounds = rotation(a).inverse().transform_rect_bbox(bounds);
    out.push_str("q\n");
    matrix(out, rotation(a));
    for (list, ink) in record(a) {
        let mut content = format!(
            "{} {} {} rg {} {} {} RG\n",
            ink[0] as f64 / 255.,
            ink[1] as f64 / 255.,
            ink[2] as f64 / 255.,
            ink[0] as f64 / 255.,
            ink[1] as f64 / 255.,
            ink[2] as f64 / 255.
        );
        // Outer text strokes subtract glyph interiors in an isolated luminosity
        // mask. The mask itself is paths, so transparent glyphs stay sharp too.
        if let Some(start) = list
            .iter()
            .position(|op| matches!(op,Op::Layer(b) if b.compose==Compose::DestOut))
        {
            let end = start
                + 1
                + list[start + 1..]
                    .iter()
                    .position(|op| matches!(op, Op::PopLayer))
                    .unwrap();
            let mut mask = format!(
                "1 g {} {} {} {} re f\n0 g\n",
                bounds.x0,
                bounds.y0,
                bounds.width(),
                bounds.height()
            );
            ops(&mut mask, &list[start + 1..end]);
            let name = pdf.form(&mask, bounds, true)?;
            let id = pdf.resources.iter().find(|(n, _)| *n == name).unwrap().1;
            let state = pdf.state(format!("/SMask << /S /Luminosity /G {id} 0 R >>"));
            let _ = writeln!(content, "/{state} gs");
            ops(&mut content, &list[..start]);
            ops(&mut content, &list[end + 1..]);
        } else {
            ops(&mut content, &list);
        }
        let form = pdf.form(&content, bounds, false)?;
        let alpha = pdf.state(format!(
            "/ca {} /CA {}",
            ink[3] as f64 / 255.,
            ink[3] as f64 / 255.
        ));
        let _ = writeln!(out, "q /{alpha} gs /{form} Do Q");
    }
    out.push_str("Q\n");
    Ok(())
}
fn image(pdf: &mut Pdf, out: &mut String, pixels: &RgbaImage, origin: [i32; 2]) -> Result<()> {
    if pixels.pixels().all(|p| p[3] == 0) {
        return Ok(());
    }
    let name = pdf.image(pixels)?;
    let _ = writeln!(
        out,
        "q {} 0 0 -{} {} {} cm /{name} Do Q",
        pixels.width(),
        pixels.height(),
        origin[0],
        origin[1] + pixels.height() as i32
    );
    Ok(())
}
fn erase(pixels: &mut RgbaImage, origin: [i32; 2], rects: &[Rect]) {
    for r in rects {
        let x0 = (r.x0 - origin[0] as f64).clamp(0., pixels.width() as f64) as u32;
        let y0 = (r.y0 - origin[1] as f64).clamp(0., pixels.height() as f64) as u32;
        let x1 = (r.x1 - origin[0] as f64).clamp(0., pixels.width() as f64) as u32;
        let y1 = (r.y1 - origin[1] as f64).clamp(0., pixels.height() as f64) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                pixels.put_pixel(x, y, image::Rgba([0; 4]));
            }
        }
    }
}
pub(super) fn encode(document: &Document, options: &Options) -> Result<Vec<u8>> {
    encode_parts(std::iter::once(Ok(document.clone())), options)
}
/// One PDF from several documents, drawn and written one at a time, so a
/// set of linked parts never needs all of its pages in memory at once.
pub(super) fn encode_parts(
    documents: impl Iterator<Item = Result<Document>>,
    options: &Options,
) -> Result<Vec<u8>> {
    let mut pdf = Pdf::default();
    pdf.add(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    pdf.add(vec![]);
    let mut kids = vec![];
    for document in documents {
        add_document(&mut pdf, &mut kids, &document?, options)?;
    }
    if kids.is_empty() {
        return Err(Error::Invalid("Nothing to export".into()));
    }
    pdf.objects[1] = format!(
        "<< /Type /Pages /Count {} /Kids [{}] >>",
        kids.len(),
        kids.join(" ")
    )
    .into_bytes();
    super::finish_pdf(pdf.objects)
}
fn add_document(
    pdf_slot: &mut Pdf,
    kids_slot: &mut Vec<String>,
    document: &Document,
    options: &Options,
) -> Result<()> {
    document.validate()?;
    let mut list = document.display_list()?;
    // Monochrome system cursors can invert their backdrop; retain that exact
    // bounded composite rather than exporting invisible XOR pixels.
    for layer in &mut list.layers {
        if let Layer::Vector(a) = layer
            && a.image.as_ref().is_some_and(|b| b.is_cursor())
        {
            let (lo, hi) = a.bounds();
            let x = lo[0].floor().clamp(0., document.width as f32) as u32;
            let y = lo[1].floor().clamp(0., document.height as f32) as u32;
            let w = (hi[0].ceil().clamp(x as f32, document.width as f32) as u32) - x;
            let h = (hi[1].ceil().clamp(y as f32, document.height as f32) as u32) - y;
            if w > 0 && h > 0 {
                let mut prefix = document.clone();
                let end = prefix
                    .annotations
                    .iter()
                    .position(|p| p.id == a.id)
                    .unwrap();
                prefix.annotations.truncate(end + 1);
                prefix.prune_canvas_behavior();
                // The export-only prefix keeps page geometry but no references
                // to objects that have not been drawn yet (including step markers).
                prefix.reconcile_guide();
                let pixels = prefix.render()?;
                *layer = Layer::ReplacePixels {
                    origin: [x, y],
                    pixels: image::imageops::crop_imm(&pixels, x, y, w, h).to_image(),
                };
            }
        }
    }
    let mut pdf = std::mem::take(pdf_slot);
    let bleed = document
        .guide
        .as_ref()
        .and_then(|g| g.pages.as_ref())
        .map_or(0., |p| {
            p.setup
                .iter()
                .map(|s| s.bleed * s.ppi / 72.)
                .fold(0., f64::max)
        });
    let bounds = Rect::new(
        -bleed,
        -bleed,
        document.width as f64 + bleed,
        document.height as f64 + bleed,
    );
    let mut future: Vec<_> = list
        .layers
        .iter()
        .filter_map(|l| match l {
            Layer::ReplacePixels { origin, pixels } => Some(Rect::new(
                origin[0] as f64,
                origin[1] as f64,
                (origin[0] + pixels.width()) as f64,
                (origin[1] + pixels.height()) as f64,
            )),
            _ => None,
        })
        .collect();
    // Everything a redaction, blur or pixelation covers, for the text layer.
    let covers = future.clone();
    let mut content = String::new();
    let mut base = list.base;
    erase(&mut base, [0, 0], &future);
    if base.pixels().all(|p| p == base.get_pixel(0, 0)) {
        let p = base.get_pixel(0, 0);
        if p[3] > 0 {
            let g = pdf.state(format!("/ca {}", p[3] as f64 / 255.));
            let _ = writeln!(
                content,
                "q /{g} gs {} {} {} rg 0 0 {} {} re f Q",
                p[0] as f64 / 255.,
                p[1] as f64 / 255.,
                p[2] as f64 / 255.,
                document.width,
                document.height
            );
        }
    } else {
        image(&mut pdf, &mut content, &base, [0, 0])?;
    }
    let full_bounds = bounds;
    let mut bounds = bounds;
    for layer in list.layers {
        match layer {
            Layer::BeginClip(r) => {
                bounds = Rect::new(
                    r[0] as f64,
                    r[1] as f64,
                    (r[0] + r[2]) as f64,
                    (r[1] + r[3]) as f64,
                );
                let _ = writeln!(content, "q {} {} {} {} re W n", r[0], r[1], r[2], r[3]);
            }
            Layer::EndClip => {
                content.push_str("Q\n");
                bounds = full_bounds;
            }
            Layer::ReplacePixels { origin, mut pixels } => {
                future.remove(0);
                erase(&mut pixels, origin.map(|n| n as i32), &future);
                image(&mut pdf, &mut content, &pixels, origin.map(|n| n as i32))?;
            }
            Layer::Shadow(mut p) => {
                erase(std::sync::Arc::make_mut(&mut p.pixels), p.origin, &future);
                image(&mut pdf, &mut content, &p.pixels, p.origin)?;
            }
            Layer::Vector(a) => {
                // Drawings stay shapes; only what a later redaction touches
                // is flattened, with the covered part removed.
                if drawing::write(&mut pdf, &mut content, &a, &future, bounds)? {
                    continue;
                }
                let (lo, hi) = a.bounds();
                let reach = a.width as f64 * 4. + 3.;
                let r = Rect::new(
                    lo[0] as f64 - reach,
                    lo[1] as f64 - reach,
                    hi[0] as f64 + reach,
                    hi[1] as f64 + reach,
                )
                .intersect(bounds);
                let raster =
                    a.image.is_some() && !a.image.as_ref().is_some_and(|b| b.vector().is_some());
                if raster || future.iter().any(|p| p.intersect(r).area() > 0.) {
                    let x = r.x0.floor().max(0.) as i32;
                    let y = r.y0.floor().max(0.) as i32;
                    let w = (r.x1.ceil() - x as f64).max(0.) as u32;
                    let h = (r.y1.ceil() - y as f64).max(0.) as u32;
                    if w == 0 || h == 0 {
                        continue;
                    }
                    let mut pixels = RgbaImage::new(w, h);
                    let mut a = a.clone();
                    a.shadow = None;
                    let shape_frame = a.is_shape()
                        && a.is_image_frame()
                        && !future.iter().any(|p| p.intersect(r).area() > 0.);
                    if shape_frame {
                        let mut shell = a.clone();
                        shell.image = None;
                        shell.color[3] = 0;
                        vector(&mut pdf, &mut content, &shell, bounds)?;
                        crate::bitmap::render(
                            &mut pixels,
                            &a,
                            crate::display::Viewport {
                                size: [w, h],
                                origin: [x as f64, y as f64],
                                scale: 1.,
                            },
                        );
                    } else {
                        crate::display::render_vector(
                            &mut pixels,
                            &a,
                            crate::display::Viewport {
                                size: [w, h],
                                origin: [x as f64, y as f64],
                                scale: 1.,
                            },
                        );
                    }
                    erase(&mut pixels, [x, y], &future);
                    image(&mut pdf, &mut content, &pixels, [x, y])?;
                    if shape_frame {
                        a.image = None;
                        a.fill = None;
                        vector(&mut pdf, &mut content, &a, bounds)?;
                    }
                } else {
                    vector(&mut pdf, &mut content, &a, bounds)?;
                }
            }
        }
    }
    if options.searchable_text {
        text_layer::write(&mut pdf, &mut content, document, &covers)?;
    }
    let drawing = pdf.form(&content, bounds, false)?;
    let mut kids = std::mem::take(kids_slot);
    for (page_index, page) in options
        .page
        .document_pages(document)?
        .into_iter()
        .enumerate()
    {
        let media = page.media_size;
        let bleed = page.setup.as_ref().map_or(0., |s| s.bleed);
        let [dx, dy, dw, dh] = page.destination;
        let scale = dw / page.source[2] as f64;
        let tx = dx - page.source[0] as f64 * scale;
        let mut page_content = format!(
            "q {} {} {} rg 0 0 {} {} re f\n{dx} {} {dw} {dh} re W n\n{scale} 0 0 -{scale} {tx} {} cm\n/{drawing} Do Q\n",
            options.background[0] as f64 / 255.,
            options.background[1] as f64 / 255.,
            options.background[2] as f64 / 255.,
            media[0],
            media[1],
            media[1] - dy - dh,
            media[1] - dy + page.source[1] as f64 * scale
        );
        let mut boxes = String::new();
        if let Some(s) = &page.setup {
            // Bleed exposes actual neighboring artwork, rather than stretching edge pixels.
            page_content = page_content.replacen(
                &format!("{dx} {} {dw} {dh} re W n", media[1] - dy - dh),
                &format!(
                    "{} {} {} {} re W n",
                    dx - bleed,
                    media[1] - dy - dh - bleed,
                    dw + bleed * 2.,
                    dh + bleed * 2.
                ),
                1,
            );
            boxes = format!(
                " /TrimBox [{dx} {dy} {} {}] /BleedBox [{} {} {} {}]",
                dx + dw,
                dy + dh,
                dx - bleed,
                dy - bleed,
                dx + dw + bleed,
                dy + dh + bleed
            );
            if s.crop_marks {
                page_content.push_str(&format!("q 0 G {} w\n", s.mark_weight));
                for x in [dx, dx + dw] {
                    for (y, dir) in [(dy, -1.), (dy + dh, 1.)] {
                        let _ = writeln!(
                            page_content,
                            "{x} {} m {x} {} l S",
                            y + dir * s.mark_offset,
                            y + dir * (s.mark_offset + s.mark_length)
                        );
                    }
                }
                for y in [dy, dy + dh] {
                    for (x, dir) in [(dx, -1.), (dx + dw, 1.)] {
                        let _ = writeln!(
                            page_content,
                            "{} {y} m {} {y} l S",
                            x + dir * s.mark_offset,
                            x + dir * (s.mark_offset + s.mark_length)
                        );
                    }
                }
                page_content.push_str("Q\n");
            }
        }
        let mut annotations = Vec::new();
        for (i, a) in document.annotations.iter().enumerate().filter(|(_, a)| {
            document.object_visible(a.id)
                && (document.canvas_behavior.page_owners.is_empty()
                    || document.object_page(a) == page_index)
        }) {
            for (b, url) in crate::text::rich::links(a) {
                // Any later visible covering object suppresses the entire link.
                // In particular, replacement effects must not leave a URI that
                // exposes text removed from the PDF's drawing streams.
                if document.annotations[i + 1..]
                    .iter()
                    .filter(|a| document.object_visible(a.id))
                    .any(|a| {
                        let (lo, hi) = a.bounds();
                        lo[0] < b[2] && hi[0] > b[0] && lo[1] < b[3] && hi[1] > b[1]
                    })
                {
                    continue;
                }
                let x0 = (b[0] as f64).max(page.source[0] as f64);
                let y0 = (b[1] as f64).max(page.source[1] as f64);
                let x1 = (b[2] as f64).min((page.source[0] + page.source[2]) as f64);
                let y1 = (b[3] as f64).min((page.source[1] + page.source[3]) as f64);
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                let rect = [
                    dx + (x0 - page.source[0] as f64) * scale,
                    media[1] - dy - (y1 - page.source[1] as f64) * scale,
                    dx + (x1 - page.source[0] as f64) * scale,
                    media[1] - dy - (y0 - page.source[1] as f64) * scale,
                ];
                let uri = url.bytes().map(|b| format!("{b:02X}")).collect::<String>();
                let id=pdf.add(format!("<< /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] /A << /S /URI /URI <{uri}> >> >>",rect[0],rect[1],rect[2],rect[3]).into_bytes());
                annotations.push(format!("{id} 0 R"));
            }
        }
        let annotations = if annotations.is_empty() {
            String::new()
        } else {
            format!(" /Annots [{}]", annotations.join(" "))
        };
        let stream = pdf.stream("", page_content.as_bytes())?;
        let id=pdf.add(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}]{boxes}{annotations} /Resources {} /Contents {stream} 0 R >>",media[0],media[1],pdf.resources()).into_bytes());
        kids.push(format!("{id} 0 R"));
    }
    *pdf_slot = pdf;
    *kids_slot = kids;
    Ok(())
}
