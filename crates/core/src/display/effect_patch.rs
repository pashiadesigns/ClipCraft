//! Bounded replacement pixels for a retained display. Geometry and export are
//! unchanged; only the dependency region needed by a pixel effect is rendered.
use crate::{Annotation, Kind, Result};
use image::RgbaImage;

pub fn needs_pixels(a: &Annotation) -> bool {
    super::needs_pixels(a) || a.image.as_ref().is_some_and(|b| b.has_inversion())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub output: [u32; 4],
    pub work: [u32; 4],
    objects: Vec<Annotation>,
    /// The Focus backdrop shared by all objects shown with this one (BJ).
    backdrop: crate::focus::Backdrop,
}

fn intersects(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}
fn union(a: [u32; 4], b: [u32; 4]) -> [u32; 4] {
    let x = a[0].min(b[0]);
    let y = a[1].min(b[1]);
    [
        x,
        y,
        (a[0] + a[2]).max(b[0] + b[2]) - x,
        (a[1] + a[3]).max(b[1] + b[3]) - y,
    ]
}
fn reads(a: &Annotation, bounds: [u32; 4], canvas: [u32; 2]) -> [u32; 4] {
    let reach = if a.kind == Kind::Blur { a.width * 3 } else { 0 };
    let x = bounds[0].saturating_sub(reach);
    let y = bounds[1].saturating_sub(reach);
    [
        x,
        y,
        (bounds[0] + bounds[2] + reach).min(canvas[0]) - x,
        (bounds[1] + bounds[3] + reach).min(canvas[1]) - y,
    ]
}
fn visible_bounds(a: &Annotation, canvas: [u32; 2]) -> Option<[u32; 4]> {
    if a.focus.is_some() {
        return Some([0, 0, canvas[0], canvas[1]]);
    }
    let (mut lo, mut hi) = a.bounds();
    let margin = a.width as f32 * 4. + 2.;
    for i in 0..2 {
        lo[i] -= margin;
        hi[i] += margin;
    }
    if let Some((slo, shi)) = crate::shadow::bounds(a) {
        for i in 0..2 {
            lo[i] = lo[i].min(slo[i]);
            hi[i] = hi[i].max(shi[i]);
        }
    }
    let left = lo[0].floor().clamp(0., canvas[0] as f32) as u32;
    let top = lo[1].floor().clamp(0., canvas[1] as f32) as u32;
    let right = hi[0].ceil().clamp(left as f32, canvas[0] as f32) as u32;
    let bottom = hi[1].ceil().clamp(top as f32, canvas[1] as f32) as u32;
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}
fn effect_bounds(a: &Annotation, canvas: [u32; 2]) -> Option<[u32; 4]> {
    if a.image.as_ref().is_some_and(|b| b.has_inversion()) {
        visible_bounds(a, canvas)
    } else {
        super::effect_bounds(a, canvas[0], canvas[1])
    }
}
impl Plan {
    pub fn magnified_region(&self) -> Option<[u32; 4]> {
        self.lens_region(self.objects.last()?)
    }
    /// For the Focus drawing a shared backdrop: each later member with a
    /// Magnify view and the source region that view reads.
    pub fn member_lens_regions(&self) -> Vec<(u64, [u32; 4])> {
        let Some(last) = self.objects.last() else {
            return vec![];
        };
        if self.backdrop.lead() != Some(last.id) {
            return vec![];
        }
        self.backdrop
            .members()
            .iter()
            .filter(|m| self.backdrop.is_quiet(m))
            .filter_map(|m| Some((m.id, self.lens_region(m)?)))
            .collect()
    }
    fn lens_region(&self, a: &Annotation) -> Option<[u32; 4]> {
        a.focus?.magnify?;
        let (lo, hi) = a.bounds();
        let [x, y, w, h] = self.output;
        let left = (lo[0].floor() - 1.).clamp(x as f32, (x + w) as f32) as u32;
        let top = (lo[1].floor() - 1.).clamp(y as f32, (y + h) as f32) as u32;
        let right = (hi[0].ceil() + 1.).clamp(left as f32, (x + w) as f32) as u32;
        let bottom = (hi[1].ceil() + 1.).clamp(top as f32, (y + h) as f32) as u32;
        (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
    }
    /// Keep the magnified detail at source resolution without recompositing the
    /// entire native-size video. Nested raster effects retain their dependencies.
    pub fn render_magnified_region(
        &self,
        base: &RgbaImage,
        offset: [i32; 2],
        region: [u32; 4],
    ) -> Result<RgbaImage> {
        let (lower, _) = self
            .focus_surfaces()
            .ok_or_else(|| crate::Error::Invalid("Expected a magnified callout".into()))?;
        if lower.objects.iter().any(needs_pixels) {
            let pixels = lower.render(base, offset)?;
            return Ok(image::imageops::crop_imm(
                &pixels,
                region[0] - self.output[0],
                region[1] - self.output[1],
                region[2],
                region[3],
            )
            .to_image());
        }
        lower
            .clipped(region)
            .ok_or_else(|| crate::Error::Invalid("Callout source is outside the canvas".into()))?
            .render(base, offset)
    }
    /// Build a Focus backdrop once, then derive its affected surface. Live video
    /// uses display-sized surfaces; saved/exported pixels never use this path.
    pub fn render_focus_surfaces(
        &self,
        base: &RgbaImage,
        offset: [i32; 2],
        density: f32,
    ) -> Result<(RgbaImage, RgbaImage)> {
        let (lower, _) = self.focus_surfaces().ok_or_else(|| {
            crate::Error::Invalid("Focus surface requested for another effect".into())
        })?;
        let last = self.objects.last().unwrap();
        // A later member of a shared backdrop adds no dimming of its own; the
        // first member dims with the strongest settings of the group.
        let focus = if self.backdrop.is_quiet(last) {
            crate::focus::quiet(last).focus.unwrap()
        } else {
            self.backdrop.settings(last).unwrap()
        };
        let density = density.clamp(0.05, 1.);
        let [x, y, w, h] = self.output;
        let size = [
            (w as f32 * density).ceil().max(1.) as u32,
            (h as f32 * density).ceil().max(1.) as u32,
        ];
        let clear = if density == 1. || lower.objects.iter().any(needs_pixels) {
            let pixels = lower.render(base, offset)?;
            if density == 1. {
                pixels
            } else {
                image::imageops::resize(
                    &pixels,
                    size[0],
                    size[1],
                    image::imageops::FilterType::Triangle,
                )
            }
        } else {
            let mut pixels = RgbaImage::new(size[0], size[1]);
            let uniform = offset == [0, 0]
                && x + w <= base.width()
                && y + h <= base.height()
                && base.pixels().all(|p| p == base.get_pixel(0, 0));
            if uniform {
                pixels.pixels_mut().for_each(|p| *p = *base.get_pixel(0, 0));
            } else {
                super::viewport::replace(
                    &mut pixels,
                    base,
                    [0, 0],
                    super::Viewport {
                        size,
                        origin: [
                            (x as i64 - offset[0] as i64) as f64,
                            (y as i64 - offset[1] as i64) as f64,
                        ],
                        scale: density as f64,
                    },
                );
            }
            let viewport = super::Viewport {
                size,
                origin: [x as f64, y as f64],
                scale: density as f64,
            };
            for a in &lower.objects {
                if let Some(shadow) = crate::shadow::patch(a) {
                    super::viewport::blend_patch(&mut pixels, &shadow, viewport);
                }
                if a.image.is_some() {
                    let mut shell = a.clone();
                    shell.image = None;
                    shell.text = None;
                    shell.color[3] = 0;
                    if a.is_shape() {
                        super::viewport::render_vector(&mut pixels, &shell, viewport);
                    }
                    if !super::focus_preview::bitmap(&mut pixels, a, viewport) {
                        super::viewport::render_vector(&mut pixels, a, viewport);
                    } else if a.is_shape() {
                        shell.color = a.color;
                        shell.fill = None;
                        super::viewport::render_vector(&mut pixels, &shell, viewport);
                    }
                } else {
                    super::viewport::render_vector(&mut pixels, a, viewport);
                }
            }
            pixels
        };
        let mut affected = if focus.blur > 0. {
            super::focus_preview::blur(&clear, focus.blur * density)
        } else {
            clear.clone()
        };
        for (out, original) in affected.pixels_mut().zip(clear.pixels()) {
            for c in 0..3 {
                out[c] = (original[c] as f32 * (1. - focus.opacity)
                    + out[c] as f32 * (1. - focus.darken) * focus.opacity
                    + 0.5) as u8;
            }
            out[3] = (original[3] as f32 * (1. - focus.opacity)
                + out[3] as f32 * focus.opacity
                + 0.5) as u8;
        }
        Ok((clear, affected))
    }
    /// Cache Focus sampling independently of its movable window. The first
    /// plan is the lower composite; the second treats every pixel as outside.
    pub fn focus_surfaces(&self) -> Option<(Self, Self)> {
        let last = self.objects.last()?;
        last.focus?;
        let mut lower = self.clone();
        lower.objects.pop();
        let mut outside = self.clone();
        // The cached surfaces depend on the strongest settings, not on where
        // any window is, so dragging one Focus rebuilds nothing (BJ4).
        let settings = self.backdrop.settings(last)?;
        outside.backdrop = Default::default();
        let a = outside.objects.last_mut()?;
        a.focus = Some(settings);
        a.focus.as_mut()?.magnify = None;
        a.start = [-4., -4.];
        a.end = [-2., -2.];
        a.kind = Kind::Rectangle;
        a.corner_radius = 0.;
        a.rotation_degrees = 0.;
        a.mirror_x = false;
        a.width = 0;
        a.color = [0; 4];
        a.line_style = None;
        Some((lower, outside))
    }
    pub fn new(objects: &[Annotation], index: usize, canvas: [u32; 2]) -> Option<Self> {
        let all = objects;
        let effect = objects.get(index)?;
        if !needs_pixels(effect) {
            return None;
        }
        let output = effect_bounds(effect, canvas)?;
        let mut work = reads(effect, output, canvas);
        // Backwards dependency closure includes each earlier effect's complete
        // sampling neighborhood. Pixelation keeps its original block anchor.
        for a in objects[..index].iter().rev().filter(|a| needs_pixels(a)) {
            if let Some(bounds) = effect_bounds(a, canvas)
                && intersects(work, bounds)
            {
                work = union(work, reads(a, bounds, canvas));
            }
        }
        let objects = objects[..=index]
            .iter()
            .filter(|a| visible_bounds(a, canvas).is_some_and(|b| intersects(b, work)))
            .cloned()
            .collect();
        Some(Self {
            output,
            work,
            objects,
            backdrop: crate::focus::Backdrop::new(all),
        })
    }
    /// Canvas-aligned tiles of `size` pixels inside `output` that `effect` can
    /// change. An eraser changes only the pixels along its stroke, so a display
    /// renders and replaces just these tiles instead of the stroke's whole
    /// bounding box (beta 4: long eraser strokes made the canvas lag). Effects
    /// that read beyond their output, and every other kind, use one cell.
    pub fn changed_tiles(&self, effect: &Annotation, size: u32) -> Vec<[u32; 4]> {
        let [ox, oy, ow, oh] = self.output;
        // Erasers and redactions read only the pixels they cover, so a tile of
        // their stack renders exactly; blur, pixelation and Focus read around
        // themselves and keep the whole patch.
        let pointwise = self.objects.iter().all(|a| {
            !needs_pixels(a) || (a.focus.is_none() && matches!(a.kind, Kind::Eraser | Kind::Redact))
        });
        if effect.kind != Kind::Eraser || effect.points.len() < 2 || !pointwise || size == 0 {
            return vec![self.output];
        }
        let reach = crate::line_style::stroke_reach(effect) + 2.0;
        let clamp_x = |v: f32| (v.floor().max(ox as f32) as u32).min(ox + ow);
        let clamp_y = |v: f32| (v.floor().max(oy as f32) as u32).min(oy + oh);
        let mut cells = std::collections::BTreeSet::new();
        for pair in effect.points.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            let (x0, x1) = (
                clamp_x(a[0].min(b[0]) - reach),
                clamp_x(a[0].max(b[0]) + reach + 1.0),
            );
            let (y0, y1) = (
                clamp_y(a[1].min(b[1]) - reach),
                clamp_y(a[1].max(b[1]) + reach + 1.0),
            );
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            for ty in y0 / size..=(y1 - 1) / size {
                for tx in x0 / size..=(x1 - 1) / size {
                    cells.insert((ty, tx));
                }
            }
        }
        let tiles: Vec<_> = cells
            .into_iter()
            .filter_map(|(ty, tx)| {
                let lo = [(tx * size).max(ox), (ty * size).max(oy)];
                let hi = [
                    ((tx + 1) * size).min(ox + ow),
                    ((ty + 1) * size).min(oy + oh),
                ];
                (hi[0] > lo[0] && hi[1] > lo[1])
                    .then(|| [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]])
            })
            .collect();
        if tiles.is_empty() {
            vec![self.output]
        } else {
            tiles
        }
    }
    /// This plan restricted to one cell of `changed_tiles`, keeping only the
    /// objects that reach it.
    pub fn tile(&self, cell: [u32; 4], canvas: [u32; 2]) -> Option<Self> {
        let mut tile = self.clone().clipped(cell)?;
        let work = tile.work;
        tile.objects
            .retain(|a| visible_bounds(a, canvas).is_some_and(|b| intersects(b, work)));
        Some(tile)
    }
    /// The backdrop this plan shares when `new` was given only part of the page.
    pub fn with_backdrop(mut self, backdrop: crate::focus::Backdrop) -> Self {
        self.backdrop = backdrop;
        self
    }
    pub fn clipped(mut self, bounds: [u32; 4]) -> Option<Self> {
        let intersect = |r: [u32; 4]| {
            let lo = [r[0].max(bounds[0]), r[1].max(bounds[1])];
            let hi = [
                (r[0] + r[2]).min(bounds[0] + bounds[2]),
                (r[1] + r[3]).min(bounds[1] + bounds[3]),
            ];
            (hi[0] > lo[0] && hi[1] > lo[1]).then(|| [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]])
        };
        self.output = intersect(self.output)?;
        self.work = intersect(self.work)?;
        Some(self)
    }
    pub fn render(&self, base: &RgbaImage, offset: [i32; 2]) -> Result<RgbaImage> {
        let [x, y, w, h] = self.work;
        crate::validate_dimensions(w, h)?;
        let mut pixels = RgbaImage::new(w, h);
        // Copy only the required background rows, including transparent margins.
        for row in 0..h {
            let sy = i64::from(y + row) - i64::from(offset[1]);
            if sy < 0 || sy >= i64::from(base.height()) {
                continue;
            }
            let left = i64::from(x).max(i64::from(offset[0]));
            let right = i64::from(x + w).min(i64::from(offset[0]) + i64::from(base.width()));
            if right <= left {
                continue;
            }
            let src =
                (sy as usize * base.width() as usize + (left - i64::from(offset[0])) as usize) * 4;
            let dst = (row as usize * w as usize + (left - i64::from(x)) as usize) * 4;
            let len = (right - left) as usize * 4;
            pixels.as_mut()[dst..dst + len].copy_from_slice(&base.as_raw()[src..src + len]);
        }
        let shift = [-(x as f32), -(y as f32)];
        let backdrop = crate::focus::Backdrop::new(&crate::canvas_expansion::translated(
            self.backdrop.members(),
            shift,
        )?);
        for a in crate::canvas_expansion::translated(&self.objects, shift)? {
            if !backdrop.render(&mut pixels, &a) {
                crate::render_annotation(&mut pixels, &a);
            }
        }
        let [ox, oy, ow, oh] = self.output;
        Ok(image::imageops::crop_imm(&pixels, ox - x, oy - y, ow, oh).to_image())
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    /// Tracked redactions follow scrolling text and can sit partly or fully
    /// outside the canvas. Their patches must still render, or the whole video
    /// preview stops after its background (the AZ16 grey canvas).
    #[test]
    fn redactions_at_and_beyond_the_canvas_edges_render() {
        let canvas = [320u32, 180u32];
        let base = RgbaImage::from_pixel(canvas[0], canvas[1], image::Rgba([90, 90, 90, 255]));
        let mut frame =
            crate::steps::marker(1, [160., 90.], 40, [0; 4], (1, false), canvas).unwrap();
        frame.kind = Kind::Rectangle;
        frame.text = None;
        frame.start = [0., 0.];
        frame.end = [320., 180.];
        frame.fill = Some([10, 200, 60, 255]);
        for (start, end) in [
            ([300., 170.], [340., 190.]),
            ([-30., -10.], [20., 8.]),
            ([330., 50.], [380., 70.]),
            ([100., 185.], [150., 200.]),
            ([-60., 50.], [-10., 70.]),
            ([319.6, 179.6], [325., 185.]),
        ] {
            let mut r = frame.clone();
            r.id = 2;
            r.kind = Kind::Redact;
            r.fill = None;
            r.color = [0, 0, 0, 255];
            r.start = start;
            r.end = end;
            let objects = vec![frame.clone(), r];
            if let Some(plan) = Plan::new(&objects, 1, canvas) {
                let out = plan.render(&base, [0, 0]);
                assert!(
                    out.is_ok(),
                    "{start:?}-{end:?}: {:?} {:?}",
                    plan.output,
                    out.err()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_focus_surfaces_match_exact_pixels_and_preview_bounds() {
        let base = RgbaImage::from_fn(160, 100, |x, y| {
            image::Rgba([(x * 7 % 256) as u8, (y * 9 % 256) as u8, 80, 255])
        });
        for blur in [0., 4.] {
            let mut a =
                crate::steps::marker(1, [70., 50.], 20, [0; 4], (1, false), [160, 100]).unwrap();
            a.kind = Kind::Rectangle;
            a.text = None;
            a.focus = Some(crate::focus::Focus {
                blur,
                darken: 0.4,
                opacity: 0.7,
                magnify: Some(Default::default()),
            });
            let plan = Plan::new(&[a], 0, [160, 100]).unwrap();
            let (lower, outside) = plan.focus_surfaces().unwrap();
            let (clear, affected) = plan.render_focus_surfaces(&base, [0, 0], 1.).unwrap();
            assert_eq!(clear, lower.render(&base, [0, 0]).unwrap());
            assert_eq!(affected, outside.render(&base, [0, 0]).unwrap());
            let (small, blurred) = plan.render_focus_surfaces(&base, [0, 0], 0.5).unwrap();
            assert_eq!(small.dimensions(), (80, 50));
            assert_eq!(blurred.dimensions(), (80, 50));
            assert_eq!(base.dimensions(), (160, 100));
            let region = plan.magnified_region().unwrap();
            let detail = plan.render_magnified_region(&base, [0, 0], region).unwrap();
            assert_eq!(
                detail,
                image::imageops::crop_imm(&clear, region[0], region[1], region[2], region[3])
                    .to_image()
            );
        }
    }
    #[test]
    fn patches_match_at_canvas_edges_and_with_translated_background() {
        let base = RgbaImage::from_fn(160, 100, |x, y| {
            image::Rgba([(x * 7 % 255) as u8, (y * 13 % 255) as u8, 180, 200])
        });
        for offset in [[0, 0], [24, 17]] {
            let size = [160 + offset[0] as u32, 100 + offset[1] as u32];
            let mut full = RgbaImage::new(size[0], size[1]);
            image::imageops::replace(&mut full, &base, offset[0] as i64, offset[1] as i64);
            let mut objects = vec![];
            for (n, (kind, start, end)) in [
                (Kind::Blur, [1., 2.], [42., 37.]),
                (Kind::Pixelate, [26., 19.], [94., 63.]),
                (Kind::Blur, [22., 25.], [125., 82.]),
                (Kind::Eraser, [110., 60.], [158., 98.]),
            ]
            .into_iter()
            .enumerate()
            {
                let mut a = crate::steps::marker(
                    n as u64 + 1,
                    [60., 50.],
                    30,
                    [20, 40, 70, 255],
                    (1, false),
                    size,
                )
                .unwrap();
                a.kind = kind;
                a.start = start;
                a.end = end;
                a.width = 8;
                a.text = None;
                a.fill = None;
                if kind == Kind::Eraser {
                    a.points = vec![start, end];
                }
                crate::render_annotation(&mut full, &a);
                objects.push(a);
                let plan = Plan::new(&objects, objects.len() - 1, size).unwrap();
                let [x, y, w, h] = plan.output;
                let actual = plan.render(&base, offset).unwrap();
                let expected = image::imageops::crop_imm(&full, x, y, w, h).to_image();
                let differing = actual
                    .pixels()
                    .zip(expected.pixels())
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(differing, 0, "{kind:?}, {offset:?}, step {n}");
            }
        }
    }
    #[test]
    fn bounded_effects_match_full_composite_including_overlaps_and_erased_alpha() {
        let source = RgbaImage::from_fn(640, 400, |x, y| {
            image::Rgba([
                (x % 251) as u8,
                (y % 239) as u8,
                100,
                if x < 20 { 80 } else { 255 },
            ])
        });
        let mut d = crate::Document::from_capture_image(source).unwrap();
        let template: crate::Document =
            serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json"))
                .unwrap();
        for (n, kind) in [
            Kind::Blur,
            Kind::Pixelate,
            Kind::Eraser,
            Kind::Redact,
            Kind::Blur,
        ]
        .into_iter()
        .enumerate()
        {
            let mut a = template.annotations[0].clone();
            a.kind = kind;
            a.id = n as u64 + 2;
            a.start = [40. + n as f32 * 30., 50.];
            a.end = [185., 155.];
            a.text = None;
            a.callout = None;
            a.fill = None;
            a.width = 7;
            a.rotation_degrees = 0.;
            a.corner_radius = 0.;
            if kind == Kind::Eraser {
                a.points = vec![a.start, [135., 90.], a.end];
            } else {
                a.points.clear();
            }
            d.annotations.push(a);
            d.promote_features();
            let plan = Plan::new(&d.annotations, d.annotations.len() - 1, [640, 400]).unwrap();
            assert!(plan.work[2] * plan.work[3] < 640 * 400 / 2);
            let actual = plan.render(&d.base_image().unwrap(), [0, 0]).unwrap();
            let [x, y, w, h] = plan.output;
            let full = d.render().unwrap();
            let expected = image::imageops::crop_imm(&full, x, y, w, h).to_image();
            assert_eq!(actual, expected, "{kind:?}");
        }
        let index = d.annotations.len() - 1;
        let before = Plan::new(&d.annotations, index, [640, 400]).unwrap();
        let mut far = template.annotations[0].clone();
        far.id = 100;
        far.start = [500., 300.];
        far.end = [600., 350.];
        d.annotations.insert(1, far);
        assert_eq!(
            before,
            Plan::new(&d.annotations, index + 1, [640, 400]).unwrap()
        );
    }
}
