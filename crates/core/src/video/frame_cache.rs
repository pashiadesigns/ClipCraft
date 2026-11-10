//! Bounded live-preview cache. Frame masks, strokes and shadows are static;
//! only the opaque decoded picture changes. Export retains the exact renderer.
use super::frame::Frame;
use crate::Result;
use image::{Rgba, RgbaImage};
use std::{collections::VecDeque, sync::Arc};

#[derive(Default)]
pub struct PreviewCache {
    entries: VecDeque<Entry>,
    seen: VecDeque<Key>,
}
type Key = (Frame, [f32; 4], [u32; 2], [u32; 2]);
struct Entry {
    style: Frame,
    rect: [f32; 4],
    size: [u32; 2],
    source: [u32; 2],
    base: RgbaImage,
    gain: Vec<[u8; 3]>,
    columns: Vec<Option<(u32, u32, u32)>>,
    rows: Vec<Option<(u32, u32, u32)>>,
    /// Where each visible pixel reads from when the frame is rotated, which
    /// rows and columns can't describe.
    samples: Vec<Sample>,
    /// For Fit, where the picture sits in the frame-sized image.
    area: Option<[u32; 4]>,
}
/// Copies the picture's edge outward over the empty band around it, so
/// blending at the edge doesn't pull in the band's black.
fn bleed(image: &mut RgbaImage, [x, y, w, h]: [u32; 4]) {
    let (width, height) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    for py in 0..height {
        let inside = (y..y + h).contains(&py);
        for px in 0..width {
            if inside && (x..x + w).contains(&px) {
                continue;
            }
            let from = image.get_pixel(px.clamp(x, x + w - 1), py.clamp(y, y + h - 1));
            let from = *from;
            image.put_pixel(px, py, from);
        }
    }
}
struct Sample {
    index: u32,
    x: u16,
    y: u16,
    fx: u16,
    fy: u16,
}
impl PreviewCache {
    /// For export. A frame style held for more than one video frame is drawn from
    /// its cached mask and matches the exact renderer to within 3 levels. The
    /// masks cost several exact renders, so a frame that is moving does not build
    /// them.
    pub fn render_steady(
        &mut self,
        style: &Frame,
        source: Arc<RgbaImage>,
        rect: [f32; 4],
        size: [u32; 2],
        id: u64,
    ) -> Result<RgbaImage> {
        let key = (style.clone(), rect, size, [source.width(), source.height()]);
        if !self.seen.contains(&key) {
            self.seen.push_back(key);
            if self.seen.len() > 8 {
                self.seen.pop_front();
            }
            return style.render(source, rect, size, id);
        }
        if let Some((plain, colored)) = style.color_first(&source) {
            return self.render_steady(&plain, colored, rect, size, id);
        }
        if style.fill {
            return self.render(style, source, rect, size, id);
        }
        if let Some(plain) = style.plain_snapped(&source, rect, size) {
            return Ok(plain);
        }
        if source.pixels().any(|p| p[3] != 255) {
            return style.render(source, rect, size, id);
        }
        // Fit: resize once, then treat the frame-sized picture as Fill.
        let (mut padded, area) = style.padded_fit(&source, rect, size);
        bleed(&mut padded, area);
        let filled = Frame {
            fill: true,
            view: Default::default(),
            ..style.clone()
        };
        self.cached(&filled, &padded, rect, size, Some(area))
    }
    pub fn render(
        &mut self,
        style: &Frame,
        source: Arc<RgbaImage>,
        rect: [f32; 4],
        size: [u32; 2],
        id: u64,
    ) -> Result<RgbaImage> {
        if let Some((plain, colored)) = style.color_first(&source) {
            return self.render(&plain, colored, rect, size, id);
        }
        if !style.fill || source.pixels().any(|p| p[3] != 255) {
            return style.render(source, rect, size, id);
        }
        self.cached(style, &source, rect, size, None)
    }
    fn cached(
        &mut self,
        style: &Frame,
        source: &RgbaImage,
        rect: [f32; 4],
        size: [u32; 2],
        area: Option<[u32; 4]>,
    ) -> Result<RgbaImage> {
        let dimensions = [source.width(), source.height()];
        let index = self.entries.iter().position(|e| {
            e.style == *style
                && e.rect == rect
                && e.size == size
                && e.source == dimensions
                && e.area == area
        });
        let entry = if let Some(index) = index {
            self.entries.remove(index).unwrap()
        } else {
            Entry::new(style, rect, size, dimensions, area)?
        };
        let rendered = entry.render(source);
        self.entries.push_back(entry);
        while self.entries.len() > 4
            || self
                .entries
                .iter()
                .map(|e| {
                    e.base.as_raw().len()
                        + e.gain.len() * 3
                        + e.samples.len() * std::mem::size_of::<Sample>()
                })
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
            self.entries.pop_front();
        }
        Ok(rendered)
    }
}
impl Entry {
    fn new(
        style: &Frame,
        rect: [f32; 4],
        size: [u32; 2],
        source: [u32; 2],
        area: Option<[u32; 4]>,
    ) -> Result<Self> {
        let template = |color: [u8; 4]| {
            let mut image = match area {
                Some(_) => RgbaImage::new(source[0], source[1]),
                None => RgbaImage::from_pixel(source[0], source[1], Rgba(color)),
            };
            if let Some([x, y, w, h]) = area {
                for (px, py, pixel) in image.enumerate_pixels_mut() {
                    if (x..x + w).contains(&px) && (y..y + h).contains(&py) {
                        *pixel = Rgba(color);
                    }
                }
            }
            Arc::new(image)
        };
        let black = template([0, 0, 0, 255]);
        let a = style.annotation(black.clone(), rect, size, 0)?;
        let view = a.image.as_ref().unwrap().view().rect;
        let (lo, hi) = a.body_bounds();
        let base = style.render(black, rect, size, 0)?;
        let white = style.render(template([255; 4]), rect, size, 0)?;
        let gain: Vec<[u8; 3]> = base
            .pixels()
            .zip(white.pixels())
            .map(|(b, w)| [0, 1, 2].map(|i| w[i].saturating_sub(b[i])))
            .collect();
        let mut samples = vec![];
        if style.rotation != 0. {
            let rotation = a.rotation();
            for (i, gain) in gain.iter().enumerate() {
                if *gain == [0; 3] {
                    continue;
                }
                let at = [
                    (i as u32 % size[0]) as f32 + 0.5,
                    (i as u32 / size[0]) as f32 + 0.5,
                ];
                let local = rotation.inverse_point(at);
                let read = |axis: usize| {
                    let t = ((local[axis] - lo[axis]) / (hi[axis] - lo[axis])).clamp(0., 1.);
                    let mut point = (t * view[axis + 2] + view[axis]) * source[axis] as f32 - 0.5;
                    if axis == 1 && style.flip_y {
                        point = source[axis] as f32 - 1. - point;
                    }
                    let point = point.clamp(0., (source[axis] - 1) as f32);
                    (
                        point.floor() as u16,
                        ((point - point.floor()) * 1024.) as u16,
                    )
                };
                let ((x, fx), (y, fy)) = (read(0), read(1));
                samples.push(Sample {
                    index: i as u32,
                    x,
                    y,
                    fx,
                    fy,
                });
            }
        }
        let mapping = |axis: usize, n: u32| {
            let mut local = (n as f32 + 0.5 - lo[axis]) / (hi[axis] - lo[axis]);
            if !(0. ..1.).contains(&local) {
                return None;
            }
            if axis == 0 && style.mirror {
                local = 1. - local;
            }
            let mut point = (local * view[axis + 2] + view[axis]) * source[axis] as f32 - 0.5;
            if axis == 1 && style.flip_y {
                point = source[axis] as f32 - 1. - point;
            }
            let floor = point.floor();
            let max = (source[axis] - 1) as f32;
            Some((
                floor.clamp(0., max) as u32,
                (floor + 1.).clamp(0., max) as u32,
                ((point - floor) * 1024. + 0.5) as u32,
            ))
        };
        Ok(Self {
            style: style.clone(),
            rect,
            size,
            source,
            base,
            gain,
            columns: (0..size[0]).map(|n| mapping(0, n)).collect(),
            rows: (0..size[1]).map(|n| mapping(1, n)).collect(),
            samples,
            area,
        })
    }
    fn render(&self, source: &RgbaImage) -> RgbaImage {
        let mut out = self.base.clone();
        let pixels = out.as_mut().as_chunks_mut::<4>().0;
        let source_pixels = source.as_raw().as_chunks::<4>().0;
        let base = self.base.as_raw().as_chunks::<4>().0;
        if self.style.rotation != 0. {
            let width = source.width() as usize;
            let height = source.height() as usize;
            for s in &self.samples {
                let index = s.index as usize;
                let (x0, y0) = (s.x as usize, s.y as usize);
                let x1 = (x0 + 1).min(width - 1);
                let y1 = (y0 + 1).min(height - 1);
                let (fx, fy) = (s.fx as u32, s.fy as u32);
                let gain = self.gain[index];
                let p = [
                    source_pixels[y0 * width + x0],
                    source_pixels[y0 * width + x1],
                    source_pixels[y1 * width + x0],
                    source_pixels[y1 * width + x1],
                ];
                for c in 0..3 {
                    let top = p[0][c] as u32 * (1024 - fx) + p[1][c] as u32 * fx;
                    let bottom = p[2][c] as u32 * (1024 - fx) + p[3][c] as u32 * fx;
                    let value = (top * (1024 - fy) + bottom * fy + (1 << 19)) >> 20;
                    pixels[index][c] = (base[index][c] as u32
                        + (gain[c] as u32 * value + 127) / 255)
                        .min(255) as u8;
                }
            }
            return out;
        }
        for (y, row) in self.rows.iter().enumerate() {
            let Some((y0, y1, fy)) = *row else { continue };
            let row0 = y0 as usize * source.width() as usize;
            let row1 = y1 as usize * source.width() as usize;
            for (x, col) in self.columns.iter().enumerate() {
                let Some((x0, x1, fx)) = *col else { continue };
                let index = y * self.size[0] as usize + x;
                let gain = self.gain[index];
                if gain == [0; 3] {
                    continue;
                }
                let p = [
                    source_pixels[row0 + x0 as usize],
                    source_pixels[row0 + x1 as usize],
                    source_pixels[row1 + x0 as usize],
                    source_pixels[row1 + x1 as usize],
                ];
                for c in 0..3 {
                    // Fixed-point bilinear sampling avoids millions of scalar
                    // roundf calls on Windows. Ten fractional bits keep sample
                    // error below one channel level without unsafe access.
                    let top = p[0][c] as u32 * (1024 - fx) + p[1][c] as u32 * fx;
                    let bottom = p[2][c] as u32 * (1024 - fx) + p[3][c] as u32 * fx;
                    let value = (top * (1024 - fy) + bottom * fy + (1 << 19)) >> 20;
                    pixels[index][c] = (base[index][c] as u32
                        + (gain[c] as u32 * value + 127) / 255)
                        .min(255) as u8;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_cache_matches_frame_masks_crop_stroke_shadow_and_invalidates_edits() {
        let mut cache = PreviewCache::default();
        let source = Arc::new(RgbaImage::from_fn(160, 90, |x, y| {
            Rgba([(x * 3 % 256) as u8, (y * 7 % 256) as u8, 80, 255])
        }));
        for shape in [
            crate::Kind::Rectangle,
            crate::Kind::Ellipse,
            crate::Kind::Polygon,
        ] {
            for mirror in [false, true] {
                let style = Frame {
                    shape,
                    mirror,
                    flip_y: mirror,
                    view: crate::image_frame::ImageView {
                        rect: [0.15, 0.1, 0.75, 0.65],
                    },
                    width: 3,
                    outline: [30, 200, 240, 180],
                    opacity: 180,
                    shadow: Some(Default::default()),
                    ..Default::default()
                };
                let rect = [0.12, 0.15, 0.7, 0.65];
                for phase in 0..3 {
                    let source = if phase == 0 {
                        source.clone()
                    } else {
                        Arc::new(RgbaImage::from_fn(160, 90, |x, y| {
                            Rgba([(x * 2 % 256) as u8, 40, (y * 3 + phase * 15) as u8, 255])
                        }))
                    };
                    let expected = style.render(source.clone(), rect, [240, 160], 1).unwrap();
                    let actual = cache
                        .render(&style, source.clone(), rect, [240, 160], 1)
                        .unwrap();
                    let max = actual
                        .as_raw()
                        .iter()
                        .zip(expected.as_raw())
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    assert!(max <= 3, "{shape:?} mirror {mirror}: {max}");
                }
            }
        }
        assert!(cache.entries.len() <= 4);
    }
    #[test]
    fn export_uses_the_masks_only_for_a_frame_that_holds_still() {
        let source = Arc::new(RgbaImage::from_fn(192, 102, |x, y| {
            Rgba([(x * 3 % 256) as u8, (y * 7 % 256) as u8, 80, 255])
        }));
        let style = Frame {
            radius: 0.,
            ..Default::default()
        };
        let rect = [0.1003, 0.0517, 0.8011, 0.9013];
        let size = [192, 108];
        let exact = style.render(source.clone(), rect, size, 1).unwrap();
        let mut cache = PreviewCache::default();
        let first = cache
            .render_steady(&style, source.clone(), rect, size, 1)
            .unwrap();
        assert_eq!(first, exact);
        assert!(cache.entries.is_empty(), "one frame builds nothing");
        let held = cache
            .render_steady(&style, source.clone(), rect, size, 1)
            .unwrap();
        assert_eq!(cache.entries.len(), 1);
        let max = held
            .as_raw()
            .iter()
            .zip(exact.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(max <= 3, "{max}");
        let mut moving = PreviewCache::default();
        for n in 0..12 {
            let rect = [0.1 + n as f32 * 0.001, 0.05, 0.8, 0.9];
            moving
                .render_steady(&style, source.clone(), rect, size, 1)
                .unwrap();
        }
        assert!(moving.entries.is_empty(), "a moving frame stays exact");
        let fit = Frame {
            fill: false,
            ..style.clone()
        };
        let mut cache = PreviewCache::default();
        for _ in 0..2 {
            cache
                .render_steady(&fit, source.clone(), rect, size, 1)
                .unwrap();
        }
        assert!(cache.entries.is_empty());
    }
    fn gentle(width: u32, height: u32) -> Arc<RgbaImage> {
        Arc::new(RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                (x * 200 / width.max(1)) as u8 + 20,
                (y * 200 / height.max(1)) as u8 + 20,
                90,
                255,
            ])
        }))
    }
    fn close(a: &RgbaImage, b: &RgbaImage, limit: u8, what: &str) {
        assert_eq!(a.dimensions(), b.dimensions());
        let worst = a
            .as_raw()
            .iter()
            .zip(b.as_raw())
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap();
        assert!(worst <= limit, "{what}: {worst}");
    }
    #[test]
    fn a_colored_clip_matches_on_every_path() {
        let source = gentle(320, 180);
        let style = Frame {
            color: crate::picture_finish::ColorAdjust::LOOKS[5].1,
            radius: 12.,
            ..Default::default()
        };
        let (rect, size) = ([0.1, 0.1, 0.7, 0.7], [320, 180]);
        let exact = style.render(source.clone(), rect, size, 1).unwrap();
        let mut plain_source = (*source).clone();
        style.color.apply(&mut plain_source);
        let plain = Frame {
            color: Default::default(),
            ..style.clone()
        };
        assert_eq!(
            exact,
            plain.render(Arc::new(plain_source), rect, size, 1).unwrap()
        );
        let mut cache = PreviewCache::default();
        for _ in 0..3 {
            let held = cache
                .render_steady(&style, source.clone(), rect, size, 1)
                .unwrap();
            close(&held, &exact, 40, "colored");
        }
        assert!(!style.unmodified_pixels());
    }
    #[test]
    fn rotated_and_fit_frames_use_the_masks_and_match_the_exact_renderer() {
        let source = gentle(320, 171);
        let rect = [0.1003, 0.1517, 0.7011, 0.7013];
        let size = [320, 180];
        for (what, style, limit) in [
            (
                "rotated fill",
                Frame {
                    rotation: 12.,
                    ..Default::default()
                },
                40,
            ),
            (
                "rotated mirrored",
                Frame {
                    rotation: -20.,
                    mirror: true,
                    radius: 0.,
                    ..Default::default()
                },
                40,
            ),
            (
                "fit rounded",
                Frame {
                    fill: false,
                    radius: 14.,
                    ..Default::default()
                },
                6,
            ),
            (
                "fit stroke and shadow",
                Frame {
                    fill: false,
                    radius: 8.,
                    width: 3,
                    outline: [30, 200, 240, 255],
                    shadow: Some(Default::default()),
                    ..Default::default()
                },
                6,
            ),
        ] {
            let exact = style.render(source.clone(), rect, size, 1).unwrap();
            let mut cache = PreviewCache::default();
            cache
                .render_steady(&style, source.clone(), rect, size, 1)
                .unwrap();
            let held = cache
                .render_steady(&style, source.clone(), rect, size, 1)
                .unwrap();
            assert_eq!(cache.entries.len(), 1, "{what}");
            close(&held, &exact, limit, what);
        }
    }
}
