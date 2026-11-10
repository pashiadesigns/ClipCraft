use super::*;
use crate::{
    Command, Session, canvas_expansion::translated, display::Viewport, guide_sections::Action,
};
use image::{Rgba, RgbaImage};
fn bytes(d: &Document) -> Vec<u8> {
    serde_json::to_vec(d).unwrap()
}
fn crop(s: &mut Session, r: [u32; 4], mode: CropMode, page: usize) {
    s.execute(Command::CropImage {
        x: r[0],
        y: r[1],
        width: r[2],
        height: r[3],
        mode,
        page,
    })
    .unwrap();
}
fn sample() -> Document {
    let mut d = Document::from_capture_image(RgbaImage::from_fn(320, 240, |x, y| {
        Rgba([x as u8, y as u8, 77, 255])
    }))
    .unwrap();
    let mut a = crate::steps::marker(
        2,
        [65., 90.],
        64,
        [25, 150, 230, 255],
        (1, false),
        [320, 240],
    )
    .unwrap();
    a.rotation_degrees = 15.;
    d.annotations.push(a);
    d
}
#[test]
fn editable_crop_modes_keep_native_objects_and_raster_sources_undo_roundtrip() {
    for mode in [CropMode::Delete, CropMode::Preserve] {
        let d = sample();
        let before = bytes(&d);
        let raster = d.annotations[0].image.clone().unwrap();
        let rendered = d.render().unwrap();
        let mut s = Session::new(d.clone()).unwrap();
        crop(&mut s, [60, 35, 170, 140], mode, 0);
        assert_eq!(
            s.document.annotations[1],
            translated(&d.annotations[1..], [-60., -35.]).unwrap()[0]
        );
        assert!(s.document.canvas_behavior.held.contains(&2));
        assert_eq!(
            s.document.render().unwrap(),
            image::imageops::crop_imm(&rendered, 60, 35, 170, 140).to_image()
        );
        if mode == CropMode::Preserve {
            assert!(Arc::ptr_eq(
                s.document.annotations[0].image.as_ref().unwrap(),
                &raster
            ));
        } else {
            assert_eq!(
                s.document.annotations[0].image.as_ref().unwrap().size(),
                [170, 140]
            );
            assert_eq!(s.document.annotations[0].start, [0., 0.]);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crop.iclip");
        s.save(&path).unwrap();
        let restored = Document::load(&path).unwrap();
        assert_eq!(bytes(&restored), bytes(&s.document));
        s.execute(Command::Undo).unwrap();
        assert_eq!(bytes(&s.document), before);
        assert!(!s.can_undo());
        s.execute(Command::Redo).unwrap();
        assert_eq!(bytes(&s.document), bytes(&restored));
    }
}
#[test]
fn preserved_base_becomes_shared_editable_image_and_invalid_crop_is_atomic() {
    let pixels = RgbaImage::from_fn(120, 80, |x, y| Rgba([x as u8, y as u8, 39, 180]));
    for mode in [CropMode::Delete, CropMode::Preserve] {
        let d = Document::from_image(pixels.clone()).unwrap();
        let mut s = Session::new(d.clone()).unwrap();
        crop(&mut s, [10, 20, 70, 40], mode, 0);
        assert_eq!(
            s.document.render().unwrap(),
            image::imageops::crop_imm(&pixels, 10, 20, 70, 40).to_image()
        );
        assert_eq!(
            s.document.annotations.len(),
            usize::from(mode == CropMode::Preserve)
        );
        let saved = bytes(&s.document);
        for r in [[0, 0, 0, 20], [0, 0, 20, 100], [u32::MAX, 0, 20, 20]] {
            assert!(
                s.execute(Command::CropImage {
                    x: r[0],
                    y: r[1],
                    width: r[2],
                    height: r[3],
                    mode,
                    page: 0
                })
                .is_err()
            );
            assert_eq!(bytes(&s.document), saved);
        }
        s.execute(Command::Undo).unwrap();
        assert_eq!(bytes(&s.document), bytes(&d));
    }
}
#[test]
fn pasteboard_rearms_only_after_committed_inside_drop_and_setting_persists() {
    let mut s = Session::new(sample()).unwrap();
    crop(&mut s, [60, 35, 170, 140], CropMode::Preserve, 0);
    let original = s.document.annotations[1].clone();
    let inside = translated(std::slice::from_ref(&original), [65., 0.]).unwrap();
    let preview = s.document.plan_expanding_move(&inside, false).unwrap();
    assert_eq!(preview.dimensions, [170, 140]);
    let outside = translated(std::slice::from_ref(&original), [-100., 0.]).unwrap();
    assert_eq!(
        s.document
            .plan_expanding_move(&outside, false)
            .unwrap()
            .dimensions,
        [170, 140]
    );
    s.execute(Command::MoveAndExpand {
        annotations: inside,
        duplicate: false,
    })
    .unwrap();
    assert!(!s.document.canvas_behavior.held.contains(&2));
    let again = translated(&s.document.annotations[1..2], [-160., 0.]).unwrap();
    assert!(
        s.document
            .plan_expanding_move(&again, false)
            .unwrap()
            .dimensions[0]
            > 170
    );
    s.execute(Command::SetCanvasExpansion { enabled: false })
        .unwrap();
    s.execute(Command::MoveAndExpand {
        annotations: again,
        duplicate: false,
    })
    .unwrap();
    assert_eq!([s.document.width, s.document.height], [170, 140]);
    s.execute(Command::SetCanvasExpansion { enabled: true })
        .unwrap();
    assert!(s.document.canvas_behavior.held.contains(&2));
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("held.iclip");
    s.save(&p).unwrap();
    assert_eq!(
        Document::load(&p).unwrap().canvas_behavior,
        s.document.canvas_behavior
    );
}
#[test]
fn cropped_page_retains_neighbors_native_layers_and_output_clips() {
    let d = crate::templates::build("two-steps", "Crop test", &[], crate::templates::Scale::Fill)
        .unwrap()
        .edit_guide(&Action::AddPage {
            page: 0,
            copy: true,
        })
        .unwrap();
    let original = d.render().unwrap();
    let g = d.guide.as_ref().unwrap();
    let size = g.page_size(0, [d.width, d.height]);
    let other_rect = d.page_rect(1).map(|n| n as u32);
    let other = image::imageops::crop_imm(
        &original,
        other_rect[0],
        other_rect[1],
        other_rect[2],
        other_rect[3],
    )
    .to_image();
    let mut s = Session::new(d.clone()).unwrap();
    let r = [90, 60, size[0] - 180, size[1] - 180];
    crop(&mut s, r, CropMode::Preserve, 0);
    let output = s.document.render().unwrap();
    let after_rect = s.document.page_rect(1).map(|n| n as u32);
    assert_eq!(
        other,
        image::imageops::crop_imm(
            &output,
            after_rect[0],
            after_rect[1],
            after_rect[2],
            after_rect[3]
        )
        .to_image()
    );
    assert_eq!(
        image::imageops::crop_imm(&original, r[0], r[1], r[2], r[3]).to_image(),
        image::imageops::crop_imm(&output, 0, 0, r[2], r[3]).to_image()
    );
    assert_eq!(d.annotations.len(), s.document.annotations.len());
    assert_eq!(
        output,
        s.document
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [s.document.width, s.document.height],
                origin: [0.; 2],
                scale: 1.
            })
            .unwrap()
    );
    let path = std::env::var("CLIPDESIGNER_CROP_FIXTURE").ok();
    if let Some(path) = path {
        s.document.save(std::path::Path::new(&path)).unwrap();
    }
    let json = bytes(&s.document);
    let reopened: Document = serde_json::from_slice(&json).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened.render().unwrap(), output);
    for action in [
        Action::MovePage { page: 0, index: 1 },
        Action::AddPage {
            page: 0,
            copy: true,
        },
        Action::DeletePage { page: 0 },
    ] {
        let changed = s.document.edit_guide(&action).unwrap();
        changed.validate().unwrap();
        assert!(
            changed
                .canvas_behavior
                .page_owners
                .values()
                .all(|p| *p < changed.guide.as_ref().unwrap().page_count())
        );
        changed.render().unwrap();
    }
    let invalid = Command::CropImage {
        x: 0,
        y: 0,
        width: 30,
        height: 20,
        mode: CropMode::Delete,
        page: 1,
    };
    assert!(s.execute(invalid).is_err());
    assert_eq!(bytes(&s.document), json);
    s.execute(Command::Undo).unwrap();
    assert_eq!(bytes(&s.document), bytes(&d));
}
#[test]
fn rotated_mirrored_and_shape_frame_raster_trim_does_not_delete_geometry() {
    let mut d = sample();
    let a = &mut d.annotations[0];
    a.kind = crate::Kind::Ellipse;
    a.color = [0, 0, 0, 255];
    a.rotation_degrees = 28.;
    a.mirror_x = true;
    a.start = [35., 35.];
    a.end = [280., 210.];
    let mut s = Session::new(d.clone()).unwrap();
    crop(&mut s, [60, 35, 170, 140], CropMode::Delete, 0);
    let a = &s.document.annotations[0];
    assert_eq!(a.rotation_degrees, 28.);
    assert!(a.mirror_x);
    assert_eq!(a.kind, crate::Kind::Ellipse);
    assert_eq!(a.start, [-25., 0.]);
    assert_eq!(a.end, [220., 175.]);
    let pixels = a.image.as_ref().unwrap().pixels().unwrap();
    assert!(pixels.pixels().any(|p| p.0 == [0; 4]));
    assert!(pixels.pixels().any(|p| p[3] == 255));
    s.document.validate().unwrap();
}
