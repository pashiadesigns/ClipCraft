use crate::{Annotation, Document, Error, Result, validate_dimensions};
use image::{Rgba, RgbaImage};

impl Document {
    pub fn can_revert(&self) -> bool {
        self.source_size.is_some() || self.raster.is_some() || !self.annotations.is_empty()
    }

    /// Preflight for both dialog and command. Pixel allocation happens only
    /// when rendering; invalid sizes/styles cannot modify document or history.
    pub fn resized_annotations(&self, width: u32, height: u32) -> Result<Vec<Annotation>> {
        validate_dimensions(width, height)?;
        let sx = width as f32 / self.width as f32;
        let sy = height as f32 / self.height as f32;
        if (sx - sy).abs() > 0.0001 && self.annotations.iter().any(|a| a.rotation_degrees != 0.0) {
            return Err(Error::Invalid("Keep proportions when resizing an image with rotated objects, or use flattened resize.".into()));
        }
        let style_scale = sx.min(sy);
        let point = |p: &mut [f32; 2]| {
            p[0] = (p[0] * sx).clamp(0.0, width as f32);
            p[1] = (p[1] * sy).clamp(0.0, height as f32);
        };
        let mut annotations = self.annotations.clone();
        for annotation in &mut annotations {
            point(&mut annotation.start);
            point(&mut annotation.end);
            for p in &mut annotation.points {
                point(p);
            }
            annotation.corner_radius =
                (annotation.corner_radius * style_scale).min(annotation.max_corner_radius());
            if let Some(shadow) = &mut annotation.shadow {
                shadow.distance *= style_scale;
                shadow.blur *= style_scale;
                shadow.validate()?;
            }
            if let Some(outline) = &mut annotation.step_outline {
                outline.width *= style_scale;
                outline.validate()?;
            }
            if annotation.kind != crate::Kind::Redact {
                let stroke = (annotation.width as f32 * style_scale).round().max(1.0);
                if stroke > 32.0 {
                    return Err(Error::Invalid("This resize would exceed the supported 32-pixel annotation stroke. Choose a smaller size.".into()));
                }
                annotation.width = stroke as u32;
            }
            if let Some(style) = &mut annotation.text {
                if let Some(outline) = &mut style.outline {
                    outline.width *= style_scale;
                }
                if let Some(typo) = &mut style.typography {
                    typo.spacing *= style_scale;
                }
                style.size_points *= style_scale;
                crate::text::rich::scale_runs(style, style_scale);
                style.padding *= style_scale;
                style.validate().map_err(|_| Error::Invalid("This resize exceeds the supported text size or padding. Choose dimensions closer to the current size.".into()))?;
            }
            if let Some(style) = &mut annotation.callout {
                point(&mut style.pointer);
                style.padding *= style_scale;
            }
            if let Some(style) = &annotation.callout {
                style.validate(annotation, [width, height], self.canvas_behavior.allow_overflow).map_err(|_| Error::Invalid("This resize exceeds supported callout padding or geometry. Choose dimensions closer to the current size.".into()))?;
            }
        }
        Ok(annotations)
    }
}

/// Alpha-aware triangle resampling. Hidden RGB must not bleed into visible
/// pixels; the unchanged PNG remains the baseline for later resizes and Revert.
pub(crate) fn resample(source: RgbaImage, width: u32, height: u32) -> RgbaImage {
    let mut linear = image::DynamicImage::ImageRgba8(source).into_rgba32f();
    for pixel in linear.pixels_mut() {
        for channel in 0..3 {
            pixel.0[channel] *= pixel.0[3];
        }
    }
    let resized = image::imageops::resize(
        &linear,
        width,
        height,
        image::imageops::FilterType::Triangle,
    );
    drop(linear);
    RgbaImage::from_fn(width, height, |x, y| {
        let pixel = resized.get_pixel(x, y).0;
        let alpha = pixel[3].clamp(0.0, 1.0);
        let mut color = [0; 4];
        if alpha > 0.0 {
            for i in 0..3 {
                color[i] = (pixel[i] / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
        color[3] = (alpha * 255.0).round() as u8;
        Rgba(color)
    })
}
