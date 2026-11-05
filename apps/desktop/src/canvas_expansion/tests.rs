use super::*;
use egui::{Event, Modifiers, PointerButton};
fn frame(ctx: &egui::Context, e: &mut Editor, mut events: Vec<Event>, focused: bool) -> Transform {
    if e.needs_render {
        e.refresh_canvas_display(ctx);
    }
    let mut viewport = egui::Rect::NOTHING;
    let modifiers = events
        .iter()
        .find_map(|event| match event {
            Event::PointerButton { modifiers, .. } | Event::Key { modifiers, .. } => {
                Some(*modifiers)
            }
            _ => None,
        })
        .unwrap_or_else(|| ctx.input(|i| i.modifiers));
    events.insert(0, Event::ModifiersChanged(modifiers));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            focused,
            ..Default::default()
        },
        |ui| {
            viewport = ui.available_rect_before_wrap();
            e.show_canvas(ui);
        },
    );
    output.textures_delta.clear();
    e.view.transform(
        viewport,
        egui::vec2(
            e.session.document.width as f32,
            e.session.document.height as f32,
        ),
        ctx.pixels_per_point(),
    )
}
fn button(pos: egui::Pos2, pressed: bool, alt: bool) -> Event {
    Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers {
            alt,
            ..Default::default()
        },
    }
}
fn fixture(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    e.session.document.annotations.push(
        capture_core::steps::marker(
            1,
            [100.0, 100.0],
            44,
            [220, 50, 50, 255],
            (8, false),
            [640, 400],
        )
        .unwrap(),
    );
    e.session.document.annotations.push(
        capture_core::steps::marker(
            2,
            [190.0, 100.0],
            44,
            [30, 60, 210, 255],
            (9, false),
            [640, 400],
        )
        .unwrap(),
    );
    e.tool = Some(crate::Kind::Step);
    e.selected = Some(1);
    e.quick_controls.set_snapping(false);
    e.needs_render = true;
    e
}

#[test]
fn screenshot_objects_track_the_pointer_at_fractional_zoom_and_dpi_without_raster_rebuilds() {
    let pixels = image::RgbaImage::from_fn(1920, 1080, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 241) as u8, 180, 255])
    });
    for editable in [false, true] {
        let mut document = if editable {
            capture_core::Document::from_capture_image(pixels.clone()).unwrap()
        } else {
            capture_core::Document::from_image(pixels.clone()).unwrap()
        };
        document.annotations.push(
            capture_core::steps::marker(
                2,
                [960., 540.],
                64,
                [200, 60, 90, 255],
                (1, false),
                [1920, 1080],
            )
            .unwrap(),
        );
        for ppp in [1., 1.5] {
            for zoom in [0.35, 0.79, 0.97, 1., 1.25] {
                for selected in if editable { vec![1, 2] } else { vec![2] } {
                    let ctx = egui::Context::default();
                    ctx.set_pixels_per_point(ppp);
                    let mut e = crate::canvas::tests::editor(&ctx);
                    e.session = capture_core::Session::new(document.clone()).unwrap();
                    e.tool = None;
                    e.select_only(Some(selected));
                    e.quick_controls.set_snapping(false);
                    e.view.mode = crate::canvas_view::ZoomMode::Manual(zoom);
                    e.needs_render = true;
                    frame(&ctx, &mut e, vec![], true);
                    let t = frame(&ctx, &mut e, vec![], true);
                    // Start inside the screenshot away from the overlaid Step and
                    // its resize handles. Keep the click offset while dragging.
                    let point = if selected == 1 {
                        [800., 450.]
                    } else {
                        [960., 540.]
                    };
                    let anchor = document
                        .annotations
                        .iter()
                        .find(|a| a.id == selected)
                        .unwrap()
                        .start;
                    let offset = [point[0] - anchor[0], point[1] - anchor[1]];
                    let origin = t.screen_point(point);
                    let before = serde_json::to_vec(&e.session.document).unwrap();
                    frame(
                        &ctx,
                        &mut e,
                        vec![Event::PointerMoved(origin), button(origin, true, false)],
                        true,
                    );
                    for n in 1..=8 {
                        let pointer = origin + egui::vec2(n as f32 * 8.75, n as f32 * 3.25);
                        frame(&ctx, &mut e, vec![Event::PointerMoved(pointer)], true);
                        frame(&ctx, &mut e, vec![], true);
                        let plan = e.expanding_move_plan().unwrap();
                        let a = plan.annotations.iter().find(|a| a.id == selected).unwrap();
                        let actual =
                            t.screen_point([a.start[0] + offset[0], a.start[1] + offset[1]]);
                        assert!(
                            actual.distance(pointer) < 0.05,
                            "{zoom} zoom/{ppp} DPI: {actual:?} vs {pointer:?}"
                        );
                        assert!(
                            e.display.uses_retained_scene() && e.texture.is_none(),
                            "drag fell back to full CPU raster at {zoom}"
                        );
                        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
                    }
                    let end = origin + egui::vec2(70., 26.);
                    frame(&ctx, &mut e, vec![button(end, false, false)], true);
                    e.execute(Command::Undo);
                    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
                }
            }
        }
    }
}
#[test]
fn live_drag_expands_all_edges_and_commits_one_undo_without_screen_jump() {
    for end in [
        [-40.0, 100.0],
        [100.0, -40.0],
        [710.0, 100.0],
        [100.0, 470.0],
        [-40.0, -40.0],
    ] {
        let ctx = egui::Context::default();
        let mut e = fixture(&ctx);
        let original = serde_json::to_vec(&e.session.document).unwrap();
        let t = frame(&ctx, &mut e, vec![], true);
        let p = t.screen_point([100.0, 100.0]);
        let q = t.screen_point(end);
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(p), button(p, true, false)],
            true,
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(q)], true);
        frame(&ctx, &mut e, vec![], true);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert!(!e.session.can_undo());
        let plan = e.expanding_move_plan().unwrap();
        assert_ne!(plan.dimensions, [640, 400]);
        let checkpoint = e.preview_document();
        checkpoint.validate().unwrap();
        let display = e.expansion_transform(t);
        assert_eq!(
            display.screen_point(plan.offset.map(|x| x as f32)),
            t.image.min
        );
        let base = checkpoint.base_image().unwrap();
        assert!(base.pixels().any(|p| p.0 == [0, 0, 0, 0]));
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("held.json");
        checkpoint.save(&path).unwrap();
        assert_eq!(
            Document::load(&path).unwrap().annotations,
            checkpoint.annotations
        );
        let after = frame(&ctx, &mut e, vec![button(q, false, false)], true);
        let release_transform = e.view.current_transform().unwrap();
        assert_eq!(
            release_transform.image, after.image,
            "release must retain the committed boundary immediately"
        );
        let settled = frame(&ctx, &mut e, vec![], true);
        assert_eq!(
            release_transform.image, settled.image,
            "the next painted frame must not flash the old boundary"
        );
        assert_eq!(e.session.document.annotations, plan.annotations);
        assert_eq!(
            [e.session.document.width, e.session.document.height],
            plan.dimensions
        );
        assert!((after.screen_point(plan.offset.map(|x| x as f32)) - t.image.min).length() < 0.01);
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert!(!e.session.can_undo());
        e.execute(Command::Redo);
        assert_eq!(e.session.document.annotations, plan.annotations);
        // Drag back from the current padded coordinate system. Preview shrinks
        // live and release keeps the original screenshot at the same screen point.
        let grown = serde_json::to_vec(&e.session.document).unwrap();
        let t2 = frame(&ctx, &mut e, vec![], true);
        let base = e.session.document.auto_canvas_base.unwrap();
        let object = &e.session.document.annotations[0];
        let p = t2.screen_point([
            (object.start[0] + object.end[0]) * 0.5,
            (object.start[1] + object.end[1]) * 0.5,
        ]);
        let q = t2.screen_point([base[0] as f32 + 100.0, base[1] as f32 + 100.0]);
        let source_screen = t2.screen_point([base[0] as f32, base[1] as f32]);
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(p), button(p, true, false)],
            true,
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(q)], true);
        let shrink = e.expanding_move_plan().unwrap();
        assert_eq!(shrink.dimensions, [640, 400]);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), grown);
        let held = e.preview_document();
        held.validate().unwrap();
        assert_eq!([held.width, held.height], [640, 400]);
        let after = frame(&ctx, &mut e, vec![button(q, false, false)], true);
        assert_eq!(
            [e.session.document.width, e.session.document.height],
            [640, 400]
        );
        assert!(e.session.document.auto_canvas_base.is_none());
        assert!((after.image.min - source_screen).length() < 0.01);
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), grown);
        e.execute(Command::Redo);
        assert_eq!(
            [e.session.document.width, e.session.document.height],
            [640, 400]
        );
    }
}
#[test]
fn group_and_alt_duplicate_drag_expand_together_keep_selection_and_cancel_cleanly() {
    for duplicate in [false, true] {
        let ctx = egui::Context::default();
        let mut e = fixture(&ctx);
        e.extra_selected.insert(2);
        let original = serde_json::to_vec(&e.session.document).unwrap();
        let t = frame(&ctx, &mut e, vec![], true);
        let p = t.screen_point([100.0, 100.0]);
        let q = t.screen_point([-40.0, -25.0]);
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(p), button(p, true, duplicate)],
            true,
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(q)], true);
        let plan = e.expanding_move_plan().unwrap();
        assert!(e.group_drag.is_some());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        frame(&ctx, &mut e, vec![button(q, false, duplicate)], true);
        assert_eq!(e.session.document.annotations, plan.annotations);
        assert_eq!(e.selected_objects().len(), 2);
        assert_eq!(
            e.session.document.annotations.len(),
            if duplicate { 4 } else { 2 }
        );
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        // An unfocused held gesture never commits expansion or creates duplicates.
        e.select_only(Some(1));
        frame(&ctx, &mut e, vec![], true);
        let t = frame(&ctx, &mut e, vec![], true);
        let p = t.screen_point([100.0, 100.0]);
        let q = t.screen_point([-40.0, -25.0]);
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(p), button(p, true, duplicate)],
            true,
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(q)], true);
        frame(&ctx, &mut e, vec![button(q, false, duplicate)], false);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    }
}
#[test]
fn expanding_snaps_remain_live_and_oversize_moves_do_not_change_the_canvas() {
    let ctx = egui::Context::default();
    let e = fixture(&ctx);
    let r = move_with_snapping(
        &e.session.document,
        &e.session.document.annotations[..1],
        [88.0, -150.0],
        Some(6.0),
        false,
    )
    .unwrap();
    assert!(r.guides[0].is_some());
    assert_eq!(
        r.annotations[0].start[0],
        e.session.document.annotations[1].start[0]
    );
    assert!(r.annotations[0].start[1] < 0.0);
    assert!(
        move_with_snapping(
            &e.session.document,
            &e.session.document.annotations[..1],
            [1_000_000.0, 0.0],
            None,
            false
        )
        .is_err()
    );
    assert_eq!(
        [e.session.document.width, e.session.document.height],
        [640, 400]
    );
    assert!(!e.session.can_undo());
}

#[test]
fn held_new_step_expands_canvas_without_committing_until_release() {
    let ctx = egui::Context::default();
    let mut e = fixture(&ctx);
    e.session.document.annotations.clear();
    e.select_only(None);
    e.step_number = 8;
    let original = serde_json::to_vec(&e.session.document).unwrap();
    let t = frame(&ctx, &mut e, vec![], true);
    let p = t.screen_point([100.0, 100.0]);
    let q = t.screen_point([-35.0, -30.0]);
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerMoved(p), button(p, true, false)],
        true,
    );
    frame(&ctx, &mut e, vec![Event::PointerMoved(q)], true);
    let plan = e.expanding_move_plan().unwrap();
    assert_ne!(plan.dimensions, [640, 400]);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    assert_eq!(
        e.preview_document().annotations[0]
            .text
            .as_ref()
            .unwrap()
            .content,
        "8"
    );
    frame(&ctx, &mut e, vec![button(q, false, false)], true);
    assert_eq!(e.session.document.annotations, plan.annotations);
    assert_eq!(e.step_number, 9);
    assert_eq!(e.selected, Some(1));
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    assert!(!e.session.can_undo());
}

#[test]
fn finite_snap_candidates_follow_interaction_order_and_exclude_the_moving_object() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    for i in 1..=6 {
        let mut a = e.new_annotation(
            crate::Kind::Rectangle,
            [i as f32 * 80., 50.],
            [i as f32 * 80. + 30., 100.],
        );
        a.id = i;
        e.execute(Command::Add { annotation: a });
    }
    e.quick_controls
        .change_toolbar(capture_core::quick_preferences::Choice::SnappingLimit(
            Some(1),
        ))
        .unwrap();
    e.select_only(Some(1));
    e.select_only(Some(3));
    e.select_only(Some(6));
    assert_eq!(e.quick_controls.snap_recent.last(), Some(&6));
    let moving = e.session.document.annotations[5].clone();
    let delta = [-238., 0.]; // moving left edge is two pixels from object 3's left edge
    let limited = super::move_with_candidates(
        &e.session.document,
        std::slice::from_ref(&moving),
        delta,
        Some(6.),
        false,
        e.quick_controls.snap_candidates(),
    )
    .unwrap();
    assert_eq!(limited.annotations[0].start[0], 240.);
    e.select_only(Some(1));
    let excluded = super::move_with_candidates(
        &e.session.document,
        &[moving],
        delta,
        Some(6.),
        false,
        e.quick_controls.snap_candidates(),
    )
    .unwrap();
    assert_eq!(excluded.annotations[0].start[0], 242.);
}
