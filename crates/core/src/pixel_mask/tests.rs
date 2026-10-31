use super::*;
use crate::{Command, Session};
fn ellipse() -> PixelMask {
    PixelMask {
        bounds: PixelRect {
            x: 8,
            y: 7,
            width: 41,
            height: 27,
        },
        shape: ShapeMask::Ellipse,
        inverted: false,
    }
}
#[test]
fn ellipse_polygon_and_inverse_have_complementary_antialiased_coverage() {
    let mut m = ellipse();
    for shape in [
        ShapeMask::Ellipse,
        ShapeMask::Polygon {
            points: vec![[0., 0.2], [0.8, 0.], [1., 0.7], [0.3, 1.]],
        },
    ] {
        m.shape = shape;
        m.inverted = false;
        let a = m.coverage([64, 48]).unwrap();
        m.inverted = true;
        let inverse = m.coverage([64, 48]).unwrap();
        let mut fractional = 0;
        for y in 0..48 {
            for x in 0..64 {
                let v = a.at(x, y);
                assert_eq!(u16::from(v) + u16::from(inverse.at(x, y)), 255);
                fractional += usize::from(v > 0 && v < 255);
            }
        }
        assert!(fractional > 20);
        assert_eq!(a.at(0, 0), 0);
        assert_eq!(inverse.at(0, 0), 255);
    }
    let d = Document::from_image(RgbaImage::from_fn(64, 48, |x, y| {
        Rgba([x as u8, y as u8, 80, 128])
    }))
    .unwrap();
    let rect = ellipse().bounds;
    assert_eq!(
        d.copy_pixels(rect).unwrap(),
        d.copy_masked_pixels(&rect.into()).unwrap()
    );
}
#[test]
fn masked_cut_preserves_outside_source_and_restores_editable_objects_in_one_undo() {
    let mut d: Document = serde_json::from_str(include_str!(
        "../../tests/fixtures/translation.capture.json"
    ))
    .unwrap();
    let mut m = ellipse();
    m.bounds = PixelRect {
        x: 80,
        y: 55,
        width: 170,
        height: 115,
    };
    let original = serde_json::to_vec(&d).unwrap();
    let before = d.render().unwrap();
    let c = m.coverage([d.width, d.height]).unwrap();
    let copy = d.copy_masked_pixels(&m).unwrap();
    assert_eq!(copy.get_pixel(0, 0)[3], 0);
    let mut session = Session::new(d.clone()).unwrap();
    session
        .execute(Command::ClearMaskedPixels { mask: m.clone() })
        .unwrap();
    let after = session.document.render().unwrap();
    for (x, y, p) in after.enumerate_pixels() {
        let a = before.get_pixel(x, y)[3];
        assert_eq!(
            p[3],
            a - ((u16::from(a) * u16::from(c.at(x, y)) + 127) / 255) as u8
        );
        if c.at(x, y) == 0 {
            assert_eq!(p, before.get_pixel(x, y));
        }
    }
    session.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
    assert!(!session.can_undo());
    d = session.document.clone();
    m.shape = ShapeMask::Polygon {
        points: vec![[0., 0.], [f32::NAN, 1.], [1., 0.]],
    };
    assert!(
        session
            .execute(Command::ClearMaskedPixels { mask: m })
            .is_err()
    );
    assert_eq!(
        serde_json::to_vec(&session.document).unwrap(),
        serde_json::to_vec(&d).unwrap()
    );
    assert!(session.can_redo());
}
#[test]
fn lifted_pixels_keep_alpha_move_expand_save_reopen_and_undo() {
    let pixels = RgbaImage::from_fn(64, 48, |x, y| {
        Rgba([
            30 + x as u8 * 2,
            40 + y as u8 * 2,
            160,
            if x < 32 { 255 } else { 128 },
        ])
    });
    let mut s = Session::new(Document::from_image(pixels.clone()).unwrap()).unwrap();
    s.execute(Command::LiftMaskedPixels { mask: ellipse() })
        .unwrap();
    let lift = s.document.clone();
    assert_eq!(lift.annotations.len(), 1);
    let a = &lift.annotations[0];
    assert_eq!(a.kind, crate::Kind::Image);
    assert_eq!(a.start, [8., 7.]);
    let shown = lift.render().unwrap();
    for (a, b) in shown.as_raw().iter().zip(pixels.as_raw()) {
        assert!(a.abs_diff(*b) <= 1, "{a} != {b}");
    }
    let moved = crate::canvas_expansion::translated(&lift.annotations, [35., 25.]).unwrap();
    s.execute(Command::MoveAndExpand {
        annotations: moved,
        duplicate: false,
    })
    .unwrap();
    assert!(s.document.width > 64 && s.document.height > 48);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("selection.capture.json");
    s.save(&path).unwrap();
    assert_eq!(
        Document::load(&path).unwrap().render().unwrap(),
        s.document.render().unwrap()
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(
        serde_json::to_vec(&s.document).unwrap(),
        serde_json::to_vec(&lift).unwrap()
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), pixels);
    assert!(s.document.annotations.is_empty());
    assert!(!s.can_undo());
    if let Ok(root) = std::env::var("CLIPCRAFT_MASK_DIR") {
        let root = std::path::Path::new(&root);
        std::fs::create_dir_all(root).unwrap();
        s.execute(Command::Redo).unwrap();
        s.execute(Command::Redo).unwrap();
        s.document.save(&root.join("mask.capture.json")).unwrap();
        s.document.export_png(&root.join("mask.png")).unwrap();
    }
}
#[test]
fn empty_mask_edits_and_invalid_bounds_do_not_consume_history() {
    let mut s = Session::new(Document::canvas(64, 48, [0; 4]).unwrap()).unwrap();
    for lift in [false, true] {
        let mask = ellipse();
        s.execute(if lift {
            Command::LiftMaskedPixels { mask }
        } else {
            Command::ClearMaskedPixels { mask }
        })
        .unwrap();
        assert!(!s.can_undo());
    }
    let mut mask = ellipse();
    mask.bounds.x = u32::MAX;
    assert!(s.execute(Command::LiftMaskedPixels { mask }).is_err());
    assert!(!s.can_undo());
}
