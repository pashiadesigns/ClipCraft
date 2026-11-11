use super::*;

#[test]
fn crop_handles_keep_pixel_scale_reveal_source_anchor_zoom_and_undo() {
    let pixels =
        image::RgbaImage::from_fn(200, 120, |x, y| image::Rgba([x as u8, y as u8, 100, 255]));
    let bitmap = crate::bitmap::Bitmap::new(std::sync::Arc::new(
        crate::RasterImage::from_image(&pixels).unwrap(),
    ))
    .unwrap()
    .with_focus([0.25, 0.4])
    .unwrap();
    let mut a = std::sync::Arc::new(bitmap).annotation(1);
    a.start = [40., 30.];
    a.end = [240., 150.];
    let before = a.clone();
    let cropped = a
        .crop_image_handle(crate::selection::Handle::Right, [180., 90.])
        .unwrap();
    assert_eq!(cropped.end, [180., 150.]);
    assert!((cropped.image.as_ref().unwrap().view().rect[2] - 0.7).abs() < 0.00001);
    assert!(std::sync::Arc::ptr_eq(
        before.image.as_ref().unwrap().pixels().unwrap(),
        cropped.image.as_ref().unwrap().pixels().unwrap()
    ));
    let revealed = cropped
        .crop_image_handle(crate::selection::Handle::Right, [240., 90.])
        .unwrap();
    assert_eq!(revealed, before);
    let v = ImageView::default()
        .zoomed_about(2., Some([0.25, 0.4]))
        .unwrap();
    assert!(((0.25 - v.rect[0]) / v.rect[2] - 0.25).abs() < 0.00001);
    assert!(((0.4 - v.rect[1]) / v.rect[3] - 0.4).abs() < 0.00001);
    let mut d = crate::Document::blank(300, 200).unwrap();
    d.annotations.push(before.clone());
    d.promote_features();
    let mut s = crate::Session::new(d).unwrap();
    let original = s.document.render().unwrap();
    s.execute(crate::Command::Update {
        annotation: cropped.clone(),
    })
    .unwrap();
    assert_eq!(
        s.document.render().unwrap().get_pixel(179, 90),
        original.get_pixel(179, 90)
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("crop.json");
    s.save(&file).unwrap();
    let loaded = crate::Document::load(&file).unwrap();
    assert_eq!(loaded.annotations, [cropped]);
    assert_eq!(loaded.version, crate::MAX_PROJECT_VERSION);
    s.execute(crate::Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), original);
    let mut bad = s.document.clone();
    bad.version = 18;
    assert!(bad.validate().is_err());
}
use crate::{
    Command, Document, RasterImage, Session, bitmap::Bitmap, clipboard_payload::ObjectPayload,
    display::Viewport,
};
use image::{Rgba, RgbaImage};
use std::sync::Arc;

#[test]
fn cropped_rounded_frame_retains_source_undo_clipboard_and_saved_pixels() {
    let source = RgbaImage::from_fn(80, 60, |x, y| Rgba([x as u8 * 3, y as u8 * 4, 140, 255]));
    let bitmap =
        Arc::new(Bitmap::new(Arc::new(RasterImage::from_image(&source).unwrap())).unwrap());
    let mut a = bitmap.clone().annotation(1);
    a.start = [10., 10.];
    a.end = [90., 70.];
    let mut document = Document::canvas(100, 80, [0; 4]).unwrap();
    document.annotations.push(a.clone());
    document.promote_features();
    let original = document.render().unwrap();
    let mut session = Session::new(document).unwrap();
    let mut edited = a
        .with_image_view(Some(
            ImageView::default()
                .zoomed(2.)
                .unwrap()
                .panned([0.1, -0.1])
                .unwrap(),
        ))
        .unwrap();
    edited.corner_radius = 12.;
    session
        .execute(Command::Update {
            annotation: edited.clone(),
        })
        .unwrap();
    assert_eq!(session.document.version, crate::MAX_PROJECT_VERSION);
    assert!(Arc::ptr_eq(
        bitmap.pixels().unwrap(),
        edited.image.as_ref().unwrap().pixels().unwrap()
    ));
    assert_eq!(**bitmap.pixels().unwrap(), source);
    let framed = session.document.render().unwrap();
    if let Some(dir) = std::env::var_os("CLIPDESIGNER_IMAGE_FRAME_EVIDENCE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        session.save(&dir.join("image-frame.capture.json")).unwrap();
        framed.save(dir.join("image-frame.png")).unwrap();
    }
    assert_ne!(framed, original);
    assert_eq!(framed.get_pixel(10, 10)[3], 0);
    assert!(framed.pixels().any(|p| p[3] > 0 && p[3] < 255));
    assert_eq!(
        session
            .document
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [100, 80],
                origin: [0.; 2],
                scale: 1.
            })
            .unwrap(),
        framed
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("framed.capture.json");
    session.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.render().unwrap(), framed);
    let payload = ObjectPayload::capture(&loaded, &[1]).unwrap();
    assert_eq!(payload.document_version, crate::MAX_PROJECT_VERSION);
    payload.validate().unwrap();
    let mut old = loaded.clone();
    old.version = 9;
    assert!(old.validate().is_err());
    let mut reset = edited.with_image_view(None).unwrap();
    reset.corner_radius = 0.;
    session
        .execute(Command::Update { annotation: reset })
        .unwrap();
    assert_eq!(session.document.render().unwrap(), original);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.render().unwrap(), framed);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.render().unwrap(), original);
    assert!(edited.handles().iter().any(|(h, _)| h.is_radius()));
    assert!(
        edited
            .rounded_by_drag(
                crate::selection::Handle::RadiusTopLeft,
                [12., 12.],
                [15., 15.]
            )
            .unwrap()
            .corner_radius
            > 12.
    );
}

#[test]
fn views_reject_invalid_crops_and_keep_zoom_pan_inside_original() {
    for rect in [
        [f32::NAN, 0., 1., 1.],
        [-0.1, 0., 1., 1.],
        [0., 0., 0., 1.],
        [0.6, 0., 0.5, 1.],
        [0., 0., 1.0000005, 1.],
    ] {
        assert!(ImageView { rect }.validate().is_err());
    }
    let mut view = ImageView::default();
    for _ in 0..100 {
        view = view.zoomed(1.25).unwrap();
        view.validate().unwrap();
    }
    view = view.panned([10., -10.]).unwrap();
    view.validate().unwrap();
    for _ in 0..100 {
        view = view.zoomed(0.8).unwrap();
        view.validate().unwrap();
    }
    assert_eq!(view, ImageView::default());
    assert!(view.zoomed(f32::NAN).is_err());
    assert!(view.panned([f32::INFINITY, 0.]).is_err());
    let cursor =
        Bitmap::cursor(&RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255])), vec![]).unwrap();
    assert!(cursor.with_view(Some(view.zoomed(2.).unwrap())).is_err());
    assert_eq!(Arc::new(cursor).annotation(1).max_corner_radius(), 0.);
}
