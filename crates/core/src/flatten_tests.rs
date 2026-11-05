use crate::{Command, Document, Placement, Session};

fn fixture() -> Document {
    serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json")).unwrap()
}

#[test]
fn flatten_all_preserves_every_rendered_pixel_and_undo_restores_editable_objects() {
    let original = fixture();
    let pixels = original.render().unwrap();
    let mut session = Session::new(original.clone()).unwrap();
    let command: Command = serde_json::from_str(r#"{"command":"flatten_all"}"#).unwrap();
    session.execute(command).unwrap();
    assert!(session.document.annotations.is_empty());
    assert_eq!(session.document.version, crate::MAX_PROJECT_VERSION);
    assert_eq!(session.document.render().unwrap(), pixels);
    assert!(session.is_modified());
    assert_eq!(session.document.source_png, original.source_png);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.annotations, original.annotations);
    assert_eq!(session.document.version, crate::MAX_PROJECT_VERSION);
    assert_eq!(session.document.render().unwrap(), pixels);
    assert!(!session.is_modified());
    session.execute(Command::Redo).unwrap();
    assert!(session.document.annotations.is_empty());
    assert_eq!(session.document.render().unwrap(), pixels);
}

#[test]
fn flatten_reopened_resized_canvas_preserves_original_baseline_and_saved_state() {
    let original = fixture();
    let baseline = original.base_image().unwrap();
    let mut session = Session::new(original.clone()).unwrap();
    session
        .execute(Command::ResizeCanvas {
            width: 740,
            height: 480,
            placement: Placement::BottomRight,
        })
        .unwrap();
    let expanded = session.document.render().unwrap();
    session.execute(Command::FlattenAll).unwrap();
    assert_eq!(session.document.render().unwrap(), expanded);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("flattened.capture.json");
    session.save(&path).unwrap();
    assert!(!session.is_modified());
    let mut reopened = Session::new(Document::load(&path).unwrap()).unwrap();
    assert_eq!(reopened.document.render().unwrap(), expanded);
    assert!(reopened.document.annotations.is_empty());
    assert!(reopened.document.can_revert());
    reopened.execute(Command::RevertOriginal).unwrap();
    assert_eq!(reopened.document.render().unwrap(), baseline);
    assert_eq!(
        (reopened.document.width, reopened.document.height),
        (640, 400)
    );
    reopened.execute(Command::Undo).unwrap();
    assert!(!reopened.is_modified());
    assert_eq!(reopened.document.render().unwrap(), expanded);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.annotations.len(), 7);
    assert_eq!(session.document.render().unwrap(), expanded);
}

#[test]
fn flatten_noop_and_corrupt_source_cannot_destroy_redo_or_modify_document() {
    let mut session = Session::new(fixture()).unwrap();
    session.execute(Command::FlattenAll).unwrap();
    session
        .execute(Command::ResizeImage {
            width: 320,
            height: 200,
        })
        .unwrap();
    session.execute(Command::Undo).unwrap();
    assert!(session.can_redo());
    let before = serde_json::to_vec(&session.document).unwrap();
    let undo = session.undo.len();
    session.execute(Command::FlattenAll).unwrap();
    assert!(session.can_redo());
    assert_eq!(session.undo.len(), undo);
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
    let mut broken = fixture();
    broken.source_png = "not png".into();
    let mut invalid = Session::new(broken).unwrap();
    let before = serde_json::to_vec(&invalid.document).unwrap();
    assert!(invalid.execute(Command::FlattenAll).is_err());
    assert_eq!(serde_json::to_vec(&invalid.document).unwrap(), before);
    assert!(!invalid.can_undo());
    assert!(!invalid.is_modified());
}

#[test]
fn flattened_resize_is_one_history_step_with_exact_composite_resampling() {
    let original = fixture();
    let pixels = original.render().unwrap();
    let mut session = Session::new(original.clone()).unwrap();
    let command: Command =
        serde_json::from_str(r#"{"command":"resize_image_flattened","width":320,"height":200}"#)
            .unwrap();
    session.execute(command).unwrap();
    assert_eq!(session.undo.len(), 1);
    assert!(session.document.annotations.is_empty());
    assert_eq!(
        session.document.render().unwrap(),
        crate::resize::resample(pixels.clone(), 320, 200)
    );
    assert_eq!(session.document.source_png, original.source_png);
    assert_eq!(session.document.source_size, Some([640, 400]));
    let resized = session.document.render().unwrap();
    session.execute(Command::Undo).unwrap();
    assert!(!session.can_undo());
    assert!(!session.is_modified());
    assert_eq!(
        serde_json::to_vec(&session.document).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
    session.execute(Command::Redo).unwrap();
    assert_eq!(session.document.render().unwrap(), resized);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resized.capture.json");
    session.save(&path).unwrap();
    let mut reopened = Session::new(Document::load(&path).unwrap()).unwrap();
    assert_eq!(reopened.document.render().unwrap(), resized);
    reopened.execute(Command::RevertOriginal).unwrap();
    assert_eq!(
        reopened.document.render().unwrap(),
        original.base_image().unwrap()
    );
    reopened.execute(Command::Undo).unwrap();
    assert!(!reopened.is_modified());
    assert_eq!(reopened.document.render().unwrap(), resized);
}

#[test]
fn flattened_resize_small_sizes_and_canvas_baseline_do_not_apply_object_style_limits() {
    let mut session = Session::new(fixture()).unwrap();
    session
        .execute(Command::ResizeCanvas {
            width: 740,
            height: 480,
            placement: Placement::BottomRight,
        })
        .unwrap();
    let before = session.document.clone();
    let pixels = before.render().unwrap();
    assert!(before.resized_annotations(1, 1).is_err());
    session
        .execute(Command::ResizeImageFlattened {
            width: 1,
            height: 1,
        })
        .unwrap();
    assert_eq!(
        session.document.render().unwrap(),
        crate::resize::resample(pixels.clone(), 1, 1)
    );
    assert_eq!(session.document.source_size, Some([640, 400]));
    session.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_vec(&session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    assert_eq!(session.document.render().unwrap(), pixels);
    let mut flat = Session::new(Document::blank(40, 20).unwrap()).unwrap();
    flat.execute(Command::ResizeImageFlattened {
        width: 10,
        height: 5,
    })
    .unwrap();
    assert_eq!(flat.document.version, crate::MAX_PROJECT_VERSION);
    assert!(flat.document.raster.is_none());
    flat.execute(Command::ResizeImageFlattened {
        width: 40,
        height: 20,
    })
    .unwrap();
    assert!(!flat.document.can_revert());
    assert!(!flat.is_modified());
}

#[test]
fn flattened_resize_failures_and_same_size_preserve_document_and_redo() {
    let mut session = Session::new(fixture()).unwrap();
    session.execute(Command::FlattenAll).unwrap();
    session.execute(Command::Undo).unwrap();
    let before = serde_json::to_vec(&session.document).unwrap();
    for (width, height) in [(0, 40), (40, 0), (32769, 1), (32768, 32768)] {
        assert!(
            session
                .execute(Command::ResizeImageFlattened { width, height })
                .is_err()
        );
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
        assert!(session.can_redo());
        assert!(!session.can_undo());
        assert!(!session.is_modified());
    }
    session
        .execute(Command::ResizeImageFlattened {
            width: 640,
            height: 400,
        })
        .unwrap();
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
    assert!(session.can_redo());
    session.document.source_png = "invalid png".into();
    let broken = serde_json::to_vec(&session.document).unwrap();
    assert!(
        session
            .execute(Command::ResizeImageFlattened {
                width: 320,
                height: 200
            })
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), broken);
    assert!(session.can_redo());
    assert!(!session.can_undo());
}
