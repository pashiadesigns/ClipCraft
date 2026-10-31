use super::*;
use crate::{
    Command, Session,
    display::Viewport,
    object_styles::{ObjectStyle, Store, paint_format},
};
fn fixture() -> Document {
    let mut d = Document::from_image(image::RgbaImage::new(200, 120)).unwrap();
    d.annotations.push(
        marker(
            1,
            [60.0, 60.0],
            64,
            [210, 30, 60, 255],
            (8, false),
            [200, 120],
        )
        .unwrap(),
    );
    d
}
#[test]
fn outlined_step_roundtrips_versions_commands_styles_defaults_and_payloads() {
    let original = fixture();
    let old = serde_json::to_vec(&original).unwrap();
    assert!(
        !String::from_utf8(old.clone())
            .unwrap()
            .contains("step_outline")
    );
    let mut outlined = original.annotations[0].clone();
    outlined.step_outline = Some(Outline {
        color: [15, 65, 190, 120],
        width: 5.5,
    });
    let mut invalid = original.clone();
    invalid.annotations[0] = outlined.clone();
    assert!(invalid.validate().is_err());
    for command in [
        Command::Update {
            annotation: outlined.clone(),
        },
        Command::UpdateMany {
            annotations: vec![outlined.clone()],
        },
    ] {
        let mut s = Session::new(original.clone()).unwrap();
        s.execute(command).unwrap();
        assert_eq!(s.document.version, 6);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("step.capture.json");
        s.save(&path).unwrap();
        let saved = Document::load(&path).unwrap();
        assert_eq!(saved.annotations, s.document.annotations);
        assert_eq!(saved.render().unwrap(), s.document.render().unwrap());
        let payload = crate::clipboard_payload::ObjectPayload::capture(&saved, &[1]).unwrap();
        assert_eq!(payload.document_version, 6);
        assert_eq!(
            payload.place(&original, [110.0, 65.0]).unwrap()[0].step_outline,
            outlined.step_outline
        );
        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), old);
        assert!(!s.can_undo());
        s.execute(Command::Redo).unwrap();
        s.execute(Command::FlattenAll).unwrap();
        assert_eq!(s.document.version, 2);
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.version, 6);
        s.execute(Command::ResizeImage {
            width: 400,
            height: 240,
        })
        .unwrap();
        assert_eq!(
            s.document.annotations[0]
                .step_outline
                .as_ref()
                .unwrap()
                .width,
            11.0
        );
        assert_eq!(s.document.version, 6);
    }
    let mut added = Session::new(Document::blank(200, 120).unwrap()).unwrap();
    added
        .execute(Command::Add {
            annotation: outlined.clone(),
        })
        .unwrap();
    assert_eq!(added.document.version, 6);
    added.execute(Command::Undo).unwrap();
    assert_eq!(added.document.version, 1);
    let dir = tempfile::tempdir().unwrap();
    let style = ObjectStyle::from_annotation("Outlined step".into(), &outlined).unwrap();
    Store::open(dir.path())
        .unwrap()
        .save(style.clone())
        .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.entries(), std::slice::from_ref(&style));
    assert_eq!(
        store.entries()[0]
            .apply(&original.annotations[0])
            .unwrap()
            .step_outline,
        outlined.step_outline
    );
    let prefs = crate::quick_preferences::Store::open(dir.path()).unwrap();
    prefs
        .remember(crate::quick_preferences::Choice::DrawingStyle(style))
        .unwrap();
    let prefs = crate::quick_preferences::Store::open(dir.path())
        .unwrap()
        .read()
        .unwrap();
    assert_eq!(prefs.drawing_styles[0].step_outline, outlined.step_outline);
    for width in [-1.0, 33.0, f32::NAN, f32::INFINITY] {
        assert!(
            Outline {
                color: [0; 4],
                width
            }
            .validate()
            .is_err()
        );
    }
    let mut wrong = outlined.clone();
    wrong.kind = Kind::Text;
    invalid.version = 6;
    invalid.annotations = vec![wrong];
    assert!(invalid.validate().is_err());
}
#[test]
fn outline_is_antialiased_inside_fixed_marker_bounds_and_matches_native_export() {
    for angle in [0.0, 27.0] {
        for width in [0.5, 6.0, 32.0] {
            let mut d = fixture();
            let a = &mut d.annotations[0];
            a.color = [0; 4];
            a.text.as_mut().unwrap().content.clear();
            a.rotation_degrees = angle;
            a.step_outline = Some(Outline {
                color: [40, 110, 220, 128],
                width,
            });
            d.promote_features();
            let export = d.render().unwrap();
            let viewport = d
                .display_list()
                .unwrap()
                .render_viewport(Viewport {
                    size: [200, 120],
                    origin: [0.0, 0.0],
                    scale: 1.0,
                })
                .unwrap();
            assert_eq!(export, viewport);
            assert!(export.pixels().any(|p| p[3] > 0 && p[3] < 128));
            assert!(export.pixels().all(|p| p[3] <= 128));
            for (x, y, p) in export.enumerate_pixels() {
                if p[3] > 0 {
                    assert!(
                        (27..94).contains(&x) && (27..94).contains(&y),
                        "outline grew beyond circle: {x},{y}"
                    );
                }
            }
            assert_eq!(
                d.annotations[0].body_bounds(),
                fixture().annotations[0].body_bounds()
            );
        }
    }
    let legacy = fixture();
    let mut zero = legacy.clone();
    zero.annotations[0].step_outline = Some(Outline {
        color: [0; 4],
        width: 0.0,
    });
    zero.promote_features();
    assert_eq!(legacy.render().unwrap(), zero.render().unwrap());
}
#[test]
fn step_outline_painter_roles_and_batch_changes_keep_fill_label_and_geometry_independent() {
    let d: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let callout = d
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Callout)
        .unwrap();
    let step = fixture().annotations.remove(0);
    let painted = paint_format(callout, &step).unwrap();
    let style = painted.step_outline.as_ref().unwrap();
    assert_eq!(style.color, callout.callout.as_ref().unwrap().outline);
    assert_eq!(style.width, callout.width as f32);
    assert_eq!(painted.start, step.start);
    assert_eq!(painted.text.as_ref().unwrap().content, "8");
    let shape = d
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Rectangle)
        .unwrap();
    let shape = paint_format(&painted, shape).unwrap();
    assert_eq!(shape.color, style.color);
    assert_eq!(shape.fill, Some(painted.color));
    let mut target = painted.clone();
    target.step_outline.as_mut().unwrap().color = [18, 28, 38, 255];
    let mut changed = painted.clone();
    changed.step_outline.as_mut().unwrap().width = 12.0;
    let edited = crate::multi_selection::format_like_change(&target, &painted, &changed);
    assert_eq!(edited.step_outline.as_ref().unwrap().width, 12.0);
    assert_eq!(
        edited.step_outline.as_ref().unwrap().color,
        [18, 28, 38, 255]
    );
    assert_eq!(edited.text, target.text);
    assert_eq!(edited.color, target.color);
    changed.step_outline = None;
    assert!(
        crate::multi_selection::format_like_change(&target, &painted, &changed)
            .step_outline
            .is_none()
    );
}
