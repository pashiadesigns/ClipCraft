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

/// Transform editable objects as one selection. Local source geometry is retained.
pub fn transform_objects(
    objects: &[crate::Annotation],
    transform: ImageTransform,
) -> Result<Vec<crate::Annotation>> {
    if objects.is_empty() {
        return Err(crate::Error::Invalid("Select one or more objects.".into()));
    }
    let (lo, hi) = if objects.len() == 1 {
        objects[0].bounds()
    } else {
        crate::groups::bounds(objects)?
    };
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let degrees = match transform {
        ImageTransform::Clockwise => 90.,
        ImageTransform::CounterClockwise => -90.,
        ImageTransform::HalfTurn => 180.,
        _ => 0.,
    };
    let rotation = crate::rotation::Rotation::new(center, degrees)?;
    let map = |p: [f32; 2]| match transform {
        ImageTransform::FlipHorizontal => [2. * center[0] - p[0], p[1]],
        ImageTransform::FlipVertical => [p[0], 2. * center[1] - p[1]],
        _ => rotation.point(p),
    };
    objects
        .iter()
        .map(|a| {
            if !a.can_rotate() {
                let mut b = a.clone();
                b.start = map(a.start);
                b.end = map(a.end);
                b.points = a.points.iter().copied().map(map).collect();
                return Ok(b);
            }
            let old = a.rotation().center;
            let new = map(old);
            let mut b = crate::canvas_expansion::translated(
                std::slice::from_ref(a),
                [new[0] - old[0], new[1] - old[1]],
            )?
            .remove(0);
            let angle = match transform {
                ImageTransform::FlipHorizontal => {
                    b.mirror_x = !a.mirror_x;
                    -a.rotation_degrees
                }
                ImageTransform::FlipVertical => {
                    b.mirror_x = !a.mirror_x;
                    180. - a.rotation_degrees
                }
                _ => a.rotation_degrees + degrees,
            };
            b = b.with_rotation(angle)?;
            Ok(b)
        })
        .collect()
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
        self.version = crate::MAX_PROJECT_VERSION;
        self.annotations.clear();
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Placement, Session};
    use image::{Rgba, RgbaImage};
    #[test]
    fn selected_transforms_keep_sources_vectors_text_and_exact_undo() {
        use std::sync::Arc;
        let mut doc = Document::blank(256, 256).unwrap();
        let mut shape = crate::templates::text(1, [40., 50., 40., 50.], "", 12., false);
        shape.kind = crate::Kind::Rectangle;
        shape.fill = Some([20, 130, 200, 255]);
        shape.color = [200, 35, 70, 255];
        shape.width = 2;
        let pixels = RgbaImage::from_fn(40, 50, |x, y| Rgba([x as u8 * 5, y as u8 * 4, 80, 255]));
        let bitmap = Arc::new(
            crate::bitmap::Bitmap::new(Arc::new(RasterImage::from_image(&pixels).unwrap()))
                .unwrap(),
        );
        let image =
            crate::canvas_expansion::translated(&[bitmap.clone().annotation(2)], [176., 156.])
                .unwrap()
                .remove(0);
        let text = crate::templates::text(3, [90., 110., 76., 40.], "Flip", 18., true);
        doc.annotations = vec![shape, image, text];
        doc.validate().unwrap();
        let original = doc.render().unwrap();
        for action in ALL {
            let mut s = Session::new(doc.clone()).unwrap();
            let annotations = transform_objects(&doc.annotations, action).unwrap();
            s.execute(Command::UpdateMany { annotations }).unwrap();
            assert_eq!(s.document.annotations.len(), 3);
            assert!(s.document.raster.is_none());
            assert!(Arc::ptr_eq(
                s.document.annotations[1].image.as_ref().unwrap(),
                &bitmap
            ));
            assert_eq!(s.document.annotations[2].text, doc.annotations[2].text);
            let actual = s.document.render().unwrap();
            let expected = expected(&original, action);
            let error = actual
                .as_raw()
                .iter()
                .zip(expected.as_raw())
                .map(|(a, b)| a.abs_diff(*b) as u64)
                .sum::<u64>() as f64
                / actual.as_raw().len() as f64;
            assert!(error < 0.1, "{action:?}: {error}");
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("transformed.iclip");
            s.save(&path).unwrap();
            assert_eq!(Document::load(&path).unwrap().render().unwrap(), actual);
            if action == ImageTransform::FlipHorizontal
                && let Ok(dir) = std::env::var("CLIPDESIGNER_PATTERN_FIXTURES")
            {
                s.document
                    .save(&std::path::Path::new(&dir).join("editable-transforms.capture.json"))
                    .unwrap();
            }
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, doc.annotations);
            assert!(!s.can_undo());
        }
        for action in [ImageTransform::FlipHorizontal, ImageTransform::FlipVertical] {
            let twice = transform_objects(
                &transform_objects(&doc.annotations, action).unwrap(),
                action,
            )
            .unwrap();
            assert_eq!(twice, doc.annotations);
        }
        let mirrored = transform_objects(&doc.annotations, ImageTransform::FlipHorizontal).unwrap();
        let a = &mirrored[2];
        let (lo, hi) = a.body_bounds();
        let point = [lo[0] + 8., lo[1] + 9.];
        let roundtrip = a.rotation().inverse_point(a.rotation().point(point));
        assert!((roundtrip[0] - point[0]).abs() < 0.001);
        let _ = hi;
    }

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
