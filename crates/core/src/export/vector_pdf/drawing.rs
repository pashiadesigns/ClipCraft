//! Drawings (imported SVGs and editable PDF pages) export as shapes
//! (approved `vector-redaction-bounded`). Plain-color shapes, single-path
//! clips and pictures stay vector. Gradients, patterns, masks, filters and
//! group effects become small high-resolution pictures of just that part.
//! Anything a later redaction, blur or pixelation touches is left out of the
//! vector content entirely and drawn as a picture with the covered area
//! erased, so covered content is removed from the file, not hidden.
use super::*;
use resvg::{tiny_skia, usvg};
use std::collections::HashMap;

/// Pixels per document pixel for the parts that become pictures.
const RASTER_SCALE: f64 = 3.;
/// Largest picture made for one part.
const MAX_PART_PIXELS: f64 = 16_000_000.;

fn affine(t: tiny_skia::Transform) -> Affine {
    Affine::new([
        f64::from(t.sx),
        f64::from(t.ky),
        f64::from(t.kx),
        f64::from(t.sy),
        f64::from(t.tx),
        f64::from(t.ty),
    ])
}
fn transform(a: Affine) -> tiny_skia::Transform {
    let [sx, ky, kx, sy, tx, ty] = a.as_coeffs().map(|v| v as f32);
    tiny_skia::Transform::from_row(sx, ky, kx, sy, tx, ty)
}
fn rect(r: tiny_skia::Rect) -> Rect {
    Rect::new(
        f64::from(r.left()),
        f64::from(r.top()),
        f64::from(r.right()),
        f64::from(r.bottom()),
    )
}
/// Writes a path with its points moved by `t` (for clips, which must not
/// change the coordinate system they are set in).
fn moved_path(out: &mut String, p: &tiny_skia::Path, t: Affine) {
    let mut moved = p.clone();
    if t != Affine::IDENTITY {
        match p.clone().transform(transform(t)) {
            Some(m) => moved = m,
            None => return,
        }
    }
    skia_path(out, &moved);
}
fn skia_path(out: &mut String, p: &tiny_skia::Path) {
    use tiny_skia::PathSegment as S;
    let mut current = (0f32, 0f32);
    for s in p.segments() {
        match s {
            S::MoveTo(p) => {
                let _ = writeln!(out, "{:.4} {:.4} m", p.x, p.y);
                current = (p.x, p.y);
            }
            S::LineTo(p) => {
                let _ = writeln!(out, "{:.4} {:.4} l", p.x, p.y);
                current = (p.x, p.y);
            }
            S::QuadTo(c, p) => {
                let a = (
                    current.0 + (c.x - current.0) * 2. / 3.,
                    current.1 + (c.y - current.1) * 2. / 3.,
                );
                let b = (p.x + (c.x - p.x) * 2. / 3., p.y + (c.y - p.y) * 2. / 3.);
                let _ = writeln!(
                    out,
                    "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
                    a.0, a.1, b.0, b.1, p.x, p.y
                );
                current = (p.x, p.y);
            }
            S::CubicTo(a, b, p) => {
                let _ = writeln!(
                    out,
                    "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
                    a.x, a.y, b.x, b.y, p.x, p.y
                );
                current = (p.x, p.y);
            }
            S::Close => out.push_str("h\n"),
        }
    }
}

struct Writer<'a> {
    pdf: &'a mut Pdf,
    out: &'a mut String,
    /// Tree coordinates to document coordinates.
    place: Affine,
    future: &'a [Rect],
    bounds: Rect,
    states: HashMap<String, String>,
}
impl Writer<'_> {
    fn state(&mut self, dict: String) -> String {
        if let Some(name) = self.states.get(&dict) {
            return name.clone();
        }
        let name = self.pdf.state(dict.clone());
        self.states.insert(dict, name.clone());
        name
    }
    fn covered(&self, r: Rect) -> bool {
        self.future.iter().any(|p| p.intersect(r).area() > 0.)
    }
    fn doc_rect(&self, r: Rect) -> Rect {
        self.place.transform_rect_bbox(r)
    }
    /// Draws one node as a picture of its own area, with later redactions
    /// erased from it. `parent` is the transform of the node's parent.
    fn picture(&mut self, node: &usvg::Node, parent: Affine) -> Result<()> {
        let Some(layer) = node.abs_layer_bounding_box() else {
            return Ok(());
        };
        let area = self
            .doc_rect(rect(layer.to_rect()))
            .inflate(1., 1.)
            .intersect(self.bounds);
        if area.width() <= 0. || area.height() <= 0. {
            return Ok(());
        }
        let scale = RASTER_SCALE
            .min((MAX_PART_PIXELS / (area.width() * area.height())).sqrt())
            .max(0.05);
        let w = (area.width() * scale).ceil().max(1.) as u32;
        let h = (area.height() * scale).ceil().max(1.) as u32;
        let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
            return Ok(());
        };
        let want =
            Affine::scale(scale) * Affine::translate((-area.x0, -area.y0)) * self.place * parent;
        // render_node shifts by the node's top left first; undo that.
        let pass = want * Affine::translate((f64::from(layer.x()), f64::from(layer.y())));
        resvg::render_node(node, transform(pass), &mut pixmap.as_mut());
        self.emit(area, scale, &pixmap)
    }
    /// The whole drawing as one picture of its area: for drawings whose blend
    /// modes need what is beneath them inside the drawing.
    fn whole(&mut self, tree: &usvg::Tree) -> Result<()> {
        let size = tree.size();
        let area = self
            .doc_rect(Rect::new(
                0.,
                0.,
                f64::from(size.width()),
                f64::from(size.height()),
            ))
            .intersect(self.bounds);
        if area.width() <= 0. || area.height() <= 0. {
            return Ok(());
        }
        let scale = RASTER_SCALE
            .min((MAX_PART_PIXELS / (area.width() * area.height())).sqrt())
            .max(0.05);
        let w = (area.width() * scale).ceil().max(1.) as u32;
        let h = (area.height() * scale).ceil().max(1.) as u32;
        let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
            return Ok(());
        };
        let want = Affine::scale(scale) * Affine::translate((-area.x0, -area.y0)) * self.place;
        resvg::render(tree, transform(want), &mut pixmap.as_mut());
        self.emit(area, scale, &pixmap)
    }
    /// Erases later redactions from a picture of `area` and places it.
    fn emit(&mut self, area: Rect, scale: f64, pixmap: &tiny_skia::Pixmap) -> Result<()> {
        let (w, h) = (pixmap.width(), pixmap.height());
        let mut pixels = RgbaImage::new(w, h);
        for (p, c) in pixels.pixels_mut().zip(pixmap.pixels()) {
            let c = c.demultiply();
            *p = image::Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
        }
        // Remove whatever a later redaction covers, before it reaches the file.
        for r in self.future {
            let r = r.intersect(area);
            if r.area() <= 0. {
                continue;
            }
            let x0 = ((r.x0 - area.x0) * scale).floor().max(0.) as u32;
            let y0 = ((r.y0 - area.y0) * scale).floor().max(0.) as u32;
            let x1 = (((r.x1 - area.x0) * scale).ceil() as u32).min(w);
            let y1 = (((r.y1 - area.y0) * scale).ceil() as u32).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    pixels.put_pixel(x, y, image::Rgba([0; 4]));
                }
            }
        }
        if pixels.pixels().all(|p| p[3] == 0) {
            return Ok(());
        }
        let name = self.pdf.image(&pixels)?;
        // Placed from inside the drawing's coordinates, so the clips around
        // this part still apply to it.
        let unit = Affine::new([area.width(), 0., 0., -area.height(), area.x0, area.y1]);
        if self.place.determinant().abs() < 1e-12 {
            return Ok(());
        }
        self.out.push_str("q\n");
        matrix(self.out, self.place.inverse() * unit);
        let _ = writeln!(self.out, "/{name} Do Q");
        Ok(())
    }
    fn color(&mut self, paint: &usvg::Paint, opacity: f32, stroke: bool) -> Option<String> {
        let usvg::Paint::Color(c) = paint else {
            return None;
        };
        let mut s = format!(
            "{:.4} {:.4} {:.4} {}\n",
            f32::from(c.red) / 255.,
            f32::from(c.green) / 255.,
            f32::from(c.blue) / 255.,
            if stroke { "RG" } else { "rg" }
        );
        if opacity < 1. {
            let g = self.state(format!(
                "/{} {opacity:.4}",
                if stroke { "CA" } else { "ca" }
            ));
            let _ = writeln!(s, "/{g} gs");
        }
        Some(s)
    }
    fn path(&mut self, node: &usvg::Node, p: &usvg::Path) -> Result<()> {
        if !p.is_visible() {
            return Ok(());
        }
        let parent = affine(p.abs_transform());
        let fill = match p.fill() {
            Some(f) => match self.color(f.paint(), f.opacity().get(), false) {
                Some(c) => Some((c, f.rule())),
                None => return self.picture(node, parent),
            },
            None => None,
        };
        let stroke = match p.stroke() {
            Some(s) => match self.color(s.paint(), s.opacity().get(), true) {
                Some(c) => Some((c, s)),
                None => return self.picture(node, parent),
            },
            None => None,
        };
        if self.covered(self.doc_rect(rect(p.abs_stroke_bounding_box()))) {
            return self.picture(node, parent);
        }
        let mut body = String::new();
        skia_path(&mut body, p.data());
        let mut fill_ops = String::new();
        if let Some((color, rule)) = &fill {
            fill_ops.push_str(color);
            fill_ops.push_str(&body);
            fill_ops.push_str(if *rule == usvg::FillRule::EvenOdd {
                "f*\n"
            } else {
                "f\n"
            });
        }
        let mut stroke_ops = String::new();
        if let Some((color, s)) = &stroke {
            stroke_ops.push_str(color);
            let cap = match s.linecap() {
                usvg::LineCap::Butt => 0,
                usvg::LineCap::Round => 1,
                usvg::LineCap::Square => 2,
            };
            let join = match s.linejoin() {
                usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => 0,
                usvg::LineJoin::Round => 1,
                usvg::LineJoin::Bevel => 2,
            };
            let dash = s
                .dasharray()
                .map(|d| {
                    d.iter()
                        .map(|v| format!("{v:.4}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let _ = writeln!(
                stroke_ops,
                "{:.4} w {cap} J {join} j {:.4} M [{dash}] {:.4} d",
                s.width().get(),
                s.miterlimit().get(),
                s.dashoffset()
            );
            stroke_ops.push_str(&body);
            stroke_ops.push_str("S\n");
        }
        self.out.push_str("q\n");
        matrix(self.out, parent);
        if p.paint_order() == usvg::PaintOrder::StrokeAndFill {
            self.out.push_str(&stroke_ops);
            self.out.push_str(&fill_ops);
        } else {
            self.out.push_str(&fill_ops);
            self.out.push_str(&stroke_ops);
        }
        self.out.push_str("Q\n");
        Ok(())
    }
    fn image(&mut self, node: &usvg::Node, i: &usvg::Image) -> Result<()> {
        if !i.is_visible() {
            return Ok(());
        }
        let parent = affine(i.abs_transform());
        let data = match i.kind() {
            usvg::ImageKind::JPEG(d)
            | usvg::ImageKind::PNG(d)
            | usvg::ImageKind::GIF(d)
            | usvg::ImageKind::WEBP(d) => d.clone(),
            usvg::ImageKind::SVG(_) => return self.picture(node, parent),
        };
        if self.covered(self.doc_rect(rect(i.abs_bounding_box()))) {
            return self.picture(node, parent);
        }
        let Ok(decoded) = image::load_from_memory(&data) else {
            return self.picture(node, parent);
        };
        let pixels = decoded.to_rgba8();
        if u64::from(pixels.width()) * u64::from(pixels.height()) > crate::MAX_PIXELS {
            return self.picture(node, parent);
        }
        let name = self.pdf.image(&pixels)?;
        let size = i.size();
        self.out.push_str("q\n");
        matrix(self.out, parent);
        let _ = writeln!(
            self.out,
            "{} 0 0 -{} 0 {} cm /{name} Do Q",
            size.width(),
            size.height(),
            size.height()
        );
        Ok(())
    }
    /// A clip the PDF can express directly: one visible path, no nested clip.
    fn simple_clip(clip: &usvg::ClipPath) -> Option<(Affine, &usvg::Path)> {
        if clip.clip_path().is_some() {
            return None;
        }
        let mut transform = affine(clip.transform());
        let mut group = clip.root();
        loop {
            match group.children() {
                [usvg::Node::Path(p)] if p.is_visible() => return Some((transform, p)),
                [usvg::Node::Group(g)]
                    if g.clip_path().is_none() && g.mask().is_none() && g.filters().is_empty() =>
                {
                    transform *= affine(g.transform());
                    group = g;
                }
                _ => return None,
            }
        }
    }
    fn group(&mut self, node: &usvg::Node, g: &usvg::Group) -> Result<()> {
        let own = affine(g.abs_transform());
        let parent = own * affine(g.transform()).inverse();
        let clip = match g.clip_path() {
            Some(c) => match Self::simple_clip(c) {
                Some(found) => Some(found),
                None => return self.picture(node, parent),
            },
            None => None,
        };
        if g.opacity().get() < 1.
            || g.blend_mode() != usvg::BlendMode::Normal
            || g.mask().is_some()
            || !g.filters().is_empty()
        {
            return self.picture(node, parent);
        }
        if let Some((t, p)) = clip {
            self.out.push_str("q\n");
            moved_path(self.out, p.data(), own * t);
            let even_odd = p
                .fill()
                .is_some_and(|f| f.rule() == usvg::FillRule::EvenOdd);
            self.out.push_str(if even_odd { "W* n\n" } else { "W n\n" });
        }
        self.children(g)?;
        if clip.is_some() {
            self.out.push_str("Q\n");
        }
        Ok(())
    }
    fn children(&mut self, g: &usvg::Group) -> Result<()> {
        for node in g.children() {
            match node {
                usvg::Node::Group(c) => self.group(node, c)?,
                usvg::Node::Path(p) => self.path(node, p)?,
                usvg::Node::Image(i) => self.image(node, i)?,
                usvg::Node::Text(t) => self.children(t.flattened())?,
            }
        }
        Ok(())
    }
}

/// Writes a drawing as shapes. Returns false when the object needs the
/// ordinary picture path (crops, perspective, filters, recolor, rotation
/// handled elsewhere, partial opacity).
pub(super) fn write(
    pdf: &mut Pdf,
    out: &mut String,
    a: &crate::Annotation,
    future: &[Rect],
    bounds: Rect,
) -> Result<bool> {
    let Some(bitmap) = &a.image else {
        return Ok(false);
    };
    let Some(text) = bitmap.svg_text() else {
        return Ok(false);
    };
    if a.kind != crate::Kind::Image
        || a.corner_radius != 0.
        || a.image_opacity() != 255
        || bitmap.view().rect != [0., 0., 1., 1.]
        || bitmap.perspective().is_some()
        || bitmap.finish().is_some()
        || bitmap.tint().is_some()
        || a.shadow.is_some()
    {
        return Ok(false);
    }
    let tree = crate::vector_import::parse_for_export(text, bitmap.is_pdf_page())?;
    let size = tree.size();
    let (lo, hi) = a.body_bounds();
    let place = rotation(a)
        * Affine::translate((f64::from(lo[0]), f64::from(lo[1])))
        * Affine::scale_non_uniform(
            f64::from(hi[0] - lo[0]) / f64::from(size.width()),
            f64::from(hi[1] - lo[1]) / f64::from(size.height()),
        );
    out.push_str("q\n");
    matrix(out, place);
    // An SVG shows only its own canvas.
    let _ = writeln!(out, "0 0 {} {} re W n", size.width(), size.height());
    let mut writer = Writer {
        pdf,
        out,
        place,
        future,
        bounds,
        states: HashMap::new(),
    };
    if blends(tree.root()) {
        writer.whole(&tree)?;
    } else {
        writer.children(tree.root())?;
    }
    writer.out.push_str("Q\n");
    Ok(true)
}
/// Whether any group mixes with what is beneath it.
fn blends(group: &usvg::Group) -> bool {
    group.children().iter().any(|node| match node {
        usvg::Node::Group(g) => g.blend_mode() != usvg::BlendMode::Normal || blends(g),
        usvg::Node::Text(t) => blends(t.flattened()),
        _ => false,
    })
}
