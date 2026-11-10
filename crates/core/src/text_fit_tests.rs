use crate::text::{TextFit, TextFont, TextStyle, content_bounds, editing_layout, fitted_style};
use crate::{Annotation, Command, Document, Kind, Session, object_styles::ObjectStyle};

fn sample(kind: Kind) -> Annotation {
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
        kind,
        start: [20.0, 30.0],
        end: [140.0, 75.0],
        color: [30, 50, 90, 255],
        width: 2,
        corner_radius: 0.0,
        fill: None,
        points: if kind == Kind::Polygon {
            crate::polygon::outline([20.0, 30.0], [140.0, 75.0], 5)
        } else {
            vec![]
        },
        text: Some(TextStyle {
            content: "A long line of text that needs fitting\nSecond line".into(),
            padding: 4.0,
            ..Default::default()
        }),
        callout: (kind == Kind::Callout).then(|| {
            crate::callout::CalloutStyle::for_box([20.0, 30.0], [140.0, 75.0], [640, 400])
        }),
    }
}

#[test]
fn fit_modes_keep_requested_size_and_legacy_layouts_with_optional_wrapping() {
    let old: TextStyle =
        serde_json::from_str(r#"{"content":"Legacy","font":"ubuntu_light","size_points":18}"#)
            .unwrap();
    assert_eq!(old.fit, TextFit::None);
    assert!(old.wrap);
    assert_eq!(TextStyle::default().fit, TextFit::Shrink);
    for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
        for wrap in [true, false] {
            let a = sample(Kind::Text);
            let mut style = a.text.unwrap();
            style.font = font;
            style.wrap = wrap;
            let original = style.clone();
            let resolved = fitted_style(&style, [112.0, 37.0]);
            assert_eq!(style, original);
            assert!(resolved.size_points < style.size_points);
            let rows = editing_layout(&resolved, 112.0);
            assert!(rows.rows.len() as f32 * rows.line_height <= 37.001);
            assert!(
                rows.rows
                    .iter()
                    .flat_map(|r| &r.characters)
                    .filter(|c| !c.character.is_whitespace())
                    .all(|c| c.x + c.advance <= 112.001)
            );
            if !wrap {
                assert_eq!(rows.rows.len(), 2);
            }
            style.fit = TextFit::None;
            assert_eq!(fitted_style(&style, [112.0, 37.0]), style);
            style.content = "Short".into();
            style.fit = TextFit::Shrink;
            assert_eq!(
                fitted_style(&style, [500.0, 500.0]).size_points,
                style.size_points
            );
        }
    }
}

#[test]
fn shape_fit_preserves_objects_labels_pointers_and_atomic_history_output() {
    for kind in [
        Kind::Text,
        Kind::Rectangle,
        Kind::Ellipse,
        Kind::Polygon,
        Kind::Callout,
    ] {
        for wrap in [true, false] {
            let mut a = sample(kind);
            let mut s = Session::new(Document::blank(640, 400).unwrap()).unwrap();
            s.execute(Command::Add {
                annotation: a.clone(),
            })
            .unwrap();
            let before = s.document.clone();
            a.text.as_mut().unwrap().fit = TextFit::ResizeShape;
            a.text.as_mut().unwrap().wrap = wrap;
            s.execute(Command::Update {
                annotation: a.clone(),
            })
            .unwrap();
            let resized = &s.document.annotations[0];
            let (min, max) = resized.body_bounds();
            if wrap {
                assert!((max[0] - min[0] - 120.0).abs() < 0.001);
            }
            let (inner_min, inner_max) = content_bounds(resized);
            let text = resized.text.as_ref().unwrap();
            let rows = editing_layout(text, inner_max[0] - inner_min[0]);
            assert!(
                rows.rows.len() as f32 * rows.line_height <= inner_max[1] - inner_min[1] + 0.001
            );
            assert_eq!(resized.text, a.text);
            assert_eq!(resized.callout, a.callout);
            assert_eq!(resized.fit_text_shape([640, 400]).unwrap(), *resized);
            let after = s.document.render().unwrap();
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join("fit.capture.json");
            s.save(&file).unwrap();
            let reopened = Document::load(&file).unwrap();
            assert_eq!(reopened.annotations, s.document.annotations);
            assert_eq!(reopened.render().unwrap(), after);
            let png = temp.path().join("fit.png");
            reopened.export_png(&png).unwrap();
            assert_eq!(image::open(png).unwrap().into_rgba8(), after);
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, before.annotations);
            s.execute(Command::Redo).unwrap();
            assert_eq!(s.document.render().unwrap(), after);
        }
    }
}

#[test]
fn templates_and_batch_formatting_retain_fit_wrap_and_reject_impossible_size_atomically() {
    let mut a = sample(Kind::Rectangle);
    let mut b = a.clone();
    b.id = 2;
    b.start[0] += 180.0;
    b.end[0] += 180.0;
    b.text.as_mut().unwrap().content = "Other label".into();
    let mut s = Session::new(Document::blank(640, 400).unwrap()).unwrap();
    s.execute(Command::AddMany {
        annotations: vec![a.clone(), b.clone()],
    })
    .unwrap();
    a.text.as_mut().unwrap().fit = TextFit::ResizeShape;
    a.text.as_mut().unwrap().wrap = false;
    a.text.as_mut().unwrap().bold = true;
    a.text.as_mut().unwrap().italic = true;
    let store_dir = tempfile::tempdir().unwrap();
    let mut store = crate::object_styles::Store::open(store_dir.path()).unwrap();
    store
        .save(ObjectStyle::from_annotation("Fitted".into(), &a).unwrap())
        .unwrap();
    let reopened = crate::object_styles::Store::open(store_dir.path()).unwrap();
    let command = reopened.entries()[0]
        .compatible_command(&s.document.annotations)
        .unwrap();
    s.execute(command).unwrap();
    for obj in &s.document.annotations {
        let t = obj.text.as_ref().unwrap();
        assert_eq!(t.fit, TextFit::ResizeShape);
        assert!(!t.wrap);
        assert!(t.bold && t.italic);
    }
    assert_eq!(
        s.document.annotations[1].text.as_ref().unwrap().content,
        "Other label"
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations[1], b);
    assert!(s.can_redo());
    let before = s.document.annotations.clone();
    a.text.as_mut().unwrap().content = "W".repeat(4096);
    assert!(
        s.execute(Command::UpdateMany {
            annotations: vec![b, a]
        })
        .is_err()
    );
    assert_eq!(s.document.annotations, before);
    assert!(s.can_redo());
    let original = &before[0];
    let mut changed = original.clone();
    changed.text.as_mut().unwrap().fit = TextFit::None;
    changed.text.as_mut().unwrap().wrap = false;
    let copied = crate::multi_selection::format_like_change(&before[1], original, &changed);
    assert_eq!(copied.text.as_ref().unwrap().fit, TextFit::None);
    assert!(!copied.text.as_ref().unwrap().wrap);
}

#[test]
fn real_bold_italic_faces_render_distinctly_and_roundtrip_without_mutating_requested_size() {
    for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
        let mut images = Vec::new();
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut a = sample(Kind::Text);
            a.end = [300.0, 170.0];
            let t = a.text.as_mut().unwrap();
            t.font = font;
            t.content = "Agile typography".into();
            t.bold = bold;
            t.italic = italic;
            let mut s = Session::new(Document::canvas(400, 220, [0; 4]).unwrap()).unwrap();
            s.execute(Command::Add {
                annotation: a.clone(),
            })
            .unwrap();
            let image = s.document.render().unwrap();
            assert!(image.pixels().any(|p| p.0[3] != 0));
            assert!(images.iter().all(|prior| *prior != image));
            images.push(image.clone());
            let file = tempfile::tempdir().unwrap();
            let path = file.path().join("faces.json");
            s.save(&path).unwrap();
            let loaded = Document::load(&path).unwrap();
            assert_eq!(loaded.annotations[0], a);
            assert_eq!(loaded.render().unwrap(), image);
        }
    }
}

#[test]
fn resize_fit_near_canvas_edge_moves_within_bounds_and_invalid_input_is_rejected() {
    let mut a = sample(Kind::Callout);
    a = a.translated([450.0, 250.0], [640, 400]).unwrap();
    let old_pointer = a.callout.as_ref().unwrap().pointer;
    a.text.as_mut().unwrap().fit = TextFit::ResizeShape;
    let b = a.fit_text_shape([640, 400]).unwrap();
    let (_, max) = b.body_bounds();
    assert!(max[0] <= 640.0 && max[1] <= 400.0);
    assert_eq!(b.callout.as_ref().unwrap().pointer, old_pointer);
    a.text.as_mut().unwrap().size_points = f32::NAN;
    assert!(a.fit_text_shape([640, 400]).is_err());
}
