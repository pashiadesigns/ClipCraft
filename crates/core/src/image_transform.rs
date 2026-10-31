//! Exact whole-image transforms. Source pixels remain available for Revert;
//! one history entry restores the full editable document before flattening.
use crate::{Document, RasterImage, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageTransform {
    Clockwise,
    CounterClockwise,
    HalfTurn,
    FlipHorizontal,
    FlipVertical,
}

impl Document {
    pub(crate) fn transform_image(&mut self, transform: ImageTransform) -> Result<bool> {
        let source = self.render()?;
        let image = match transform {
            ImageTransform::Clockwise => image::imageops::rotate90(&source),
            ImageTransform::CounterClockwise => image::imageops::rotate270(&source),
            ImageTransform::HalfTurn => image::imageops::rotate180(&source),
            ImageTransform::FlipHorizontal => image::imageops::flip_horizontal(&source),
            ImageTransform::FlipVertical => image::imageops::flip_vertical(&source),
        };
        if self.annotations.is_empty() && image == source {
            return Ok(false);
        }
        // Encode before changing state: bad sources or encoding errors are atomic.
        let raster = std::sync::Arc::new(RasterImage::from_image(&image)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.width = image.width();
        self.height = image.height();
        self.raster = Some(raster);
        self.auto_canvas_base = None;
        self.version = 2;
        self.annotations.clear();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Placement, Session};
    use image::{Rgba, RgbaImage};

    const ALL: [ImageTransform; 5] = [
        ImageTransform::Clockwise,
        ImageTransform::CounterClockwise,
        ImageTransform::HalfTurn,
        ImageTransform::FlipHorizontal,
        ImageTransform::FlipVertical,
    ];

    fn expected(image: &RgbaImage, transform: ImageTransform) -> RgbaImage {
        let (w, h) = image.dimensions();
        let swapped = matches!(
            transform,
            ImageTransform::Clockwise | ImageTransform::CounterClockwise
        );
        let mut result = RgbaImage::new(if swapped { h } else { w }, if swapped { w } else { h });
        for (x, y, p) in image.enumerate_pixels() {
            let (tx, ty) = match transform {
                ImageTransform::Clockwise => (h - 1 - y, x),
                ImageTransform::CounterClockwise => (y, w - 1 - x),
                ImageTransform::HalfTurn => (w - 1 - x, h - 1 - y),
                ImageTransform::FlipHorizontal => (w - 1 - x, y),
                ImageTransform::FlipVertical => (x, h - 1 - y),
            };
            result.put_pixel(tx, ty, *p);
        }
        result
    }

    #[test]
    fn all_five_transforms_preserve_every_rgba_byte_and_roundtrip() {
        let pixels = RgbaImage::from_fn(7, 3, |x, y| {
            Rgba([
                (x * 31) as u8,
                (y * 77) as u8,
                123,
                ((x + y) % 3 * 100) as u8,
            ])
        });
        for transform in ALL {
            let original = Document::from_image(pixels.clone()).unwrap();
            let mut session = Session::new(original.clone()).unwrap();
            let command: Command = serde_json::from_value(
                serde_json::json!({"command":"transform_image","transform":transform}),
            )
            .unwrap();
            session.execute(command).unwrap();
            let output = expected(&pixels, transform);
            assert_eq!(session.document.render().unwrap(), output);
            assert_eq!(session.document.source_png, original.source_png);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("transform.capture.json");
            session.save(&path).unwrap();
            assert_eq!(Document::load(&path).unwrap().render().unwrap(), output);
            session.execute(Command::Undo).unwrap();
            assert_eq!(session.document.render().unwrap(), pixels);
            assert!(!session.can_undo());
            session.execute(Command::Redo).unwrap();
            assert_eq!(session.document.render().unwrap(), output);
            session.execute(Command::RevertOriginal).unwrap();
            assert_eq!(session.document.render().unwrap(), pixels);
        }
    }

    #[test]
    fn transformed_composite_undo_restores_objects_and_original_after_canvas_resize() {
        let original: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let baseline = original.base_image().unwrap();
        let mut session = Session::new(original).unwrap();
        session
            .execute(Command::ResizeCanvas {
                width: 740,
                height: 480,
                placement: Placement::BottomRight,
            })
            .unwrap();
        let before = session.document.clone();
        let pixels = before.render().unwrap();
        for transform in ALL {
            session
                .execute(Command::TransformImage { transform })
                .unwrap();
            assert!(session.document.annotations.is_empty());
            assert_eq!(
                session.document.render().unwrap(),
                expected(&pixels, transform)
            );
            session.execute(Command::Undo).unwrap();
            assert_eq!(
                serde_json::to_vec(&session.document).unwrap(),
                serde_json::to_vec(&before).unwrap()
            );
        }
        session
            .execute(Command::TransformImage {
                transform: ImageTransform::Clockwise,
            })
            .unwrap();
        session.execute(Command::RevertOriginal).unwrap();
        assert_eq!(session.document.render().unwrap(), baseline);
        assert_eq!(
            [session.document.width, session.document.height],
            [640, 400]
        );
    }

    #[test]
    fn transform_noop_preserves_redo_and_bad_source_preserves_document() {
        let mut session = Session::new(Document::blank(10, 10).unwrap()).unwrap();
        session
            .execute(Command::ResizeImage {
                width: 20,
                height: 20,
            })
            .unwrap();
        session.execute(Command::Undo).unwrap();
        for transform in ALL {
            session
                .execute(Command::TransformImage { transform })
                .unwrap();
            assert!(session.can_redo());
            assert!(!session.can_undo());
            assert!(!session.is_modified());
        }
        assert!(
            serde_json::from_str::<Command>(
                r#"{"command":"transform_image","transform":"unknown"}"#
            )
            .is_err()
        );
        let mut broken = Document::blank(10, 10).unwrap();
        broken.source_png = "broken".into();
        let mut session = Session::new(broken).unwrap();
        let before = serde_json::to_vec(&session.document).unwrap();
        assert!(
            session
                .execute(Command::TransformImage {
                    transform: ImageTransform::Clockwise
                })
                .is_err()
        );
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
        assert!(!session.can_undo());
        assert!(!session.is_modified());
    }
}
