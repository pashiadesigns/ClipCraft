use crate::{Annotation, Command, Document, Kind, Placement, Session, selection::Handle};

fn stroke(kind: Kind) -> Annotation {
    Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        analysis: None,
        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        id: 1,
        kind,
        start: [20.0, 20.0],
        end: [80.0, 20.0],
        color: [250, 220, 20, 80],
        width: 12,
        text: None,
        callout: None,
        corner_radius: 0.0,
        fill: None,
        points: if kind == Kind::Line {
            vec![]
        } else {
            vec![[20.0, 20.0], [50.0, 70.0], [80.0, 20.0], [50.0, 70.0]]
        },
    }
}
#[test]
fn rectangle_highlighter_is_editable_translucent_and_survives_all_transfers() {
    let original = stroke(Kind::Highlighter);
    let style = crate::object_styles::groups(Kind::Highlighter)
        .remove(1)
        .styles
        .remove(3);
    let mut a = style.apply(&original).unwrap();
    assert!(a.highlighter_rect && a.points.is_empty());
    assert_eq!(a.body_bounds(), original.body_bounds());
    a.start = [20.25, 20.25];
    a.end = [180.75, 75.75];
    assert!(a.hit_test([80., 40.], 0.));
    assert!(!a.hit_test([19., 40.], 0.));
    let mut s = Session::new(Document::canvas(360, 180, [0; 4]).unwrap()).unwrap();
    s.execute(Command::Add {
        annotation: a.clone(),
    })
    .unwrap();
    assert_eq!(s.document.version, 17);
    let output = s.document.render().unwrap();
    assert_eq!(output.get_pixel(60, 40).0, a.color);
    assert!(output.pixels().any(|p| p[3] > 0 && p[3] < a.color[3]));
    assert!(output.pixels().all(|p| p[3] <= a.color[3]));
    let resized = a
        .resized(Handle::BottomRight, [210., 90.], [360, 180])
        .unwrap();
    let moved = resized.translated([10., 10.], [360, 180]).unwrap();
    let rotated = moved.with_rotation(25.).unwrap();
    s.execute(Command::Update {
        annotation: rotated,
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations[0], a);
    s.execute(Command::Redo).unwrap();
    let payload = crate::clipboard_payload::ObjectPayload::capture(&s.document, &[1]).unwrap();
    s.execute(Command::PasteObjects {
        payload,
        center: [270., 120.],
    })
    .unwrap();
    assert_eq!(s.document.annotations.len(), 2);
    assert!(
        s.document
            .annotations
            .iter()
            .all(|a| a.highlighter_rect && a.points.is_empty())
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations.len(), 1);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("highlight.capture.json");
    s.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.annotations, s.document.annotations);
    assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
    let rendered = s.document.render().unwrap();
    s.execute(Command::FlattenAll).unwrap();
    assert_eq!(s.document.render().unwrap(), rendered);
    s.execute(Command::Undo).unwrap();
    // Legacy pen appearance and geometry survive formatting between variants.
    let painted = crate::object_styles::paint_format(&a, &original).unwrap();
    assert!(!painted.highlighter_rect);
    assert_eq!(painted.points, original.points);
    if let Ok(root) = std::env::var("CLIPCRAFT_HIGHLIGHTER_DIR") {
        let root = std::path::Path::new(&root);
        std::fs::create_dir_all(root).unwrap();
        let mut d = Document::canvas(360, 180, [255; 4]).unwrap();
        d.annotations.push(a);
        let mut pen = original;
        pen.id = 2;
        pen.start = [30., 110.];
        pen.end = [320., 140.];
        pen.points = vec![pen.start, [90., 150.], [160., 110.], pen.end];
        pen.width = 18;
        d.annotations.push(pen);
        d.promote_features();
        d.save(&root.join("highlighter.capture.json")).unwrap();
        d.render()
            .unwrap()
            .save(root.join("highlighter.png"))
            .unwrap();
    }
}
#[test]
fn highlighter_variants_validate_and_remember_independent_defaults() {
    use crate::quick_preferences::{Choice, Preferences};
    let mut p = Preferences::default();
    for mut g in crate::object_styles::groups(Kind::Highlighter) {
        let mut style = g.styles.remove(0);
        style.width = 23;
        p.choose(Choice::DrawingStyle(style)).unwrap();
    }
    let p: Preferences = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(p.drawing_styles.len(), 2);
    assert!(!p.drawing_styles[0].highlighter_rect);
    assert!(p.drawing_styles[1].highlighter_rect);
    let mut s = Session::new(Document::blank(100, 100).unwrap()).unwrap();
    s.execute(Command::Add {
        annotation: stroke(Kind::Highlighter),
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    let mut invalid = stroke(Kind::Pen);
    invalid.highlighter_rect = true;
    assert!(
        s.execute(Command::Add {
            annotation: invalid
        })
        .is_err()
    );
    assert!(s.can_redo());
    let mut invalid = stroke(Kind::Highlighter);
    invalid.highlighter_rect = true;
    assert!(
        s.execute(Command::Add {
            annotation: invalid
        })
        .is_err()
    );
    assert!(s.can_redo());
}
#[test]
fn freehand_join_opacity_and_actual_path_hit_testing() {
    let mut doc = Document::canvas(100, 100, [0, 0, 0, 0]).unwrap();
    doc.annotations.push(stroke(Kind::Highlighter));
    let image = doc.render().unwrap();
    assert_eq!(image.get_pixel(50, 70).0, [250, 220, 20, 80]);
    assert_eq!(image.get_pixel(50, 20).0, [0, 0, 0, 0]);
    assert!(image.pixels().all(|p| p[3] <= 80));
    assert_eq!(doc.hit_test([50.0, 70.0], 0.0), Some(1));
    assert_eq!(doc.hit_test([50.0, 20.0], 0.0), None);
    doc.annotations[0] = stroke(Kind::Line);
    let image = doc.render().unwrap();
    assert_eq!(image.get_pixel(50, 20).0, [250, 220, 20, 80]);
    assert_eq!(image.get_pixel(70, 30).0, [0, 0, 0, 0]); // no arrowhead
}
#[test]
fn stroke_tools_transform_undo_and_roundtrip_without_losing_paths() {
    for kind in [Kind::Line, Kind::Pen, Kind::Highlighter] {
        let mut s = Session::new(Document::blank(100, 100).unwrap()).unwrap();
        let original = stroke(kind);
        s.execute(Command::Add {
            annotation: original.clone(),
        })
        .unwrap();
        let moved = original.translated([10.0, 5.0], [100, 100]).unwrap();
        s.execute(Command::Update {
            annotation: moved.clone(),
        })
        .unwrap();
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.annotations[0], original);
        s.execute(Command::Redo).unwrap();
        assert_eq!(s.document.annotations[0], moved);
        let resized = moved
            .resized(
                if kind == Kind::Line {
                    Handle::End
                } else {
                    Handle::BottomRight
                },
                [95.0, 95.0],
                [100, 100],
            )
            .unwrap();
        s.execute(Command::Update {
            annotation: resized,
        })
        .unwrap();
        let before = s.document.clone();
        s.execute(Command::ResizeCanvas {
            width: 120,
            height: 120,
            placement: Placement::Center,
        })
        .unwrap();
        for (p, q) in before.annotations[0]
            .points
            .iter()
            .zip(&s.document.annotations[0].points)
        {
            assert_eq!(*q, [p[0] + 10.0, p[1] + 10.0]);
        }
        s.execute(Command::Undo).unwrap();
        s.execute(Command::ResizeImage {
            width: 50,
            height: 50,
        })
        .unwrap();
        for (p, q) in before.annotations[0]
            .points
            .iter()
            .zip(&s.document.annotations[0].points)
        {
            assert_eq!(*q, [p[0] * 0.5, p[1] * 0.5]);
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stroke.json");
        s.save(&path).unwrap();
        let loaded = Document::load(&path).unwrap();
        assert_eq!(loaded.annotations, s.document.annotations);
        assert_eq!(loaded.render().unwrap(), s.document.render().unwrap());
        s.execute(Command::FlattenAll).unwrap();
        assert!(s.document.annotations.is_empty());
        assert_eq!(s.document.render().unwrap(), loaded.render().unwrap());
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.annotations, loaded.annotations);
    }
}
#[test]
fn invalid_freehand_data_cannot_mutate_document_or_redo_history() {
    let mut s = Session::new(Document::blank(100, 100).unwrap()).unwrap();
    s.execute(Command::Add {
        annotation: stroke(Kind::Pen),
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    for points in [
        vec![],
        vec![[f32::NAN, 20.0], [50.0, 50.0]],
        vec![[101.0, 20.0], [50.0, 50.0]],
        vec![[10.0, 10.0]; 8193],
    ] {
        let mut a = stroke(Kind::Pen);
        a.points = points;
        assert!(s.execute(Command::Add { annotation: a }).is_err());
        assert!(s.document.annotations.is_empty());
        assert!(s.can_redo());
    }
    let mut line = stroke(Kind::Line);
    line.points = vec![[1.0, 1.0], [2.0, 2.0]];
    assert!(s.execute(Command::Add { annotation: line }).is_err());
    s.execute(Command::Redo).unwrap();
    assert_eq!(s.document.annotations[0], stroke(Kind::Pen));
}
