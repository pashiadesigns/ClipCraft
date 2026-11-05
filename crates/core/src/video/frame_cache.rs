//! Bounded live-preview cache. Frame masks, strokes and shadows are static;
//! only the opaque decoded picture changes. Export retains the exact renderer.
use super::frame::Frame;
use crate::Result;
use image::{Rgba, RgbaImage};
use std::{collections::VecDeque, sync::Arc};

#[derive(Default)]
pub struct PreviewCache {
    entries: VecDeque<Entry>,
}
struct Entry {
    style: Frame,
    rect: [f32; 4],
    size: [u32; 2],
    source: [u32; 2],
    base: RgbaImage,
    gain: Vec<[u8; 3]>,
    columns: Vec<Option<(u32, u32, u32)>>,
    rows: Vec<Option<(u32, u32, u32)>>,
}
impl PreviewCache {
    pub fn render(
        &mut self,
        style: &Frame,
        source: Arc<RgbaImage>,
        rect: [f32; 4],
        size: [u32; 2],
        id: u64,
    ) -> Result<RgbaImage> {
        if !style.fill || style.rotation != 0. || source.pixels().any(|p| p[3] != 255) {
            return style.render(source, rect, size, id);
        }
        let dimensions = [source.width(), source.height()];
        let index = self.entries.iter().position(|e| {
            e.style == *style && e.rect == rect && e.size == size && e.source == dimensions
        });
        let entry = if let Some(index) = index {
            self.entries.remove(index).unwrap()
        } else {
            Entry::new(style, rect, size, dimensions)?
        };
        let rendered = entry.render(&source);
        self.entries.push_back(entry);
        while self.entries.len() > 4
            || self
                .entries
                .iter()
                .map(|e| e.base.as_raw().len() + e.gain.len() * 3)
                .sum::<usize>()
                > 64 * 1024 * 1024
        {
            self.entries.pop_front();
        }
        Ok(rendered)
    }
}
impl Entry {
    fn new(style: &Frame, rect: [f32; 4], size: [u32; 2], source: [u32; 2]) -> Result<Self> {
        let black = Arc::new(RgbaImage::from_pixel(
            source[0],
            source[1],
            Rgba([0, 0, 0, 255]),
        ));
        let a = style.annotation(black.clone(), rect, size, 0)?;
        let view = a.image.as_ref().unwrap().view().rect;
        let (lo, hi) = a.body_bounds();
        let base = style.render(black, rect, size, 0)?;
        let white = style.render(
            Arc::new(RgbaImage::from_pixel(source[0], source[1], Rgba([255; 4]))),
            rect,
            size,
            0,
        )?;
        let gain = base
            .pixels()
            .zip(white.pixels())
            .map(|(b, w)| [0, 1, 2].map(|i| w[i].saturating_sub(b[i])))
            .collect();
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
        })
    }
    fn render(&self, source: &RgbaImage) -> RgbaImage {
        let mut out = self.base.clone();
        let pixels = out.as_mut().as_chunks_mut::<4>().0;
        let source_pixels = source.as_raw().as_chunks::<4>().0;
        let base = self.base.as_raw().as_chunks::<4>().0;
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
}
