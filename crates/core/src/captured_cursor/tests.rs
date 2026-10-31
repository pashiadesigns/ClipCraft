use super::*;
use crate::{Command, Document, Session, clipboard_payload::ObjectPayload, display::Viewport};
fn fixture() -> Cursor {
    // Transparent, opaque red, half-alpha green, and an XOR pixel.
    let black = RgbaImage::from_raw(
        2,
        2,
        vec![
            0, 0, 0, 255, 255, 0, 0, 255, 0, 128, 0, 255, 255, 255, 255, 255,
        ],
    )
    .unwrap();
    let white = RgbaImage::from_raw(
        2,
        2,
        vec![
            255, 255, 255, 255, 255, 0, 0, 255, 127, 255, 127, 255, 0, 0, 0, 255,
        ],
    )
    .unwrap();
    Cursor::from_backgrounds([-99, 21], &black, &white).unwrap()
}
#[test]
fn cursor_retains_clean_base_masks_transforms_delete_undo_and_portable_copy() {
    let c = fixture();
    let mut d = Document::canvas(8, 8, [20, 80, 120, 255]).unwrap();
    let clean = d.render().unwrap();
    d.annotations
        .push(c.annotation([-100, 20], [8, 8], 1).unwrap().unwrap());
    d.promote_features();
    assert_eq!(d.version, 9);
    let render = d.render().unwrap();
    assert_eq!(render.get_pixel(2, 2).0, [235, 175, 135, 255]);
    assert_eq!(render.get_pixel(2, 1).0, [255, 0, 0, 255]);
    assert_eq!(d.base_image().unwrap(), clean);
    let mut s = Session::new(d).unwrap();
    let payload = ObjectPayload::capture(&s.document, &[1]).unwrap();
    assert_eq!(payload.document_version, 9);
    payload.validate().unwrap();
    s.execute(Command::Remove { id: 1 }).unwrap();
    assert_eq!(s.document.render().unwrap(), clean);
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), render);
    let mut a = s.document.annotations[0].clone();
    a.start = [2., 2.];
    a.end = [6., 6.];
    a.rotation_degrees = 35.;
    s.execute(Command::Update { annotation: a }).unwrap();
    let output = s.document.render().unwrap();
    assert_ne!(output, render);
    assert_eq!(
        s.document
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                origin: [0.; 2],
                size: [8, 8],
                scale: 1.
            })
            .unwrap(),
        output
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cursor.capture.json");
    s.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.render().unwrap(), output);
    assert!(reopened.annotations[0].image.as_ref().unwrap().is_cursor());
    let mut invalid = reopened;
    invalid.version = 8;
    assert!(invalid.validate().is_err());
}
#[test]
fn cursor_crop_remaps_inversion_with_negative_monitor_coordinates() {
    let c = fixture();
    let a = c.annotation([-98, 22], [4, 4], 1).unwrap().unwrap();
    assert_eq!(a.start, [0., 0.]);
    assert_eq!(a.end, [1., 1.]);
    assert!(a.image.unwrap().inverted(0));
    assert!(c.annotation([0, 0], [4, 4], 1).unwrap().is_none());
    let mut invalid = fixture().bitmap.as_ref().clone();
    // Mask validation also runs after untrusted document deserialization.
    let mut json = serde_json::to_value(&invalid).unwrap();
    json["cursor_invert"] = serde_json::json!([99999]);
    invalid = serde_json::from_value(json).unwrap();
    assert!(invalid.validate_cursor().is_err());
}
#[test]
fn step_guide_keeps_a_separate_cursor_and_original_frame() {
    let c = fixture();
    let image = RgbaImage::from_pixel(8, 8, Rgba([20, 80, 120, 255]));
    let mut guide = crate::step_capture::Guide::new([8, 8]).unwrap();
    guide
        .push(
            crate::step_capture::Frame {
                context: Default::default(),
                image: image.clone(),
                cursor: c.annotation([-100, 20], [8, 8], 1).unwrap(),
            },
            crate::step_capture::Gesture {
                start: [3, 3],
                end: [3, 3],
                button: crate::step_capture::Button::Left,
            },
        )
        .unwrap();
    let d = guide.document().unwrap();
    assert_eq!(d.annotations.len(), 5);
    assert_eq!(d.version, 19);
    assert_eq!(
        **d.annotations[0].image.as_ref().unwrap().pixels().unwrap(),
        image
    );
    assert!(d.annotations[2].image.as_ref().unwrap().is_cursor());
    let circle = &d.annotations[1];
    assert_eq!(circle.kind, crate::Kind::Ellipse);
    assert!(circle.fill.is_some());
    assert!(circle.group.is_some());
    assert_eq!(circle.group, d.annotations[2].group);
    assert_eq!(
        d.group_members(circle.id),
        vec![circle.id, d.annotations[2].id]
    );
}
