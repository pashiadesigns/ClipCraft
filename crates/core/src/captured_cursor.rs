//! Editable system cursors, independent of screenshot pixels and platform APIs.
use crate::{Annotation, Error, Result, bitmap::Bitmap};
use image::{Rgba, RgbaImage};
use std::sync::Arc;

#[derive(Clone)]
pub struct Cursor {
    pub position: [i32; 2],
    pub bitmap: Arc<Bitmap>,
}
impl Cursor {
    /// Recover alpha from two offscreen draws, preserving monochrome inversion.
    pub fn from_backgrounds(
        position: [i32; 2],
        black: &RgbaImage,
        white: &RgbaImage,
    ) -> Result<Self> {
        if black.dimensions() != white.dimensions()
            || black.width() == 0
            || black.height() == 0
            || black.width() > 512
            || black.height() > 512
        {
            return Err(Error::Invalid("Invalid system cursor dimensions".into()));
        }
        let mut output = RgbaImage::new(black.width(), black.height());
        let mut invert = Vec::new();
        for (index, ((_, _, b), (_, _, w))) in black
            .enumerate_pixels()
            .zip(white.enumerate_pixels())
            .enumerate()
        {
            let pixel = if b.0[..3] == [255; 3] && w.0[..3] == [0; 3] {
                invert.push(index as u32);
                Rgba([0; 4])
            } else {
                let delta: [i16; 3] = [0, 1, 2].map(|i| w[i] as i16 - b[i] as i16);
                if delta.iter().any(|d| *d < 0) {
                    return Err(Error::Invalid(
                        "This cursor uses an unsupported color inversion mask".into(),
                    ));
                }
                let alpha = 255 - (delta.iter().map(|v| *v as u32).sum::<u32>() + 1) / 3;
                Rgba([
                    (b[0] as u32 * 255 + alpha / 2)
                        .checked_div(alpha)
                        .unwrap_or(0)
                        .min(255) as u8,
                    (b[1] as u32 * 255 + alpha / 2)
                        .checked_div(alpha)
                        .unwrap_or(0)
                        .min(255) as u8,
                    (b[2] as u32 * 255 + alpha / 2)
                        .checked_div(alpha)
                        .unwrap_or(0)
                        .min(255) as u8,
                    alpha as u8,
                ])
            };
            output.put_pixel(
                index as u32 % black.width(),
                index as u32 / black.width(),
                pixel,
            );
        }
        Ok(Self {
            position,
            bitmap: Arc::new(Bitmap::cursor(&output, invert)?),
        })
    }
    /// Only the part actually inside the captured area belongs to this image.
    pub fn annotation(
        &self,
        origin: [i32; 2],
        size: [u32; 2],
        id: u64,
    ) -> Result<Option<Annotation>> {
        let position = [0, 1].map(|i| i64::from(self.position[i]) - i64::from(origin[i]));
        let source_size = self.bitmap.size();
        let low = [0, 1].map(|i| position[i].max(0));
        let high =
            [0, 1].map(|i| (position[i] + i64::from(source_size[i])).min(i64::from(size[i])));
        if high[0] <= low[0] || high[1] <= low[1] {
            return Ok(None);
        }
        let offset = [0, 1].map(|i| (low[i] - position[i]) as u32);
        let extent = [0, 1].map(|i| (high[i] - low[i]) as u32);
        let bitmap = if offset == [0, 0] && extent == source_size {
            self.bitmap.clone()
        } else {
            let cropped = image::imageops::crop_imm(
                self.bitmap.pixels()?.as_ref(),
                offset[0],
                offset[1],
                extent[0],
                extent[1],
            )
            .to_image();
            let invert = (0..extent[0] * extent[1])
                .filter(|index| {
                    self.bitmap.inverted(
                        (offset[1] + index / extent[0]) * source_size[0]
                            + offset[0]
                            + index % extent[0],
                    )
                })
                .collect();
            Arc::new(Bitmap::cursor(&cropped, invert)?)
        };
        let mut a = bitmap.annotation(id);
        a.start = low.map(|n| n as f32);
        a.end = high.map(|n| n as f32);
        Ok(Some(a))
    }
}

#[cfg(test)]
mod tests;
