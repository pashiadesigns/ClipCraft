use super::*;
use crate::{
    Command, Document, Kind, Session,
    display::Viewport,
    object_styles::{ObjectStyle, Store, paint_format},
};

fn fixture() -> Document {
    serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap()
}
fn outline() -> outline::Outline {
    outline::Outline {
        color: [185, 25, 40, 160],
        width: 1.75,
    }
}

#[test]
fn outlined_labels_match_export_viewport_reopen_and_leave_legacy_untouched() {
    let original = fixture();
    assert!(
        original
            .annotations
            .iter()
            .all(|a| a.text.as_ref().is_none_or(|t| t.outline.is_none()))
    );
    let mut evidence = original.clone();
    for a in &mut evidence.annotations {
        if let Some(t) = &mut a.text {
            t.outline = Some(outline());
        }
    }
    evidence.promote_features();
    evidence.validate().unwrap();
    let mut old = evidence.clone();
    old.version = 11;
    assert!(old.validate().is_err());
    for a in original.annotations.iter().filter(|a| a.text.is_some()) {
        for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
            let mut a = a.clone();
            a.color = [0; 4];
            a.fill = None;
            if let Some(c) = &mut a.callout {
                c.fill = [0; 4];
                c.outline = [0; 4];
            }
            let t = a.text.as_mut().unwrap();
            t.font = font;
            t.content = "O H\nAbc".into();
            t.color = Some([0; 4]);
            t.outline = Some(outline());
            t.underline = true;
            t.fit = TextFit::Shrink;
            let mut d = Document::canvas(640, 400, [0; 4]).unwrap();
            d.annotations.push(a);
            d.promote_features();
            d.validate().unwrap();
            let exported = d.render().unwrap();
            assert!(exported.pixels().any(|p| p[3] > 0));
            assert!(exported.pixels().any(|p| p[3] > 0 && p[3] < 160));
            assert!(exported.pixels().all(|p| p[3] <= 160));
            assert_eq!(
                d.display_list()
                    .unwrap()
                    .render_viewport(Viewport {
                        size: [640, 400],
                        origin: [0., 0.],
                        scale: 1.
                    })
                    .unwrap(),
                exported
            );
            let mut rotated = d.clone();
            rotated.annotations[0].rotation_degrees = 12.;
            rotated.validate().unwrap();
            assert_eq!(
                rotated
                    .display_list()
                    .unwrap()
                    .render_viewport(Viewport {
                        size: [640, 400],
                        origin: [0., 0.],
                        scale: 1.
                    })
                    .unwrap(),
                rotated.render().unwrap()
            );
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("outline.capture.json");
            d.save(&path).unwrap();
            let reopened = Document::load(&path).unwrap();
            assert_eq!(reopened.render().unwrap(), exported);
            let mut no_outline = d.clone();
            no_outline.annotations[0].text.as_mut().unwrap().outline = None;
            assert_ne!(no_outline.render().unwrap(), exported);
        }
    }
    if let Ok(root) = std::env::var("CLIPCRAFT_TEXT_OUTLINE_DIR") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        evidence
            .save(&root.join("text-outline.capture.json"))
            .unwrap();
        evidence.export_png(&root.join("text-outline.png")).unwrap();
    }
}

#[test]
fn outline_commands_styles_painter_clipboard_and_resize_preserve_editable_state() {
    let d = fixture();
    let mut source = d
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Callout)
        .unwrap()
        .clone();
    source.text.as_mut().unwrap().outline = Some(outline());
    let dir = tempfile::tempdir().unwrap();
    let style = ObjectStyle::from_annotation("Outlined".into(), &source).unwrap();
    Store::open(dir.path())
        .unwrap()
        .save(style.clone())
        .unwrap();
    assert_eq!(
        Store::open(dir.path()).unwrap().entries(),
        std::slice::from_ref(&style)
    );
    assert_eq!(style.text_style().unwrap().outline, Some(outline()));
    let mut s = Session::new(d.clone()).unwrap();
    let painted = d
        .annotations
        .iter()
        .map(|a| paint_format(&source, a).unwrap())
        .collect::<Vec<_>>();
    for (a, b) in d.annotations.iter().zip(&painted) {
        if let Some(t) = &b.text {
            assert_eq!(t.outline, Some(outline()));
            assert_eq!(t.content, a.text.as_ref().map_or("", |t| &t.content));
        }
    }
    s.execute(Command::UpdateMany {
        annotations: painted,
    })
    .unwrap();
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    let payload =
        crate::clipboard_payload::ObjectPayload::capture(&s.document, &[source.id]).unwrap();
    assert_eq!(payload.document_version, crate::MAX_PROJECT_VERSION);
    payload.validate().unwrap();
    let pasted = payload
        .place(&Document::blank(640, 400).unwrap(), [300., 200.])
        .unwrap();
    assert_eq!(pasted[0].text.as_ref().unwrap().outline, Some(outline()));
    let snapshot = s.document.clone();
    s.execute(Command::ResizeImage {
        width: 800,
        height: 500,
    })
    .unwrap();
    assert_eq!(
        s.document
            .annotations
            .iter()
            .find(|a| a.id == source.id)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .outline
            .unwrap()
            .width,
        1.75 * 1.25
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations, snapshot.annotations);
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations, d.annotations);
    assert!(!s.can_undo());
    s.execute(Command::Redo).unwrap();
    assert_eq!(s.document.annotations, snapshot.annotations);
}

#[test]
fn invalid_outlines_are_atomic_and_batch_color_changes_keep_other_widths() {
    let d = fixture();
    let a = d.annotations.iter().find(|a| a.kind == Kind::Text).unwrap();
    let mut before = a.clone();
    before.text.as_mut().unwrap().outline = Some(outline());
    let mut target = before.clone();
    target
        .text
        .as_mut()
        .unwrap()
        .outline
        .as_mut()
        .unwrap()
        .width = 6.;
    let mut after = before.clone();
    after.text.as_mut().unwrap().outline.as_mut().unwrap().color = [20, 180, 50, 200];
    let changed = crate::multi_selection::format_like_change(&target, &before, &after);
    let style = changed.text.unwrap();
    assert_eq!(style.outline.unwrap().width, 6.);
    assert_eq!(style.outline.unwrap().color, [20, 180, 50, 200]);
    let mut s = Session::new(d.clone()).unwrap();
    for width in [-1., 33., f32::NAN, f32::INFINITY] {
        let mut bad = a.clone();
        bad.text.as_mut().unwrap().outline = Some(outline::Outline { width, ..outline() });
        assert!(s.execute(Command::Update { annotation: bad }).is_err());
        assert_eq!(s.document.annotations, d.annotations);
        assert!(!s.can_undo());
    }
}
