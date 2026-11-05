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
    /// Cache Focus sampling independently of its movable window. The first
    /// plan is the lower composite; the second treats every pixel as outside.
    pub fn focus_surfaces(&self) -> Option<(Self, Self)> {
        let last = self.objects.last()?;
        last.focus?;
        let mut lower = self.clone();
        lower.objects.pop();
        let mut outside = self.clone();
        let a = outside.objects.last_mut()?;
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
        })
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
        for a in crate::canvas_expansion::translated(&self.objects, [-(x as f32), -(y as f32)])? {
            crate::render_annotation(&mut pixels, &a);
        }
        let [ox, oy, ow, oh] = self.output;
        Ok(image::imageops::crop_imm(&pixels, ox - x, oy - y, ow, oh).to_image())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
