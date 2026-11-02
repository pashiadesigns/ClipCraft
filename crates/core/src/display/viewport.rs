//! Viewport-sized vector rasterization. Source pixels and image effects retain
//! their native sampling; editable geometry is rasterized at display resolution.
//! Original implementation using Vello CPU (MIT OR Apache-2.0). No document edits.
use super::{DisplayList, Layer};
use crate::{Annotation, Error, Kind, Result};
use image::{Rgba, RgbaImage};
use vello_cpu::{
    Pixmap, RenderContext, Resources,
    color::{AlphaColor, Srgb},
    kurbo::{Affine, BezPath, Cap, Ellipse, Join, Point, Rect, Shape, Stroke},
};

#[cfg(test)]
#[path = "viewport_tests.rs"]
mod tests;

/// Physical output pixels; `origin` is the image coordinate at the upper left.
/// Scale includes both canvas zoom and device pixel ratio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub size: [u32; 2],
    pub origin: [f64; 2],
    pub scale: f64,
}

impl Viewport {
    fn validate(self) -> Result<()> {
        if self.size.contains(&0)
            || self.size.iter().any(|n| *n > 8192)
            || u64::from(self.size[0]) * u64::from(self.size[1]) > 8_388_608
            || !self
                .origin
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 1_000_000.0)
            || !self.scale.is_finite()
            || !(0.001..=256.0).contains(&self.scale)
        {
            return Err(Error::Invalid(
                "Vector viewport is invalid or exceeds its display memory budget".into(),
            ));
        }
        Ok(())
    }

    fn transform(self) -> Affine {
        Affine::scale(self.scale) * Affine::translate((-self.origin[0], -self.origin[1]))
    }
}

impl DisplayList {
    pub fn render_viewport(&self, viewport: Viewport) -> Result<RgbaImage> {
        viewport.validate()?;
        let mut result = if let Some(color) = self.base_color {
            let color = if color[3] == 0 && viewport.scale < 1.0 {
                Rgba([0; 4])
            } else {
                color
            };
            RgbaImage::from_pixel(viewport.size[0], viewport.size[1], color)
        } else {
            let mut image = RgbaImage::new(viewport.size[0], viewport.size[1]);
            replace(&mut image, &self.base, [0, 0], viewport);
            image
        };
        for layer in &self.layers {
            match layer {
                Layer::Vector(annotation) => render_vector(&mut result, annotation, viewport),
                Layer::Shadow(patch) => blend_patch(&mut result, patch, viewport),
                Layer::ReplacePixels { origin, pixels } => {
                    // Erased alpha replaces the earlier pixels, never source-over.
                    replace(&mut result, pixels, *origin, viewport);
                }
            }
        }
        // Clip annotation overshoot to the image, including when the viewport
        // extends beyond it. The checkerboard is painted by the UI underneath.
        for (x, y, pixel) in result.enumerate_pixels_mut() {
            let ix = viewport.origin[0] + (f64::from(x) + 0.5) / viewport.scale;
            let iy = viewport.origin[1] + (f64::from(y) + 0.5) / viewport.scale;
            if ix < 0.0
                || iy < 0.0
                || ix >= f64::from(self.base.width())
                || iy >= f64::from(self.base.height())
            {
                *pixel = Rgba([0; 4]);
            }
        }
        Ok(result)
    }
}

/// Transparent overlay using the exact committed vector geometry. The editor
/// positions this above the existing image while an annotation is being drawn.
pub fn render_annotation_preview(annotation: &Annotation, viewport: Viewport) -> Result<RgbaImage> {
    viewport.validate()?;
    if matches!(annotation.kind, Kind::Blur | Kind::Pixelate | Kind::Eraser) {
        return Err(Error::Invalid(
            "Pixel effects require the underlying image".into(),
        ));
    }
    let mut result = RgbaImage::new(viewport.size[0], viewport.size[1]);
    if let Some(patch) = crate::shadow::patch(annotation) {
        blend_patch(&mut result, &patch, viewport);
    }
    render_vector(&mut result, annotation, viewport);
    Ok(result)
}

fn object_transform(a: &Annotation) -> Affine {
    let rotation = a.rotation();
    Affine::translate((f64::from(rotation.center[0]), f64::from(rotation.center[1])))
        * Affine::rotate(f64::from(rotation.degrees).to_radians())
        * Affine::translate((
            -f64::from(rotation.center[0]),
            -f64::from(rotation.center[1]),
        ))
}

pub(crate) fn render_rotated(image: &mut RgbaImage, a: &Annotation) {
    let viewport = Viewport {
        size: [image.width(), image.height()],
        origin: [0.0; 2],
        scale: 1.0,
    };
    render_vector(image, a, viewport);
}

// Rasterize only the visible object's bounds. Normalize geometry before layout,
// so adding integer canvas margins cannot change curve/glyph rounding.
pub(crate) fn render_vector(image: &mut RgbaImage, a: &Annotation, viewport: Viewport) {
    if a.analysis.is_some() {
        return;
    }
    if a.is_shape() && a.image.is_some() {
        let mut shell = a.clone();
        shell.image = None;
        shell.text = None;
        shell.color[3] = 0;
        render_vector(image, &shell, viewport);
        crate::bitmap::render(image, a, viewport);
        shell.color = a.color;
        shell.fill = None;
        render_vector(image, &shell, viewport);
        return;
    }
    if a.kind == Kind::Image && !a.image.as_ref().is_some_and(|b| b.vector().is_some()) {
        crate::bitmap::render(image, a, viewport);
        return;
    }
    let (min, max) = a.bounds();
    let margin = if a.kind == Kind::Arrow {
        a.width as f32 * 4.0 + 2.0
    } else {
        a.width as f32 * 0.5 + 2.0
    };
    let low = [min[0] - margin, min[1] - margin];
    let high = [max[0] + margin, max[1] + margin];
    let screen = |v: f32, axis: usize| (f64::from(v) - viewport.origin[axis]) * viewport.scale;
    let left = screen(low[0], 0)
        .floor()
        .clamp(0.0, f64::from(image.width())) as u32;
    let top = screen(low[1], 1)
        .floor()
        .clamp(0.0, f64::from(image.height())) as u32;
    let right = screen(high[0], 0)
        .ceil()
        .clamp(0.0, f64::from(image.width())) as u32;
    let bottom = screen(high[1], 1)
        .ceil()
        .clamp(0.0, f64::from(image.height())) as u32;
    if right <= left || bottom <= top {
        return;
    }
    let anchor = a.body_bounds().0.map(f32::floor);
    let local = crate::canvas_expansion::translated(std::slice::from_ref(a), anchor.map(|v| -v))
        .expect("validated annotation coordinates")
        .remove(0);
    let transform = Affine::translate((-f64::from(left), -f64::from(top)))
        * viewport.transform()
        * Affine::translate((f64::from(anchor[0]), f64::from(anchor[1])))
        * object_transform(&local);
    let size = [right - left, bottom - top];
    let mut context = RenderContext::new(size[0] as u16, size[1] as u16);
    let mut resources = Resources::new();
    let mut pixels = Pixmap::new(size[0] as u16, size[1] as u16);
    let mut finish = |context: &mut RenderContext, ink: [u8; 4]| {
        context.flush();
        context.render(&mut pixels, &mut resources);
        for (index, coverage) in pixels.data().iter().enumerate() {
            let mut color = ink;
            color[3] = ((u16::from(ink[3]) * u16::from(coverage.a) + 127) / 255) as u8;
            if color[3] != 0 {
                let x = index as u32 % size[0] + left;
                let y = index as u32 / size[0] + top;
                crate::text::composite(image.get_pixel_mut(x, y), color);
            }
        }
        context.reset();
        context.set_transform(transform);
        context.set_paint(color([255; 4]));
        context.set_stroke(
            Stroke::new(f64::from(a.width))
                .with_caps(Cap::Round)
                .with_join(Join::Round),
        );
    };
    context.set_transform(transform);
    context.set_paint(color([255; 4]));
    paint_annotation(&mut context, &local, viewport.scale, &mut finish);
}

fn blend_patch(target: &mut RgbaImage, patch: &crate::shadow::Patch, viewport: Viewport) {
    let origin = patch.origin.map(f64::from);
    let size = [patch.pixels.width(), patch.pixels.height()];
    let low = [0, 1].map(|i| {
        ((origin[i] - viewport.origin[i]) * viewport.scale)
            .floor()
            .max(0.) as u32
    });
    let high = [0, 1].map(|i| {
        ((origin[i] + f64::from(size[i]) - viewport.origin[i]) * viewport.scale)
            .ceil()
            .clamp(0., f64::from(viewport.size[i])) as u32
    });
    for y in low[1]..high[1] {
        for x in low[0]..high[0] {
            let p = [x, y].map(f64::from);
            let sample = [0, 1]
                .map(|i| viewport.origin[i] + (p[i] + 0.5) / viewport.scale - origin[i] - 0.5);
            let pixel = sample_linear(&patch.pixels, sample);
            if pixel[3] != 0 {
                crate::text::composite(target.get_pixel_mut(x, y), pixel.0);
            }
        }
    }
}

fn replace(target: &mut RgbaImage, source: &RgbaImage, origin: [u32; 2], viewport: Viewport) {
    let left = ((f64::from(origin[0]) - viewport.origin[0]) * viewport.scale)
        .floor()
        .max(0.0) as u32;
    let top = ((f64::from(origin[1]) - viewport.origin[1]) * viewport.scale)
        .floor()
        .max(0.0) as u32;
    let right = ((f64::from(origin[0]) + f64::from(source.width()) - viewport.origin[0])
        * viewport.scale)
        .ceil()
        .clamp(0.0, f64::from(target.width())) as u32;
    let bottom = ((f64::from(origin[1]) + f64::from(source.height()) - viewport.origin[1])
        * viewport.scale)
        .ceil()
        .clamp(0.0, f64::from(target.height())) as u32;
    for y in top..bottom {
        let sy = (viewport.origin[1] + (f64::from(y) + 0.5) / viewport.scale).floor()
            - f64::from(origin[1]);
        if sy < 0.0 || sy >= f64::from(source.height()) {
            continue;
        }
        for x in left..right {
            let sx = (viewport.origin[0] + (f64::from(x) + 0.5) / viewport.scale).floor()
                - f64::from(origin[0]);
            if sx >= 0.0 && sx < f64::from(source.width()) {
                let pixel = if viewport.scale < 1.0 {
                    sample_linear(
                        source,
                        [
                            viewport.origin[0] + (f64::from(x) + 0.5) / viewport.scale
                                - f64::from(origin[0])
                                - 0.5,
                            viewport.origin[1] + (f64::from(y) + 0.5) / viewport.scale
                                - f64::from(origin[1])
                                - 0.5,
                        ],
                    )
                } else {
                    *source.get_pixel(sx as u32, sy as u32)
                };
                target.put_pixel(x, y, pixel);
            }
        }
    }
}

// Match the canvas's existing linear minification and nearest magnification.
// Interpolate premultiplied colors so hidden RGB cannot fringe transparent edges.
fn sample_linear(source: &RgbaImage, p: [f64; 2]) -> Rgba<u8> {
    let mut sum = [0.0; 4];
    let fraction = [p[0] - p[0].floor(), p[1] - p[1].floor()];
    for y in 0..2 {
        for x in 0..2 {
            let weight = (if x == 0 {
                1.0 - fraction[0]
            } else {
                fraction[0]
            }) * (if y == 0 {
                1.0 - fraction[1]
            } else {
                fraction[1]
            });
            let pixel = source.get_pixel(
                (p[0].floor() + f64::from(x)).clamp(0.0, f64::from(source.width() - 1)) as u32,
                (p[1].floor() + f64::from(y)).clamp(0.0, f64::from(source.height() - 1)) as u32,
            );
            let alpha = f64::from(pixel[3]) * weight;
            for c in 0..3 {
                sum[c] += f64::from(pixel[c]) * alpha;
            }
            sum[3] += alpha;
        }
    }
    if sum[3] == 0.0 {
        return Rgba([0; 4]);
    }
    Rgba([
        (sum[0] / sum[3]).round() as u8,
        (sum[1] / sum[3]).round() as u8,
        (sum[2] / sum[3]).round() as u8,
        sum[3].round() as u8,
    ])
}

fn point(p: [f32; 2]) -> Point {
    Point::new(f64::from(p[0]), f64::from(p[1]))
}
fn rect(min: [f32; 2], max: [f32; 2]) -> Rect {
    Rect::from_points(point(min), point(max))
}
fn color(c: [u8; 4]) -> AlphaColor<Srgb> {
    AlphaColor::from_rgba8(c[0], c[1], c[2], c[3])
}

fn polygon(points: &[[f32; 2]], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    if let Some(first) = points.first() {
        path.move_to(point(*first));
        for p in points.iter().skip(1) {
            path.line_to(point(*p));
        }
        if closed {
            path.close_path();
        }
    }
    path
}

// Keep the shared callout's pointer placement and radius limits, but express
// its four rounded body corners as curves rather than a fixed-resolution mesh.
pub(crate) fn body_path(a: &Annotation) -> BezPath {
    let (min, max) = a.body_bounds();
    let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
    let points = if a.kind == Kind::Callout {
        let mut unrounded = a.clone();
        unrounded.corner_radius = 0.0;
        unrounded.callout_polygon()
    } else {
        corners.to_vec()
    };
    let mut path = BezPath::new();
    for (i, &p) in points.iter().enumerate() {
        let corner = point(p);
        let before = point(points[(i + points.len() - 1) % points.len()]) - corner;
        let after = point(points[(i + 1) % points.len()]) - corner;
        let radius = if corners.contains(&p) {
            f64::from(a.corner_radius)
                .min(before.hypot() * 0.5)
                .min(after.hypot() * 0.5)
        } else {
            0.0
        };
        let entry = if radius > 0.0 {
            corner + before * (radius / before.hypot())
        } else {
            corner
        };
        if i == 0 {
            path.move_to(entry);
        } else {
            path.line_to(entry);
        }
        if radius > 0.0 {
            let exit = corner + after * (radius / after.hypot());
            let k = 0.552_284_749_830_793_6;
            path.curve_to(
                entry + (corner - entry) * k,
                exit + (corner - exit) * k,
                exit,
            );
        }
    }
    path.close_path();
    path
}

pub(super) fn paint_annotation<P: super::vector::Painter>(
    context: &mut P,
    a: &Annotation,
    scale: f64,
    finish: &mut impl FnMut(&mut P, [u8; 4]),
) {
    let (min, max) = a.body_bounds();
    let tolerance = 0.1 / scale;
    context.set_stroke(
        Stroke::new(f64::from(a.width))
            .with_caps(Cap::Round)
            .with_join(Join::Round),
    );
    match a.kind {
        Kind::Simplify => {
            context.fill_path(&body_path(a));
            finish(context, a.color);
        }
        Kind::Image => {
            if let Some(vector) = a.image.as_ref().and_then(|b| b.vector()) {
                let sx = f64::from(max[0] - min[0]) / 128.;
                let sy = f64::from(max[1] - min[1]) / 128.;
                let transform = Affine::translate((f64::from(min[0]), f64::from(min[1])))
                    * Affine::scale_non_uniform(sx, sy);
                for layer in &vector.paths {
                    if let Ok(path) = BezPath::from_svg(&layer.data) {
                        let path = transform * path;
                        if let Some(width) = layer.stroke {
                            context.set_stroke(
                                Stroke::new(width * sx.min(sy))
                                    .with_caps(Cap::Round)
                                    .with_join(Join::Round),
                            );
                            context.stroke_path(&path);
                        } else {
                            context.fill_path(&path);
                        }
                        let mut ink = vector.colors[layer.color];
                        ink[3] = ((u16::from(ink[3]) * u16::from(a.color[3]) + 127) / 255) as u8;
                        finish(context, ink);
                    }
                }
            }
        }
        Kind::Rectangle | Kind::Ellipse | Kind::Polygon | Kind::Callout | Kind::Step => {
            let path = match a.kind {
                Kind::Rectangle | Kind::Callout => body_path(a),
                Kind::Polygon => polygon(&a.points, true),
                _ => Ellipse::from_rect(rect(min, max)).to_path(tolerance),
            };
            let fill = match a.kind {
                Kind::Callout => a.callout.as_ref().map(|s| s.fill),
                Kind::Step => Some(a.color),
                _ => a.fill,
            };
            if let Some(fill) = fill.filter(|c| c[3] > 0) {
                context.fill_path(&path);
                finish(context, fill);
            }
            if let Some(outline) = a.step_outline.as_ref().filter(|_| a.kind == Kind::Step) {
                // Inset stroke keeps the marker diameter and selection bounds fixed.
                let width = f64::from(outline.width)
                    .min(f64::from((max[0] - min[0]).min(max[1] - min[1])) * 0.5);
                if width > 0.0 && outline.color[3] > 0 {
                    let path =
                        Ellipse::from_rect(rect(min, max).inset(-width * 0.5)).to_path(tolerance);
                    context.set_stroke(Stroke::new(width));
                    context.stroke_path(&path);
                    finish(context, outline.color);
                }
            } else if a.kind != Kind::Step {
                let outline = a
                    .callout
                    .as_ref()
                    .filter(|_| a.kind == Kind::Callout)
                    .map(|s| s.outline)
                    .unwrap_or(a.color);
                context.stroke_path(&path);
                finish(context, outline);
            }
        }
        Kind::Line | Kind::Arrow if a.line_style.is_some() => {
            let style = a.line_style.unwrap();
            let geometry = crate::line_style::geometry(a);
            let stroke = Stroke::new(f64::from(a.width))
                .with_caps(Cap::Butt)
                .with_join(Join::Round)
                .with_dashes(0., style.pattern.dashes(f64::from(a.width)));
            context.set_stroke(stroke);
            context.stroke_path(&geometry.shaft);
            for head in geometry.heads {
                if let Some(width) = head.outline {
                    context.set_stroke(
                        Stroke::new(width)
                            .with_caps(Cap::Round)
                            .with_join(Join::Round),
                    );
                    context.stroke_path(&head.path);
                } else {
                    context.fill_path(&head.path);
                }
            }
            finish(context, a.color);
        }
        Kind::Highlighter if a.highlighter_rect => {
            context.fill_rect(&rect(min, max));
            finish(context, a.color);
        }
        Kind::Line | Kind::Arrow | Kind::Pen | Kind::Highlighter => {
            let mut path = if matches!(a.kind, Kind::Pen | Kind::Highlighter) {
                polygon(&a.points, false)
            } else {
                polygon(&[a.start, a.end], false)
            };
            if a.kind == Kind::Arrow
                && let Some(wings) = a.arrow_head()
            {
                path.move_to(point(wings[0]));
                path.line_to(point(a.end));
                path.line_to(point(wings[1]));
            }
            context.stroke_path(&path);
            finish(context, a.color);
        }
        Kind::Redact => {
            context.fill_rect(&rect(min, max));
            finish(context, [0, 0, 0, 255]);
        }
        Kind::Text => {}
        Kind::Blur | Kind::Pixelate | Kind::Eraser => {
            unreachable!("raster effects are replacement layers")
        }
    }
    let foreground;
    let label = if a.kind == Kind::Step {
        foreground = crate::steps::foreground(a);
        &foreground
    } else {
        a
    };
    if let Some(ink) = crate::text::typography::highlight(context, label) {
        finish(context, ink);
    }
    if let Some(outline) = label.text.as_ref().and_then(|s| s.outline)
        && outline.color[3] > 0
        && crate::text::outline::paint_coverage(context, label, outline.width.into(), tolerance)
            .unwrap_or(false)
    {
        finish(context, outline.color);
    }
    if let Some(style) = &label.text
        && let Some(ink) = crate::text::paint_vector_coverage(
            context,
            label,
            style.underline,
            style.strikethrough,
            tolerance,
        )
    {
        finish(context, ink);
    }
}
