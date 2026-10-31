use super::*;
use crate::{Command, Document, Session, display::Viewport, selection::Handle};

fn annotation(kind: Kind) -> Annotation {
    let d: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let mut a = d
        .annotations
        .into_iter()
        .find(|a| a.kind == Kind::Arrow)
        .unwrap();
    a.id = 1;
    a.kind = kind;
    a.start = [40., 60.];
    a.end = [220., 60.];
    a.width = 4;
    a.line_style = Some(Style {
        start: Endpoint::Circle,
        end: Endpoint::Triangle,
        ..Style::for_kind(kind)
    });
    a
}
fn document(a: Annotation) -> Document {
    let mut d = Document::canvas(300, 220, [255; 4]).unwrap();
    d.annotations = vec![a];
    d.promote_features();
    d.validate().unwrap();
    d
}
fn exact(d: &Document) -> image::RgbaImage {
    let image = d.render().unwrap();
    assert_eq!(
        image,
        d.display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [d.width, d.height],
                origin: [0.; 2],
                scale: 1.
            })
            .unwrap()
    );
    image
}

#[test]
fn endpoint_tips_follow_tangents_and_filled_heads_trim_without_hitting_empty_boxes() {
    let mut a = annotation(Kind::Arrow);
    let g = geometry(&a);
    assert_eq!(g.heads.len(), 2);
    assert!(g.shaft.bounding_box().x1 < 220.);
    assert_eq!(g.heads[1].path.bounding_box().x1, 220.);
    assert!(hit(&a, [211., 60.], 0.));
    assert!(!hit(&a, [211., 75.], 0.));
    a.line_style.as_mut().unwrap().curve = Curve::Bezier;
    a.line_style.as_mut().unwrap().bend = 0.3;
    let g = geometry(&a);
    let tangent = direction(g.full_shaft.segments().last().unwrap(), false);
    let head_center = g.heads[1].path.bounding_box().center();
    assert!((head_center - p(a.end)).dot(tangent) < 0.);
    for kind in Endpoint::ALL {
        a.line_style.as_mut().unwrap().end = kind;
        for size in [0.5, 3., 7.] {
            a.line_style.as_mut().unwrap().end_size = size;
            exact(&document(a.clone()));
        }
    }
}

#[test]
fn circular_bend_is_round_and_node_endpoints_and_patterns_share_rendering() {
    let a = annotation(Kind::Line);
    let b = bend_to(&a, [130., 105.]);
    assert_eq!(b.line_style.unwrap().curve, Curve::Circular);
    assert_eq!(bend_point(&b), [130., 105.]);
    let g = geometry(&b);
    let start = g.full_shaft.segments().next().unwrap().start();
    let end = g.full_shaft.segments().last().unwrap().end();
    assert!((start - p(a.start)).hypot() < 0.001);
    assert!((end - p(a.end)).hypot() < 0.001);
    let center = Point::new(130., -7.5);
    let radius = (start - center).hypot();
    for segment in g.full_shaft.segments() {
        for t in [0., 0.25, 0.5, 0.75, 1.] {
            assert!(((segment.eval(t) - center).hypot() - radius).abs() < 0.01);
        }
    }
    assert!(b.hit_test(bend_point(&b), 1.));
    assert_eq!(b.hit_handle(bend_point(&b), 2.), Some(Handle::Bend));
    let resized = b.resized(Handle::End, [260., 60.], [300, 220]).unwrap();
    assert_eq!(resized.line_style, b.line_style);
    assert_eq!(bend_point(&resized), [150., 115.]);
    let mut fixture = Document::canvas(600, 540, [255; 4]).unwrap();
    for (i, pattern) in Pattern::ALL.into_iter().enumerate() {
        let mut line = b.clone();
        line.id = i as u64 + 1;
        line.start = [60., 25. + i as f32 * 85.];
        line.end = [520., 25. + i as f32 * 85.];
        let style = line.line_style.as_mut().unwrap();
        style.bend = 0.09;
        style.pattern = pattern;
        style.start = Endpoint::ALL[i];
        style.end = Endpoint::ALL[i + 4];
        fixture.annotations.push(line);
    }
    fixture.promote_features();
    fixture.validate().unwrap();
    exact(&fixture);
    if let Ok(root) = std::env::var("CLIPCRAFT_LINE_DIR") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        fixture.save(&root.join("lines.capture.json")).unwrap();
        fixture.export_png(&root.join("lines.png")).unwrap();
    }
}

#[test]
fn line_styles_bends_painter_clipboard_persistence_and_undo_preserve_source_and_labels() {
    let original = annotation(Kind::Arrow);
    let mut s = Session::new(document(original.clone())).unwrap();
    let before = exact(&s.document);
    let changed = bend_to(&original, [130., 100.]);
    s.execute(Command::Update {
        annotation: changed.clone(),
    })
    .unwrap();
    assert_eq!(s.document.version, 15);
    let style =
        crate::object_styles::ObjectStyle::from_annotation("Curved".into(), &changed).unwrap();
    let style: crate::object_styles::ObjectStyle =
        serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
    assert_eq!(
        style.apply(&original).unwrap().line_style,
        changed.line_style
    );
    let line = annotation(Kind::Line);
    assert_eq!(
        crate::object_styles::paint_format(&changed, &line)
            .unwrap()
            .line_style,
        changed.line_style
    );
    let copied = crate::clipboard_payload::ObjectPayload::capture(&s.document, &[1]).unwrap();
    copied.validate().unwrap();
    assert_eq!(copied.document_version, 15);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lines.capture.json");
    s.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(exact(&loaded), exact(&s.document));
    s.execute(Command::Undo).unwrap();
    assert_eq!(exact(&s.document), before);
    assert!(!s.can_undo());
    let mut bad = changed;
    bad.line_style.as_mut().unwrap().end_size = f32::NAN;
    assert!(s.execute(Command::Update { annotation: bad }).is_err());
    assert_eq!(s.document.annotations, std::slice::from_ref(&original));
    let mut old = loaded;
    old.version = 14;
    assert!(old.validate().is_err());
}
