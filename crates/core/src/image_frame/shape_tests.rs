use super::*;
use crate::{Command, Document, Session};
use std::sync::Arc;

fn frame(kind: Kind) -> Annotation {
    let bitmap = crate::bitmap::Bitmap::new(Arc::new(
        crate::RasterImage::from_image(&image::RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([40, 130, 220, 255]),
        ))
        .unwrap(),
    ))
    .unwrap();
    let mut a = Arc::new(bitmap).annotation(1);
    a.kind = kind;
    a.start = [20., 20.];
    a.end = [120., 120.];
    a.color = [180, 30, 40, 255];
    a.width = 4;
    if kind == Kind::Polygon {
        a.points = crate::polygon::outline(a.start, a.end, 5);
    }
    a
}

#[test]
fn shape_frames_mask_source_preserve_geometry_crop_and_roundtrip() {
    for kind in [Kind::Rectangle, Kind::Ellipse, Kind::Polygon] {
        let a = frame(kind);
        let mut d = Document::canvas(150, 150, [255; 4]).unwrap();
        d.annotations.push(a.clone());
        d.promote_features();
        d.validate().unwrap();
        let rendered = d.render().unwrap();
        assert_eq!(
            *rendered.get_pixel(70, 70),
            image::Rgba([40, 130, 220, 255])
        );
        if kind != Kind::Rectangle {
            assert_eq!(*rendered.get_pixel(21, 21), image::Rgba([255; 4]));
        }
        let cropped = a
            .with_image_view(Some(ImageView::default().zoomed(2.).unwrap()))
            .unwrap();
        assert_eq!(cropped.start, a.start);
        assert_eq!(cropped.points, a.points);
        assert!(Arc::ptr_eq(
            cropped.image.as_ref().unwrap().pixels().unwrap(),
            a.image.as_ref().unwrap().pixels().unwrap()
        ));
        let json = serde_json::to_vec(&d).unwrap();
        let roundtrip: Document = serde_json::from_slice(&json).unwrap();
        roundtrip.validate().unwrap();
        assert_eq!(roundtrip.render().unwrap(), rendered);
        if let Ok(dir) = std::env::var("CLIPDESIGNER_FRAME_PAGES_FIXTURES") {
            std::fs::create_dir_all(&dir).unwrap();
            d.save(&std::path::Path::new(&dir).join(format!("frame-{kind:?}.capture.json")))
                .unwrap();
            rendered
                .save(std::path::Path::new(&dir).join(format!("frame-{kind:?}.png")))
                .unwrap();
            let pdf = crate::export::encode_document(
                &d,
                &crate::export::Options {
                    format: crate::export::Format::Pdf,
                    ..Default::default()
                },
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&dir).join(format!("frame-{kind:?}.pdf")),
                pdf,
            )
            .unwrap();
        }
    }
}

#[test]
fn frame_outline_and_image_opacity_are_independent_and_polygon_crop_updates_geometry() {
    let mut a = frame(Kind::Ellipse);
    a.color[3] = 0;
    let mut d = Document::canvas(150, 150, [255; 4]).unwrap();
    d.annotations.push(a.clone());
    d.promote_features();
    d.validate().unwrap();
    assert_eq!(
        *d.render().unwrap().get_pixel(70, 70),
        image::Rgba([40, 130, 220, 255])
    );
    a.image = Some(Arc::new(a.image.as_ref().unwrap().with_frame_opacity(0)));
    d.annotations[0] = a;
    assert_eq!(
        *d.render().unwrap().get_pixel(70, 70),
        image::Rgba([255; 4])
    );
    let polygon = frame(Kind::Polygon);
    let cropped = polygon
        .crop_image_handle(crate::selection::Handle::Right, [90., 70.])
        .unwrap();
    assert_eq!(cropped.body_bounds().1[0], 90.);
    assert!(cropped.points.iter().all(|p| p[0] <= 90.));
    assert!(Arc::ptr_eq(
        polygon.image.as_ref().unwrap().pixels().unwrap(),
        cropped.image.as_ref().unwrap().pixels().unwrap()
    ));
}

#[test]
fn conversion_and_inverse_are_single_undo_transactions_and_versioned() {
    let mut shape = frame(Kind::Polygon);
    shape.image = None;
    shape.text = Some(crate::text::TextStyle {
        content: "Original text".into(),
        ..Default::default()
    });
    let mut d = Document::blank(150, 150).unwrap();
    d.annotations.push(shape.clone());
    d.promote_features();
    let mut s = Session::new(d.clone()).unwrap();
    let converted = shape.converted_frame(true).unwrap();
    assert!(converted.text.is_none());
    assert!(converted.image.as_ref().unwrap().is_placeholder());
    assert_eq!(converted.points, shape.points);
    assert_eq!(converted.color, shape.color);
    s.execute(Command::Update {
        annotation: converted.clone(),
    })
    .unwrap();
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    let mut old = s.document.clone();
    old.version = 26;
    assert!(old.validate().is_err());
    s.execute(Command::Update {
        annotation: converted.converted_frame(false).unwrap(),
    })
    .unwrap();
    assert!(s.document.annotations[0].image.is_none());
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations[0], converted);
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations[0], shape);
}
