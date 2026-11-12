//! Export watermark (FX-003, approved bi-proposal 5): text or a logo laid
//! over the whole output when exporting, copying or printing. The project
//! itself never changes. A watermark marks status or ownership; it hides
//! nothing, so the privacy review ignores it.
use crate::{Annotation, Error, Result, bitmap::Bitmap};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    #[default]
    Tile,
    Center,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
}
impl Placement {
    pub const ALL: [Self; 10] = [
        Self::Tile,
        Self::Center,
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
        Self::Top,
        Self::Bottom,
        Self::Left,
        Self::Right,
    ];
    /// The single positions as a 3 × 3 grid, row by row (bj-proposal).
    pub const GRID: [Self; 9] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Left,
        Self::Center,
        Self::Right,
        Self::BottomLeft,
        Self::Bottom,
        Self::BottomRight,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Tile => "Tile",
            Self::Center => "Center",
            Self::TopLeft => "Top left",
            Self::TopRight => "Top right",
            Self::BottomLeft => "Bottom left",
            Self::BottomRight => "Bottom right",
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Watermark {
    /// Preset name, such as "Confidential" or "Draft".
    pub name: String,
    pub text: String,
    /// A logo instead of text (PNG or SVG made into an image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<Arc<Bitmap>>,
    pub placement: Placement,
    /// 5–100 %.
    pub opacity: u8,
    /// Size as a share of the output's shorter side, 5–60 %.
    pub size: u8,
    /// −90° to 90°.
    pub rotation: f32,
    pub color: [u8; 4],
}
impl Default for Watermark {
    fn default() -> Self {
        Self {
            name: "Confidential".into(),
            text: "CONFIDENTIAL".into(),
            image: None,
            placement: Placement::Tile,
            opacity: 30,
            size: 12,
            rotation: -30.,
            color: [29, 36, 39, 255],
        }
    }
}
impl Watermark {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty()
            || self.name.chars().count() > 60
            || self.text.chars().count() > 120
            || (self.text.trim().is_empty() && self.image.is_none())
            || !(5..=100).contains(&self.opacity)
            || !(5..=60).contains(&self.size)
            || !self.rotation.is_finite()
            || !(-90. ..=90.).contains(&self.rotation)
        {
            return Err(Error::Invalid(
                "Add text or a logo, and keep Opacity 5–100% and Size 5–60%.".into(),
            ));
        }
        Ok(())
    }
    /// One stamp of the watermark, its top-left at `at`.
    fn stamp(&self, id: u64, at: [f32; 2], side: f32) -> Annotation {
        let height = side * f32::from(self.size) / 100.;
        let alpha = (u16::from(self.color[3]) * u16::from(self.opacity) / 100) as u8;
        let mut a = if let Some(image) = &self.image {
            let [w, h] = image.size().map(|v| v as f32);
            let mut a = image.clone().annotation(id);
            // An image object's strength is its color alpha.
            a.color[3] = (255 * u16::from(self.opacity) / 100) as u8;
            a.start = at;
            a.end = [at[0] + height * w / h.max(1.), at[1] + height];
            a
        } else {
            let font = height * 0.6;
            let width = self.text.chars().count() as f32 * font * 0.68 + font;
            let mut a =
                crate::templates::text(id, [at[0], at[1], width, height], &self.text, font, true);
            if let Some(t) = a.text.as_mut() {
                t.horizontal = crate::text::HorizontalAlignment::Center;
                t.vertical = crate::text::VerticalAlignment::Middle;
            }
            a.color = [self.color[0], self.color[1], self.color[2], alpha];
            if let Some(t) = a.text.as_mut() {
                t.color = Some(a.color);
            }
            a
        };
        a.rotation_degrees = self.rotation;
        a
    }
    /// Width and height of what one upright stamp actually draws. Text sits
    /// inside a looser box, and spacing by the box looks uneven.
    fn ink_size(&self, side: f32, stamp: [f32; 2]) -> [f32; 2] {
        if self.image.is_some() {
            return stamp;
        }
        let upright = Self {
            rotation: 0.,
            ..self.clone()
        };
        let mut pixels = RgbaImage::new(stamp[0].ceil() as u32 + 2, stamp[1].ceil() as u32 + 2);
        crate::render_annotation(&mut pixels, &upright.stamp(1, [0., 0.], side));
        let (mut lo, mut hi) = ([u32::MAX; 2], [0u32; 2]);
        for (x, y, p) in pixels.enumerate_pixels() {
            if p[3] > 0 {
                lo = [lo[0].min(x), lo[1].min(y)];
                hi = [hi[0].max(x + 1), hi[1].max(y + 1)];
            }
        }
        if hi[0] <= lo[0] || hi[1] <= lo[1] {
            return stamp;
        }
        [(hi[0] - lo[0]) as f32, (hi[1] - lo[1]) as f32]
    }
    /// The watermark drawn alone on a transparent image of the output size.
    pub fn layer(&self, size: [u32; 2]) -> Result<RgbaImage> {
        self.validate()?;
        let mut out = RgbaImage::new(size[0], size[1]);
        let [w, h] = size.map(|v| v as f32);
        let side = w.min(h);
        let probe = self.stamp(1, [0., 0.], side);
        let (lo, hi) = probe.body_bounds();
        let [sw, sh] = [hi[0] - lo[0], hi[1] - lo[1]];
        let margin = side * 0.04;
        let spots: Vec<[f32; 2]> = match self.placement {
            Placement::Tile => {
                // A grid along the watermark's own angle, with the same gap
                // between what is actually drawn in both directions.
                let ink = self.ink_size(side, [sw, sh]);
                let gap = ink[1].max(side * 0.04) * 1.6;
                let (step_x, step_y) = (ink[0] + gap, ink[1] + gap);
                let (sin, cos) = self.rotation.to_radians().sin_cos();
                let diagonal = w.hypot(h);
                let reach = sw.hypot(sh);
                let (nx, ny) = (
                    (diagonal / step_x).ceil() as i32 + 1,
                    (diagonal / step_y).ceil() as i32 + 1,
                );
                let mut centers = vec![];
                for row in -ny..=ny {
                    for column in -nx..=nx {
                        let (u, v) = (column as f32 * step_x, row as f32 * step_y);
                        let c = [w * 0.5 + u * cos - v * sin, h * 0.5 + u * sin + v * cos];
                        if c[0] > -reach && c[0] < w + reach && c[1] > -reach && c[1] < h + reach {
                            centers.push(c);
                        }
                    }
                }
                let middle = [w * 0.5, h * 0.5];
                let distance = |c: &[f32; 2]| (c[0] - middle[0]).hypot(c[1] - middle[1]);
                centers.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
                centers.truncate(600);
                centers
                    .into_iter()
                    .map(|c| [c[0] - sw * 0.5, c[1] - sh * 0.5])
                    .collect()
            }
            Placement::Center => vec![[(w - sw) * 0.5, (h - sh) * 0.5]],
            Placement::TopLeft => vec![[margin, margin]],
            Placement::TopRight => vec![[w - sw - margin, margin]],
            Placement::BottomLeft => vec![[margin, h - sh - margin]],
            Placement::BottomRight => vec![[w - sw - margin, h - sh - margin]],
            Placement::Top => vec![[(w - sw) * 0.5, margin]],
            Placement::Bottom => vec![[(w - sw) * 0.5, h - sh - margin]],
            Placement::Left => vec![[margin, (h - sh) * 0.5]],
            Placement::Right => vec![[w - sw - margin, (h - sh) * 0.5]],
        };
        for (i, at) in spots.into_iter().enumerate() {
            crate::render_annotation(&mut out, &self.stamp(i as u64 + 1, at, side));
        }
        Ok(out)
    }
}
/// Lays a watermark layer over finished pixels of the same size. The mark
/// covers the picture only: fully transparent areas (around a perspective
/// tilt, say) stay transparent, and soft edges take it in proportion, so the
/// export preview and every output agree (BI10).
pub fn blend(pixels: &mut RgbaImage, layer: &RgbaImage) {
    if pixels.dimensions() != layer.dimensions() {
        return;
    }
    for (p, w) in pixels.pixels_mut().zip(layer.pixels()) {
        if w[3] == 0 || p[3] == 0 {
            continue;
        }
        let alpha = (u16::from(w[3]) * u16::from(p[3]) / 255) as u8;
        let alpha_before = p[3];
        crate::text::composite(p, [w[0], w[1], w[2], alpha]);
        p[3] = alpha_before;
    }
}
/// Applies `watermark` to finished pixels, if there is one.
pub fn apply(pixels: &mut RgbaImage, watermark: Option<&Watermark>) -> Result<()> {
    if let Some(w) = watermark {
        let layer = w.layer([pixels.width(), pixels.height()])?;
        blend(pixels, &layer);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watermark_marks_output_without_changing_the_project() {
        let doc = crate::Document::canvas(400, 240, [255; 4]).unwrap();
        let before = serde_json::to_vec(&doc).unwrap();
        let clean = doc.render().unwrap();
        for placement in Placement::ALL {
            let w = Watermark {
                placement,
                ..Default::default()
            };
            let mut pixels = clean.clone();
            apply(&mut pixels, Some(&w)).unwrap();
            let changed = pixels
                .pixels()
                .zip(clean.pixels())
                .filter(|(a, b)| a != b)
                .count();
            assert!(changed > 50, "{placement:?} draws ({changed})");
            assert!(
                pixels.pixels().all(|p| p[3] == 255),
                "an opaque output stays opaque"
            );
        }
        // Tile covers the whole output; a corner stamp stays in its corner.
        let corner = Watermark {
            placement: Placement::BottomRight,
            rotation: 0.,
            ..Default::default()
        }
        .layer([400, 240])
        .unwrap();
        assert!(
            corner
                .enumerate_pixels()
                .all(|(x, y, p)| p[3] == 0 || (x > 150 && y > 150))
        );
        assert_eq!(serde_json::to_vec(&doc).unwrap(), before);
        let bad = Watermark {
            text: " ".into(),
            ..Default::default()
        };
        assert!(bad.layer([10, 10]).is_err());
        // BI10: transparent areas stay transparent; the picture is marked.
        let mut half = RgbaImage::from_fn(400, 240, |x, _| {
            if x < 200 {
                image::Rgba([0; 4])
            } else {
                image::Rgba([255; 4])
            }
        });
        let untouched = half.clone();
        apply(&mut half, Some(&Watermark::default())).unwrap();
        assert!(
            half.enumerate_pixels()
                .all(|(x, _, p)| x >= 200 || p[3] == 0)
        );
        assert!(
            half.enumerate_pixels()
                .all(|(x, _, p)| x < 200 || p[3] == 255)
        );
        assert_ne!(half, untouched, "the opaque half is marked");
    }
    /// BI10, BJ9: rotated tiles keep equal gaps along the watermark's axes,
    /// and a logo's strength follows Opacity.
    #[test]
    fn tiles_stay_evenly_spaced_when_rotated_and_logos_follow_opacity() {
        let w = Watermark {
            rotation: -30.,
            ..Default::default()
        };
        let layer = w.layer([900, 900]).unwrap();
        // Project the ink onto the watermark's cross axis: rows repeat at one pitch.
        let (sin, cos) = (-30f32).to_radians().sin_cos();
        let mut rows = vec![0u32; 2600];
        for (x, y, p) in layer.enumerate_pixels() {
            if p[3] > 0 {
                let v = -(x as f32 - 450.) * sin + (y as f32 - 450.) * cos;
                rows[(v + 1300.) as usize] += 1;
            }
        }
        let starts: Vec<usize> = (1..rows.len())
            .filter(|&i| rows[i] > 0 && rows[i - 1] == 0)
            .collect();
        let gaps: Vec<i32> = starts.windows(2).map(|p| (p[1] - p[0]) as i32).collect();
        // The outermost rows are clipped by the canvas corners, so they start late.
        let whole = &gaps[1..gaps.len().saturating_sub(1).max(1)];
        assert!(whole.len() >= 3, "{gaps:?}");
        assert!(whole.iter().all(|g| (g - whole[0]).abs() <= 2), "{gaps:?}");
        let logo = Arc::new(
            crate::bitmap::Bitmap::new(
                crate::clipboard_payload::ImagePayload::from_rgba(
                    8,
                    8,
                    RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255])).into_raw(),
                )
                .unwrap()
                .image,
            )
            .unwrap(),
        );
        let strength = |opacity| {
            let w = Watermark {
                image: Some(logo.clone()),
                placement: Placement::Center,
                rotation: 0.,
                opacity,
                ..Default::default()
            };
            let layer = w.layer([200, 200]).unwrap();
            layer.get_pixel(100, 100)[3]
        };
        assert!(strength(100) > strength(25), "Opacity changes a logo");
    }
    /// BI10: a logo watermark marks the output.
    #[test]
    fn logo_watermark_marks_the_output() {
        let logo = crate::bitmap::Bitmap::new(
            crate::clipboard_payload::ImagePayload::from_rgba(
                40,
                20,
                RgbaImage::from_pixel(40, 20, image::Rgba([200, 30, 30, 255])).into_raw(),
            )
            .unwrap()
            .image,
        )
        .unwrap();
        for placement in Placement::ALL {
            let w = Watermark {
                image: Some(Arc::new(logo.clone())),
                text: String::new(),
                placement,
                opacity: 80,
                ..Default::default()
            };
            let mut pixels = RgbaImage::from_pixel(400, 240, image::Rgba([255; 4]));
            apply(&mut pixels, Some(&w)).unwrap();
            let reddish = pixels
                .pixels()
                .filter(|p| u16::from(p[0]) > u16::from(p[1]) + 60)
                .count();
            assert!(reddish > 100, "{placement:?}: {reddish}");
        }
    }
}
