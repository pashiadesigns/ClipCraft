use crate::{
    Annotation, Command, Document, Kind, Session,
    text::{HorizontalAlignment, TextStyle, VerticalAlignment},
};

fn document() -> Document {
    let mut doc = Document::canvas(240, 160, [0; 4]).unwrap();
    doc.annotations.push(Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        analysis: None,
        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        id: 1,
        kind: Kind::Text,
        start: [10.0, 10.0],
        end: [210.0, 130.0],
        color: [20, 80, 160, 255],
        width: 1,
        text: Some(TextStyle {
            content: "Inset".into(),
            horizontal: HorizontalAlignment::Left,
            vertical: VerticalAlignment::Top,
            ..TextStyle::default()
        }),
        corner_radius: 0.0,
        fill: None,
        points: Vec::new(),
        callout: None,
    });
    doc
}

#[test]
fn padding_moves_short_text_exactly_and_clips_all_alignment_modes() {
    let mut doc = document();
    let before = doc.render().unwrap();
    doc.annotations[0].text.as_mut().unwrap().padding = 12.0;
    let padded = doc.render().unwrap();
    assert!(padded.pixels().any(|p| p.0[3] != 0));
    for y in 0..148 {
        for x in 0..228 {
            assert_eq!(before.get_pixel(x, y), padded.get_pixel(x + 12, y + 12));
        }
    }
    for horizontal in [
        HorizontalAlignment::Left,
        HorizontalAlignment::Center,
        HorizontalAlignment::Right,
    ] {
        for vertical in [
            VerticalAlignment::Top,
            VerticalAlignment::Middle,
            VerticalAlignment::Bottom,
        ] {
            let text = doc.annotations[0].text.as_mut().unwrap();
            text.horizontal = horizontal;
            text.vertical = vertical;
            text.content = "A long line that wraps\nA second line, also long".into();
            let rendered = doc.render().unwrap();
            assert!(rendered.pixels().any(|p| p.0[3] != 0));
            for (x, y, pixel) in rendered.enumerate_pixels() {
                if !(22..198).contains(&x) || !(22..118).contains(&y) {
                    assert_eq!(pixel.0, [0; 4]);
                }
            }
        }
    }
    // Dragging a box across its origin must not invert its padding.
    let normal = doc.render().unwrap();
    let annotation = &mut doc.annotations[0];
    std::mem::swap(&mut annotation.start, &mut annotation.end);
    assert_eq!(doc.render().unwrap(), normal);
    doc.annotations[0].text.as_mut().unwrap().padding = 128.0;
    assert!(doc.render().unwrap().pixels().all(|p| p.0 == [0; 4]));
}

#[test]
fn padding_and_opacity_edit_undo_redo_save_and_export_exactly() {
    let mut session = Session::new(document()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("padded.capture.json");
    session.save(&path).unwrap();
    let before = session.document.render().unwrap();
    let mut edited = session.document.annotations[0].clone();
    edited.text.as_mut().unwrap().padding = 16.5;
    edited.text.as_mut().unwrap().horizontal = HorizontalAlignment::Right;
    edited.text.as_mut().unwrap().vertical = VerticalAlignment::Bottom;
    edited.color[3] = 128;
    session
        .execute(Command::Update {
            annotation: edited.clone(),
        })
        .unwrap();
    let after = session.document.render().unwrap();
    assert_ne!(before, after);
    assert_eq!(after.pixels().map(|p| p.0[3]).max(), Some(128));
    assert!(session.is_modified());
    session.execute(Command::Undo).unwrap();
    assert!(!session.is_modified());
    assert_eq!(session.document.render().unwrap(), before);
    session.execute(Command::Redo).unwrap();
    assert_eq!(session.document.annotations, vec![edited]);
    session.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.render().unwrap(), after);
    let png = dir.path().join("padded.png");
    loaded.export_png(&png).unwrap();
    assert_eq!(Document::open_image(&png).unwrap().render().unwrap(), after);
    let serialized = serde_json::to_value(&loaded.annotations[0]).unwrap();
    assert_eq!(serialized["text"]["padding"], 16.5);
}

#[test]
fn invalid_padding_cannot_change_document_or_destroy_redo() {
    let mut session = Session::new(document()).unwrap();
    let mut edited = session.document.annotations[0].clone();
    edited.text.as_mut().unwrap().padding = 12.0;
    session
        .execute(Command::Update {
            annotation: edited.clone(),
        })
        .unwrap();
    session.execute(Command::Undo).unwrap();
    let before = session.document.render().unwrap();
    for padding in [-0.1, 129.0, f32::INFINITY, f32::NAN] {
        edited.text.as_mut().unwrap().padding = padding;
        assert!(
            session
                .execute(Command::Update {
                    annotation: edited.clone()
                })
                .is_err()
        );
        assert_eq!(session.document.render().unwrap(), before);
        assert!(session.can_redo());
        assert!(!session.can_undo());
        assert!(!session.is_modified());
    }
}
