use super::*;
use crate::{
    Command, Document, Session,
    clipboard_payload::{ImagePayload, ObjectPayload},
};
use image::Rgba;

fn payload() -> ImagePayload {
    let pixels = RgbaImage::from_fn(12, 8, |x, y| {
        Rgba([
            x as u8 * 20,
            y as u8 * 30,
            160,
            [0, 64, 128, 255][x as usize % 4],
        ])
    });
    ImagePayload::from_rgba(12, 8, pixels.into_raw()).unwrap()
}

#[test]
fn aligned_image_copy_matches_reference_sampler_for_alpha_crop_outline_and_rounding() {
    for alpha in [true, false] {
        let pixels = RgbaImage::from_fn(32, 24, |x, y| {
            Rgba([
                x as u8 * 7,
                y as u8 * 9,
                123,
                if alpha {
                    ((x + y) * 17 % 256) as u8
                } else {
                    255
                },
            ])
        });
        let a = ImagePayload::from_rgba(32, 24, pixels.into_raw())
            .unwrap()
            .into_objects()
            .unwrap()
            .objects
            .remove(0);
        for opacity in [0, 73, 255] {
            for rounded in [0., 5.] {
                for crop in [false, true] {
                    for origin in [[0., 0.], [-4., -7.], [6., 8.]] {
                        let mut a = a.clone();
                        a.color[3] = opacity;
                        a.width = 7;
                        a.corner_radius = rounded;
                        a.kind = Kind::Rectangle;
                        let (size, view) = if crop {
                            (
                                [16., 12.],
                                crate::image_frame::ImageView {
                                    rect: [0.25, 0.25, 0.5, 0.5],
                                },
                            )
                        } else {
                            ([32., 24.], Default::default())
                        };
                        a = a.with_image_view(Some(view)).unwrap();
                        a.start = origin;
                        a.end = [origin[0] + size[0], origin[1] + size[1]];
                        let view = Viewport {
                            size: [48, 40],
                            origin: [0., 0.],
                            scale: 1.,
                        };
                        let mut fast = RgbaImage::from_pixel(48, 40, Rgba([91, 132, 64, 123]));
                        let mut reference = fast.clone();
                        render_impl(&mut fast, &a, view, true);
                        render_impl(&mut reference, &a, view, false);
                        assert_eq!(
                            fast, reference,
                            "alpha={alpha} opacity={opacity} rounded={rounded} crop={crop} origin={origin:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn paste_transform_save_library_undo_retains_original_pixels() {
    let source = payload();
    let pixels = source.decode().unwrap();
    let mut s = Session::new(Document::canvas(40, 30, [30, 40, 50, 255]).unwrap()).unwrap();
    let before = serde_json::to_vec(&s.document).unwrap();
    let original = s.document.source_png.clone();
    s.execute(Command::PasteObjects {
        payload: source.into_objects().unwrap(),
        center: [20., 15.],
    })
    .unwrap();
    let a = s.document.annotations[0].clone();
    assert_eq!(a.kind, Kind::Image);
    assert_eq!(s.document.version, crate::MAX_PROJECT_VERSION);
    assert_eq!(a.start, [14., 11.]);
    assert_eq!(a.end, [26., 19.]);
    assert_eq!(**a.image.as_ref().unwrap().pixels().unwrap(), pixels);
    let render = s.document.render().unwrap();
    for (x, y, p) in pixels.enumerate_pixels() {
        let mut expected = Rgba([30, 40, 50, 255]);
        crate::text::composite(&mut expected, p.0);
        assert_eq!(*render.get_pixel(x + 14, y + 11), expected);
    }
    assert_eq!(
        s.document
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [40, 30],
                origin: [0.; 2],
                scale: 1.
            })
            .unwrap(),
        render
    );
    s.execute(Command::Undo).unwrap();
    assert_eq!(serde_json::to_vec(&s.document).unwrap(), before);
    s.execute(Command::Redo).unwrap();
    let mut resized = a
        .resized(crate::selection::Handle::BottomRight, [35., 25.], [40, 30])
        .unwrap();
    resized.rotation_degrees = 25.;
    s.execute(Command::Update {
        annotation: resized,
    })
    .unwrap();
    let transformed = s.document.render().unwrap();
    assert_ne!(transformed, render);
    assert_eq!(s.document.source_png, original);
    assert_eq!(
        **s.document.annotations[0]
            .image
            .as_ref()
            .unwrap()
            .pixels()
            .unwrap(),
        pixels
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pasted.capture.json");
    s.save(&path).unwrap();
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.render().unwrap(), transformed);
    assert_eq!(
        loaded
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [40, 30],
                origin: [0.; 2],
                scale: 1.
            })
            .unwrap(),
        transformed
    );
    // Portable copying preserves the bitmap and transforms, not source pixels.
    let copy = ObjectPayload::capture(&loaded, &[a.id]).unwrap();
    assert_eq!(copy.document_version, crate::MAX_PROJECT_VERSION);
    let json = serde_json::to_vec(&copy).unwrap();
    assert!(!String::from_utf8_lossy(&json).contains("source_png"));
    s.execute(Command::RemoveMany { ids: vec![a.id] }).unwrap();
    assert!(s.document.annotations.is_empty());
    s.execute(Command::Undo).unwrap();
    assert_eq!(s.document.render().unwrap(), transformed);
}

#[test]
fn image_paste_expands_canvas_keeps_layers_and_rolls_back_invalid_payload() {
    let mut s = Session::new(Document::canvas(8, 6, [0; 4]).unwrap()).unwrap();
    let before = serde_json::to_vec(&s.document).unwrap();
    s.execute(Command::PasteObjects {
        payload: payload().into_objects().unwrap(),
        center: [0., 0.],
    })
    .unwrap();
    assert!(s.document.width > 8 && s.document.height > 6);
    let a = s.document.annotations[0].clone();
    let first = s.document.render().unwrap();
    let mut top = a.clone();
    top.id += 1;
    top.image = Some(Arc::new(
        Bitmap::new(Arc::new(
            RasterImage::from_image(&RgbaImage::from_pixel(12, 8, Rgba([250, 50, 30, 255])))
                .unwrap(),
        ))
        .unwrap(),
    ));
    s.execute(Command::Add {
        annotation: top.clone(),
    })
    .unwrap();
    let top_image = s.document.render().unwrap();
    assert_ne!(top_image, first);
    assert_eq!(
        s.document
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [s.document.width, s.document.height],
                origin: [0.; 2],
                scale: 1.
            })
            .unwrap(),
        top_image
    );
    s.execute(Command::SetObjectsVisible {
        ids: vec![top.id],
        visible: false,
    })
    .unwrap();
    assert_eq!(s.document.render().unwrap(), first);
    let valid = serde_json::to_vec(&s.document).unwrap();
    let mut bad = payload().into_objects().unwrap();
    bad.document_version = 6;
    assert!(
        s.execute(Command::PasteObjects {
            payload: bad,
            center: [4., 3.]
        })
        .is_err()
    );
    assert_eq!(serde_json::to_vec(&s.document).unwrap(), valid);
    for _ in 0..3 {
        s.execute(Command::Undo).unwrap();
    }
    assert_eq!(serde_json::to_vec(&s.document).unwrap(), before);
}

#[test]
fn bitmap_validation_and_cache_are_bounded_without_redecoding() {
    let image = payload().into_objects().unwrap().objects.remove(0);
    let bitmap = image.image.as_ref().unwrap().clone();
    assert!(Arc::ptr_eq(
        bitmap.pixels().unwrap(),
        bitmap.pixels().unwrap()
    ));
    let mut d = Document::blank(20, 20).unwrap();
    d.version = crate::MAX_PROJECT_VERSION;
    d.annotations.push(image.clone());
    assert!(d.validate().is_ok());
    for version in 1..8 {
        d.version = version;
        assert!(d.validate().is_err());
    }
    d.version = crate::MAX_PROJECT_VERSION;
    d.annotations[0].image = None;
    assert!(d.validate().is_err());
    d.annotations[0] = image.clone();
    d.annotations[0].kind = Kind::Pen;
    assert!(d.validate().is_err());
    let mut value = serde_json::to_value(&image).unwrap();
    value["image"]["raster"]["width"] = 13.into();
    d.annotations[0] = serde_json::from_value(value).unwrap();
    assert!(d.validate().is_err());
    assert!(
        Bitmap::new(Arc::new(RasterImage {
            width: u32::MAX,
            height: u32::MAX,
            png: String::new()
        }))
        .is_err()
    );
    let states = vec![
        crate::EditState::from_document(&{
            let mut d = Document::blank(20, 20).unwrap();
            d.annotations = vec![image.clone()];
            d
        }),
        crate::EditState::from_document(&{
            let mut d = Document::blank(20, 20).unwrap();
            d.annotations = vec![image];
            d
        }),
    ];
    assert_eq!(crate::raster_history_bytes(&states), bitmap.memory_bytes());
}
#[test]
fn upright_screenshot_fast_path_matches_general_sampler_exactly() {
    let pixels = RgbaImage::from_fn(171, 113, |x, y| {
        Rgba([
            (x * 37 % 256) as u8,
            (y * 19 % 256) as u8,
            ((x + y) * 11 % 256) as u8,
            255,
        ])
    });
    let bitmap =
        Arc::new(Bitmap::new(Arc::new(RasterImage::from_image(&pixels).unwrap())).unwrap());
    for (start, end) in [
        ([0., 0.], [171., 113.]),
        ([2.25, 4.5], [202.75, 119.25]),
        ([-4.75, -3.5], [80.25, 52.25]),
    ] {
        let mut a = bitmap.clone().annotation(1);
        a.start = start;
        a.end = end;
        for alpha in [0, 93, 255] {
            a.color[3] = alpha;
            for scale in [0.33, 0.79, 0.85, 0.97, 1., 1.25, 4.] {
                let view = Viewport {
                    size: [233, 157],
                    origin: [-1.75, 2.375],
                    scale,
                };
                let mut fast = RgbaImage::from_fn(233, 157, |x, y| {
                    Rgba([x as u8, y as u8, 71, (x * 13 % 256) as u8])
                });
                let mut reference = fast.clone();
                super::render_impl(&mut fast, &a, view, true);
                super::render_impl(&mut reference, &a, view, false);
                assert_eq!(
                    fast, reference,
                    "start {start:?}, scale {scale}, alpha {alpha}"
                );
            }
        }
    }
}
#[test]
fn opaque_shaped_sampling_matches_general_path_at_fractional_origins() {
    let source = Arc::new(RgbaImage::from_fn(90, 70, |x, y| {
        Rgba([x as u8, y as u8, 120, 255])
    }));
    for kind in [crate::Kind::Ellipse, crate::Kind::Polygon] {
        let frame = crate::video::frame::Frame {
            shape: kind,
            ..Default::default()
        };
        let a = frame
            .annotation(source.clone(), [0.11, 0.13, 0.6, 0.7], [160, 100], 1)
            .unwrap();
        let view = Viewport {
            size: [160, 100],
            origin: [0., 0.],
            scale: 1.,
        };
        let mut fast = RgbaImage::new(160, 100);
        let mut expected = fast.clone();
        super::render_impl(&mut fast, &a, view, true);
        super::render_impl(&mut expected, &a, view, false);
        assert!(
            fast.as_raw()
                .iter()
                .zip(expected.as_raw())
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
    }
}

#[test]
fn perspective_tilts_image_objects_and_lays_them_flat_again() {
    use crate::bitmap::{FLAT, Perspective};
    let pixels = image::RgbaImage::from_pixel(80, 60, image::Rgba([30, 120, 200, 255]));
    let bitmap =
        Arc::new(Bitmap::new(Arc::new(RasterImage::from_image(&pixels).unwrap())).unwrap());
    let flat = bitmap.clone().annotation(1);
    let quad = Perspective::TiltLeft.quad(0.8, 0.6);
    let tilted_bitmap = bitmap.with_perspective(Some(quad)).unwrap();
    assert_eq!(tilted_bitmap.perspective(), Some(quad));
    let mut tilted = flat.clone();
    tilted.image = Some(Arc::new(tilted_bitmap));
    let mut a = image::RgbaImage::new(80, 60);
    let mut b = image::RgbaImage::new(80, 60);
    crate::render_annotation(&mut a, &flat);
    crate::render_annotation(&mut b, &tilted);
    assert_eq!(a.get_pixel(1, 1)[3], 255);
    assert_eq!(b.get_pixel(1, 1)[3], 0, "the receding corner leaves space");
    assert_eq!(b.get_pixel(70, 30)[3], 255, "the near edge stays");
    // Flat corners mean no perspective; crossed corners are refused.
    assert_eq!(
        bitmap.with_perspective(Some(FLAT)).unwrap().perspective(),
        None
    );
    assert!(
        bitmap
            .with_perspective(Some([[0., 0.], [1., 1.], [1., 0.], [0., 1.]]))
            .is_err()
    );
    // It saves and reopens with the image.
    let json = serde_json::to_string(&tilted).unwrap();
    let back: crate::Annotation = serde_json::from_str(&json).unwrap();
    assert_eq!(back.image.unwrap().perspective(), Some(quad));
}

#[test]
fn svg_image_objects_stay_sharp_when_enlarged_and_reopen() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect x="0" y="0" width="32" height="64" fill="#ff0000"/></svg>"##;
    let bitmap = Arc::new(Bitmap::from_svg(svg).unwrap());
    let mut a = bitmap.clone().annotation(1);
    a.end = [640., 640.]; // ten times larger
    let mut out = image::RgbaImage::new(640, 640);
    crate::render_annotation(&mut out, &a);
    // The edge between red and transparent is one pixel wide, not a blur.
    assert_eq!(out.get_pixel(318, 300).0, [255, 0, 0, 255]);
    assert_eq!(out.get_pixel(321, 300)[3], 0);
    let json = serde_json::to_string(&a).unwrap();
    let back: crate::Annotation = serde_json::from_str(&json).unwrap();
    assert!(back.image.as_ref().unwrap().is_svg());
    let tinted = bitmap.with_tint(Some([0, 0, 255, 255])).unwrap();
    let mut b = a.clone();
    b.image = Some(Arc::new(tinted));
    let mut out = image::RgbaImage::new(640, 640);
    crate::render_annotation(&mut out, &b);
    assert_eq!(out.get_pixel(100, 300).0, [0, 0, 255, 255]);
}
