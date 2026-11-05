use super::*;
use crate::{
    Command, Document, Session,
    text::{TextFont, TextStyle},
};

fn annotation(kind: Kind) -> Annotation {
    Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,

        focus: None,

        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        mirror_x: false,
        id: 1,
        kind,
        start: [20.0, 20.0],
        end: [100.0, 80.0],
        width: 4,
        color: [40, 90, 160, 255],
        fill: None,
        corner_radius: 0.0,
        points: vec![],
        text: (kind == Kind::Step).then(TextStyle::default),
        callout: None,
    }
}
fn view(scale: f64) -> Viewport {
    Viewport {
        size: [(128.0 * scale) as u32, (112.0 * scale) as u32],
        origin: [0.0; 2],
        scale,
    }
}

#[test]
fn source_minification_filters_premultiplied_alpha_and_magnification_preserves_pixels() {
    let mut source = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 255, 0]));
    source.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    let list = Document::from_image(source.clone())
        .unwrap()
        .display_list()
        .unwrap();
    let small = list
        .render_viewport(Viewport {
            size: [1, 1],
            origin: [0.0; 2],
            scale: 0.5,
        })
        .unwrap();
    assert_eq!(small.get_pixel(0, 0).0, [255, 0, 0, 64]);
    let large = list
        .render_viewport(Viewport {
            size: [8, 8],
            origin: [0.0; 2],
            scale: 4.0,
        })
        .unwrap();
    for (x, y, p) in large.enumerate_pixels() {
        assert_eq!(p, source.get_pixel(x / 4, y / 4));
    }
}

#[test]
fn blur_uses_same_obscured_pixels_at_low_and_high_zoom_without_source_leaking_through() {
    let mut source_doc = Document::blank(320, 160).unwrap();
    let mut text = annotation(Kind::Text);
    text.start = [16.0, 32.0];
    text.end = [300.0, 112.0];
    text.color = [0, 0, 0, 255];
    text.text = Some(TextStyle {
        content: "SYNTHETIC 12345".into(),
        size_points: 26.0,
        ..Default::default()
    });
    source_doc.annotations.push(text);
    let screenshot = source_doc.render().unwrap();
    let mut doc = Document::from_image(screenshot).unwrap();
    let mut blur = annotation(Kind::Blur);
    blur.id = 2;
    blur.start = [0.0, 0.0];
    blur.end = [320.0, 160.0];
    blur.width = 12;
    doc.annotations.push(blur);
    let obscured = doc.render().unwrap();
    let flattened = Document::from_image(obscured.clone())
        .unwrap()
        .display_list()
        .unwrap();
    let editable = doc.display_list().unwrap();
    let original_json = serde_json::to_vec(&doc).unwrap();
    for scale in [0.1, 0.125, 0.25, 0.5, 0.75, 1.0, 2.0, 4.0] {
        for origin in [[0.0, 0.0], [3.25, 1.75]] {
            let viewport = Viewport {
                size: [(320.0 * scale) as u32, (160.0 * scale) as u32],
                scale,
                origin,
            };
            let actual = editable.render_viewport(viewport).unwrap();
            assert_eq!(
                actual,
                flattened.render_viewport(viewport).unwrap(),
                "effect must be exactly the obscured export at scale {scale} origin {origin:?}"
            );
            if let Ok(path) = std::env::var("CLIPCRAFT_BLUR_FIXTURE_DIR")
                && origin == [0.0, 0.0]
            {
                let directory = std::path::Path::new(&path);
                std::fs::create_dir_all(directory).unwrap();
                actual
                    .save(directory.join(format!("blur-{}pct.png", (scale * 100.0).round())))
                    .unwrap();
                obscured.save(directory.join("export.png")).unwrap();
            }
        }
    }
    assert_eq!(serde_json::to_vec(&doc).unwrap(), original_json);
}

#[test]
fn circle_edges_are_rasterized_at_zoom_not_enlarged_source_pixels() {
    let mut doc = Document::canvas(128, 112, [0; 4]).unwrap();
    let mut a = annotation(Kind::Step);
    a.start = [24.0, 16.0];
    a.end = [104.0, 96.0];
    doc.annotations.push(a);
    let list = doc.display_list().unwrap();
    for scale in [1.0, 2.0, 4.0] {
        let image = list.render_viewport(view(scale)).unwrap();
        let mut edge = 0;
        for (x, y, p) in image.enumerate_pixels() {
            let distance =
                ((f64::from(x) + 0.5) / scale - 64.0).hypot((f64::from(y) + 0.5) / scale - 56.0);
            if distance < 40.0 - 1.5 / scale {
                assert_eq!(p[3], 255);
            }
            if distance > 40.0 + 1.5 / scale {
                assert_eq!(p[3], 0);
            }
            if p[3] > 0 && p[3] < 255 {
                edge += 1;
            }
        }
        assert!(edge > 30, "smooth edge coverage at {scale}");
    }
    let actual = list.render_viewport(view(4.0)).unwrap();
    let enlarged = image::imageops::resize(
        &doc.render().unwrap(),
        512,
        448,
        image::imageops::FilterType::Nearest,
    );
    assert_ne!(actual, enlarged);
}

#[test]
fn raster_patches_replace_erased_alpha_and_keep_vector_stack_order() {
    let mut doc = Document::canvas(128, 112, [0; 4]).unwrap();
    let mut step = annotation(Kind::Step);
    step.color = [200, 60, 20, 180];
    doc.annotations.push(step.clone());
    let mut eraser = annotation(Kind::Eraser);
    eraser.id = 2;
    eraser.start = [30.0, 50.0];
    eraser.end = [90.0, 50.0];
    eraser.points = vec![eraser.start, eraser.end];
    eraser.width = 20;
    eraser.color = [0, 0, 0, 255];
    doc.annotations.push(eraser);
    let list = doc.display_list().unwrap();
    let actual = list.render_viewport(view(4.0)).unwrap();
    let native = doc.render().unwrap();
    let Layer::ReplacePixels { origin, pixels } = &list.layers[1] else {
        panic!()
    };
    for y in origin[1] * 4..(origin[1] + pixels.height()) * 4 {
        for x in origin[0] * 4..(origin[0] + pixels.width()) * 4 {
            assert_eq!(actual.get_pixel(x, y), native.get_pixel(x / 4, y / 4));
        }
    }
    assert_eq!(actual.get_pixel(240, 200)[3], 0);
    step.id = 3;
    step.start = [50.0, 40.0];
    step.end = [70.0, 60.0];
    step.color = [20, 200, 80, 255];
    doc.annotations.push(step);
    assert_eq!(
        doc.display_list()
            .unwrap()
            .render_viewport(view(4.0))
            .unwrap()
            .get_pixel(240, 200)
            .0,
        [20, 200, 80, 255]
    );
}

#[test]
fn panning_cropping_dpi_and_invalid_viewports_do_not_change_the_document() {
    let mut doc = Document::canvas(128, 112, [0; 4]).unwrap();
    let mut a = annotation(Kind::Callout);
    a.callout = Some(crate::callout::CalloutStyle::for_box(
        a.start,
        a.end,
        [128, 112],
    ));
    a.text = Some(TextStyle {
        content: "Hi".into(),
        ..Default::default()
    });
    a.corner_radius = 12.0;
    doc.annotations.push(a);
    let bytes = serde_json::to_vec(&doc).unwrap();
    let list = doc.display_list().unwrap();
    let full = list.render_viewport(view(4.0)).unwrap();
    let crop = list
        .render_viewport(Viewport {
            size: [240, 200],
            origin: [20.0, 15.0],
            scale: 4.0,
        })
        .unwrap();
    assert_eq!(
        image::imageops::crop_imm(&full, 80, 60, 240, 200).to_image(),
        crop
    );
    for invalid in [
        Viewport {
            size: [8192, 8192],
            ..view(1.0)
        },
        Viewport {
            scale: f64::NAN,
            ..view(1.0)
        },
        Viewport {
            origin: [f64::INFINITY, 0.0],
            ..view(1.0)
        },
        Viewport {
            size: [0, 1],
            ..view(1.0)
        },
    ] {
        assert!(list.render_viewport(invalid).is_err());
    }
    assert_eq!(serde_json::to_vec(&doc).unwrap(), bytes);
}

#[test]
fn shared_font_outlines_match_export_bounds_for_all_faces() {
    for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut doc = Document::canvas(128, 112, [0; 4]).unwrap();
            let mut a = annotation(Kind::Text);
            a.text = Some(TextStyle {
                content: "Ag 42\ntext".into(),
                font,
                bold,
                italic,
                size_points: 18.0,
                ..Default::default()
            });
            doc.annotations.push(a);
            let original = doc.render().unwrap();
            let zoomed = doc
                .display_list()
                .unwrap()
                .render_viewport(view(4.0))
                .unwrap();
            let bounds = |im: &RgbaImage, scale: f64| {
                let mut min = [f64::INFINITY; 2];
                let mut max = [0.0f64; 2];
                for (x, y, p) in im.enumerate_pixels().filter(|(_, _, p)| p[3] > 32) {
                    let _ = p;
                    for (axis, v) in [f64::from(x) / scale, f64::from(y) / scale]
                        .into_iter()
                        .enumerate()
                    {
                        min[axis] = min[axis].min(v);
                        max[axis] = max[axis].max(v);
                    }
                }
                [min[0], min[1], max[0], max[1]]
            };
            for (a, b) in bounds(&original, 1.0).into_iter().zip(bounds(&zoomed, 4.0)) {
                assert!(
                    (a - b).abs() <= 1.25,
                    "{font:?}/{bold}/{italic}: {a} vs {b}"
                );
            }
        }
    }
}

#[test]
fn each_vector_kind_survives_edit_undo_and_reopen_with_one_opacity_at_joins() {
    for kind in [
        Kind::Rectangle,
        Kind::Ellipse,
        Kind::Polygon,
        Kind::Callout,
        Kind::Text,
        Kind::Step,
        Kind::Line,
        Kind::Arrow,
        Kind::Pen,
        Kind::Highlighter,
        Kind::Redact,
    ] {
        let mut doc = Document::canvas(128, 112, [0; 4]).unwrap();
        let mut a = annotation(kind);
        a.color[3] = 128;
        if matches!(kind, Kind::Pen | Kind::Highlighter) {
            a.points = vec![a.start, [70.0, 30.0], a.end];
        }
        if kind == Kind::Polygon {
            a.points = crate::polygon::outline(a.start, a.end, 5);
        }
        if matches!(kind, Kind::Text | Kind::Callout | Kind::Step) {
            a.text = Some(TextStyle {
                content: "Aa".into(),
                ..Default::default()
            });
        }
        if kind == Kind::Callout {
            a.callout = Some(crate::callout::CalloutStyle::for_box(
                a.start,
                a.end,
                [128, 112],
            ));
        }
        doc.annotations.push(a.clone());
        let mut s = Session::new(doc).unwrap();
        let before = s
            .document
            .display_list()
            .unwrap()
            .render_viewport(view(4.0))
            .unwrap();
        assert!(before.pixels().any(|p| p[3] != 0), "{kind:?}");
        if matches!(
            kind,
            Kind::Line | Kind::Arrow | Kind::Pen | Kind::Highlighter
        ) {
            assert!(
                before.pixels().all(|p| p[3] <= 129),
                "opacity applied twice at {kind:?} join"
            );
        }
        let original_export = s.document.render().unwrap();
        a = a.translated([2.0, 3.0], [128, 112]).unwrap();
        s.execute(Command::Update { annotation: a }).unwrap();
        assert_ne!(
            s.document
                .display_list()
                .unwrap()
                .render_viewport(view(4.0))
                .unwrap(),
            before
        );
        s.execute(Command::Undo).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("vector.capture.json");
        s.save(&file).unwrap();
        let reopened = Document::load(&file).unwrap();
        assert_eq!(reopened.render().unwrap(), original_export);
        assert_eq!(
            reopened
                .display_list()
                .unwrap()
                .render_viewport(view(4.0))
                .unwrap(),
            before
        );
    }
}
#[test]
fn creation_overlay_uses_identical_vector_geometry_as_committed_display() {
    let fixture: crate::Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    for mut annotation in fixture.annotations.into_iter().filter(|a| {
        !matches!(
            a.kind,
            crate::Kind::Blur | crate::Kind::Pixelate | crate::Kind::Eraser
        )
    }) {
        if annotation.kind != crate::Kind::Step
            && let Some(text) = &mut annotation.text
        {
            text.content.clear();
        }
        for scale in [0.79, 1.0, 2.0] {
            let viewport = super::Viewport {
                size: [(640.0 * scale) as u32, (400.0 * scale) as u32],
                origin: [0.0, 0.0],
                scale,
            };
            let overlay = super::render_annotation_preview(&annotation, viewport).unwrap();
            let mut document = crate::Document::canvas(640, 400, [0; 4]).unwrap();
            document.annotations = vec![annotation.clone()];
            assert_eq!(
                overlay,
                document
                    .display_list()
                    .unwrap()
                    .render_viewport(viewport)
                    .unwrap(),
                "{:?} at{scale}",
                annotation.kind
            );
        }
    }
}

#[test]
fn native_export_matches_viewport_with_antialiased_steps_and_joined_callout_tails() {
    let fixture: Document =
        serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json")).unwrap();
    let mut doc = Document::from_image(RgbaImage::new(fixture.width, fixture.height)).unwrap();
    doc.annotations = fixture.annotations.clone();
    let before = serde_json::to_vec(&doc).unwrap();
    let viewport = Viewport {
        size: [doc.width, doc.height],
        origin: [0.0; 2],
        scale: 1.0,
    };
    assert_eq!(
        doc.render().unwrap(),
        doc.display_list()
            .unwrap()
            .render_viewport(viewport)
            .unwrap()
    );
    assert_eq!(serde_json::to_vec(&doc).unwrap(), before);
    // Pixel-reading effects and conservative opaque redaction remain exact too.
    let mut effect = annotation(Kind::Redact);
    effect.id = 100;
    effect.start = [30.25, 32.75];
    effect.end = [85.5, 75.25];
    doc.annotations.push(effect.clone());
    assert_eq!(
        doc.render().unwrap(),
        doc.display_list()
            .unwrap()
            .render_viewport(viewport)
            .unwrap()
    );
    let upright = doc.render().unwrap();
    for degrees in [-45.0, 37.0, 90.0] {
        doc.rotate_annotation(100, degrees).unwrap();
        assert_eq!(
            doc.render().unwrap(),
            doc.display_list()
                .unwrap()
                .render_viewport(viewport)
                .unwrap()
        );
        assert_ne!(doc.render().unwrap(), upright);
    }
    doc.rotate_annotation(100, 0.0).unwrap();
    effect.id = 101;
    effect.kind = Kind::Blur;
    effect.width = 5;
    doc.annotations.push(effect);
    assert_eq!(
        doc.render().unwrap(),
        doc.display_list()
            .unwrap()
            .render_viewport(viewport)
            .unwrap()
    );
    for (kind, tip) in [
        (Kind::Step, [0.0; 2]),
        (Kind::Callout, [60.25, 5.5]),
        (Kind::Callout, [120.5, 55.25]),
        (Kind::Callout, [70.25, 104.5]),
        (Kind::Callout, [4.5, 44.25]),
    ] {
        let mut a = annotation(kind);
        a.corner_radius = if kind == Kind::Callout { 12.0 } else { 0.0 };
        a.start = [20.25, 20.5];
        a.end = [100.25, 80.5];
        a.text = Some(TextStyle {
            content: "1".into(),
            color: Some([0; 4]),
            ..Default::default()
        });
        if kind == Kind::Callout {
            a.callout = Some(crate::callout::CalloutStyle {
                fill: [80, 140, 200, 128],
                outline: [40, 90, 160, 255],
                pointer: tip,
                padding: 8.0,
            });
        }
        let mut d = Document::from_image(RgbaImage::new(128, 112)).unwrap();
        d.annotations.push(a.clone());
        let output = d.render().unwrap();
        assert_eq!(
            output,
            d.display_list()
                .unwrap()
                .render_viewport(view(1.0))
                .unwrap()
        );
        let edge_count = output.pixels().filter(|p| p[3] > 0 && p[3] < 127).count();
        assert!(
            edge_count > 20,
            "{kind:?} {tip:?} requires partial edge coverage: {edge_count}"
        );
        if kind == Kind::Callout {
            // Pointer lands at the requested point, with no body/tail seam.
            assert!(output.get_pixel(tip[0].floor() as u32, tip[1].floor() as u32)[3] > 0);
            assert_eq!(output.get_pixel(60, 40).0, [80, 140, 200, 128]);
        }
        // Pixel-center weighted centroid stays put when zooming the geometry.
        let high = d
            .display_list()
            .unwrap()
            .render_viewport(view(4.0))
            .unwrap();
        let centroid = |im: &RgbaImage, scale: f64| {
            let mut sum = [0.0; 3];
            for (x, y, p) in im.enumerate_pixels() {
                sum[0] += (f64::from(x) + 0.5) / scale * f64::from(p[3]);
                sum[1] += (f64::from(y) + 0.5) / scale * f64::from(p[3]);
                sum[2] += f64::from(p[3]);
            }
            [sum[0] / sum[2], sum[1] / sum[2]]
        };
        let low = centroid(&output, 1.0);
        let hi = centroid(&high, 4.0);
        assert!(
            (low[0] - hi[0]).abs() < 0.12 && (low[1] - hi[1]).abs() < 0.12,
            "{kind:?} centroid {low:?} vs {hi:?}"
        );
    }
}
#[test]
fn uniform_base_optimization_matches_sampled_canvas_with_alpha_and_clipping() {
    for color in [
        [255; 4],
        [33, 89, 147, 0],
        [33, 89, 147, 93],
        [33, 89, 147, 255],
    ] {
        let doc = crate::Document::canvas(83, 59, color).unwrap();
        for scale in [0.79, 0.97, 1., 1.25, 4.] {
            for origin in [[0.; 2], [-1.75, 3.25], [45.5, 32.25]] {
                let mut list = doc.display_list().unwrap();
                let viewport = Viewport {
                    size: [101, 77],
                    scale,
                    origin,
                };
                let fast = list.render_viewport(viewport).unwrap();
                list.base_color = None;
                assert_eq!(fast, list.render_viewport(viewport).unwrap());
            }
        }
    }
}
