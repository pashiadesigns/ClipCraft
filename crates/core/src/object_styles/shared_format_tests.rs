use super::*;
use crate::{Document, Session};

#[test]
fn mixed_roles_preserve_content_geometry_and_roundtrip_with_atomic_undo() {
    let mut doc: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let source = doc
        .annotations
        .iter_mut()
        .find(|a| a.kind == Kind::Callout)
        .unwrap();
    source.color = [11, 22, 33, 255];
    source.width = 8;
    source.corner_radius = 12.0;
    let c = source.callout.as_mut().unwrap();
    c.fill = [25, 120, 180, 135];
    c.outline = [180, 20, 30, 220];
    c.padding = 11.0;
    let t = source.text.as_mut().unwrap();
    t.bold = true;
    t.italic = true;
    t.size_points = 29.0;
    t.color = Some([80, 12, 130, 230]);
    let source = source.clone();
    let mut redact = doc
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Rectangle)
        .unwrap()
        .clone();
    redact.id = 99;
    redact.kind = Kind::Redact;
    redact.text = None;
    redact.corner_radius = 0.0;
    redact.fill = None;
    let mut blur = redact.clone();
    blur.id = 100;
    blur.kind = Kind::Blur;
    doc.annotations.extend([redact, blur]);
    let before = doc.annotations.clone();
    let painted: Vec<_> = before
        .iter()
        .map(|a| paint_format(&source, a).unwrap())
        .collect();
    for (a, b) in before.iter().zip(&painted) {
        assert_eq!(
            (a.id, a.kind, a.start, a.end, &a.points, a.rotation_degrees),
            (b.id, b.kind, b.start, b.end, &b.points, b.rotation_degrees)
        );
        if let Some(t) = &a.text {
            assert_eq!(t.content, b.text.as_ref().unwrap().content);
        }
        if let Some(c) = &a.callout {
            assert_eq!(c.pointer, b.callout.as_ref().unwrap().pointer);
        }
        if is_shape(b.kind) {
            assert_eq!(b.color, source.callout.as_ref().unwrap().outline);
            assert_eq!(b.fill, Some(source.callout.as_ref().unwrap().fill));
        }
        if b.kind == Kind::Step {
            assert_eq!(b.color, source.callout.as_ref().unwrap().fill);
        }
        if matches!(
            b.kind,
            Kind::Line | Kind::Arrow | Kind::Pen | Kind::Highlighter
        ) {
            assert_eq!(b.color, source.callout.as_ref().unwrap().outline);
            assert_eq!(b.width, 8);
        }
        if matches!(b.kind, Kind::Blur | Kind::Redact) {
            assert_eq!(a, b);
        }
        if let Some(t) = &b.text {
            assert_eq!(t.color, source.text.as_ref().unwrap().color);
            assert_eq!(t.size_points, 29.0);
            assert!(t.bold && t.italic);
        }
    }
    let mut s = Session::new(doc).unwrap();
    s.execute(Command::UpdateMany {
        annotations: painted.clone(),
    })
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shared.capture.json");
    s.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.annotations, painted);
    assert_eq!(loaded.render().unwrap(), s.document.render().unwrap());
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.annotations, before);
    assert!(!s.can_undo());
    s.execute(Command::Redo).unwrap();
    assert_eq!(s.document.annotations, painted);
}

#[test]
fn explicit_no_fill_and_step_contrast_copy_by_role_without_content_or_intensity() {
    let doc: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let mut source = doc
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Rectangle)
        .unwrap()
        .clone();
    source.fill = None;
    source.text = None;
    let callout = doc
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Callout)
        .unwrap();
    let painted = paint_format(&source, callout).unwrap();
    assert_eq!(painted.callout.as_ref().unwrap().fill, [0; 4]);
    assert_eq!(painted.text, callout.text);
    let mut step = doc
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Step)
        .unwrap()
        .clone();
    step.color = [2, 3, 4, 255];
    step.text.as_mut().unwrap().color = None;
    let text = doc
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Text)
        .unwrap();
    let painted = paint_format(&step, text).unwrap();
    assert_eq!(
        painted.text.as_ref().unwrap().color,
        Some(crate::steps::foreground(&step).color)
    );
    assert_eq!(
        painted.text.as_ref().unwrap().content,
        text.text.as_ref().unwrap().content
    );
    let mut unsupported = source.clone();
    unsupported.kind = Kind::Blur;
    unsupported.corner_radius = 0.0;
    unsupported.width = 19;
    assert_eq!(paint_format(&unsupported, callout).unwrap(), *callout);
    unsupported.width = 0;
    assert!(paint_format(&unsupported, callout).is_err());
}
