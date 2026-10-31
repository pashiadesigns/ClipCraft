//! Pixel-selection operations independent of the native canvas. A selection is
//! transient UI state; copying never mutates the image or its editable objects.
use crate::{Document, Error, RasterImage, Result};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub fn from_drag(start: [f32; 2], end: [f32; 2], size: [u32; 2]) -> Option<Self> {
        if start.into_iter().chain(end).any(|v| !v.is_finite()) || size.contains(&0) {
            return None;
        }
        let x = start[0].min(end[0]).floor().clamp(0.0, size[0] as f32) as u32;
        let y = start[1].min(end[1]).floor().clamp(0.0, size[1] as f32) as u32;
        let right = start[0].max(end[0]).ceil().clamp(0.0, size[0] as f32) as u32;
        let bottom = start[1].max(end[1]).ceil().clamp(0.0, size[1] as f32) as u32;
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    pub fn validate(self, size: [u32; 2]) -> Result<()> {
        if self.width == 0
            || self.height == 0
            || self
                .x
                .checked_add(self.width)
                .is_none_or(|right| right > size[0])
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|bottom| bottom > size[1])
        {
            return Err(Error::Invalid(
                "Pixel selection must be a nonempty rectangle inside the image.".into(),
            ));
        }
        Ok(())
    }
}

impl Document {
    pub fn copy_pixels(&self, rect: PixelRect) -> Result<RgbaImage> {
        rect.validate([self.width, self.height])?;
        Ok(
            image::imageops::crop_imm(&self.render()?, rect.x, rect.y, rect.width, rect.height)
                .to_image(),
        )
    }

    pub(crate) fn clear_pixels(&mut self, rect: PixelRect) -> Result<bool> {
        rect.validate([self.width, self.height])?;
        let mut image = self.render()?;
        let mut changed = false;
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                let pixel = image.get_pixel_mut(x, y);
                changed |= pixel.0 != [0; 4];
                *pixel = Rgba([0; 4]);
            }
        }
        if !changed {
            return Ok(false);
        }
        let raster = std::sync::Arc::new(RasterImage::from_image(&image)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.raster = Some(raster);
        self.annotations.clear();
        self.auto_canvas_base = None;
        self.version = 2;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};

    #[test]
    fn reverse_subpixel_selection_clips_and_rejects_empty_or_invalid_geometry() {
        assert_eq!(
            PixelRect::from_drag([50.2, 30.8], [-2.0, 10.4], [40, 25]),
            Some(PixelRect {
                x: 0,
                y: 10,
                width: 40,
                height: 15
            })
        );
        assert!(PixelRect::from_drag([50.0, 30.0], [60.0, 40.0], [40, 25]).is_none());
        assert!(PixelRect::from_drag([f32::NAN, 0.0], [1.0, 1.0], [40, 25]).is_none());
        assert!(
            PixelRect {
                x: u32::MAX,
                y: 0,
                width: 2,
                height: 1
            }
            .validate([40, 25])
            .is_err()
        );
        assert!(
            PixelRect {
                x: 0,
                y: 0,
                width: 0,
                height: 1
            }
            .validate([40, 25])
            .is_err()
        );
    }

    #[test]
    fn copy_is_exact_and_read_only_clear_preserves_outside_and_undo_objects() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let original = serde_json::to_vec(&doc).unwrap();
        let before = doc.render().unwrap();
        let rect = PixelRect {
            x: 60,
            y: 40,
            width: 120,
            height: 80,
        };
        let copied = doc.copy_pixels(rect).unwrap();
        assert_eq!(serde_json::to_vec(&doc).unwrap(), original);
        assert_eq!(copied.dimensions(), (120, 80));
        for (x, y, pixel) in copied.enumerate_pixels() {
            assert_eq!(pixel, before.get_pixel(x + rect.x, y + rect.y));
        }
        let mut session = Session::new(doc).unwrap();
        session.execute(Command::ClearPixels { rect }).unwrap();
        let cleared = session.document.render().unwrap();
        for (x, y, pixel) in cleared.enumerate_pixels() {
            assert_eq!(
                pixel,
                if (60..180).contains(&x) && (40..120).contains(&y) {
                    &Rgba([0; 4])
                } else {
                    before.get_pixel(x, y)
                }
            );
        }
        assert!(session.document.annotations.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("cleared.capture.json");
        session.save(&project).unwrap();
        assert_eq!(Document::load(&project).unwrap().render().unwrap(), cleared);
        session.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
        assert!(!session.can_undo());
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.render().unwrap(), cleared);
    }

    #[test]
    fn transparent_copy_and_noop_or_failed_clear_preserve_history() {
        let pixels = RgbaImage::from_fn(8, 6, |x, y| Rgba([20, x as u8, y as u8, 128]));
        let mut session = Session::new(Document::from_image(pixels.clone()).unwrap()).unwrap();
        let rect = PixelRect {
            x: 0,
            y: 0,
            width: 8,
            height: 6,
        };
        assert_eq!(session.document.copy_pixels(rect).unwrap(), pixels);
        session.execute(Command::ClearPixels { rect }).unwrap();
        session.execute(Command::ClearPixels { rect }).unwrap();
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), pixels);
        assert!(!session.can_undo());
        let invalid = PixelRect {
            x: 8,
            y: 0,
            width: 1,
            height: 1,
        };
        assert!(session.document.copy_pixels(invalid).is_err());
        assert!(
            session
                .execute(Command::ClearPixels { rect: invalid })
                .is_err()
        );
        assert!(session.can_redo());
        assert_eq!(session.document.render().unwrap(), pixels);
    }
}
