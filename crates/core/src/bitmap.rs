//! Embedded image objects retain original PNG pixels, separate from their transforms.
use crate::{Annotation, Error, Kind, RasterImage, Result, display::Viewport};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bitmap {
    raster: Arc<RasterImage>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    stamp: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vector: Option<Arc<crate::stamps::vector::VectorStamp>>,
    /// Presence identifies a captured cursor. Indices preserve Windows XOR
    /// pixels so an I-beam still inverts the background after it is moved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cursor_invert: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    view: Option<crate::image_frame::ImageView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    focus: Option<[f32; 2]>,
    #[serde(skip)]
    decoded: OnceLock<std::result::Result<Arc<RgbaImage>, String>>,
    #[serde(skip)]
    opaque: OnceLock<bool>,
}

impl PartialEq for Bitmap {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.raster, &other.raster) || self.raster == other.raster)
            && self.stamp == other.stamp
            && self.vector == other.vector
            && self.cursor_invert == other.cursor_invert
            && self.view == other.view
            && self.focus == other.focus
    }
}

impl Bitmap {
    pub fn vector(&self) -> Option<&crate::stamps::vector::VectorStamp> {
        self.vector.as_deref()
    }
    pub fn from_vector(vector: crate::stamps::vector::VectorStamp) -> Result<Self> {
        vector.validate()?;
        let mut value = Self::new(Arc::new(RasterImage::from_image(&RgbaImage::new(
            128, 128,
        ))?))?
        .into_stamp();
        value.vector = Some(Arc::new(vector));
        let a = Arc::new(value.clone()).annotation(1);
        let pixels = crate::display::render_annotation_preview(
            &a,
            Viewport {
                size: [128, 128],
                origin: [0.; 2],
                scale: 1.,
            },
        )?;
        value.raster = Arc::new(RasterImage::from_image(&pixels)?);
        value.decoded = OnceLock::new();
        value.opaque = OnceLock::new();
        Ok(value)
    }
    pub fn is_stamp(&self) -> bool {
        self.stamp
    }
    pub fn into_stamp(mut self) -> Self {
        self.stamp = true;
        self
    }
    pub fn view(&self) -> crate::image_frame::ImageView {
        self.view.unwrap_or_default()
    }
    pub fn focus(&self) -> Option<[f32; 2]> {
        self.focus
    }
    pub fn with_focus(&self, focus: [f32; 2]) -> Result<Self> {
        if !focus
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(Error::Invalid("Invalid image focus point".into()));
        }
        let mut next = self.clone();
        next.focus = Some(focus);
        Ok(next)
    }
    pub fn has_view(&self) -> bool {
        self.view.is_some()
    }
    pub(crate) fn validate_view(&self) -> Result<()> {
        if let Some(vector) = &self.vector {
            vector.validate()?;
            if !self.stamp || self.is_cursor() || self.view.is_some() || self.size() != [128, 128] {
                return Err(Error::Invalid("Invalid vector stamp properties".into()));
            }
        }
        if self
            .focus
            .is_some_and(|p| !p.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)))
        {
            return Err(Error::Invalid("Invalid image focus point".into()));
        }
        if let Some(view) = self.view {
            if self.is_cursor() {
                return Err(Error::Invalid("Cursor objects cannot be cropped".into()));
            }
            view.validate()?;
        }
        Ok(())
    }
    pub fn with_view(&self, view: Option<crate::image_frame::ImageView>) -> Result<Self> {
        let mut next = self.clone();
        next.view = view.filter(|v| *v != crate::image_frame::ImageView::default());
        next.validate_view()?;
        Ok(next)
    }

    pub(crate) fn memory_bytes(&self) -> usize {
        let [w, h] = self.size();
        self.raster
            .png
            .len()
            .saturating_add(w as usize * h as usize * 4)
            .saturating_add(self.cursor_invert.as_ref().map_or(0, |v| v.len() * 4))
            .saturating_add(self.vector.as_ref().map_or(0, |v| {
                v.paths.iter().map(|p| p.data.len()).sum::<usize>() + v.colors.len() * 4
            }))
    }
    pub fn new(raster: Arc<RasterImage>) -> Result<Self> {
        let value = Self {
            raster,
            stamp: false,
            vector: None,
            cursor_invert: None,
            view: None,
            focus: None,
            decoded: OnceLock::new(),
            opaque: OnceLock::new(),
        };
        value.pixels()?;
        Ok(value)
    }
    pub fn size(&self) -> [u32; 2] {
        [self.raster.width, self.raster.height]
    }
    pub fn is_cursor(&self) -> bool {
        self.cursor_invert.is_some()
    }
    pub fn has_inversion(&self) -> bool {
        self.cursor_invert.as_ref().is_some_and(|v| !v.is_empty())
    }
    pub(crate) fn inverted(&self, index: u32) -> bool {
        self.cursor_invert
            .as_ref()
            .is_some_and(|v| v.binary_search(&index).is_ok())
    }
    pub(crate) fn cursor(image: &RgbaImage, invert: Vec<u32>) -> Result<Self> {
        let mut bitmap = Self::new(Arc::new(RasterImage::from_image(image)?))?;
        bitmap.cursor_invert = Some(invert);
        bitmap.validate_cursor()?;
        Ok(bitmap)
    }
    pub(crate) fn validate_cursor(&self) -> Result<()> {
        if let Some(indices) = &self.cursor_invert {
            let [w, h] = self.size();
            if w > 512
                || h > 512
                || indices.windows(2).any(|v| v[0] >= v[1])
                || indices.iter().any(|i| {
                    *i >= w * h
                        || self
                            .pixels()
                            .is_ok_and(|p| p.get_pixel(*i % w, *i / w)[3] != 0)
                })
            {
                return Err(Error::Invalid("Invalid captured cursor mask".into()));
            }
        }
        Ok(())
    }
    pub fn pixels(&self) -> Result<&Arc<RgbaImage>> {
        self.decoded
            .get_or_init(|| {
                crate::validate_dimensions(self.raster.width, self.raster.height)
                    .and_then(|()| {
                        if self.raster.png.len() as u64 > crate::MAX_FILE_BYTES {
                            return Err(Error::Invalid("Embedded image exceeds 160 MB".into()));
                        }
                        self.raster
                            .render(self.raster.width, self.raster.height)
                            .map(Arc::new)
                    })
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|message| Error::Invalid(message.clone()))
    }
    pub fn annotation(self: Arc<Self>, id: u64) -> Annotation {
        let [w, h] = self.size();
        Annotation {
            image: Some(self),
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id,
            kind: Kind::Image,
            start: [0.0; 2],
            end: [w as f32, h as f32],
            color: [255; 4],
            width: 1,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            text: None,
            callout: None,
        }
    }
}

#[cfg(test)]
mod tests;

/// One sampler for export and zoomed display. Sample centers share the vector
/// coordinate convention; premultiplied interpolation avoids dark alpha fringes.
pub(crate) fn render(target: &mut RgbaImage, a: &Annotation, view: Viewport) {
    if a.image.as_ref().is_some_and(|b| b.vector().is_some()) {
        crate::display::render_vector(target, a, view);
        return;
    }
    render_impl(target, a, view, true);
}
fn render_impl(target: &mut RgbaImage, a: &Annotation, view: Viewport, fast: bool) {
    let Some(bitmap) = &a.image else { return };
    let Ok(source) = bitmap.pixels() else { return };
    let (min, max) = a.body_bounds();
    let extent = [max[0] - min[0], max[1] - min[1]];
    if extent.iter().any(|n| *n <= 0.0) {
        return;
    }
    let (low, high) = a.bounds();
    let screen = |v: f32, i: usize| (f64::from(v) - view.origin[i]) * view.scale;
    let left = screen(low[0], 0).floor().clamp(0.0, target.width() as f64) as u32;
    let top = screen(low[1], 1).floor().clamp(0.0, target.height() as f64) as u32;
    let right = screen(high[0], 0).ceil().clamp(0.0, target.width() as f64) as u32;
    let bottom = screen(high[1], 1).ceil().clamp(0.0, target.height() as f64) as u32;
    if right <= left || bottom <= top {
        return;
    }
    if fast
        && a.rotation_degrees == 0.0
        && !bitmap.is_cursor()
        && *bitmap
            .opaque
            .get_or_init(|| source.pixels().all(|p| p[3] == 255))
    {
        render_upright_opaque(target, a, source, view, [left, top, right, bottom]);
        return;
    }
    let center = [0, 1].map(|i| f64::from((min[i] + max[i]) * 0.5));
    let (sin, cos) = f64::from(-a.rotation_degrees).to_radians().sin_cos();
    let crop = bitmap.view().rect.map(f64::from);
    let ratio = [
        source.width() as f64 * crop[2] / f64::from(extent[0]),
        source.height() as f64 * crop[3] / f64::from(extent[1]),
    ];
    let supersample = a.rotation_degrees != 0.0;
    let samples: &[[f64; 2]] = if supersample {
        &[[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]
    } else {
        &[[0.5, 0.5]]
    };
    for y in top..bottom {
        for x in left..right {
            let mut sum = [0.0; 5];
            for offset in samples {
                let world = [
                    view.origin[0] + (x as f64 + offset[0]) / view.scale - center[0],
                    view.origin[1] + (y as f64 + offset[1]) / view.scale - center[1],
                ];
                let local = [
                    world[0] * cos - world[1] * sin + center[0] - min[0] as f64,
                    world[0] * sin + world[1] * cos + center[1] - min[1] as f64,
                ];
                if local[0] < 0.0
                    || local[1] < 0.0
                    || local[0] >= extent[0] as f64
                    || local[1] >= extent[1] as f64
                {
                    continue;
                }
                let coverage =
                    crate::image_frame::round_coverage(local, extent, a.corner_radius, view.scale);
                let point = [
                    local[0] * ratio[0] + crop[0] * source.width() as f64 - 0.5,
                    local[1] * ratio[1] + crop[1] * source.height() as f64 - 0.5,
                ];
                let pixel = sample(source, bitmap, point);
                for i in 0..5 {
                    sum[i] += pixel[i] * coverage / samples.len() as f64;
                }
            }
            if sum[4] > 0.0 {
                let background = target.get_pixel(x, y);
                for i in 0..3 {
                    let channel = f64::from(background[i]) * f64::from(background[3]) / 255.0
                        + 255.0
                        - f64::from(background[3]);
                    sum[i] += sum[4] * (255.0 - channel);
                }
                sum[3] += sum[4];
            }
            if sum[3] > 0.0 {
                let rgba = [0, 1, 2].map(|i| (sum[i] / sum[3]).round().clamp(0.0, 255.0) as u8);
                let alpha = (sum[3] * f64::from(a.color[3])).round().clamp(0.0, 255.0) as u8;
                crate::text::composite(
                    target.get_pixel_mut(x, y),
                    [rgba[0], rgba[1], rgba[2], alpha],
                );
            }
        }
    }
}

/// Ordinary screenshots need neither rotation sampling nor cursor XOR/alpha
/// lookups. Cache horizontal mapping once per row width, retaining the exact
/// premultiplied four-sample arithmetic used by the general renderer.
fn render_upright_opaque(
    target: &mut RgbaImage,
    a: &Annotation,
    source: &RgbaImage,
    view: Viewport,
    bounds: [u32; 4],
) {
    let (min, max) = a.body_bounds();
    let extent = [max[0] - min[0], max[1] - min[1]];
    let center = [0, 1].map(|i| f64::from((min[i] + max[i]) * 0.5));
    let size = [source.width(), source.height()];
    let crop = a.image.as_ref().unwrap().view().rect.map(f64::from);
    let mapping = |pixel: u32, axis: usize| {
        // Keep the original operation order, including center subtraction, so
        // fractional/negative origins round to the same source sample.
        let local = (view.origin[axis] + (f64::from(pixel) + 0.5) / view.scale - center[axis])
            + center[axis]
            - f64::from(min[axis]);
        if local < 0.0 || local >= f64::from(extent[axis]) {
            return None;
        }
        let point = local * (f64::from(size[axis]) * crop[axis + 2] / f64::from(extent[axis]))
            + crop[axis] * f64::from(size[axis])
            - 0.5;
        let index = point.floor();
        let max = f64::from(size[axis] - 1);
        Some((
            index.clamp(0.0, max) as u32,
            (index + 1.0).clamp(0.0, max) as u32,
            point - index,
        ))
    };
    let [left, top, right, bottom] = bounds;
    let columns: Vec<_> = (left..right).map(|x| mapping(x, 0)).collect();
    for y in top..bottom {
        let Some((y0, y1, fy)) = mapping(y, 1) else {
            continue;
        };
        for (index, column) in columns.iter().enumerate() {
            let Some((x0, x1, fx)) = *column else {
                continue;
            };
            let weights = [
                (1.0 - fx) * (1.0 - fy),
                fx * (1.0 - fy),
                (1.0 - fx) * fy,
                fx * fy,
            ];
            let pixels = [
                source.get_pixel(x0, y0),
                source.get_pixel(x1, y0),
                source.get_pixel(x0, y1),
                source.get_pixel(x1, y1),
            ];
            let mut sum = [0.0; 4];
            for (p, w) in pixels.into_iter().zip(weights) {
                for i in 0..3 {
                    sum[i] += f64::from(p[i]) * w;
                }
                sum[3] += w;
            }
            let rgb = [0, 1, 2].map(|i| (sum[i] / sum[3]).round().clamp(0., 255.) as u8);
            let alpha = (sum[3] * f64::from(a.color[3])).round().clamp(0., 255.) as u8;
            let coverage = if a.corner_radius > 0. {
                let local = [
                    view.origin[0] + (f64::from(left + index as u32) + 0.5) / view.scale
                        - f64::from(min[0]),
                    view.origin[1] + (f64::from(y) + 0.5) / view.scale - f64::from(min[1]),
                ];
                crate::image_frame::round_coverage(local, extent, a.corner_radius, view.scale)
            } else {
                1.
            };
            crate::text::composite(
                target.get_pixel_mut(left + index as u32, y),
                [
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    (f64::from(alpha) * coverage).round() as u8,
                ],
            );
        }
    }
}

fn sample(source: &RgbaImage, bitmap: &Bitmap, point: [f64; 2]) -> [f64; 5] {
    let [x, y] = point;
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let mut result = [0.0; 5];
    for (dx, dy, weight) in [
        (0.0, 0.0, (1.0 - fx) * (1.0 - fy)),
        (1.0, 0.0, fx * (1.0 - fy)),
        (0.0, 1.0, (1.0 - fx) * fy),
        (1.0, 1.0, fx * fy),
    ] {
        let x = (ix + dx).clamp(0.0, (source.width() - 1) as f64) as u32;
        let y = (iy + dy).clamp(0.0, (source.height() - 1) as f64) as u32;
        let pixel = source.get_pixel(x, y).0;
        if bitmap.inverted(y * source.width() + x) {
            result[4] += weight;
        }
        let alpha = f64::from(pixel[3]) / 255.0 * weight;
        for i in 0..3 {
            result[i] += f64::from(pixel[i]) * alpha;
        }
        result[3] += alpha;
    }
    result
}
