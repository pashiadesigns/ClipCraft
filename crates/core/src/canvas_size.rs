use crate::{Annotation, Document, Error, Result, STANDARD, validate_dimensions};
use base64::Engine as _;
use image::{ImageFormat, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{io::Cursor, sync::Arc};

/// Alignment of the existing image in the new canvas. Center rounds negative
/// odd offsets toward the leading edge, keeping the rule deterministic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    TopLeft,
    Top,
    TopRight,
    Left,
    #[default]
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}
impl Placement {
    pub const ALL: [Self; 9] = [
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
            Self::TopLeft => "Top left",
            Self::Top => "Top center",
            Self::TopRight => "Top right",
            Self::Left => "Middle left",
            Self::Center => "Center",
            Self::Right => "Middle right",
            Self::BottomLeft => "Bottom left",
            Self::Bottom => "Bottom center",
            Self::BottomRight => "Bottom right",
        }
    }
    pub fn offset(self, old: [u32; 2], new: [u32; 2]) -> [i64; 2] {
        let index = Self::ALL.iter().position(|p| *p == self).unwrap();
        let axis = |i: usize, align: usize| {
            let delta = i64::from(new[i]) - i64::from(old[i]);
            match align {
                0 => 0,
                1 => delta.div_euclid(2),
                _ => delta,
            }
        };
        [axis(0, index % 3), axis(1, index / 3)]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    pub png: String,
}
impl RasterImage {
    pub(crate) fn render(&self, width: u32, height: u32) -> Result<RgbaImage> {
        self.render_with(width, height, crate::resize::Resampling::Automatic)
    }
    pub(crate) fn render_with(
        &self,
        width: u32,
        height: u32,
        mode: crate::resize::Resampling,
    ) -> Result<RgbaImage> {
        let bytes = STANDARD.decode(&self.png)?;
        let reader = image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
        if reader.into_dimensions()? != (self.width, self.height) {
            return Err(Error::Invalid(
                "Canvas raster dimensions do not match the project".into(),
            ));
        }
        let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)?.into_rgba8();
        if (width, height) == (self.width, self.height) {
            Ok(image)
        } else {
            Ok(crate::resize::resample_with(image, width, height, mode))
        }
    }
    pub(crate) fn from_image(image: &RgbaImage) -> Result<Self> {
        let mut data = Cursor::new(Vec::new());
        image.write_to(&mut data, ImageFormat::Png)?;
        Ok(Self {
            width: image.width(),
            height: image.height(),
            png: STANDARD.encode(data.into_inner()),
        })
    }
}

impl Document {
    /// Preserve geometry and editability; never clamp or silently discard an
    /// object crossing the proposed canvas edge. Full off-canvas edits follow.
    pub fn canvas_annotations(
        &self,
        width: u32,
        height: u32,
        placement: Placement,
    ) -> Result<Vec<Annotation>> {
        validate_dimensions(width, height)?;
        let delta = placement.offset([self.width, self.height], [width, height]);
        let mut objects = self.annotations.clone();
        for object in &mut objects {
            for (axis, amount) in delta.into_iter().enumerate() {
                object.start[axis] += amount as f32;
                object.end[axis] += amount as f32;
                for p in &mut object.points {
                    p[axis] += amount as f32;
                }
                if let Some(callout) = &mut object.callout {
                    callout.pointer[axis] += amount as f32;
                }
            }
            let (min, max) = object.bounds();
            if min[0] < 0.0 || min[1] < 0.0 || max[0] > width as f32 || max[1] > height as f32 {
                return Err(Error::Invalid("This placement would cut an editable object. Move it inside the new canvas first; off-canvas objects are not supported yet.".into()));
            }
        }
        Ok(objects)
    }
    pub(crate) fn resize_canvas(
        &mut self,
        width: u32,
        height: u32,
        placement: Placement,
    ) -> Result<()> {
        let annotations = self.canvas_annotations(width, height, placement)?;
        let source = self.base_image()?;
        let delta = placement.offset([self.width, self.height], [width, height]);
        let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 0]));
        // Copy exact bytes, including hidden RGB. This is translation/cropping,
        // never alpha compositing or resampling.
        let source_x = (-delta[0]).max(0) as u32;
        let source_y = (-delta[1]).max(0) as u32;
        let target_x = delta[0].max(0) as u32;
        let target_y = delta[1].max(0) as u32;
        let copy_width = (self.width - source_x).min(width - target_x);
        let copy_height = (self.height - source_y).min(height - target_y);
        for y in 0..copy_height {
            for x in 0..copy_width {
                canvas.put_pixel(
                    target_x + x,
                    target_y + y,
                    *source.get_pixel(source_x + x, source_y + y),
                );
            }
        }
        let raster = Arc::new(RasterImage::from_image(&canvas)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.width = width;
        self.height = height;
        self.annotations = annotations;
        if let Some(guide) = &mut self.guide {
            guide.top += delta[1] as f32;
        }
        self.raster = Some(raster);
        self.auto_canvas_base = None;
        self.version = crate::MAX_PROJECT_VERSION;
        Ok(())
    }
}
