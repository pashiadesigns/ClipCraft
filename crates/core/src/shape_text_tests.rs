use crate::{
    Annotation, Command, Document, Kind, Session,
    text::{HorizontalAlignment as H, TextStyle, VerticalAlignment as V},
};

#[test]
fn centered_defaults_preserve_legacy_omitted_alignment_and_independent_shape_color() {
    let new = TextStyle::default();
    assert_eq!((new.horizontal, new.vertical), (H::Center, V::Middle));
    let legacy: TextStyle =
        serde_json::from_str(r#"{"content":"Legacy","font":"ubuntu_light","size_points":18}"#)
            .unwrap();
    assert_eq!(
        (legacy.horizontal, legacy.vertical, legacy.color),
        (H::Left, V::Top, None)
    );
    let new_roundtrip: TextStyle =
        serde_json::from_slice(&serde_json::to_vec(&new).unwrap()).unwrap();
    assert_eq!(new, new_roundtrip);
    for kind in [Kind::Rectangle, Kind::Ellipse, Kind::Polygon] {
        let mut original = Document::canvas(240, 160, [0; 4]).unwrap();
        original.annotations.push(Annotation {
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
            end: [220.0, 140.0],
            width: 3,
            color: [200, 20, 30, 255],
            fill: None,
            corner_radius: 0.0,
            callout: None,
            text: None,
            points: if kind == Kind::Polygon {
                crate::polygon::outline([20.0, 20.0], [220.0, 140.0], 4)
            } else {
                vec![]
            },
        });
        let mut session = Session::new(original.clone()).unwrap();
        let before = session.document.render().unwrap();
        let mut annotation = original.annotations[0].clone();
        annotation.text = Some(TextStyle {
            content: "Label".into(),
            color: Some([20, 80, 220, 255]),
            padding: 10.0,
            ..Default::default()
        });
        session
            .execute(Command::Update {
                annotation: annotation.clone(),
            })
            .unwrap();
        let after = session.document.render().unwrap();
        let mut changed = 0;
        for (x, y, p) in after.enumerate_pixels() {
            if p != before.get_pixel(x, y) {
                changed += 1;
                assert!((30..210).contains(&x) && (30..130).contains(&y));
                assert!(p.0[2] > p.0[0]);
            }
        }
        assert!(changed > 100);
        assert_eq!(session.document.annotations[0].color, [200, 20, 30, 255]);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shape.capture.json");
        session.save(&path).unwrap();
        let loaded = Document::load(&path).unwrap();
        assert_eq!(loaded.annotations[0], annotation);
        assert_eq!(loaded.render().unwrap(), after);
        assert_eq!(loaded.source_png, original.source_png);
        let preset =
            crate::object_styles::ObjectStyle::from_annotation("Label style".into(), &annotation)
                .unwrap();
        let mut target = annotation.clone();
        target.text.as_mut().unwrap().content = "Different".into();
        assert_eq!(
            preset.apply(&target).unwrap().text.unwrap().content,
            "Different"
        );
        let no_text_preset = &crate::object_styles::presets(kind)[0];
        assert_eq!(no_text_preset.apply(&target).unwrap().text, target.text);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, original.annotations);
        assert_eq!(session.document.render().unwrap(), before);
        assert!(!session.can_undo());
        // A rejected label must preserve the redo branch.
        annotation.text.as_mut().unwrap().content = "\u{0001}".into();
        assert!(session.execute(Command::Update { annotation }).is_err());
        assert!(session.can_redo());
    }
}
