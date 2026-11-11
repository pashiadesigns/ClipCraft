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

/// Rotates objects about `center` by `degrees` (clockwise on screen), then
/// moves them by `offset`. Objects stay editable; their own geometry is kept.
pub(crate) fn rotate_objects(
    objects: &[crate::Annotation],
    center: [f32; 2],
    degrees: f32,
    offset: [f32; 2],
) -> Result<Vec<crate::Annotation>> {
    let rotation = crate::rotation::Rotation::new(center, degrees)?;
    let map = |p: [f32; 2]| {
        let q = rotation.point(p);
        [q[0] + offset[0], q[1] + offset[1]]
    };
    objects
        .iter()
        .map(|a| {
            if !a.can_rotate() {
                let mut b = a.clone();
                b.start = map(a.start);
                b.end = map(a.end);
                b.points = a.points.iter().copied().map(map).collect();
                if let Some(style) = &mut b.callout {
                    style.pointer = map(style.pointer);
                }
                return Ok(b);
            }
            let old = a.rotation().center;
            let new = map(old);
            let b = crate::canvas_expansion::translated(
                std::slice::from_ref(a),
                [new[0] - old[0], new[1] - old[1]],
            )?
            .remove(0);
            b.with_rotation(a.rotation_degrees + degrees)
        })
        .collect()
}

/// The picture rotated about its center, sampled smoothly. `grow` makes the
/// canvas big enough for every corner; new corners get `corners`.
pub(crate) fn rotate_pixels(
    source: &image::RgbaImage,
    degrees: f32,
    grow: bool,
    corners: [u8; 4],
) -> image::RgbaImage {
    let (w, h) = (f64::from(source.width()), f64::from(source.height()));
    let (sin, cos) = f64::from(degrees).to_radians().sin_cos();
    let (nw, nh) = if grow {
        (
            (w * cos.abs() + h * sin.abs()).round().max(1.),
            (w * sin.abs() + h * cos.abs()).round().max(1.),
        )
    } else {
        (w, h)
    };
    let premultiplied = |p: &image::Rgba<u8>| {
        let a = f64::from(p[3]) / 255.;
        [
            f64::from(p[0]) * a,
            f64::from(p[1]) * a,
            f64::from(p[2]) * a,
            a,
        ]
    };
    let at = |x: i64, y: i64| -> [f64; 4] {
        if x < 0 || y < 0 || x >= i64::from(source.width()) || y >= i64::from(source.height()) {
            [0.; 4]
        } else {
            premultiplied(source.get_pixel(x as u32, y as u32))
        }
    };
    let back = premultiplied(&image::Rgba(corners));
    image::RgbaImage::from_fn(nw as u32, nh as u32, |x, y| {
        // Back from the output pixel's center to the source, turning the other way.
        let dx = f64::from(x) + 0.5 - nw / 2.;
        let dy = f64::from(y) + 0.5 - nh / 2.;
        let sx = dx * cos + dy * sin + w / 2. - 0.5;
        let sy = -dx * sin + dy * cos + h / 2. - 0.5;
        let (x0, y0) = (sx.floor(), sy.floor());
        let (fx, fy) = (sx - x0, sy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let mut c = [0.; 4];
        for (px, py, weight) in [
            (x0, y0, (1. - fx) * (1. - fy)),
            (x0 + 1, y0, fx * (1. - fy)),
            (x0, y0 + 1, (1. - fx) * fy),
            (x0 + 1, y0 + 1, fx * fy),
        ] {
            let p = at(px, py);
            for i in 0..4 {
                c[i] += p[i] * weight;
            }
        }
        // The picture over the corner color.
        let a = c[3] + back[3] * (1. - c[3]);
        if a <= 0. {
            return image::Rgba([0; 4]);
        }
        let mut out = [0u8; 4];
        for i in 0..3 {
            // Color channels are already 0-255 (premultiplied); alpha is 0-1.
            out[i] = ((c[i] + back[i] * (1. - c[3])) / a).round().clamp(0., 255.) as u8;
        }
        out[3] = (a * 255.).round().clamp(0., 255.) as u8;
        image::Rgba(out)
    })
}

impl Document {
    /// Rotate Image by any angle (EDIT-038). Returns false when nothing changes.
    pub(crate) fn rotate_image(
        &mut self,
        degrees: f32,
        grow: bool,
        corners: Option<[u8; 4]>,
    ) -> Result<bool> {
        if !degrees.is_finite() || degrees.abs() > 360. {
            return Err(crate::Error::Invalid(
                "Choose an angle from -360 to 360 degrees.".into(),
            ));
        }
        if degrees.rem_euclid(360.) == 0. {
            return Ok(false);
        }
        if self.guide.as_ref().is_some_and(|g| g.pages.is_some()) {
            return Err(crate::Error::Invalid(
                "Rotate Image works on single images. On guide pages, rotate the objects instead."
                    .into(),
            ));
        }
        // Every fallible step happens before the document changes.
        let base = self.base_image()?;
        let pixels = rotate_pixels(&base, degrees, grow, corners.unwrap_or([0; 4]));
        crate::validate_dimensions(pixels.width(), pixels.height())?;
        let center = [self.width as f32 / 2., self.height as f32 / 2.];
        let offset = [
            pixels.width() as f32 / 2. - center[0],
            pixels.height() as f32 / 2. - center[1],
        ];
        let annotations = rotate_objects(&self.annotations, center, degrees, offset)?;
        let raster = std::sync::Arc::new(RasterImage::from_image(&pixels)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.width = pixels.width();
        self.height = pixels.height();
        self.raster = Some(raster);
        self.annotations = annotations;
        self.auto_canvas_base = None;
        self.version = crate::MAX_PROJECT_VERSION;
        Ok(true)
    }
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

    fn photo(w: u32, h: u32) -> Document {
        let pixels = RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 3) as u8, (y * 5) as u8, 90, 255]));
        Document::from_image(pixels).unwrap()
    }
    #[test]
    fn rotate_by_any_angle_keeps_objects_editable_and_is_one_undo() {
        let mut doc = photo(200, 100);
        let mut shape = crate::templates::text(1, [10., 10., 40., 30.], "", 12., false);
        shape.kind = crate::Kind::Rectangle;
        shape.fill = Some([20, 130, 200, 255]);
        doc.annotations.push(shape.clone());
        let before = serde_json::to_vec(&doc).unwrap();
        let mut session = Session::new(doc).unwrap();
        // A quarter turn with Grow to fit matches the exact 90-degree turn.
        session
            .execute(Command::RotateImage {
                degrees: 90.,
                grow: true,
                corners: None,
            })
            .unwrap();
        let d = &session.document;
        assert_eq!([d.width, d.height], [100, 200]);
        let exact = image::imageops::rotate90(&photo(200, 100).base_image().unwrap());
        let base = d.base_image().unwrap();
        let off = base
            .pixels()
            .zip(exact.pixels())
            .filter(|(a, b)| (0..4).any(|i| (i16::from(a[i]) - i16::from(b[i])).abs() > 2))
            .count();
        assert!(
            off < 300,
            "{off} pixels differ from an exact quarter turn: {:?} vs {:?}, {:?} vs {:?}",
            base.get_pixel(0, 0),
            exact.get_pixel(0, 0),
            base.get_pixel(50, 120),
            exact.get_pixel(50, 120)
        );
        // The object turned with the picture and is still a rectangle object.
        let a = &d.annotations[0];
        assert_eq!(a.kind, crate::Kind::Rectangle);
        assert!((a.rotation_degrees - 90.).abs() < 0.01);
        let c = a.rotation().center;
        // A clockwise quarter turn of a 200 x 100 picture sends (x, y) to (100 - y, x).
        let o = shape.rotation().center;
        assert!(
            (c[0] - (100. - o[1])).abs() < 0.6 && (c[1] - o[0]).abs() < 0.6,
            "{o:?} -> {c:?}"
        );
        session.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
    }
    #[test]
    fn rotate_keep_size_crops_corners_and_grow_fits_them() {
        let mut keep = photo(200, 100);
        assert!(keep.rotate_image(30., false, None).unwrap());
        assert_eq!([keep.width, keep.height], [200, 100]);
        let k = keep.base_image().unwrap();
        assert_eq!(k.get_pixel(0, 0)[3], 0, "new corners are transparent");
        let mut grow = photo(200, 100);
        assert!(
            grow.rotate_image(30., true, Some([255, 255, 255, 255]))
                .unwrap()
        );
        assert_eq!([grow.width, grow.height], [223, 187]);
        assert_eq!(
            grow.base_image().unwrap().get_pixel(0, 0).0,
            [255, 255, 255, 255]
        );
        let mut same = photo(20, 20);
        assert!(!same.rotate_image(360., true, None).unwrap());
        assert!(same.rotate_image(f32::NAN, true, None).is_err());
        assert!(same.rotate_image(400., true, None).is_err());
    }
    #[test]
    fn resampling_choice_is_saved_and_changes_only_resized_pictures() {
        let doc = photo(64, 48);
        let mut session = Session::new(doc).unwrap();
        session
            .execute(Command::ResizeImageResampled {
                width: 128,
                height: 96,
                resampling: crate::resize::Resampling::Nearest,
            })
            .unwrap();
        let d = &session.document;
        assert_eq!(
            d.canvas_behavior.resampling,
            crate::resize::Resampling::Nearest
        );
        let nearest = d.base_image().unwrap();
        // Nearest copies source pixels exactly: each 2 x 2 block is one color.
        assert_eq!(nearest.get_pixel(10, 10), nearest.get_pixel(11, 11));
        let json = serde_json::to_string(d).unwrap();
        assert!(json.contains("\"resampling\":\"nearest\""));
        let mut automatic = Session::new(photo(64, 48)).unwrap();
        automatic
            .execute(Command::ResizeImage {
                width: 128,
                height: 96,
            })
            .unwrap();
        assert!(
            !serde_json::to_string(&automatic.document)
                .unwrap()
                .contains("resampling")
        );
        assert_ne!(automatic.document.base_image().unwrap(), nearest);
        // Changing only the method at the same size is still one undoable step.
        session
            .execute(Command::ResizeImageResampled {
                width: 128,
                height: 96,
                resampling: crate::resize::Resampling::Sharp,
            })
            .unwrap();
        session.execute(Command::Undo).unwrap();
        assert_eq!(
            session.document.canvas_behavior.resampling,
            crate::resize::Resampling::Nearest
        );
    }
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
