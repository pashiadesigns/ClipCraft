//! Original per-object outer shadows. The silhouette uses the shared renderer;
//! its native-resolution alpha patch is cached by the ordered display list.
use crate::{Annotation, Error, Kind, Result, display::Viewport};
use image::{GrayImage, Luma, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    pub color: [u8; 4],
    /// Clockwise from right; 90 degrees casts downward, independent of rotation.
    pub angle: f32,
    pub distance: f32,
    /// Blur diameter in image pixels (Gaussian sigma = blur / 2).
    pub blur: f32,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 153],
            angle: 90.,
            distance: 5.,
            blur: 2.,
        }
    }
}

impl Shadow {
    pub fn validate(self) -> Result<()> {
        if !self.angle.is_finite()
            || !(0.0..=360.0).contains(&self.angle)
            || !self.distance.is_finite()
            || !(0.0..=256.0).contains(&self.distance)
            || !self.blur.is_finite()
            || !(0.0..=64.0).contains(&self.blur)
        {
            return Err(Error::Invalid(
                "Shadow angle must be 0–360°, distance 0–256 and blur 0–64 pixels".into(),
            ));
        }
        Ok(())
    }
    pub fn offset(self) -> [f32; 2] {
        let (sin, cos) = self.angle.to_radians().sin_cos();
        [cos * self.distance, sin * self.distance].map(|v| if v.abs() < 0.0001 { 0. } else { v })
    }
    pub fn changed_fields(self, before: Self, after: Self) -> Self {
        Self {
            color: if before.color != after.color {
                after.color
            } else {
                self.color
            },
            angle: if before.angle != after.angle {
                after.angle
            } else {
                self.angle
            },
            distance: if before.distance != after.distance {
                after.distance
            } else {
                self.distance
            },
            blur: if before.blur != after.blur {
                after.blur
            } else {
                self.blur
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Patch {
    pub origin: [i32; 2],
    pub pixels: Arc<RgbaImage>,
}

pub(crate) fn above_pixel_effect(kind: Kind) -> bool {
    matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Eraser)
}

/// Conservative ink + Gaussian reach, including empty body borders and tails.
pub fn bounds(a: &Annotation) -> Option<([f32; 2], [f32; 2])> {
    let shadow = a.shadow.filter(|s| s.color[3] != 0)?;
    let (lo, hi) = a.bounds();
    let stroke = if a.kind == Kind::Arrow {
        a.width as f32 * 4.0
    } else {
        a.width as f32 * 0.5
    };
    let text = a
        .text
        .as_ref()
        .and_then(|t| t.outline)
        .map_or(0., |o| o.width);
    let step = a.step_outline.as_ref().map_or(0., |o| o.width * 0.5);
    let reach = stroke.max(text).max(step) + shadow.blur * 1.5 + 2.;
    let offset = shadow.offset();
    Some((
        [0, 1].map(|i| (lo[i] + offset[i] - reach).floor()),
        [0, 1].map(|i| (hi[i] + offset[i] + reach).ceil()),
    ))
}

pub(crate) fn validate_patch(a: &Annotation) -> Result<()> {
    if let Some((lo, hi)) = bounds(a) {
        let size = [0, 1].map(|i| hi[i] - lo[i]);
        if size
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > 65535.)
            || f64::from(size[0]) * f64::from(size[1]) > crate::MAX_PIXELS as f64
        {
            return Err(Error::Invalid("Object shadow exceeds the 25-megapixel effect budget; reduce the object or blur size".into()));
        }
    }
    Ok(())
}

// Cache normalized object silhouettes across editor transactions. Integer
// movement reuses the pixels; fractional movement, style or source edits change
// the key. Arc shares pixels with display lists. The process-wide LRU is bounded
// to 25M pixels and 64 entries and never stores a document or history snapshot.
type ShadowCache = VecDeque<(Annotation, Patch)>;
static CACHE: OnceLock<Mutex<ShadowCache>> = OnceLock::new();

fn cache_bytes((a, patch): &(Annotation, Patch)) -> usize {
    patch.pixels.as_raw().len()
        + a.image.as_ref().map_or(0, |image| image.memory_bytes())
        + a.points.len() * std::mem::size_of::<[f32; 2]>()
        + a.text.as_ref().map_or(0, |text| text.content.len())
}

pub(crate) fn patch(a: &Annotation) -> Option<Patch> {
    bounds(a)?;
    validate_patch(a).ok()?;
    let anchor = a.body_bounds().0.map(f32::floor);
    let mut key = crate::canvas_expansion::translated(std::slice::from_ref(a), anchor.map(|v| -v))
        .ok()?
        .remove(0);
    key.id = 0;
    let cache = CACHE.get_or_init(Default::default);
    let cached = {
        let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .iter()
            .position(|(stored, _)| stored == &key)
            .map(|index| {
                let entry = guard.remove(index).unwrap();
                let patch = entry.1.clone();
                guard.push_back(entry);
                patch
            })
    };
    let mut patch = if let Some(patch) = cached {
        patch
    } else {
        let patch = uncached_patch(&key)?;
        let mut guard = cache.lock().unwrap_or_else(|p| p.into_inner());
        guard.push_back((key, patch.clone()));
        while guard.len() > 64
            || guard.iter().map(cache_bytes).sum::<usize>() > crate::MAX_PIXELS as usize * 4
        {
            guard.pop_front();
        }
        patch
    };
    for (i, offset) in anchor.into_iter().enumerate() {
        patch.origin[i] += offset as i32;
    }
    Some(patch)
}

fn uncached_patch(a: &Annotation) -> Option<Patch> {
    let (lo, hi) = bounds(a)?;
    // Documents and edits validate this before rendering. Preview callers may
    // still be dragging an oversized object; avoid unbounded allocations there.
    validate_patch(a).ok()?;
    let shadow = a.shadow?;
    let offset = shadow.offset();
    let size = [0, 1].map(|i| (hi[i] - lo[i]) as u32);
    let mut silhouette = RgbaImage::new(size[0], size[1]);
    let mut ink = a.clone();
    ink.shadow = None;
    // Pixel operations cast the outline of their region/brush. Their shadow is
    // knocked out underneath the operation so it cannot contaminate blur input.
    let pixel_effect = matches!(a.kind, Kind::Blur | Kind::Pixelate | Kind::Eraser);
    if matches!(a.kind, Kind::Blur | Kind::Pixelate) {
        ink.kind = Kind::Simplify;
        ink.color = [255; 4];
    } else if a.kind == Kind::Eraser {
        ink.kind = Kind::Pen;
        ink.color = [255; 4];
    }
    let view = Viewport {
        size,
        origin: [0, 1].map(|i| f64::from(lo[i] - offset[i])),
        scale: 1.,
    };
    crate::display::render_vector(&mut silhouette, &ink, view);
    let alpha = GrayImage::from_fn(size[0], size[1], |x, y| {
        Luma([silhouette.get_pixel(x, y)[3]])
    });
    let alpha = if shadow.blur > 0. {
        image::imageops::blur(&alpha, shadow.blur * 0.5)
    } else {
        alpha
    };
    if pixel_effect {
        silhouette.fill(0);
        crate::display::render_vector(
            &mut silhouette,
            &ink,
            Viewport {
                origin: lo.map(f64::from),
                ..view
            },
        );
    }
    let pixels = RgbaImage::from_fn(size[0], size[1], |x, y| {
        let mut color = shadow.color;
        let alpha = u32::from(alpha.get_pixel(x, y)[0]) * u32::from(color[3]);
        color[3] = if pixel_effect {
            ((alpha * (255 - u32::from(silhouette.get_pixel(x, y)[3])) + 32512) / 65025) as u8
        } else {
            ((alpha + 127) / 255) as u8
        };
        Rgba(color)
    });
    Some(Patch {
        origin: lo.map(|v| v as i32),
        pixels: Arc::new(pixels),
    })
}

impl Patch {
    pub(crate) fn paint(&self, image: &mut RgbaImage) {
        for (x, y, pixel) in self.pixels.enumerate_pixels() {
            let x = x as i64 + i64::from(self.origin[0]);
            let y = y as i64 + i64::from(self.origin[1]);
            if pixel[3] != 0
                && x >= 0
                && y >= 0
                && x < i64::from(image.width())
                && y < i64::from(image.height())
            {
                crate::text::composite(image.get_pixel_mut(x as u32, y as u32), pixel.0);
            }
        }
    }
}

#[cfg(test)]
mod tests;
