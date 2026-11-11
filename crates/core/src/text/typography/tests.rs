use super::*;
use crate::{
    Annotation, Command, Document, Kind, Session,
    display::Viewport,
    text::{HorizontalAlignment, TextFit, TextStyle, VerticalAlignment},
};
fn sample() -> Annotation {
    let d: Document = serde_json::from_str(include_str!(
        "../../../tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    let mut a = d
        .annotations
        .into_iter()
        .find(|a| a.kind == Kind::Text)
        .unwrap();
    a.start = [15., 15.];
    a.end = [340., 130.];
    a.text = Some(TextStyle {
        content: "Abc Cafe\nSecond row".into(),
        size_points: 24.,
        fit: TextFit::None,
        horizontal: HorizontalAlignment::Left,
        vertical: VerticalAlignment::Top,
        ..Default::default()
    });
    a
}
#[test]
fn typography_metrics_highlight_clipping_render_and_persistence_match() {
    let original = sample();
    let mut evidence = Document::blank(720, 400).unwrap();
    for (i, (script, spacing, highlight)) in [
        (Script::Normal, 0., None),
        (Script::Superscript, 0., None),
        (Script::Subscript, 0., None),
        (Script::Normal, 3., None),
        (Script::Normal, -1., Some([255, 225, 25, 125])),
        (Script::Superscript, 1., Some([50, 200, 150, 90])),
    ]
    .into_iter()
    .enumerate()
    {
        let mut a = original.clone();
        a.id = i as u64 + 1;
        a.text.as_mut().unwrap().set_typography(Typography {
            script,
            spacing,
            highlight,
        });
        let plain = super::super::editing_layout(original.text.as_ref().unwrap(), 325.);
        let layout = super::super::editing_layout(a.text.as_ref().unwrap(), 325.);
        assert_eq!(
            layout
                .rows
                .iter()
                .map(|r| r.characters.len())
                .sum::<usize>(),
            plain.rows.iter().map(|r| r.characters.len()).sum::<usize>()
        );
        if script == Script::Superscript {
            assert!(layout.ascent < plain.ascent);
        }
        if script == Script::Subscript {
            assert!(layout.ascent > plain.ascent);
        }
        let mut d = Document::canvas(360, 150, [0; 4]).unwrap();
        d.annotations.push(a.clone());
        d.promote_features();
        let image = d.render().unwrap();
        assert_eq!(
            image,
            d.display_list()
                .unwrap()
                .render_viewport(Viewport {
                    size: [360, 150],
                    origin: [0.; 2],
                    scale: 1.
                })
                .unwrap()
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("text.capture.json");
        d.save(&path).unwrap();
        assert_eq!(Document::load(&path).unwrap().render().unwrap(), image);
        if highlight.is_some() {
            assert!(image.get_pixel(40, 30)[3] > 0);
            assert_eq!(image.get_pixel(5, 5)[3], 0);
        }
        a.start[0] += (i % 2) as f32 * 360.;
        a.end[0] += (i % 2) as f32 * 360.;
        a.start[1] += (i / 2) as f32 * 130.;
        a.end[1] += (i / 2) as f32 * 130.;
        evidence.annotations.push(a);
    }
    evidence.promote_features();
    assert_eq!(evidence.version, crate::MAX_PROJECT_VERSION);
    evidence.validate().unwrap();
    if let Ok(root) = std::env::var("CLIPDESIGNER_TYPOGRAPHY_DIR") {
        let root = std::path::Path::new(&root);
        std::fs::create_dir_all(root).unwrap();
        evidence
            .save(&root.join("typography.capture.json"))
            .unwrap();
        evidence.export_png(&root.join("typography.png")).unwrap();
    }
}
#[test]
fn typography_styles_batch_clipboard_resize_undo_and_invalid_inputs() {
    use crate::object_styles::{ObjectStyle, Store, paint_format};
    let a = sample();
    let mut s = Session::new(Document::blank(800, 500).unwrap()).unwrap();
    s.execute(Command::Add {
        annotation: a.clone(),
    })
    .unwrap();
    let mut b = a.clone();
    b.text.as_mut().unwrap().set_typography(Typography {
        script: Script::Subscript,
        spacing: 2.,
        highlight: Some([240, 200, 20, 100]),
    });
    s.execute(Command::Update {
        annotation: b.clone(),
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations[0], a);
    s.execute(Command::Redo).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let style = ObjectStyle::from_annotation("Custom typography".into(), &b).unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    store.save(style.clone()).unwrap();
    assert_eq!(
        Store::open(dir.path()).unwrap().entries(),
        std::slice::from_ref(&style)
    );
    assert_eq!(style.apply(&a).unwrap().text, b.text);
    assert_eq!(paint_format(&b, &a).unwrap().text, b.text);
    let mut after = b.clone();
    after
        .text
        .as_mut()
        .unwrap()
        .typography
        .as_mut()
        .unwrap()
        .spacing = 4.;
    let mut target = a.clone();
    target.text.as_mut().unwrap().set_typography(Typography {
        script: Script::Superscript,
        spacing: -1.,
        highlight: None,
    });
    let changed = crate::multi_selection::format_like_change(&target, &b, &after);
    let t = changed.text.unwrap().typography();
    assert_eq!(t.spacing, 4.);
    assert_eq!(t.script, Script::Superscript);
    assert!(t.highlight.is_none());
    let payload = crate::clipboard_payload::ObjectPayload::capture(&s.document, &[a.id]).unwrap();
    assert_eq!(payload.document_version, crate::MAX_PROJECT_VERSION);
    s.execute(Command::PasteObjects {
        payload,
        center: [550., 250.],
    })
    .unwrap();
    assert_eq!(s.document.annotations[1].text, b.text);
    s.execute(Command::Undo).unwrap();
    s.execute(Command::ResizeImage {
        width: 400,
        height: 250,
    })
    .unwrap();
    assert_eq!(
        s.document.annotations[0]
            .text
            .as_ref()
            .unwrap()
            .typography()
            .spacing,
        1.
    );
    s.execute(Command::Undo).unwrap();
    let bytes = serde_json::to_vec(&s.document).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, -4.1, 32.1] {
        let mut bad = b.clone();
        bad.text
            .as_mut()
            .unwrap()
            .typography
            .as_mut()
            .unwrap()
            .spacing = invalid;
        assert!(s.execute(Command::Update { annotation: bad }).is_err());
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), bytes);
        assert!(s.can_redo());
    }
    assert_eq!(Case::Upper.apply("Café ß"), "CAFÉ SS");
    assert_eq!(Case::Lower.apply("ABC"), "abc");
    assert_eq!(Case::Title.apply("one TWO-three"), "One Two-Three");
}
