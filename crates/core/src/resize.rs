use crate::{Annotation, Document, Error, Result, validate_dimensions};
use image::{Rgba, RgbaImage};

impl Document {
    /// Resize Image: objects scale with the picture and stay editable; the
    /// picture itself is resampled when drawn.
    pub(crate) fn resize_image_to(&mut self, width: u32, height: u32) -> Result<()> {
        let annotations = self.resized_annotations(width, height)?;
        if let Some(guide) = &mut self.guide {
            let scale = height as f32 / self.height as f32;
            guide.top *= scale;
            for section in &mut guide.sections {
                section.height *= scale;
            }
        }
        let original_size = self.source_size.unwrap_or([self.width, self.height]);
        self.source_size =
            ([width, height] != original_size || self.raster.is_some()).then_some(original_size);
        self.width = width;
        self.height = height;
        self.annotations = annotations;
        self.auto_canvas_base = None;
        Ok(())
    }
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

/// How a resized picture is resampled (EDIT-028, approved beta 6 tab 4).
/// Automatic is the long-standing method, so existing projects draw exactly
/// as before.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Resampling {
    #[default]
    Automatic,
    /// Soft, for photos.
    Smooth,
    /// Crisp, for screenshots and text.
    Sharp,
    /// Hard pixel edges, for pixel art.
    Nearest,
}
impl Resampling {
    pub const ALL: [Self; 4] = [Self::Automatic, Self::Smooth, Self::Sharp, Self::Nearest];
    pub fn is_automatic(&self) -> bool {
        *self == Self::Automatic
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic",
            Self::Smooth => "Smooth (photos)",
            Self::Sharp => "Sharp (screenshots and text)",
            Self::Nearest => "Nearest pixel (pixel art)",
        }
    }
}

/// Alpha-aware resampling with the chosen method. Hidden RGB must not bleed
/// into visible pixels; the unchanged PNG remains the baseline for later
/// resizes and Revert.
pub(crate) fn resample_with(
    source: RgbaImage,
    width: u32,
    height: u32,
    mode: Resampling,
) -> RgbaImage {
    let filter = match mode {
        Resampling::Automatic => image::imageops::FilterType::Triangle,
        Resampling::Smooth => image::imageops::FilterType::Gaussian,
        Resampling::Sharp => image::imageops::FilterType::Lanczos3,
        Resampling::Nearest => image::imageops::FilterType::Nearest,
    };
    let mut linear = image::DynamicImage::ImageRgba8(source).into_rgba32f();
    for pixel in linear.pixels_mut() {
        for channel in 0..3 {
            pixel.0[channel] *= pixel.0[3];
        }
    }
    let resized = image::imageops::resize(&linear, width, height, filter);
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
