use super::*;

#[cfg(windows)]
#[test]
#[ignore = "Opt-in synthetic OCR accuracy comparison; no desktop or clipboard"]
fn synthetic_ocr_resolution_profile() {
    let Some(dir) = std::env::var_os("CLIPDESIGNER_OCR_ACCURACY_DIR") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut results = Vec::new();
    for name in ["light", "dark"] {
        let source = image::open(dir.join(format!("{name}.png")))
            .unwrap()
            .into_rgba8();
        for scale in [1, 2, 3] {
            let pixels = image::imageops::resize(
                &source,
                source.width() * scale,
                source.height() * scale,
                image::imageops::FilterType::CatmullRom,
            );
            let result = recognize(&pixels).unwrap();
            results.push(serde_json::json!({"fixture":name,"scale":scale,"text":result.text,"language":result.language}));
        }
    }
    std::fs::write(
        dir.join("results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn ocr_pixels_composite_transparency_and_resize_without_mutating_original() {
    let source = image::RgbaImage::from_pixel(8, 4, image::Rgba([0, 0, 0, 0]));
    let (pixels, scaled) = prepared(&source, 4).unwrap();
    assert!(scaled);
    assert_eq!(pixels.dimensions(), (4, 2));
    assert!(pixels.pixels().all(|p| p.0 == [255; 4]));
    assert_eq!(source.get_pixel(0, 0).0, [0; 4]);
    assert!(prepared(&source, 0).is_err());
    assert!(prepared(&image::RgbaImage::new(0, 0), 100).is_err());
    let (large_glyphs, reduced) = prepared(&source, 100).unwrap();
    assert!(!reduced);
    assert_eq!(large_glyphs.dimensions(), (16, 8));
    assert!(large_glyphs.pixels().all(|p| p.0 == [255; 4]));
    assert_eq!(prepared(&source, 12).unwrap().0.dimensions(), (8, 4));
}

#[cfg(windows)]
#[test]
#[ignore = "Opt-in synthetic small-font Windows OCR regression; no desktop or clipboard"]
fn native_ocr_small_ui_text_light_and_dark() {
    let dir = std::path::PathBuf::from(
        std::env::var_os("CLIPDESIGNER_OCR_ACCURACY_DIR").expect("Set fixture directory"),
    );
    let expected = std::fs::read_to_string(dir.join("expected.txt")).unwrap();
    let normalize = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    for name in ["light", "dark"] {
        let pixels = image::open(dir.join(format!("{name}.png")))
            .unwrap()
            .into_rgba8();
        let result = recognize(&pixels).unwrap();
        assert_eq!(normalize(&result.text), normalize(&expected), "{name}");
        std::fs::write(dir.join(format!("{name}-after.txt")), result.text).unwrap();
    }
}

#[test]
fn result_modal_preserves_lines_and_copies_text_without_canvas_clipboard_or_document_changes() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.all_styles_mut(|style| style.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    e.ocr.present(
        Ok(Recognized {
            text: "ClipDesigner local text\nSecond line 123".into(),
            language: "en-US".into(),
            scaled: false,
            words: vec![],
            spans: vec![],
        }),
        true,
        false,
    );
    assert!(!e.editor_shortcuts_idle());
    let render = |e: &mut Editor, events| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700., 520.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |_| e.show_ocr(&ctx),
        ))
    };
    let first = render(&mut e, vec![]);
    let second = render(&mut e, vec![]);
    if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_OCR_DIR") {
        dump_meshes(&ctx, &dir, "result", first, second);
    }
    let rect = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("ocr-copy"))
            .unwrap()
    });
    let pointer = |pressed| egui::Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    render(
        &mut e,
        vec![egui::Event::PointerMoved(rect.center()), pointer(true)],
    );
    let copied = render(&mut e, vec![pointer(false)]);
    assert!(copied.0.platform_output.commands.iter().any(|c|matches!(c,egui::OutputCommand::CopyText(t) if t=="ClipDesigner local text\nSecond line 123")));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    e.ocr
        .present(Err("Injected recognition failure".into()), false, true);
    let output = render(&mut e, vec![]);
    assert!(
        !output
            .0
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c, egui::OutputCommand::CopyText(_)))
    );
}

#[cfg(windows)]
#[test]
fn tilted_word_boxes_turn_back_about_the_center() {
    // Upright text needs nothing.
    assert_eq!(
        untilted([10., 20., 30., 8.], 0., [100., 100.]),
        [10., 20., 30., 8.]
    );
    // A quarter turn clockwise sends a box right of the center below it.
    let [x, y, w, h] = untilted([70., 48., 20., 4.], 90., [100., 100.]);
    assert!((x - 48.).abs() < 0.01 && (y - 70.).abs() < 0.01, "{x} {y}");
    assert!((w - 4.).abs() < 0.01 && (h - 20.).abs() < 0.01);
    // A small tilt grows the box a little so the whole word is covered.
    let [_, _, w, h] = untilted([300., 336., 119., 27.], 7.7, [1000., 700.]);
    assert!(w > 119. && h > 27. && h < 50.);
}

/// BW13: a page tilted by several degrees keeps its word positions, and an
/// email on it is found where the ink is.
#[test]
#[ignore = "Opt-in local Windows OCR model; synthetic pixels only, no desktop or clipboard"]
fn native_ocr_finds_words_on_a_tilted_page() {
    use capture_core::{Annotation, Document, Kind, text::TextStyle};
    let mut d = Document::blank(1200, 800).unwrap();
    d.annotations.push(Annotation {
        id: 1,
        kind: Kind::Text,
        start: [150., 300.],
        end: [1050., 420.],
        color: [20, 20, 20, 255],
        width: 1,
        corner_radius: 0.,
        fill: None,
        points: vec![],
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        focus: None,
        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.,
        mirror_x: false,
        callout: None,
        text: Some(TextStyle {
            content: "Contact dana.reyes@example.com today".into(),
            size_points: 40.,
            ..Default::default()
        }),
    });
    d.promote_features();
    let upright = d.render().unwrap();
    for degrees in [6f32, -9.] {
        let page =
            capture_core::image_transform::rotate_preview(&upright, degrees, false, [255; 4]);
        let result = recognize(&page).unwrap();
        let email = result
            .words
            .iter()
            .find(|w| w.text.contains("@example"))
            .unwrap_or_else(|| panic!("{degrees}°: {:?}", result.words));
        // The box covers dark ink of the tilted email.
        let [x, y, w, h] = email.bounds;
        let mut ink = 0;
        for py in y as u32..(y + h) as u32 {
            for px in x as u32..(x + w) as u32 {
                if page.get_pixel(px.min(page.width() - 1), py.min(page.height() - 1))[0] < 100 {
                    ink += 1;
                }
            }
        }
        assert!(
            ink > 200,
            "{degrees}°: {:?} covers {ink} ink pixels",
            email.bounds
        );
        let found = capture_core::privacy::detect(
            &result.words,
            &capture_core::privacy::Options::default(),
            [page.width(), page.height()],
        )
        .unwrap();
        assert!(
            found
                .iter()
                .any(|f| f.text.contains("dana.reyes@example.com")),
            "{degrees}°: {found:?}"
        );
    }
}

#[test]
#[ignore = "Opt-in local Windows OCR model; synthetic pixels only, no desktop or clipboard"]
fn native_ocr_recognizes_generated_fixture() {
    use capture_core::{Annotation, Document, Kind, text::TextStyle};
    let mut d = Document::blank(1000, 230).unwrap();
    d.annotations.push(Annotation {
        id: 1,
        kind: Kind::Text,
        start: [25., 25.],
        end: [975., 205.],
        color: [49, 83, 157, 255],
        width: 1,
        corner_radius: 0.,
        fill: None,
        points: vec![],
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,

        focus: None,

        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.,
        mirror_x: false,
        callout: None,
        text: Some(TextStyle {
            content: "ClipDesigner local text\nSecond line 123".into(),
            size_points: 48.,
            bold: true,
            ..Default::default()
        }),
    });
    d.promote_features();
    let image = d.render().unwrap();
    let result = recognize(&image).unwrap();
    assert!(
        !result.spans.is_empty(),
        "OCR word bounds should produce estimated formatting"
    );
    assert!(result.spans[0].appearance.style.bold);
    let color = result.spans[0].appearance.style.color.unwrap();
    assert!(
        (i16::from(color[2]) - 157).abs() < 16,
        "estimated color: {color:?}"
    );
    let text = result.text.to_lowercase();
    assert!(text.contains("clipdesigner local text"), "{}", result.text);
    assert!(text.contains("second line 123"), "{}", result.text);
    assert!(result.text.contains('\n'));
    let empty = recognize(&image::RgbaImage::from_pixel(80, 60, image::Rgba([255; 4]))).unwrap();
    assert!(empty.text.trim().is_empty());
    if let Some(path) = std::env::var_os("CLIPDESIGNER_OCR_EVIDENCE_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        image.save(path.join("source.png")).unwrap();
        std::fs::write(
            path.join("recognized.txt"),
            format!("Language: {}\n{}\n", result.language, result.text),
        )
        .unwrap();
    }
}

fn styled_result() -> Recognized {
    use capture_core::text::{TextFont, TextStyle, recognition::Appearance};
    let text = "Original raster stays intact. Objects remain\neditable.".to_string();
    Recognized {
        words: vec![],
        spans: vec![formatting::Span {
            range: 0..text.len(),
            appearance: Appearance {
                style: TextStyle {
                    font: TextFont::HackRegular,
                    size_points: 24.,
                    color: Some([49, 83, 157, 255]),
                    ..Default::default()
                },
                background: [255; 3],
            },
        }],
        text,
        language: "en-US".into(),
        scaled: false,
    }
}

#[test]
fn result_formatting_survives_unicode_edits_and_html_escapes_plain_text() {
    let mut r = styled_result();
    for value in [
        "Original é <script> & \"text\"\neditable.",
        "é\n",
        "",
        "New Ω text",
    ] {
        r.spans = r.edited_spans(value);
        r.text = value.into();
        let rich = r.job(value, false, 300.);
        let plain = r.job(value, true, 300.);
        assert_eq!(rich.text, value);
        assert_eq!(plain.text, value);
        assert!(!r.html().contains("<script>"));
        for s in &r.spans {
            assert!(r.text.is_char_boundary(s.range.start) && r.text.is_char_boundary(s.range.end));
        }
    }
}

#[test]
fn rich_result_toggle_copy_error_and_close_use_real_controls_without_os_clipboard() {
    for size in [egui::vec2(700., 520.), egui::vec2(460., 340.)] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.all_styles_mut(|s| s.animation_time = 0.);
        let mut state = State::default();
        state.present(Ok(styled_result()), true, false);
        let mut copied = Vec::new();
        let mut fail = false;
        let render = |state: &mut State, events, copied: &mut Vec<(String, String)>, fail: bool| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |_| {
                    state.show_with_copy(&ctx, &mut |html, text| {
                        if fail {
                            Err("Injected clipboard busy".into())
                        } else {
                            copied.push((html.into(), text.into()));
                            Ok(())
                        }
                    })
                },
            ))
        };
        let first = render(&mut state, vec![], &mut copied, fail);
        let second = render(&mut state, vec![], &mut copied, fail);
        if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_OCR_DIR") {
            dump_meshes(&ctx, &dir, &format!("formatted-{}", size.x), first, second);
        }
        let click = |state: &mut State, id, copied: &mut Vec<(String, String)>, fail| {
            let r = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(id)).unwrap());
            assert!(
                r.min.x >= 0. && r.max.x <= size.x && r.min.y >= 0. && r.max.y <= size.y,
                "{id}: {r:?} outside {size:?}"
            );
            let event = |pressed| egui::Event::PointerButton {
                pos: r.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            render(
                state,
                vec![egui::Event::PointerMoved(r.center()), event(true)],
                copied,
                fail,
            );
            render(state, vec![event(false)], copied, fail)
        };
        click(&mut state, "ocr-copy", &mut copied, fail);
        assert_eq!(copied.len(), 1);
        assert!(copied[0].0.contains("#31539d"));
        assert_eq!(copied[0].1, styled_result().text);
        click(&mut state, "ocr-plain", &mut copied, fail);
        assert!(state.plain);
        let output = click(&mut state, "ocr-copy", &mut copied, fail);
        assert_eq!(copied.len(), 1);
        assert!(
            output
                .0
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(t) if t==&styled_result().text))
        );
        click(&mut state, "ocr-plain", &mut copied, fail);
        fail = true;
        click(&mut state, "ocr-copy", &mut copied, fail);
        assert!(
            state
                .copy_status
                .as_ref()
                .unwrap()
                .contains("clipboard busy")
        );
        click(&mut state, "ocr-close", &mut copied, fail);
        assert!(!state.open);
    }
}

#[test]
fn canvas_ocr_uses_visible_composite_and_selected_bounds() {
    let ctx = egui::Context::default();
    let mut editor = crate::canvas::tests::editor(&ctx);
    let mut doc = capture_core::Document::canvas(80, 60, [210, 100, 40, 255]).unwrap();
    let mut redact = editor.new_annotation(crate::Kind::Redact, [20., 10.], [60., 50.]);
    redact.id = 1;
    redact.color = [0, 0, 0, 255];
    doc.annotations.push(redact);
    editor.session = capture_core::Session::new(doc.clone()).unwrap();
    editor.select_only(Some(1));
    let pixels = editor.ocr_pixels().unwrap();
    assert_eq!(pixels.dimensions(), (40, 40));
    assert!(pixels.pixels().all(|p| p.0 == [0, 0, 0, 255]));
    editor.select_only(None);
    assert_eq!(editor.ocr_pixels().unwrap(), doc.render().unwrap());
    assert_eq!(
        serde_json::to_value(&editor.session.document).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
}
