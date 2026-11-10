//! One editable source window and its enlarged, connected view of lower layers.
use crate::{Annotation, Error, Kind, Result};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Magnified {
    /// Offset in source-window widths/heights, so moves/resizes remain coherent.
    pub offset: [f32; 2],
    pub scale: f32,
    pub connectors: bool,
}
impl Default for Magnified {
    fn default() -> Self {
        Self {
            offset: [2., 0.],
            scale: 2.,
            connectors: true,
        }
    }
}
impl Magnified {
    pub fn validate(self) -> Result<()> {
        if !self.scale.is_finite()
            || !(1. ..=8.).contains(&self.scale)
            || self.offset.iter().any(|v| !v.is_finite() || v.abs() > 100.)
        {
            return Err(Error::Invalid("Set callout magnification to 1–8× and keep its offset within 100 source widths/heights.".into()));
        }
        Ok(())
    }
}
pub fn lens(a: &Annotation) -> Option<Annotation> {
    let m = a.focus?.magnify?;
    let (lo, hi) = a.body_bounds();
    let size = [hi[0] - lo[0], hi[1] - lo[1]];
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let offset = a.rotation().point([
        center[0] + m.offset[0] * size[0],
        center[1] + m.offset[1] * size[1],
    ]);
    let mut out = a.clone();
    out.focus = None;
    out.fill = None;
    out.text = None;
    out.shadow = None;
    out.start = [
        offset[0] - size[0] * m.scale * 0.5,
        offset[1] - size[1] * m.scale * 0.5,
    ];
    out.end = [
        offset[0] + size[0] * m.scale * 0.5,
        offset[1] + size[1] * m.scale * 0.5,
    ];
    out.corner_radius *= m.scale;
    Some(out)
}
/// Prefer the right, then left, then above/below; keep the enlarged view on page.
pub fn place_on_canvas(a: &mut Annotation, size: [u32; 2]) {
    let Some(mut m) = a.focus.and_then(|f| f.magnify) else {
        return;
    };
    let (lo, hi) = a.body_bounds();
    let w = (hi[0] - lo[0]).max(1.);
    let h = (hi[1] - lo[1]).max(1.);
    m.scale = m
        .scale
        .min((size[0] as f32 / w).min(size[1] as f32 / h))
        .max(1.);
    let half = [w * m.scale * 0.5, h * m.scale * 0.5];
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let gap = 20.;
    let candidates = [
        [hi[0] + gap + half[0], center[1]],
        [lo[0] - gap - half[0], center[1]],
        [center[0], hi[1] + gap + half[1]],
        [center[0], lo[1] - gap - half[1]],
    ];
    let mut c = candidates
        .into_iter()
        .find(|c| (0..2).all(|i| c[i] - half[i] >= 0. && c[i] + half[i] <= size[i] as f32))
        .unwrap_or(candidates[0]);
    for i in 0..2 {
        c[i] = c[i].clamp(
            half[i].min(size[i] as f32 * 0.5),
            (size[i] as f32 - half[i]).max(size[i] as f32 * 0.5),
        );
    }
    m.offset = [(c[0] - center[0]) / w, (c[1] - center[1]) / h];
    a.focus.as_mut().unwrap().magnify = Some(m);
}
/// Connector end points for (rounded) rectangles: the source's corners nearest
/// the enlarged view link to the enlarged view's corners nearest the source,
/// pairing the same side (top with top, or left with left). Rounded corners
/// attach to the corner arc, so the lines meet both outlines.
fn rect_links(
    a: &Annotation,
    b: &Annotation,
    center: [f32; 2],
    bc: [f32; 2],
    extent: [f32; 2],
    scale: f32,
) -> Vec<([f32; 2], [f32; 2])> {
    let d = [bc[0] - center[0], bc[1] - center[1]];
    let span = [extent[0] * (1. + scale), extent[1] * (1. + scale)];
    // Overlapping windows have no outside corners to join.
    if d[0].abs() < span[0] && d[1].abs() < span[1] {
        return vec![];
    }
    let radius = |x: &Annotation, half: [f32; 2]| x.corner_radius.clamp(0., half[0].min(half[1]));
    let source_radius = radius(a, extent);
    let lens_radius = radius(b, [extent[0] * scale, extent[1] * scale]);
    // The point where a rounded corner's arc crosses its diagonal.
    let corner = |c: [f32; 2], s: f32, x: f32, y: f32, r: f32| {
        let inset = r * (1. - std::f32::consts::FRAC_1_SQRT_2);
        [
            c[0] + x * (extent[0] * s - inset),
            c[1] + y * (extent[1] * s - inset),
        ]
    };
    if d[0].abs() / span[0] >= d[1].abs() / span[1] {
        let side = d[0].signum();
        [-1., 1.]
            .map(|y| {
                (
                    corner(center, 1., side, y, source_radius),
                    corner(bc, scale, -side, y, lens_radius),
                )
            })
            .to_vec()
    } else {
        let side = d[1].signum();
        [-1., 1.]
            .map(|x| {
                (
                    corner(center, 1., x, side, source_radius),
                    corner(bc, scale, x, -side, lens_radius),
                )
            })
            .to_vec()
    }
}
pub fn connectors(a: &Annotation) -> Vec<Annotation> {
    let Some(m) = a.focus.and_then(|f| f.magnify).filter(|m| m.connectors) else {
        return vec![];
    };
    let b = lens(a).unwrap();
    let (lo, hi) = a.body_bounds();
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let extent = [(hi[0] - lo[0]) * 0.5, (hi[1] - lo[1]) * 0.5];
    if extent.iter().any(|v| *v <= 0.) {
        return vec![];
    }
    let bc = [
        center[0] + m.offset[0] * extent[0] * 2.,
        center[1] + m.offset[1] * extent[1] * 2.,
    ];
    let pairs = if a.kind == Kind::Ellipse {
        // Both ellipses are homothetic. Normalize to circles, solve the two
        // external common tangents, then restore axes and shared rotation.
        let d = [m.offset[0] * 2., m.offset[1] * 2.];
        let distance = d[0].hypot(d[1]);
        if distance <= (m.scale - 1.).abs() + 0.0001 {
            return vec![];
        }
        let u = [d[0] / distance, d[1] / distance];
        let h = (1. - m.scale) / distance;
        let side = (1. - h * h).max(0.).sqrt();
        [-1., 1.]
            .into_iter()
            .map(|sign| {
                let n = [u[0] * h - sign * u[1] * side, u[1] * h + sign * u[0] * side];
                (
                    [center[0] + extent[0] * n[0], center[1] + extent[1] * n[1]],
                    [
                        bc[0] + extent[0] * m.scale * n[0],
                        bc[1] + extent[1] * m.scale * n[1],
                    ],
                )
            })
            .collect::<Vec<_>>()
    } else {
        rect_links(a, &b, center, bc, extent, m.scale)
    };
    pairs
        .into_iter()
        .map(|(start, end)| {
            let mut line = b.clone();
            line.kind = Kind::Line;
            line.corner_radius = 0.;
            line.rotation_degrees = 0.;
            line.mirror_x = false;
            line.start = a.rotation().point(start);
            line.end = a.rotation().point(end);
            line
        })
        .collect()
}
/// The same patterned outline geometry used by export, flattened only for the
/// retained GPU stroke. No image sampling or texture allocation is involved.
pub fn stroke_paths(a: &Annotation, tolerance: f64) -> Vec<Vec<[f32; 2]>> {
    use vello_cpu::kurbo::{BezPath, Ellipse, PathEl, Rect, Shape, dash, flatten};
    if a.width == 0 || a.color[3] == 0 {
        return vec![];
    }
    let pattern = a
        .line_style
        .map_or(crate::line_style::Pattern::Solid, |s| s.pattern);
    let (lo, hi) = a.body_bounds();
    let path = match a.kind {
        Kind::Line => crate::line_style::geometry(a).shaft,
        Kind::Ellipse => pattern.path(
            &Ellipse::from_rect(Rect::new(
                lo[0] as f64,
                lo[1] as f64,
                hi[0] as f64,
                hi[1] as f64,
            ))
            .to_path(tolerance),
            a.width as f64,
        ),
        _ => pattern.path(&crate::display::frame_body_path(a), a.width as f64),
    };
    let dashes = pattern.dashes(a.width as f64);
    let path = if dashes.is_empty() {
        path
    } else {
        dash(path.iter(), 0., &dashes).collect::<BezPath>()
    };
    let mut paths = vec![];
    let mut points = vec![];
    flatten(path.iter(), tolerance.max(0.01), |el| match el {
        PathEl::MoveTo(p) => {
            if !points.is_empty() {
                paths.push(std::mem::take(&mut points));
            }
            points.push(a.rotation().point([p.x as f32, p.y as f32]));
        }
        PathEl::LineTo(p) => points.push(a.rotation().point([p.x as f32, p.y as f32])),
        PathEl::ClosePath => {
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
        }
        _ => {}
    });
    if !points.is_empty() {
        paths.push(points);
    }
    paths
}
pub(crate) fn render(out: &mut RgbaImage, source: &RgbaImage, a: &Annotation) {
    let Some(b) = lens(a) else { return };
    let opacity = a.focus.unwrap().opacity;
    for c in connectors(a) {
        crate::render_annotation(out, &c);
    }
    let (lo, hi) = b.bounds();
    let (blo, bhi) = b.body_bounds();
    let (slo, shi) = a.body_bounds();
    let size = [bhi[0] - blo[0], bhi[1] - blo[1]];
    if size.iter().any(|v| *v <= 0.) {
        return;
    }
    let inverse = b.rotation();
    let source_rotation = a.rotation();
    for y in lo[1].floor().max(0.) as u32..(hi[1].ceil().max(0.) as u32).min(out.height()) {
        for x in lo[0].floor().max(0.) as u32..(hi[0].ceil().max(0.) as u32).min(out.width()) {
            let p = inverse.inverse_point([x as f32 + 0.5, y as f32 + 0.5]);
            let local = [p[0] - blo[0], p[1] - blo[1]];
            if local[0] < 0. || local[1] < 0. || local[0] >= size[0] || local[1] >= size[1] {
                continue;
            }
            let coverage =
                crate::image_frame::coverage(&b, [local[0] as f64, local[1] as f64], size, 1.)
                    as f32;
            let uv = source_rotation.point([
                slo[0] + local[0] / size[0] * (shi[0] - slo[0]),
                slo[1] + local[1] / size[1] * (shi[1] - slo[1]),
            ]);
            let sx = uv[0].floor() as i32;
            let sy = uv[1].floor() as i32;
            if sx < 0 || sy < 0 || sx >= source.width() as i32 || sy >= source.height() as i32 {
                continue;
            }
            // Match the retained preview's linear texture sampling, including alpha.
            let fx = (uv[0] - 0.5).clamp(0., source.width() as f32 - 1.);
            let fy = (uv[1] - 0.5).clamp(0., source.height() as f32 - 1.);
            let x0 = fx.floor() as u32;
            let y0 = fy.floor() as u32;
            let dx = fx - x0 as f32;
            let dy = fy - y0 as f32;
            let mut premult = [0f32; 4];
            for (x, y, w) in [
                (x0, y0, (1. - dx) * (1. - dy)),
                ((x0 + 1).min(source.width() - 1), y0, dx * (1. - dy)),
                (x0, (y0 + 1).min(source.height() - 1), (1. - dx) * dy),
                (
                    (x0 + 1).min(source.width() - 1),
                    (y0 + 1).min(source.height() - 1),
                    dx * dy,
                ),
            ] {
                let p = source.get_pixel(x, y);
                let alpha = p[3] as f32 / 255.;
                for c in 0..3 {
                    premult[c] += p[c] as f32 * alpha * w;
                }
                premult[3] += alpha * w;
            }
            let rgb = [0, 1, 2].map(|c| {
                if premult[3] > 0. {
                    (premult[c] / premult[3]).round() as u8
                } else {
                    0
                }
            });
            let p = Rgba([rgb[0], rgb[1], rgb[2], (premult[3] * 255.).round() as u8]);
            let dst = out.get_pixel_mut(x, y);
            let alpha = p[3] as f32 / 255. * coverage * opacity;
            let old = dst[3] as f32 / 255.;
            let total = alpha + old * (1. - alpha);
            if total > 0. {
                let rgb = [0, 1, 2].map(|i| {
                    ((p[i] as f32 * alpha + dst[i] as f32 * old * (1. - alpha)) / total).round()
                        as u8
                });
                *dst = Rgba([rgb[0], rgb[1], rgb[2], (total * 255.).round() as u8]);
            }
        }
    }
    if b.width > 0 {
        crate::render_annotation(out, &b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn magnified_strokes_preserve_patterns_and_reject_invalid_hit_tests() {
        let mut a = object();
        a.width = 3;
        a.color = [20, 140, 190, 255];
        assert_eq!(stroke_paths(&a, 0.1).len(), 1);
        for pattern in [
            crate::line_style::Pattern::Dashed,
            crate::line_style::Pattern::Dotted,
        ] {
            a.line_style = Some(crate::line_style::Style {
                pattern,
                ..crate::line_style::Style::for_kind(a.kind)
            });
            assert!(stroke_paths(&a, 0.1).len() > 3);
            assert!(
                connectors(&a)
                    .iter()
                    .all(|line| stroke_paths(line, 0.1).len() > 1)
            );
        }
        assert!(!a.hit_test([130., 50.], -1.));
        assert!(!a.hit_test([130., 50.], f32::INFINITY));
    }
    fn object() -> Annotation {
        let mut a =
            crate::steps::marker(1, [50., 50.], 40, [0; 4], (1, false), [320, 180]).unwrap();
        a.kind = Kind::Rectangle;
        a.start = [30., 30.];
        a.end = [70., 70.];
        a.width = 0;
        a.fill = None;
        a.text = None;
        a.focus = Some(crate::focus::Focus {
            darken: 0.,
            magnify: Some(Magnified::default()),
            ..Default::default()
        });
        a
    }
    #[test]
    fn samples_only_lower_layers_preserves_source_and_saves_editable_geometry() {
        let mut d = crate::Document::from_image(RgbaImage::from_fn(320, 180, |x, y| {
            Rgba([x as u8, y as u8, 0, 255])
        }))
        .unwrap();
        let a = object();
        let b = lens(&a).unwrap();
        let mut s = crate::Session::new(d.clone()).unwrap();
        s.execute(crate::Command::Add {
            annotation: a.clone(),
        })
        .unwrap();
        let rendered = s.document.render().unwrap();
        assert_eq!(
            rendered.get_pixel(50, 50),
            d.render().unwrap().get_pixel(50, 50)
        );
        assert!(rendered.get_pixel(130, 50)[0].abs_diff(50) <= 1);
        assert!(a.hit_test([130., 50.], 1.));
        let raw = serde_json::to_string(&s.document).unwrap();
        d = serde_json::from_str(&raw).unwrap();
        d.validate().unwrap();
        assert_eq!(lens(&d.annotations[0]).unwrap(), b);
        s.execute(crate::Command::Undo).unwrap();
        assert!(s.document.annotations.is_empty());
        s.execute(crate::Command::Redo).unwrap();
        assert_eq!(s.document.annotations[0], a);
        let mut above = a.clone();
        above.id = 2;
        above.focus = None;
        above.width = 1;
        above.start = [120., 40.];
        above.end = [140., 60.];
        above.fill = Some([255, 0, 0, 255]);
        d.annotations.push(above);
        assert_eq!(d.render().unwrap().get_pixel(130, 50).0, [255, 0, 0, 255]);
    }
    #[test]
    fn focus_and_motion_opacity_do_not_dim_the_magnified_content() {
        let mut a = object();
        a.focus.as_mut().unwrap().darken = 0.7;
        let mut image = RgbaImage::from_pixel(320, 180, Rgba([200, 200, 200, 255]));
        crate::focus::render(&mut image, &a);
        assert_eq!(image.get_pixel(130, 50)[0], 200);
        assert_eq!(image.get_pixel(50, 50)[0], 200);
        assert!(image.get_pixel(300, 100)[0] < 65);
        a.focus.as_mut().unwrap().opacity = 0.;
        let original = RgbaImage::from_pixel(320, 180, Rgba([200, 200, 200, 255]));
        let mut out = original.clone();
        crate::focus::render(&mut out, &a);
        assert_eq!(out, original);
    }
    #[test]
    fn rectangle_connectors_join_the_nearest_facing_corners() {
        let near = |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) < 0.01;
        // Source 30..70; default lens is to the right (center x 130, half-size 40).
        let mut a = object();
        let lines = connectors(&a);
        assert_eq!(lines.len(), 2);
        let mut ends: Vec<_> = lines.iter().map(|l| (l.start, l.end)).collect();
        ends.sort_by(|x, y| x.0[1].total_cmp(&y.0[1]));
        assert!(
            near(ends[0].0, [70., 30.]) && near(ends[0].1, [90., 10.]),
            "{ends:?}"
        );
        assert!(
            near(ends[1].0, [70., 70.]) && near(ends[1].1, [90., 90.]),
            "{ends:?}"
        );
        // Lens below: bottom corners of the source join the lens's top corners.
        a.focus.as_mut().unwrap().magnify.as_mut().unwrap().offset = [0., 2.];
        let mut ends: Vec<_> = connectors(&a).iter().map(|l| (l.start, l.end)).collect();
        ends.sort_by(|x, y| x.0[0].total_cmp(&y.0[0]));
        assert!(
            near(ends[0].0, [30., 70.]) && near(ends[0].1, [10., 90.]),
            "{ends:?}"
        );
        assert!(
            near(ends[1].0, [70., 70.]) && near(ends[1].1, [90., 90.]),
            "{ends:?}"
        );
        // Lens to the left.
        a.focus.as_mut().unwrap().magnify.as_mut().unwrap().offset = [-2., 0.];
        assert!(
            connectors(&a)
                .iter()
                .all(|l| l.start[0] == 30. && near([l.end[0], 0.], [10., 0.]))
        );
    }
    #[test]
    fn rounded_connectors_touch_both_rounded_outlines() {
        let mut a = object();
        a.corner_radius = 10.;
        let b = lens(&a).unwrap();
        for line in connectors(&a) {
            for (p, shape) in [(line.start, &a), (line.end, &b)] {
                // On the outline: inside the body with a 1 px tolerance, outside when shrunk.
                assert!(shape.hit_test(p, 1.), "{p:?} off {:?}", shape.body_bounds());
                let (lo, hi) = shape.body_bounds();
                let inside_corner = p[0] > lo[0] + 0.5
                    && p[0] < hi[0] - 0.5
                    && p[1] > lo[1] + 0.5
                    && p[1] < hi[1] - 0.5;
                assert!(
                    inside_corner,
                    "{p:?} should sit on the rounded arc, inside the square corner"
                );
            }
        }
        // Overlapping lens and source: no connectors.
        a.focus.as_mut().unwrap().magnify.as_mut().unwrap().offset = [0.5, 0.];
        assert!(connectors(&a).is_empty());
    }
    #[test]
    fn source_moves_and_resizes_keep_lens_relative_and_initial_placement_is_visible() {
        let mut a = object();
        a.start = [270., 110.];
        a.end = [310., 150.];
        place_on_canvas(&mut a, [320, 180]);
        let b = lens(&a).unwrap();
        let (lo, hi) = b.bounds();
        assert!(lo[0] >= 0. && lo[1] >= 0. && hi[0] <= 320. && hi[1] <= 180.);
        let old = b.start;
        a.start[0] -= 10.;
        a.end[0] -= 10.;
        assert_eq!(lens(&a).unwrap().start[0], old[0] - 10.);
        for scale in [f32::NAN, 0., 9.] {
            let m = Magnified {
                scale,
                ..Default::default()
            };
            assert!(m.validate().is_err());
        }
    }
}
