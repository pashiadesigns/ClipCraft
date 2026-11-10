use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use capture_core::{Document, Session, bitmap::Bitmap};
use std::sync::Arc;

#[test]
fn vector_icon_colors_are_visible_and_passive_without_image_crop_controls() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut a = capture_core::icon_library::builtin::find("builtin-five-color")
        .unwrap()
        .payload()
        .unwrap()
        .objects
        .remove(0);
    let original = a.clone();
    let frame = |a: &mut Annotation| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300., 500.),
                )),
                ..Default::default()
            },
            |ui| properties(ui, a),
        ))
    };
    let first = frame(&mut a);
    let output = frame(&mut a);
    for text in [
        "Vector icon",
        "Color 1",
        "Color 2",
        "Color 3",
        "Color 4",
        "Color 5",
    ] {
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, text).is_some());
    }
    assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Crop image").is_none());
    assert_eq!(a, original);
    if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_LIBRARY_DIR") {
        dump_meshes(&ctx, &dir, "vector-icon-properties", first, output);
    }
}

fn image() -> Annotation {
    let pixels =
        image::RgbaImage::from_fn(200, 140, |x, y| image::Rgba([x as u8, y as u8, 140, 255]));
    let bitmap = Bitmap::new(
        capture_core::clipboard_payload::ImagePayload::from_rgba(200, 140, pixels.into_raw())
            .unwrap()
            .image,
    )
    .unwrap();
    let mut a = Arc::new(bitmap).annotation(1);
    a.start = [100., 80.];
    a.end = [420., 304.];
    a.with_image_view(Some(ImageView::default().zoomed(2.).unwrap()))
        .unwrap()
}

#[test]
fn choose_image_stays_visible_at_viewport_edge_and_low_zoom_until_buttons_collide() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let template = capture_core::templates::build(
        "two-steps",
        "Guide",
        &[],
        capture_core::templates::Scale::Fill,
    )
    .unwrap();
    let mut a = template
        .annotations
        .iter()
        .find(|a| a.image.as_ref().is_some_and(|b| b.is_placeholder()))
        .unwrap()
        .clone();
    a.id = 1;
    a.start = [100., 80.];
    a.end = [300., 240.];
    let mut doc = Document::blank(600, 400).unwrap();
    doc.annotations.push(a);
    doc.promote_features();
    e.session = Session::new(doc).unwrap();
    for scale in [0.1, 0.25, 0.5, 1., 2., 8., 16.] {
        let t = crate::canvas_view::Transform {
            image: egui::Rect::from_min_size(
                egui::pos2(-100. * scale + 10., -80. * scale + 10.),
                egui::vec2(600., 400.) * scale,
            ),
            viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180., 160.)),
            scale,
            zoom: scale,
        };
        let buttons = e.placeholder_buttons(t);
        let expected = egui::Rect::from_center_size(
            t.screen_point([200., 160.]),
            egui::vec2(110., 28.) * scale,
        );
        assert_eq!(
            buttons.len(),
            usize::from(expected.intersects(t.viewport)),
            "{scale}"
        );
        if let Some((_, rect)) = buttons.first() {
            assert_eq!(*rect, expected);
        }
    }
    let t = crate::canvas_view::Transform {
        image: egui::Rect::from_min_size(egui::pos2(-50., 0.), egui::vec2(600., 400.)),
        viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(180., 400.)),
        scale: 1.,
        zoom: 1.,
    };
    assert_eq!(
        e.placeholder_buttons(t).len(),
        1,
        "partly obscured frame remains accessible"
    );
    let mut second = e.session.document.annotations[0].clone();
    second.id = 2;
    e.session.document.annotations.push(second);
    assert!(
        e.placeholder_buttons(t).is_empty(),
        "overlapping image actions hide together"
    );
    e.session.document.annotations[1].start[1] += 200.;
    e.session.document.annotations[1].end[1] += 200.;
    assert_eq!(
        e.placeholder_buttons(t).len(),
        2,
        "both actions reappear without a zoom-mode reset"
    );
    e.session.document.annotations.truncate(1);
    let t = crate::canvas_view::Transform {
        image: t.image.translate(egui::vec2(-130., 0.)),
        viewport: egui::Rect::from_min_max(egui::pos2(50., 0.), egui::pos2(180., 400.)),
        ..t
    };
    let out = PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(500., 500.),
            )),
            events: vec![egui::Event::PointerMoved(egui::pos2(20., 160.))],
            ..Default::default()
        },
        |ui| {
            assert!(
                !e.image_frame_buttons_hovered(ui, t),
                "clipped part must not intercept workspace clicks"
            );
            e.show_image_frame_buttons(ui, t);
        },
    ));
    let label = out
        .shapes
        .iter()
        .find(|s| matches!(&s.shape,egui::Shape::Text(text) if text.galley.text()=="Choose Image"))
        .unwrap();
    assert!(t.viewport.contains_rect(label.clip_rect));
}

#[test]
fn canvas_zoom_buttons_and_ctrl_pan_preserve_frame_and_undo_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let mut d = Document::blank(640, 400).unwrap();
    d.annotations.push(image());
    d.promote_features();
    e.session = Session::new(d).unwrap();
    e.selected = Some(1);
    e.tool = None;
    let render = |e: &mut Editor, mut events: Vec<egui::Event>, modifiers| {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let _ = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 400.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| e.show_canvas(ui),
        ));
    };
    let pointer = |pos, pressed, modifiers| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    render(&mut e, vec![], egui::Modifiers::NONE);
    render(&mut e, vec![], egui::Modifiers::NONE);
    let before = e.session.document.annotations[0].clone();
    let plus = ctx
        .data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique(("image-frame-button", 1usize)))
                .unwrap()
        })
        .center();
    render(
        &mut e,
        vec![
            egui::Event::PointerMoved(plus),
            pointer(plus, true, egui::Modifiers::NONE),
        ],
        egui::Modifiers::NONE,
    );
    render(
        &mut e,
        vec![pointer(plus, false, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    let after = &e.session.document.annotations[0];
    assert_eq!(after.start, before.start);
    assert_eq!(after.end, before.end);
    assert!(
        after.image.as_ref().unwrap().view().rect[2]
            < before.image.as_ref().unwrap().view().rect[2]
    );
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.annotations,
        std::slice::from_ref(&before)
    );
    assert!(!e.session.can_undo());
    let transform = e.view.current_transform().unwrap();
    let from = transform.screen_point([240., 180.]);
    let to = from + egui::vec2(24., 12.);
    let ctrl = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    render(
        &mut e,
        vec![egui::Event::PointerMoved(from), pointer(from, true, ctrl)],
        ctrl,
    );
    render(&mut e, vec![egui::Event::PointerMoved(to)], ctrl);
    let preview = e.selected_annotation().unwrap();
    assert_ne!(
        preview.image.as_ref().unwrap().view(),
        before.image.as_ref().unwrap().view()
    );
    assert_eq!(preview.start, before.start);
    assert_eq!(
        e.session.document.annotations,
        std::slice::from_ref(&before)
    );
    render(&mut e, vec![pointer(to, false, ctrl)], ctrl);
    assert_ne!(
        e.session.document.annotations,
        std::slice::from_ref(&before)
    );
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.annotations,
        std::slice::from_ref(&before)
    );
    assert!(!e.session.can_undo());
    let from = transform.screen_point([before.end[0], (before.start[1] + before.end[1]) * 0.5]);
    let to = from - egui::vec2(40., 0.);
    render(
        &mut e,
        vec![egui::Event::PointerMoved(from), pointer(from, true, ctrl)],
        ctrl,
    );
    render(&mut e, vec![egui::Event::PointerMoved(to)], ctrl);
    let preview = e.selected_annotation().unwrap();
    assert!(preview.end[0] < before.end[0]);
    assert_eq!(preview.start, before.start);
    assert!(Arc::ptr_eq(
        preview.image.as_ref().unwrap().pixels().unwrap(),
        before.image.as_ref().unwrap().pixels().unwrap()
    ));
    assert_eq!(
        e.session.document.annotations,
        std::slice::from_ref(&before)
    );
    render(&mut e, vec![pointer(to, false, ctrl)], ctrl);
    assert_eq!(e.session.document.annotations, [preview]);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, [before]);
    assert!(!e.session.can_undo());
}

#[test]
fn compact_image_properties_do_not_change_passive_values() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut a = image();
    a.corner_radius = 13.25;
    let original = a.clone();
    let mut render = || {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300., 550.),
                )),
                ..Default::default()
            },
            |ui| {
                ui.set_width(262.);
                property_layout::annotation_properties(ui, &mut a, [640, 400]);
            },
        ))
    };
    let first = render();
    let second = render();
    if let Ok(dir) = std::env::var("CLIPDESIGNER_IMAGE_FRAME_UI_DIR") {
        dump_meshes(&ctx, &dir, "image", first, second);
    }
    assert_eq!(a, original);
}

#[test]
fn crop_mode_edges_and_center_handle_pan_live_without_moving_frame() {
    for mode in ["left", "right", "top", "bottom", "pan", "center"] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut d = Document::blank(640, 400).unwrap();
        d.annotations.push(image());
        d.promote_features();
        e.session = Session::new(d).unwrap();
        e.selected = Some(1);
        e.tool = None;
        let render = |e: &mut Editor, events: Vec<egui::Event>| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(650., 450.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_canvas(ui),
            ))
        };
        render(&mut e, vec![]);
        render(&mut e, vec![]);
        let before = e.session.document.annotations[0].clone();
        let transform = e.view.current_transform().unwrap();
        if mode != "center" {
            ctx.data_mut(|d| d.insert_temp(crop_mode_key(), 1u64));
        }
        let (at, delta) = match mode {
            "left" => ([100., 192.], [24., 0.]),
            "right" => ([420., 192.], [-24., 0.]),
            "top" => ([260., 80.], [0., 24.]),
            "bottom" => ([260., 304.], [0., -24.]),
            "pan" => ([190., 140.], [24., 12.]),
            _ => ([260., 192.], [24., 12.]),
        };
        let from = transform.screen_point(at);
        let to = transform.screen_point([at[0] + delta[0], at[1] + delta[1]]);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        render(
            &mut e,
            vec![egui::Event::PointerMoved(from), button(from, true)],
        );
        render(&mut e, vec![egui::Event::PointerMoved(to)]);
        let preview = e.selected_annotation().unwrap();
        assert_ne!(
            preview.image.as_ref().unwrap().view(),
            before.image.as_ref().unwrap().view(),
            "{mode}"
        );
        if mode == "pan" || mode == "center" {
            assert_eq!(preview.start, before.start);
            assert_eq!(preview.end, before.end);
        } else {
            assert!(
                preview.end[0] - preview.start[0] < before.end[0] - before.start[0]
                    || preview.end[1] - preview.start[1] < before.end[1] - before.start[1]
            );
        }
        assert!(Arc::ptr_eq(
            preview.image.as_ref().unwrap().pixels().unwrap(),
            before.image.as_ref().unwrap().pixels().unwrap()
        ));
        assert_eq!(
            e.session.document.annotations,
            std::slice::from_ref(&before)
        );
        render(&mut e, vec![button(to, false)]);
        assert_eq!(e.session.document.annotations, [preview]);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, [before]);
        assert!(!e.session.can_undo());
        e.select_only(None);
        render(&mut e, vec![]);
        assert!(!crop_mode(&ctx, 1));
    }
}
