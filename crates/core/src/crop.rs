//! Rectangular whole-image crop. Editing and automation share this exact path.
use crate::{Document, Error, RasterImage, Result, validate_dimensions};

impl Document {
    pub(crate) fn crop_image(&mut self, x: u32, y: u32, width: u32, height: u32) -> Result<bool> {
        validate_dimensions(width, height)?;
        if x.checked_add(width).is_none_or(|right| right > self.width)
            || y.checked_add(height)
                .is_none_or(|bottom| bottom > self.height)
        {
            return Err(Error::Invalid(
                "Crop rectangle must be inside the image.".into(),
            ));
        }
        if [x, y, width, height] == [0, 0, self.width, self.height] {
            return Ok(false);
        }
        let source = self.render()?;
        let cropped = image::imageops::crop_imm(&source, x, y, width, height).to_image();
        let raster = std::sync::Arc::new(RasterImage::from_image(&cropped)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.width = width;
        self.height = height;
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
    use crate::{Command, ImageTransform, Session};
    use image::{Rgba, RgbaImage};

    #[test]
    fn crop_preserves_rgba_source_history_and_real_file_roundtrip() {
        let pixels = RgbaImage::from_fn(17, 11, |x, y| {
            Rgba([x as u8 * 13, y as u8 * 21, 89, ((x + y) % 3 * 100) as u8])
        });
        let original = Document::from_image(pixels.clone()).unwrap();
        let mut session = Session::new(original.clone()).unwrap();
        session
            .execute(Command::CropImage {
                x: 3,
                y: 2,
                width: 9,
                height: 7,
            })
            .unwrap();
        let cropped = session.document.render().unwrap();
        for (x, y, p) in cropped.enumerate_pixels() {
            assert_eq!(p, pixels.get_pixel(x + 3, y + 2));
        }
        assert_eq!(session.document.source_png, original.source_png);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crop.capture.json");
        session.save(&path).unwrap();
        assert_eq!(Document::load(&path).unwrap().render().unwrap(), cropped);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), pixels);
        assert!(!session.can_undo());
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.render().unwrap(), cropped);
        session
            .execute(Command::TransformImage {
                transform: ImageTransform::Clockwise,
            })
            .unwrap();
        session.execute(Command::RevertOriginal).unwrap();
        assert_eq!(session.document.render().unwrap(), pixels);
    }

    #[test]
    fn crop_of_composite_flattens_once_and_undo_restores_editable_objects() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let before = serde_json::to_vec(&doc).unwrap();
        let pixels = doc.render().unwrap();
        let mut session = Session::new(doc).unwrap();
        let command: Command = serde_json::from_str(
            r#"{"command":"crop_image","x":50,"y":30,"width":290,"height":200}"#,
        )
        .unwrap();
        session.execute(command).unwrap();
        assert!(session.document.annotations.is_empty());
        assert_eq!(
            session.document.render().unwrap(),
            image::imageops::crop_imm(&pixels, 50, 30, 290, 200).to_image()
        );
        session.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
        assert!(!session.can_undo());
        session
            .execute(Command::CropImage {
                x: 0,
                y: 0,
                width: 640,
                height: 400,
            })
            .unwrap();
        assert!(session.can_redo());
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
        for (x, y, width, height) in [
            (0, 0, 0, 1),
            (0, 0, 1, 0),
            (639, 0, 2, 1),
            (0, 399, 1, 2),
            (u32::MAX, 0, 2, 2),
            (0, u32::MAX, 2, 2),
        ] {
            assert!(
                session
                    .execute(Command::CropImage {
                        x,
                        y,
                        width,
                        height
                    })
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
            assert!(session.can_redo());
        }
    }
}
