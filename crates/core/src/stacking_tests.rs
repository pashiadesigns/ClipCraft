use crate::{Annotation, Command, Document, Kind, Session, StackOrder};

fn fixture() -> Document {
    let mut document = Document::canvas(40, 30, [18, 52, 171, 128]).unwrap();
    document.annotations = [
        (1, [255, 0, 0, 255]),
        (2, [0, 255, 0, 255]),
        (3, [0, 0, 255, 255]),
    ]
    .into_iter()
    .map(|(id, color)| Annotation {
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
        id,
        kind: Kind::Rectangle,
        start: [5.0, 5.0],
        end: [30.0, 20.0],
        color,
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

fn ids(session: &Session) -> Vec<u64> {
    session.document.annotations.iter().map(|a| a.id).collect()
}

#[test]
fn all_stacking_directions_preserve_geometry_and_one_step_undo() {
    for (id, order, expected) in [
        (1, StackOrder::BringForward, vec![2, 1, 3]),
        (1, StackOrder::BringToFront, vec![2, 3, 1]),
        (3, StackOrder::SendBackward, vec![1, 3, 2]),
        (3, StackOrder::SendToBack, vec![3, 1, 2]),
    ] {
        let original = fixture();
        let mut session = Session::new(original.clone()).unwrap();
        assert!(session.document.can_arrange(id, order));
        session.execute(Command::Arrange { id, order }).unwrap();
        assert_eq!(ids(&session), expected);
        assert!(session.is_modified());
        for annotation in &original.annotations {
            assert_eq!(
                session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == annotation.id),
                Some(annotation)
            );
        }
        assert_eq!(session.document.source_png, original.source_png);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, original.annotations);
        assert!(!session.is_modified());
        session.execute(Command::Redo).unwrap();
        assert_eq!(ids(&session), expected);
    }
}

#[test]
fn stacking_changes_export_and_hit_testing_and_survives_save_reopen() {
    let mut session = Session::new(fixture()).unwrap();
    assert_eq!(
        session.document.render().unwrap().get_pixel(5, 12).0,
        [0, 0, 255, 255]
    );
    assert_eq!(session.document.hit_test([5.0, 12.0], 0.0), Some(3));
    let command: Command =
        serde_json::from_str(r#"{"command":"arrange","id":1,"order":"bring_to_front"}"#).unwrap();
    session.execute(command).unwrap();
    let expected = session.document.render().unwrap();
    assert_eq!(expected.get_pixel(5, 12).0, [255, 0, 0, 255]);
    assert_eq!(session.document.hit_test([5.0, 12.0], 0.0), Some(1));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stack.capture.json");
    session.save(&path).unwrap();
    assert!(!session.is_modified());
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.annotations, session.document.annotations);
    assert_eq!(reopened.render().unwrap(), expected);
    let png = dir.path().join("stack.png");
    reopened.export_png(&png).unwrap();
    assert_eq!(image::open(png).unwrap().into_rgba8(), expected);
    session.execute(Command::Undo).unwrap();
    assert!(session.is_modified());
    session.execute(Command::Redo).unwrap();
    assert!(!session.is_modified());
}

#[test]
fn boundary_and_missing_object_actions_leave_history_and_dirty_state_unchanged() {
    let mut session = Session::new(fixture()).unwrap();
    session
        .execute(Command::Arrange {
            id: 1,
            order: StackOrder::BringToFront,
        })
        .unwrap();
    session.execute(Command::Undo).unwrap();
    assert!(session.can_redo());
    for (id, order) in [
        (1, StackOrder::SendToBack),
        (1, StackOrder::SendBackward),
        (3, StackOrder::BringForward),
        (3, StackOrder::BringToFront),
    ] {
        assert!(!session.document.can_arrange(id, order));
        session.execute(Command::Arrange { id, order }).unwrap();
        assert!(!session.is_modified());
        assert!(!session.can_undo());
        assert!(session.can_redo());
    }
    assert!(!session.document.can_arrange(99, StackOrder::BringForward));
    assert!(
        session
            .execute(Command::Arrange {
                id: 99,
                order: StackOrder::BringForward
            })
            .is_err()
    );
    assert_eq!(ids(&session), [1, 2, 3]);
    assert!(session.can_redo());
    session.execute(Command::Redo).unwrap();
    assert_eq!(ids(&session), [2, 3, 1]);
}
