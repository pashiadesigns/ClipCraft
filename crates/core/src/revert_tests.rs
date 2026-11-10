use crate::{Annotation, Command, Document, Kind, Session};

#[test]
fn captured_screenshot_is_editable_deletable_and_roundtrips_without_losing_original() {
    use std::sync::Arc;
    let pixels = image::RgbaImage::from_fn(96, 64, |x, y| {
        image::Rgba([x as u8 * 2, y as u8 * 3, 120, 255])
    });
    let document = Document::from_capture_image(pixels.clone()).unwrap();
    assert_eq!(document.render().unwrap(), pixels);
    assert_eq!(document.annotations.len(), 1);
    assert_eq!(document.annotations[0].kind, Kind::Image);
    assert!(document.base_image().unwrap().pixels().all(|p| p[3] == 0));
    let original = serde_json::to_vec(&document).unwrap();
    let mut session = Session::new(document).unwrap();
    let mut image = session.document.annotations[0].clone();
    image.start = [8., 12.];
    image.end = [88., 60.];
    image.corner_radius = 8.;
    image.image = Some(Arc::new(
        image
            .image
            .as_ref()
            .unwrap()
            .with_view(Some(crate::image_frame::ImageView {
                rect: [0.25, 0.25, 0.5, 0.5],
            }))
            .unwrap(),
    ));
    session
        .execute(Command::Update {
            annotation: image.clone(),
        })
        .unwrap();
    assert_ne!(session.document.render().unwrap(), pixels);
    assert_eq!(**image.image.as_ref().unwrap().pixels().unwrap(), pixels);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("editable-capture.json");
    session.document.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.annotations, session.document.annotations);
    assert_eq!(
        reopened.render().unwrap(),
        session.document.render().unwrap()
    );
    session.execute(Command::Remove { id: 1 }).unwrap();
    assert!(
        session
            .document
            .render()
            .unwrap()
            .pixels()
            .all(|p| p[3] == 0)
    );
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.annotations[0], image);
    session.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
    session.execute(Command::RevertOriginal).unwrap();
    assert_eq!(session.document.render().unwrap(), pixels);
    session.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
}

fn fixture() -> Document {
    let mut pixels = image::RgbaImage::new(40, 30);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        *pixel = image::Rgba([x as u8 * 6, y as u8 * 8, 79, ((x + y) % 4 * 85) as u8]);
    }
    let mut document = Document::from_image(pixels).unwrap();
    document.annotations = [Kind::Rectangle, Kind::Redact]
        .into_iter()
        .enumerate()
        .map(|(i, kind)| Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,

            focus: None,

            highlighter_rect: false,
            hard_edge: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
            id: i as u64 + 1,
            kind,
            start: [5.0, 5.0],
            end: [30.0, 20.0],
            color: [230, 15, 40, 255],
            width: 3,
            text: None,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: None,
        })
        .collect();
    document
}

#[test]
fn revert_recovers_original_rgba_and_undo_restores_all_objects_and_saved_state() {
    let edited = fixture();
    let mut original = edited.clone();
    original.annotations.clear();
    let mut session = Session::new(edited.clone()).unwrap();
    let rendered = session.document.render().unwrap();
    assert_ne!(rendered, original.render().unwrap());
    let command: Command = serde_json::from_str(r#"{"command":"revert_original"}"#).unwrap();
    session.execute(command).unwrap();
    assert!(session.document.annotations.is_empty());
    assert_eq!(session.document.source_png, edited.source_png);
    assert_eq!(
        session.document.render().unwrap(),
        original.render().unwrap()
    );
    assert!(session.is_modified());
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.annotations, edited.annotations);
    assert_eq!(session.document.render().unwrap(), rendered);
    assert!(!session.is_modified());
    session.execute(Command::Redo).unwrap();
    assert_eq!(
        session.document.render().unwrap(),
        original.render().unwrap()
    );
    assert!(session.is_modified());
}

#[test]
fn empty_revert_preserves_redo_and_import_unsaved_status() {
    let original = fixture();
    let mut session = Session::new(Document::blank(40, 30).unwrap()).unwrap();
    session
        .execute(Command::Add {
            annotation: original.annotations[0].clone(),
        })
        .unwrap();
    session.execute(Command::Undo).unwrap();
    session.execute(Command::RevertOriginal).unwrap();
    assert!(session.can_redo());
    assert!(!session.can_undo());
    assert!(!session.is_modified());
    session.execute(Command::Redo).unwrap();
    assert_eq!(session.document.annotations.len(), 1);
    let mut unsaved = Session::unsaved(Document::canvas(32, 16, [12, 40, 80, 0]).unwrap()).unwrap();
    unsaved.execute(Command::RevertOriginal).unwrap();
    assert!(unsaved.is_modified());
    assert!(!unsaved.can_undo());
    unsaved
        .execute(Command::Add {
            annotation: Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.0,
                mirror_x: false,
                id: 1,
                kind: Kind::Arrow,
                start: [1.0, 1.0],
                end: [25.0, 10.0],
                color: [255, 0, 0, 255],
                width: 2,
                text: None,
                corner_radius: 0.0,
                fill: None,
                points: Vec::new(),
                callout: None,
            },
        })
        .unwrap();
    unsaved.execute(Command::RevertOriginal).unwrap();
    assert!(unsaved.is_modified());
    assert_eq!(
        unsaved.document.render().unwrap().get_pixel(10, 10).0,
        [12, 40, 80, 0]
    );
}

#[test]
fn reopened_projects_keep_the_source_baseline_and_revert_persists_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.capture.json");
    let edited = fixture();
    edited.save(&path).unwrap();
    let mut session = Session::new(Document::load(&path).unwrap()).unwrap();
    session.execute(Command::RevertOriginal).unwrap();
    let original_pixels = session.document.render().unwrap();
    session.save(&path).unwrap();
    assert!(!session.is_modified());
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.source_png, edited.source_png);
    assert!(reopened.annotations.is_empty());
    assert_eq!(reopened.render().unwrap(), original_pixels);
    let png = dir.path().join("original.png");
    reopened.export_png(&png).unwrap();
    assert_eq!(image::open(png).unwrap().into_rgba8(), original_pixels);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.annotations, edited.annotations);
    assert!(session.is_modified());
    session.save(&path).unwrap();
    assert_eq!(
        Document::load(&path).unwrap().annotations,
        edited.annotations
    );
}
