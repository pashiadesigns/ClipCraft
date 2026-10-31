use super::*;
use crate::{Command, Document, Session, object_styles::ObjectStyle};

fn shape() -> Annotation {
    let mut a = crate::steps::marker(
        1,
        [60., 60.],
        40,
        [220, 50, 70, 255],
        (1, false),
        [220, 160],
    )
    .unwrap();
    a.kind = Kind::Rectangle;
    a.start = [40., 40.];
    a.end = [100., 90.];
    a.text = None;
    a.width = 4;
    a.fill = None;
    a.shadow = Some(Shadow {
        distance: 14.,
        blur: 6.,
        ..Default::default()
    });
    a
}
fn document(objects: Vec<Annotation>) -> Document {
    let mut d = Document::from_image(RgbaImage::new(220, 160)).unwrap();
    d.annotations = objects;
    d.promote_features();
    d.validate().unwrap();
    d
}
fn exact(d: &Document) -> RgbaImage {
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
fn hollow_silhouette_alpha_rotation_order_and_pixel_effects_match_export() {
    let a = shape();
    let image = exact(&document(vec![a.clone()]));
    assert_eq!(image.get_pixel(70, 65)[3], 0, "hollow center stays hollow");
    assert!(
        image.get_pixel(70, 104)[3] > 0,
        "downward shadow below outline"
    );
    assert_eq!(
        image.get_pixel(70, 40).0,
        a.color,
        "foreground stays above shadow"
    );
    assert_eq!(image.get_pixel(10, 10)[3], 0);
    let faded = exact(&document(vec![Annotation {
        shadow: Some(Shadow {
            color: [10, 100, 200, 70],
            ..a.shadow.unwrap()
        }),
        ..a.clone()
    }]));
    assert!(faded.get_pixel(70, 104)[3] < image.get_pixel(70, 104)[3]);
    assert_eq!(faded.get_pixel(70, 104)[0], 10);
    for rotation in [0., 37., 90.] {
        let rotated = a.with_rotation(rotation).unwrap();
        for kind in [Kind::Blur, Kind::Pixelate, Kind::Redact, Kind::Eraser] {
            let mut effect = a.clone();
            effect.id = 2;
            effect.kind = kind;
            effect.start = [60., 70.];
            effect.end = [130., 115.];
            effect.width = 9;
            if kind == Kind::Eraser {
                effect.points = vec![effect.start, effect.end];
            }
            exact(&document(vec![rotated.clone(), effect]));
        }
    }
    // The pixel operation's input remains the original pixels in its interior.
    let mut blur = a.clone();
    blur.kind = Kind::Blur;
    blur.width = 1;
    blur.shadow.as_mut().unwrap().distance = 0.;
    let mut plain = blur.clone();
    plain.shadow = None;
    let mut d = document(vec![blur]);
    d.source_png =
        Document::from_image(RgbaImage::from_pixel(220, 160, Rgba([200, 160, 100, 255])))
            .unwrap()
            .source_png;
    let with = exact(&d);
    d.annotations = vec![plain];
    let without = exact(&d);
    assert_eq!(with.get_pixel(70, 65), without.get_pixel(70, 65));
}

#[test]
fn text_image_step_and_fractional_preview_keep_shared_geometry_and_cached_shadows() {
    let fixture: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let mut d = fixture.clone();
    for a in &mut d.annotations {
        a.shadow = Some(Shadow::default());
    }
    let pixels = RgbaImage::from_fn(40, 30, |x, y| {
        Rgba([20, 150, 90, if x < 6 || y < 6 { 0 } else { 180 }])
    });
    let bitmap = crate::bitmap::Bitmap::new(std::sync::Arc::new(
        crate::RasterImage::from_image(&pixels).unwrap(),
    ))
    .unwrap();
    let mut image = std::sync::Arc::new(bitmap).annotation(900);
    image.start = [30., 30.];
    image.end = [90., 85.];
    image.corner_radius = 12.;
    image.shadow = Some(Shadow::default());
    d.annotations.push(image);
    d.promote_features();
    d.validate().unwrap();
    exact(&d);
    let list = d.display_list().unwrap();
    assert!(
        list.layers
            .iter()
            .any(|l| matches!(l, crate::display::Layer::Shadow(_)))
    );
    for scale in [0.5, 0.85, 1.2, 3.] {
        let v = Viewport {
            size: [300, 220],
            origin: [20., 10.],
            scale,
        };
        assert_eq!(
            list.render_viewport(v).unwrap(),
            list.render_viewport(v).unwrap()
        );
    }
    if let Ok(root) = std::env::var("CLIPCRAFT_SHADOW_DIR") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        d.save(&root.join("shadows.capture.json")).unwrap();
        d.export_png(&root.join("shadows.png")).unwrap();
    }
}

#[test]
fn shadows_persist_copy_paint_resize_batch_and_undo_without_changing_source() {
    let d = document(vec![]);
    let source = d.source_png.clone();
    let a = shape();
    let mut s = Session::new(d).unwrap();
    s.execute(Command::Add {
        annotation: a.clone(),
    })
    .unwrap();
    assert_eq!(s.document.version, 14);
    let image = exact(&s.document);
    let mut b = a.clone();
    b.shadow.as_mut().unwrap().angle = 45.;
    b.shadow.as_mut().unwrap().color = [40, 100, 200, 123];
    let mut changed = a.clone();
    changed.shadow.as_mut().unwrap().blur = 10.;
    let mixed = crate::multi_selection::format_like_change(&b, &a, &changed);
    assert_eq!(mixed.shadow.unwrap().color, b.shadow.unwrap().color);
    assert_eq!(mixed.shadow.unwrap().angle, 45.);
    assert_eq!(mixed.shadow.unwrap().blur, 10.);
    let style = ObjectStyle::from_annotation("Shadow".into(), &a).unwrap();
    let style: ObjectStyle = serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
    assert_eq!(style.apply(&b).unwrap().shadow, a.shadow);
    b.kind = Kind::Ellipse;
    assert_eq!(
        crate::object_styles::paint_format(&a, &b).unwrap().shadow,
        a.shadow
    );
    let copied = crate::clipboard_payload::ObjectPayload::capture(&s.document, &[1]).unwrap();
    copied.validate().unwrap();
    assert_eq!(copied.document_version, 14);
    let scaled = s.document.resized_annotations(440, 320).unwrap();
    assert_eq!(scaled[0].shadow.unwrap().distance, 28.);
    assert_eq!(scaled[0].shadow.unwrap().blur, 12.);
    s.execute(Command::Update {
        annotation: changed,
    })
    .unwrap();
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), image);
    assert_eq!(s.document.source_png, source);
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("shadow.capture.json");
    s.save(&path).unwrap();
    let mut loaded = Document::load(&path).unwrap();
    assert_eq!(exact(&loaded), image);
    loaded.version = 13;
    assert!(loaded.validate().is_err());
    s.execute(Command::Undo).unwrap();
    assert!(s.document.annotations.is_empty());
    assert!(!s.can_undo());
}

#[test]
fn invalid_shadows_fail_atomically_and_expansion_keeps_the_full_shadow() {
    let a = shape();
    let mut s = Session::new(document(vec![a.clone()])).unwrap();
    for shadow in [
        Shadow {
            blur: f32::NAN,
            ..Default::default()
        },
        Shadow {
            distance: 257.,
            ..Default::default()
        },
        Shadow {
            angle: -1.,
            ..Default::default()
        },
    ] {
        let bad = Annotation {
            shadow: Some(shadow),
            ..a.clone()
        };
        assert!(s.execute(Command::Update { annotation: bad }).is_err());
        assert!(!s.can_undo());
        assert_eq!(s.document.annotations, std::slice::from_ref(&a));
    }
    let mut edge = a.clone();
    edge.start = [180., 100.];
    edge.end = [218., 155.];
    let plan = s.document.plan_expanding_move(&[edge], false).unwrap();
    assert!(plan.dimensions[1] > 160);
    let big = Annotation {
        start: [0., 0.],
        end: [5000., 5000.],
        ..a.clone()
    };
    assert!(validate_patch(&big).is_err());
    assert!(crate::display::layers_from_base(RgbaImage::new(220, 160), &[a], 1).is_err());
}

#[test]
fn integer_movement_reuses_bounded_shadow_pixels_but_style_changes_regenerate() {
    let a = shape();
    let first = patch(&a).unwrap();
    let shifted = crate::canvas_expansion::translated(std::slice::from_ref(&a), [11., -7.])
        .unwrap()
        .remove(0);
    let moved = patch(&shifted).unwrap();
    assert!(Arc::ptr_eq(&first.pixels, &moved.pixels));
    assert_eq!(moved.origin, [first.origin[0] + 11, first.origin[1] - 7]);
    let mut changed = shifted;
    changed.shadow.as_mut().unwrap().blur = 15.;
    let changed = patch(&changed).unwrap();
    assert!(!Arc::ptr_eq(&moved.pixels, &changed.pixels));
    let guard = CACHE.get().unwrap().lock().unwrap();
    assert!(guard.len() <= 64);
    assert!(guard.iter().map(cache_bytes).sum::<usize>() <= crate::MAX_PIXELS as usize * 4);
    assert!(
        guard
            .iter()
            .map(|(_, p)| u64::from(p.pixels.width()) * u64::from(p.pixels.height()))
            .sum::<u64>()
            <= crate::MAX_PIXELS
    );
}
