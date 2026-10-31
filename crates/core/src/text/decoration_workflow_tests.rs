use super::*;
use crate::{
    Command, Document, Kind, Session,
    display::Viewport,
    object_styles::{ObjectStyle, Store, paint_format},
};

fn fixture() -> Document {
    serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap()
}

#[test]
fn decoration_commands_promote_only_when_used_and_roundtrip_undo_atomically() {
    let mut doc = fixture();
    doc.annotations.retain(|a| a.text.is_some());
    let original = serde_json::to_vec(&doc).unwrap();
    assert!(
        !String::from_utf8(original.clone())
            .unwrap()
            .contains("underline")
    );
    let mut changed = doc.annotations.clone();
    for a in &mut changed {
        let t = a.text.as_mut().unwrap();
        t.underline = true;
        t.strikethrough = true;
    }
    let mut invalid = doc.clone();
    invalid.annotations = changed.clone();
    assert!(invalid.validate().is_err());
    for command in [
        Command::Update {
            annotation: changed[0].clone(),
        },
        Command::UpdateMany {
            annotations: changed.clone(),
        },
    ] {
        let mut s = Session::new(doc.clone()).unwrap();
        s.execute(command).unwrap();
        assert_eq!(s.document.version, 5);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("decorated.capture.json");
        s.save(&path).unwrap();
        let loaded = Document::load(&path).unwrap();
        assert_eq!(loaded.annotations, s.document.annotations);
        assert_eq!(loaded.render().unwrap(), s.document.render().unwrap());
        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), original);
        assert!(!s.can_undo());
        s.execute(Command::Redo).unwrap();
        s.execute(Command::ResizeImage {
            width: 800,
            height: 500,
        })
        .unwrap();
        assert_eq!(s.document.version, 5);
        let before = serde_json::to_vec(&s.document).unwrap();
        s.execute(Command::FlattenAll).unwrap();
        assert_eq!(s.document.version, 2);
        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), before);
    }
    for command in [
        Command::Add {
            annotation: changed[0].clone(),
        },
        Command::AddMany {
            annotations: changed,
        },
    ] {
        let mut s = Session::new(Document::blank(640, 400).unwrap()).unwrap();
        s.execute(command).unwrap();
        assert_eq!(s.document.version, 5);
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.version, 1);
        assert!(s.document.annotations.is_empty());
    }
}

#[test]
fn decoration_saved_styles_painter_and_portable_objects_preserve_labels() {
    let doc = fixture();
    let mut source = doc
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Callout)
        .unwrap()
        .clone();
    let text = source.text.as_mut().unwrap();
    text.underline = true;
    text.strikethrough = true;
    let temp = tempfile::tempdir().unwrap();
    let style = ObjectStyle::from_annotation("Ruled label".into(), &source).unwrap();
    Store::open(temp.path())
        .unwrap()
        .save(style.clone())
        .unwrap();
    let store = Store::open(temp.path()).unwrap();
    assert_eq!(store.entries(), &[style]);
    let restored = store.entries()[0].apply(&source).unwrap();
    assert!(restored.text.as_ref().unwrap().underline);
    assert!(restored.text.as_ref().unwrap().strikethrough);
    let defaults = temp.path().join("defaults");
    crate::quick_preferences::Store::open(&defaults)
        .unwrap()
        .remember(crate::quick_preferences::Choice::DrawingStyle(
            store.entries()[0].clone(),
        ))
        .unwrap();
    let reopened = crate::quick_preferences::Store::open(&defaults)
        .unwrap()
        .read()
        .unwrap();
    let remembered = reopened.drawing_styles[0].text_style().unwrap();
    assert!(remembered.underline && remembered.strikethrough);
    let mut s = Session::new(doc.clone()).unwrap();
    let annotations = doc
        .annotations
        .iter()
        .map(|target| {
            let painted = paint_format(&source, target).unwrap();
            if let Some(t) = &painted.text {
                assert!(t.underline && t.strikethrough);
                assert_eq!(
                    t.content,
                    target.text.as_ref().map_or("", |t| t.content.as_str())
                );
            }
            painted
        })
        .collect();
    s.execute(Command::UpdateMany { annotations }).unwrap();
    let payload =
        crate::clipboard_payload::ObjectPayload::capture(&s.document, &[source.id]).unwrap();
    // The same painter pass now also gives the Step a compatible outline.
    assert_eq!(payload.document_version, 6);
    let payload: crate::clipboard_payload::ObjectPayload =
        serde_json::from_slice(&serde_json::to_vec(&payload).unwrap()).unwrap();
    payload.validate().unwrap();
    let placed = payload
        .place(&Document::blank(640, 400).unwrap(), [300.0, 200.0])
        .unwrap();
    assert!(placed[0].text.as_ref().unwrap().underline);
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations, doc.annotations);
}

#[test]
fn decorated_text_shape_callout_and_step_export_match_viewport_without_double_ink() {
    for a in fixture()
        .annotations
        .into_iter()
        .filter(|a| a.text.is_some())
    {
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
            t.color = Some([30, 80, 140, 96]);
            for rotation in [0.0, 17.0] {
                a.rotation_degrees = rotation;
                let mut doc = Document::from_image(RgbaImage::new(640, 400)).unwrap();
                doc.version = 5;
                doc.annotations = vec![a.clone()];
                let plain = doc.render().unwrap();
                for (underline, strikethrough) in [(true, false), (false, true), (true, true)] {
                    let t = doc.annotations[0].text.as_mut().unwrap();
                    t.underline = underline;
                    t.strikethrough = strikethrough;
                    let export = doc.render().unwrap();
                    assert_ne!(export, plain, "{:?} {font:?} {rotation}", a.kind);
                    assert!(export.pixels().all(|p| p[3] <= 96));
                    assert_eq!(
                        export,
                        doc.display_list()
                            .unwrap()
                            .render_viewport(Viewport {
                                size: [640, 400],
                                origin: [0.0; 2],
                                scale: 1.0
                            })
                            .unwrap()
                    );
                    let zoomed = doc
                        .display_list()
                        .unwrap()
                        .render_viewport(Viewport {
                            size: [960, 600],
                            origin: [0.0; 2],
                            scale: 1.5,
                        })
                        .unwrap();
                    assert!(zoomed.pixels().all(|p| p[3] <= 96));
                }
            }
        }
    }
}
