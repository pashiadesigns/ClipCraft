//! Transient, antialiased selection masks. Coordinates inside a polygon are
//! normalized so the same bounds handles move/resize every selection shape.
use crate::{Document, Error, RasterImage, Result, bitmap::Bitmap, pixels::PixelRect};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vello_cpu::{
    Pixmap, RenderContext, Resources,
    color::{AlphaColor, Srgb},
    kurbo::{BezPath, Ellipse, Rect, Shape},
    peniko::Fill,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShapeMask {
    Rectangle,
    Ellipse,
    Polygon { points: Vec<[f32; 2]> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PixelMask {
    pub bounds: PixelRect,
    pub shape: ShapeMask,
    #[serde(default)]
    pub inverted: bool,
}
impl From<PixelRect> for PixelMask {
    fn from(bounds: PixelRect) -> Self {
        Self {
            bounds,
            shape: ShapeMask::Rectangle,
            inverted: false,
        }
    }
}
impl PixelMask {
    pub fn validate(&self, size: [u32; 2]) -> Result<()> {
        crate::validate_dimensions(size[0], size[1])?;
        self.bounds.validate(size)?;
        if let ShapeMask::Polygon { points } = &self.shape
            && (!(3..=1024).contains(&points.len())
                || points
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
        {
            return Err(Error::Invalid(
                "Selection polygon needs 3–1024 finite points inside its bounds.".into(),
            ));
        }
        Ok(())
    }
    pub fn output_bounds(&self, size: [u32; 2]) -> PixelRect {
        if self.inverted {
            PixelRect {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            }
        } else {
            self.bounds
        }
    }
    /// Only the bounding patch is rasterized; inversion is evaluated on lookup.
    pub fn coverage(&self, size: [u32; 2]) -> Result<Coverage> {
        self.validate(size)?;
        let b = self.bounds;
        let alpha = if matches!(self.shape, ShapeMask::Rectangle) {
            vec![255; b.width as usize * b.height as usize]
        } else {
            let w = f64::from(b.width);
            let h = f64::from(b.height);
            let path = match &self.shape {
                ShapeMask::Ellipse => Ellipse::from_rect(Rect::new(0., 0., w, h)).to_path(0.01),
                ShapeMask::Polygon { points } => {
                    let mut p = BezPath::new();
                    p.move_to((f64::from(points[0][0]) * w, f64::from(points[0][1]) * h));
                    for v in &points[1..] {
                        p.line_to((f64::from(v[0]) * w, f64::from(v[1]) * h));
                    }
                    p.close_path();
                    p
                }
                ShapeMask::Rectangle => unreachable!(),
            };
            let mut ctx = RenderContext::new(b.width as u16, b.height as u16);
            ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(255, 255, 255, 255));
            ctx.set_fill_rule(Fill::EvenOdd);
            ctx.fill_path(&path);
            ctx.flush();
            let mut pix = Pixmap::new(b.width as u16, b.height as u16);
            ctx.render(&mut pix, &mut Resources::new());
            pix.data().iter().map(|p| p.a).collect()
        };
        Ok(Coverage {
            bounds: b,
            alpha,
            inverted: self.inverted,
        })
    }
}
pub struct Coverage {
    bounds: PixelRect,
    alpha: Vec<u8>,
    inverted: bool,
}
impl Coverage {
    pub fn at(&self, x: u32, y: u32) -> u8 {
        let b = self.bounds;
        let a = if x >= b.x && y >= b.y && x < b.x + b.width && y < b.y + b.height {
            self.alpha[((y - b.y) * b.width + x - b.x) as usize]
        } else {
            0
        };
        if self.inverted { 255 - a } else { a }
    }
}
fn masked_copy(image: &RgbaImage, mask: &PixelMask, coverage: &Coverage) -> RgbaImage {
    let b = mask.output_bounds([image.width(), image.height()]);
    if !mask.inverted && matches!(mask.shape, ShapeMask::Rectangle) {
        return image::imageops::crop_imm(image, b.x, b.y, b.width, b.height).to_image();
    }
    RgbaImage::from_fn(b.width, b.height, |x, y| {
        let mut p = *image.get_pixel(x + b.x, y + b.y);
        p[3] = ((u16::from(p[3]) * u16::from(coverage.at(x + b.x, y + b.y)) + 127) / 255) as u8;
        if p[3] == 0 {
            p = Rgba([0; 4]);
        }
        p
    })
}
impl Document {
    pub fn copy_masked_pixels(&self, mask: &PixelMask) -> Result<RgbaImage> {
        let coverage = mask.coverage([self.width, self.height])?;
        Ok(masked_copy(&self.render()?, mask, &coverage))
    }
    pub(crate) fn edit_masked_pixels(&mut self, mask: &PixelMask, lift: bool) -> Result<bool> {
        let coverage = mask.coverage([self.width, self.height])?;
        let mut image = self.render()?;
        let copied = lift.then(|| masked_copy(&image, mask, &coverage));
        let bounds = mask.output_bounds([self.width, self.height]);
        let mut changed = false;
        for y in bounds.y..bounds.y + bounds.height {
            for x in bounds.x..bounds.x + bounds.width {
                let c = u32::from(coverage.at(x, y));
                if c == 0 {
                    continue;
                }
                let p = image.get_pixel_mut(x, y);
                let original = *p;
                let a = u32::from(p[3]);
                let part = (a * c + 127) / 255;
                if lift && part == 0 {
                    continue;
                }
                // A lifted feathered edge must composite back to the same alpha.
                // Clear uses the ordinary complementary mask; lift retains the
                // residual alpha required by source-over, preventing dark seams.
                p[3] = if lift {
                    if part == 255 {
                        0
                    } else {
                        ((a - part) * 255 + (255 - part) / 2) / (255 - part)
                    }
                } else {
                    a - part
                } as u8;
                if p[3] == 0 {
                    *p = Rgba([0; 4]);
                }
                changed |= *p != original || (lift && part > 0);
            }
        }
        if !changed {
            return Ok(false);
        }
        let raster = Arc::new(RasterImage::from_image(&image)?);
        let object = if let Some(copied) = copied {
            let id = self
                .annotations
                .iter()
                .map(|a| a.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("Object identifiers exhausted".into()))?;
            let mut a =
                Arc::new(Bitmap::new(Arc::new(RasterImage::from_image(&copied)?))?).annotation(id);
            a.start = [bounds.x as f32, bounds.y as f32];
            a.end = [
                (bounds.x + bounds.width) as f32,
                (bounds.y + bounds.height) as f32,
            ];
            Some(a)
        } else {
            None
        };
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.raster = Some(raster);
        self.annotations = object.into_iter().collect();
        self.guide = None;
        self.auto_canvas_base = None;
        self.version = if lift { 8 } else { 2 };
        Ok(true)
    }
}
#[cfg(test)]
mod tests;
