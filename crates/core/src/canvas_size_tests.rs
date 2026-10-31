use crate::{Annotation, Command, Document, Kind, Placement, Session};
use image::{Rgba, RgbaImage};

fn source() -> RgbaImage {
    RgbaImage::from_fn(8, 6, |x, y| {
        Rgba([
            x as u8 * 20,
            y as u8 * 30,
            120,
            if x % 2 == 0 { 0 } else { 255 },
        ])
    })
}

#[test]
fn fractional_text_callouts_and_arrow_strokes_translate_without_raster_rounding_changes() {
    let original: Document =
        serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json")).unwrap();
    let pixels = original.render().unwrap();
    for placement in Placement::ALL {
        let mut session = Session::new(original.clone()).unwrap();
        session.execute(resize(740, 480, placement)).unwrap();
        let [dx, dy] = placement.offset([640, 400], [740, 480]);
        let mut expected = RgbaImage::new(740, 480);
        for (x, y, pixel) in pixels.enumerate_pixels() {
            expected.put_pixel(x + dx as u32, y + dy as u32, *pixel);
        }
        assert_eq!(
            session.document.render().unwrap(),
            expected,
            "{placement:?}"
        );
    }
}
fn resize(width: u32, height: u32, placement: Placement) -> Command {
    Command::ResizeCanvas {
        width,
        height,
        placement,
    }
}

#[test]
fn all_placements_expand_and_crop_exact_pixels_without_resampling_or_hidden_rgb_loss() {
    for placement in Placement::ALL {
        for (width, height) in [(13, 11), (5, 3), (13, 3), (5, 11)] {
            let pixels = source();
            let mut session = Session::new(Document::from_image(pixels.clone()).unwrap()).unwrap();
            session.execute(resize(width, height, placement)).unwrap();
            let rendered = session.document.render().unwrap();
            let index = Placement::ALL.iter().position(|p| *p == placement).unwrap();
            let align = |delta: i64, n: usize| match n {
                0 => 0,
                1 => (delta as f64 / 2.0).floor() as i64,
                _ => delta,
            };
            let dx = align(width as i64 - 8, index % 3);
            let dy = align(height as i64 - 6, index / 3);
            for (x, y, pixel) in rendered.enumerate_pixels() {
                let sx = x as i64 - dx;
                let sy = y as i64 - dy;
                let expected = if (0..8).contains(&sx) && (0..6).contains(&sy) {
                    *pixels.get_pixel(sx as u32, sy as u32)
                } else {
                    Rgba([0; 4])
                };
                assert_eq!(
                    *pixel, expected,
                    "{placement:?}, {width}x{height}, ({x},{y})"
                );
            }
            session.execute(Command::Undo).unwrap();
            assert_eq!(session.document.render().unwrap(), pixels);
            assert!(!session.is_modified());
            assert_eq!(session.document.version, 1);
            session.execute(Command::Redo).unwrap();
            assert_eq!(session.document.render().unwrap(), rendered);
            assert_eq!(session.document.version, 2);
        }
    }
}

#[test]
fn canvas_preserves_editable_styles_and_pointer_geometry_then_reverts_original_after_reopen() {
    let mut original =
        Document::from_image(RgbaImage::from_pixel(160, 100, Rgba([12, 33, 55, 180]))).unwrap();
    original.annotations.push(Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        analysis: None,
        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        id: 1,
        kind: Kind::Callout,
        start: [20.0, 20.0],
        end: [100.0, 60.0],
        width: 2,
        color: [230, 30, 80, 255],
        text: Some(crate::text::TextStyle {
            content: "Canvas".into(),
            ..Default::default()
        }),
        corner_radius: 0.0,
        fill: None,
        points: Vec::new(),
        callout: Some(crate::callout::CalloutStyle {
            pointer: [110.0, 80.0],
            fill: [200, 220, 240, 160],
            outline: [20, 40, 60, 255],
            padding: 4.0,
        }),
    });
    let mut session = Session::new(original.clone()).unwrap();
    session
        .execute(resize(200, 140, Placement::Center))
        .unwrap();
    let object = &session.document.annotations[0];
    assert_eq!(object.start, [40.0, 40.0]);
    assert_eq!(object.end, [120.0, 80.0]);
    assert_eq!(object.callout.as_ref().unwrap().pointer, [130.0, 100.0]);
    assert_eq!(object.text, original.annotations[0].text);
    assert_eq!(object.width, 2);
    let expanded = session.document.render().unwrap();
    let path = std::env::temp_dir().join(format!(
        "clipcraft-canvas-{}.capture.json",
        std::process::id()
    ));
    session.save(&path).unwrap();
    assert!(!session.is_modified());
    let mut reopened = Session::new(Document::load(&path).unwrap()).unwrap();
    assert_eq!(reopened.document.render().unwrap(), expanded);
    reopened
        .execute(Command::ResizeImage {
            width: 160,
            height: 100,
        })
        .unwrap();
    assert!(reopened.document.raster.is_some());
    assert_eq!(reopened.document.source_size, Some([160, 100]));
    reopened.execute(Command::RevertOriginal).unwrap();
    assert_eq!(
        reopened.document.render().unwrap(),
        original.base_image().unwrap()
    );
    assert!(!reopened.document.can_revert());
    reopened.execute(Command::Undo).unwrap();
    reopened.execute(Command::Undo).unwrap();
    assert_eq!(reopened.document.render().unwrap(), expanded);
    assert!(!reopened.is_modified());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn canvas_invalid_inputs_clipped_objects_and_noop_preserve_saved_state_and_redo() {
    let mut original = Document::from_image(source()).unwrap();
    original.annotations.push(Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        analysis: None,
        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        id: 1,
        kind: Kind::Arrow,
        start: [1.0, 1.0],
        end: [7.0, 5.0],
        width: 1,
        color: [255; 4],
        text: None,
        corner_radius: 0.0,
        fill: None,
        points: Vec::new(),
        callout: None,
    });
    let mut session = Session::new(original.clone()).unwrap();
    session.execute(resize(10, 10, Placement::TopLeft)).unwrap();
    session.execute(Command::Undo).unwrap();
    let serialized = serde_json::to_vec(&session.document).unwrap();
    for (w, h) in [(0, 6), (32769, 1), (5001, 5000), (2, 2)] {
        assert!(session.execute(resize(w, h, Placement::Center)).is_err());
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), serialized);
        assert!(session.can_redo());
        assert!(!session.can_undo());
        assert!(!session.is_modified());
    }
    session
        .execute(resize(8, 6, Placement::BottomRight))
        .unwrap();
    assert!(session.can_redo());
    let command: Command = serde_json::from_str(
        r#"{"command":"resize_canvas","width":10,"height":8,"placement":"bottom_right"}"#,
    )
    .unwrap();
    session.execute(command).unwrap();
    assert_eq!(session.document.annotations[0].start, [3.0, 3.0]);
    let mut invalid = session.document.clone();
    invalid.version = 1;
    assert!(invalid.validate().is_err());
    invalid = session.document.clone();
    std::sync::Arc::make_mut(invalid.raster.as_mut().unwrap()).width = 1;
    assert!(invalid.base_image().is_err());
}

#[test]
fn repeated_canvas_edits_use_current_pixels_while_history_shares_rasters() {
    let mut session = Session::new(Document::from_image(source()).unwrap()).unwrap();
    session.execute(resize(4, 4, Placement::TopLeft)).unwrap();
    let cropped = session.document.render().unwrap();
    let raster = session.document.raster.clone().unwrap();
    session
        .execute(Command::Add {
            annotation: Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,
                analysis: None,
                highlighter_rect: false,
                group: None,
                rotation_degrees: 0.0,
                id: 1,
                kind: Kind::Rectangle,
                start: [1.0, 1.0],
                end: [3.0, 3.0],
                width: 1,
                color: [255; 4],
                text: None,
                corner_radius: 0.0,
                fill: None,
                points: Vec::new(),
                callout: None,
            },
        })
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        session.undo.last().unwrap().raster.as_ref().unwrap(),
        &raster
    ));
    session.execute(Command::Undo).unwrap();
    session.execute(resize(8, 6, Placement::TopLeft)).unwrap();
    let expanded = session.document.render().unwrap();
    for (x, y, pixel) in expanded.enumerate_pixels() {
        assert_eq!(
            *pixel,
            if x < 4 && y < 4 {
                *cropped.get_pixel(x, y)
            } else {
                Rgba([0; 4])
            }
        );
    }
    session.execute(Command::RevertOriginal).unwrap();
    assert_eq!(session.document.render().unwrap(), source());
}
