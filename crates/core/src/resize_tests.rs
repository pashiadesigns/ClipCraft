use crate::{Annotation, Command, Document, Kind, Session, callout::CalloutStyle, text::TextStyle};

fn fixture() -> Document {
    let source =
        image::RgbaImage::from_fn(160, 100, |x, y| image::Rgba([x as u8, y as u8, 180, 255]));
    let mut doc = Document::from_image(source).unwrap();
    doc.annotations = vec![
        Annotation {
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
            start: [10.0, 10.0],
            end: [40.0, 40.0],
            color: [230, 20, 40, 255],
            width: 2,
            text: None,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: None,
        },
        Annotation {
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
            id: 2,
            kind: Kind::Callout,
            start: [40.0, 20.0],
            end: [100.0, 70.0],
            color: [20, 30, 40, 255],
            width: 2,
            text: Some(TextStyle {
                content: "Resize".into(),
                size_points: 12.0,
                ..Default::default()
            }),
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: Some(CalloutStyle {
                shape: Default::default(),
                tail: Default::default(),
                fill: [220, 240, 250, 180],
                outline: [10, 40, 80, 255],
                pointer: [110.0, 80.0],
                padding: 8.0,
            }),
        },
    ];
    doc
}

#[test]
fn resize_scales_editable_geometry_and_restores_dimensions_history_and_saved_state() {
    let original = fixture();
    let pixels = original.render().unwrap();
    let mut session = Session::new(original.clone()).unwrap();
    let command: Command =
        serde_json::from_str(r#"{"command":"resize_image","width":80,"height":50}"#).unwrap();
    session.execute(command).unwrap();
    assert_eq!((session.document.width, session.document.height), (80, 50));
    assert_eq!(session.document.source_size, Some([160, 100]));
    assert_eq!(session.document.source_png, original.source_png);
    assert_eq!(session.document.annotations[0].start, [5.0, 5.0]);
    assert_eq!(session.document.annotations[0].width, 1);
    assert_eq!(
        session.document.annotations[1]
            .text
            .as_ref()
            .unwrap()
            .size_points,
        6.0
    );
    assert_eq!(
        session.document.annotations[1]
            .callout
            .as_ref()
            .unwrap()
            .pointer,
        [55.0, 40.0]
    );
    assert_eq!(
        session.document.annotations[1]
            .callout
            .as_ref()
            .unwrap()
            .padding,
        4.0
    );
    assert!(session.is_modified());
    let resized = session.document.render().unwrap();
    assert_eq!(resized.dimensions(), (80, 50));
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.render().unwrap(), pixels);
    assert_eq!(session.document.annotations, original.annotations);
    assert!(!session.is_modified());
    session.execute(Command::Redo).unwrap();
    assert_eq!(session.document.render().unwrap(), resized);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resize.capture.json");
    session.save(&path).unwrap();
    assert!(!session.is_modified());
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.render().unwrap(), resized);
    assert_eq!(reopened.source_png, original.source_png);
    let png = dir.path().join("resize.png");
    reopened.export_png(&png).unwrap();
    assert_eq!(image::open(png).unwrap().into_rgba8(), resized);
    session.execute(Command::Undo).unwrap();
    assert!(session.is_modified());
    session.execute(Command::Redo).unwrap();
    assert!(!session.is_modified());
}

#[test]
fn revert_after_resize_restores_original_pixels_and_dimensions_without_losing_undo() {
    for original in [
        fixture(),
        Document::blank(160, 100).unwrap(),
        Document::canvas(160, 100, [20, 90, 180, 128]).unwrap(),
    ] {
        let original_pixels = original.base_image().unwrap();
        let mut session = Session::new(original.clone()).unwrap();
        session
            .execute(Command::ResizeImage {
                width: 240,
                height: 150,
            })
            .unwrap();
        let resized = session.document.render().unwrap();
        assert!(session.document.can_revert());
        session.execute(Command::RevertOriginal).unwrap();
        assert_eq!(
            (session.document.width, session.document.height),
            (160, 100)
        );
        assert!(session.document.source_size.is_none());
        assert!(session.document.annotations.is_empty());
        assert_eq!(session.document.render().unwrap(), original_pixels);
        assert!(!session.document.can_revert());
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), resized);
        assert_eq!(session.document.source_size, Some([160, 100]));
    }
}

#[test]
fn invalid_resize_and_noop_preserve_document_pixels_redo_and_saved_state() {
    let original = fixture();
    let mut session = Session::new(original.clone()).unwrap();
    session
        .execute(Command::ResizeImage {
            width: 80,
            height: 50,
        })
        .unwrap();
    session.execute(Command::Undo).unwrap();
    for (width, height) in [(0, 40), (5001, 5000), (32769, 1), (10, 6)] {
        assert!(
            session
                .execute(Command::ResizeImage { width, height })
                .is_err()
        );
        assert_eq!(
            (session.document.width, session.document.height),
            (160, 100)
        );
        assert_eq!(session.document.annotations, original.annotations);
        assert_eq!(session.document.source_png, original.source_png);
        assert!(session.document.source_size.is_none());
        assert!(!session.is_modified());
        assert!(session.can_redo());
        assert!(!session.can_undo());
    }
    session
        .execute(Command::ResizeImage {
            width: 160,
            height: 100,
        })
        .unwrap();
    assert!(session.can_redo());
    assert!(!session.can_undo());
    session.execute(Command::Redo).unwrap();
    assert_eq!((session.document.width, session.document.height), (80, 50));
}

#[test]
fn alpha_resampling_ignores_hidden_colors_and_repeated_resize_retains_original_detail() {
    let source = image::RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 255, 0, 0])
        }
    });
    let mut session = Session::new(Document::from_image(source.clone()).unwrap()).unwrap();
    let legacy = serde_json::to_value(&session.document).unwrap();
    assert!(legacy.get("source_size").is_none());
    session
        .execute(Command::ResizeImage {
            width: 1,
            height: 1,
        })
        .unwrap();
    assert_eq!(
        session.document.base_image().unwrap().get_pixel(0, 0).0,
        [255, 0, 0, 128]
    );
    session
        .execute(Command::ResizeImage {
            width: 2,
            height: 1,
        })
        .unwrap();
    assert_eq!(session.document.base_image().unwrap(), source);
    assert!(!session.is_modified());
    assert!(session.document.source_size.is_none());
    let mut invalid = session.document.clone();
    invalid.source_size = Some([3, 1]);
    assert!(invalid.base_image().is_err());
}
