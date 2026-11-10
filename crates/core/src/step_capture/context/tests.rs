use super::*;
use image::{Rgba, RgbaImage};
#[test]
fn framed_guide_preserves_full_sources_metadata_captions_sections_and_output() {
    let source = RgbaImage::from_fn(960, 540, |x, y| {
        Rgba([
            ((x / 24) % 2 * 25 + 210) as u8,
            ((y / 24) % 2 * 25 + 210) as u8,
            245,
            255,
        ])
    });
    let mut guide = Guide::new([960, 540]).unwrap();
    guide
        .set_header(Header {
            title: "Synthetic capture guide".into(),
            author: None,
            date: "2026-10-31".into(),
            seconds: 63,
        })
        .unwrap();
    for (bounds, point, name) in [
        ([100, 80, 700, 450], [300, 200], "Save document"),
        ([650, 300, 950, 520], [800, 400], "Open settings"),
    ] {
        guide
            .push(
                Frame {
                    image: source.clone(),
                    cursor: None,
                    context: Context {
                        bounds: Some(bounds),
                        name: Some(name.into()),
                    },
                },
                Gesture {
                    start: point,
                    end: point,
                    button: Button::Left,
                },
            )
            .unwrap();
    }
    let doc = guide.document().unwrap();
    let layout = doc.guide.as_ref().unwrap();
    assert_eq!(layout.sections.len(), 3);
    assert_eq!(
        layout.sections[0].kind,
        crate::guide_sections::SectionKind::Header
    );
    let texts: Vec<_> = doc
        .annotations
        .iter()
        .filter(|a| a.kind == Kind::Text)
        .map(|a| a.text.as_ref().unwrap().content.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "Synthetic capture guide",
            "2 steps | 1:03 | 2026-10-31",
            "Click \"Save document\"",
            "Click \"Open settings\""
        ]
    );
    for a in doc.annotations.iter().filter(|a| a.kind == Kind::Image) {
        let b = a.image.as_ref().unwrap();
        assert_eq!(b.size(), [960, 540]);
        assert_eq!(**b.pixels().unwrap(), source);
        assert!(b.has_view());
        let reset = a.with_image_view(None).unwrap();
        assert_eq!(**reset.image.unwrap().pixels().unwrap(), source);
    }
    let expected = doc.render().unwrap();
    let view = doc
        .display_list()
        .unwrap()
        .render_viewport(crate::display::Viewport {
            size: [doc.width, doc.height],

            origin: [0., 0.],
            scale: 1.,
        })
        .unwrap();
    assert_eq!(expected, view);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("guide.capture.json");
    doc.save(&path).unwrap();
    let reopened = Document::load(&path).unwrap();
    assert_eq!(reopened.render().unwrap(), expected);
    let mut session = crate::Session::new(reopened).unwrap();
    let before = serde_json::to_vec(&session.document).unwrap();
    session
        .execute(crate::Command::Guide {
            action: crate::guide_sections::Action::Move { id: 2, index: 1 },
        })
        .unwrap();
    session.execute(crate::Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&session.document).unwrap(), before);
    if let Ok(root) = std::env::var("CLIPDESIGNER_CONTEXT_GUIDE_DIR") {
        std::fs::create_dir_all(&root).unwrap();
        doc.save(&Path::new(&root).join("guide-context.capture.json"))
            .unwrap();
        expected
            .save(Path::new(&root).join("guide-context.png"))
            .unwrap();
    }
}
#[test]
fn context_crop_contains_drag_ends_clips_monitor_bounds_and_falls_back_safely() {
    let gesture = Gesture {
        start: [5, 20],
        end: [900, 500],
        button: Button::Left,
    };
    for bounds in [
        [100, 100, 200, 200],
        [-500, -500, 1400, 900],
        [500, 300, 450, 200],
        [1000, 600, 1200, 800],
    ] {
        let view = Context {
            bounds: Some(bounds),
            name: None,
        }
        .view([960, 540], gesture);
        view.validate().unwrap();
        let [x, y, w, h] = view.rect;
        for p in [gesture.start, gesture.end] {
            assert!(p[0] as f32 >= x * 960. && (p[0] as f32) < (x + w) * 960.);
            assert!(p[1] as f32 >= y * 540. && (p[1] as f32) < (y + h) * 540.);
        }
    }
    assert_eq!(
        label("  Save\n\t document  ").as_deref(),
        Some("Save document")
    );
    assert_eq!(label(""), None);
    assert_eq!(label("bad\u{0}name"), None);
    assert_eq!(label(&"a".repeat(5000)).unwrap().len(), 96);
    let mut g = Guide::new([960, 540]).unwrap();
    g.push(RgbaImage::new(960, 540), gesture).unwrap();
    let before = g.document().unwrap().annotations;
    assert!(!g.set_name(4, "Wrong step"));
    assert!(!g.set_name(0, "bad\u{0}"));
    assert_eq!(g.document().unwrap().annotations, before);
    assert!(
        g.set_header(Header {
            title: "bad\u{0}".into(),
            ..Default::default()
        })
        .is_err()
    );
    assert_eq!(g.document().unwrap().annotations, before);
}

#[test]
fn initial_frames_center_the_click_and_crop_height_while_retaining_drag_endpoints() {
    for point in [[100, 100], [1900, 1070], [960, 540]] {
        let gesture = Gesture {
            start: point,
            end: point,
            button: Button::Left,
        };
        let v = Context::default().view([1920, 1080], gesture);
        v.validate().unwrap();
        assert!(v.rect[2] * 1920. <= 960.1);
        assert!(v.rect[3] * 1080. <= 480.1);
        for i in 0..2 {
            let p = point[i] as f32 / [1920., 1080.][i];
            assert!(p >= v.rect[i] && p <= v.rect[i] + v.rect[i + 2]);
        }
    }
    let g = Gesture {
        start: [100, 100],
        end: [1500, 900],
        button: Button::Left,
    };
    let v = Context::default().view([1920, 1080], g);
    for p in [g.start, g.end] {
        for i in 0..2 {
            let p = p[i] as f32 / [1920., 1080.][i];
            assert!(p >= v.rect[i] && p <= v.rect[i] + v.rect[i + 2]);
        }
    }
}
