use super::repair_tests::{editor, press, render, render_modified};
use super::*;
use capture_core::video::{editing::Item, editor::Animation, links::Layer};
use std::collections::BTreeSet;
fn bounds(ctx: &egui::Context, id: u64) -> egui::Rect {
    ctx.data(|d| d.get_temp(egui::Id::unique(("timeline-block", id))))
        .unwrap()
}
#[test]
fn zoom_edges_trim_through_multiple_preview_frames_and_undo() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let mut z = capture_core::video::zoom::Zoom::new(1, 2000, 8000);
    z.scale = 2.;
    e.session.document.video.as_mut().unwrap().zooms.push(z);
    e.set_timeline_expanded(true);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let rect = ctx
        .read_response(egui::Id::unique(("zoom-block", 1u64)))
        .unwrap()
        .rect;
    let from = rect.right_center() - egui::vec2(2., 0.);
    render(&ctx, &mut e, press(from, true));
    let to = from - egui::vec2(80., 0.);
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    assert!(e.video.zoom_drag.is_some());
    let end = e.video.timing_preview.as_ref().unwrap().zooms[0].end;
    assert!(end < 8000);
    for _ in 0..5 {
        render(&ctx, &mut e, vec![]);
        assert_eq!(e.video.timing_preview.as_ref().unwrap().zooms[0].end, end);
    }
    render(&ctx, &mut e, press(to, false));
    assert_eq!(e.session.document.video.as_ref().unwrap().zooms[0].end, end);
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().zooms[0].end,
        8000
    );
}
#[test]
fn shift_drag_changes_only_track_and_group_drag_does_not_rescale() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    let track = p.add_track();
    let original = p.clone();
    e.set_timeline_expanded(true);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let from = bounds(&ctx, 1).center();
    let rows = original.layer_index(Layer::Annotation(track)) as f32
        - original.layer_index(Layer::Annotation(original.objects[&1].track)) as f32;
    let to = from + egui::vec2(120., rows * 39.);
    render(&ctx, &mut e, press(from, true));
    render_modified(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(to)],
        egui::Modifiers::SHIFT,
    );
    render_modified(&ctx, &mut e, press(to, false), egui::Modifiers::SHIFT);
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.objects[&1].track, track);
    assert_eq!(p.objects[&1].start, original.objects[&1].start);
    e.execute(Command::Undo);
    e.video.snap = false;
    e.video.selection = BTreeSet::from([Item::Annotation(4)]);
    render(&ctx, &mut e, vec![]);
    let rect = bounds(&ctx, 4);
    let from = rect.center();
    let to = from + egui::vec2(65., 0.);
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    let end = e.video.timing_preview.as_ref().unwrap().objects[&4].end;
    assert!(end > original.duration());
    for _ in 0..5 {
        render(&ctx, &mut e, vec![]);
        assert_eq!(
            e.video.timing_preview.as_ref().unwrap().objects[&4].end,
            end
        );
    }
    render(&ctx, &mut e, press(to, false));
}
#[test]
fn step_motion_inherits_edited_style_without_copying_timing_and_one_undo() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let mut a = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.id == 1)
        .unwrap()
        .clone();
    a.color = [240, 40, 80, 255];
    a.end = [a.start[0] + 90., a.start[1] + 90.];
    e.execute(Command::Update {
        annotation: a.clone(),
    });
    let mut p = e.session.document.video.clone().unwrap();
    let t = p.objects.get_mut(&1).unwrap();
    t.entrance = Animation::Float;
    t.exit = Animation::Fade;
    t.entrance_ms = 700;
    t.exit_ms = 600;
    t.direction = 3;
    t.paired = false;
    e.commit_video(p);
    let before = e.session.document.clone();
    e.restore_drawing_defaults(Some(crate::Kind::Step));
    let mut next = e.step_marker([245., 245.]).unwrap();
    next.id = 99;
    e.execute(Command::MoveAndExpand {
        annotations: vec![next],
        duplicate: true,
    });
    assert!(
        e.session
            .document
            .video
            .as_ref()
            .unwrap()
            .objects
            .contains_key(&99),
        "{}",
        e.status
    );
    let t = &e.session.document.video.as_ref().unwrap().objects[&99];
    assert_eq!(
        (t.entrance, t.exit, t.entrance_ms, t.direction),
        (Animation::Float, Animation::Fade, 700, 3)
    );
    assert_eq!(t.start, 4500);
    assert_eq!(
        e.session.document.annotations.last().unwrap().color,
        a.color
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video, before.video);
    assert_eq!(e.session.document.annotations, before.annotations);
    e.execute(Command::Redo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&99].entrance,
        Animation::Float
    );
}
#[test]
fn muted_glyph_and_small_fade_cursor_are_visible() {
    let ctx = egui::Context::default();
    let out = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(egui::pos2(50., 50.))],
            ..Default::default()
        },
        |ui| {
            ui::icon(
                ui.painter(),
                egui::Rect::from_min_size(egui::pos2(100., 100.), egui::vec2(20., 20.)),
                "Muted",
                egui::Color32::WHITE,
            );
            timeline::fade_cursor(ui, false);
        },
    ));
    assert!(
        out.0
            .shapes
            .iter()
            .filter(|s| matches!(
                &s.shape,
                egui::Shape::LineSegment { .. } | egui::Shape::Path(_)
            ))
            .count()
            >= 4
    );
    let cursor = out
        .0
        .shapes
        .iter()
        .filter_map(|s| {
            if let egui::Shape::Path(p) = &s.shape {
                (p.fill.a() > 0).then_some(p.visual_bounding_rect())
            } else {
                None
            }
        })
        .next()
        .unwrap();
    assert!(cursor.width() < 16. && cursor.height() < 16.);
}

#[test]
fn playhead_snapping_toggle_and_group_backtracking_keep_exact_times() {
    for snap in [true, false] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.video.snap = snap;
        e.set_timeline_expanded(true);
        render(&ctx, &mut e, vec![]);
        render(&ctx, &mut e, vec![]);
        let r = bounds(&ctx, 3);
        let scale = r.width() / 6000.;
        let from = r.center();
        let to = from + egui::vec2(1470. * scale, 0.);
        render(&ctx, &mut e, press(from, true));
        render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
        assert_eq!(
            e.video.timing_preview.as_ref().unwrap().objects[&3].start,
            if snap { 4500 } else { 4470 }
        );
        render(&ctx, &mut e, press(to, false));
    }
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.video.snap = false;
    e.video.selection = BTreeSet::from([Item::Annotation(2), Item::Annotation(4)]);
    e.set_timeline_expanded(true);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let r = bounds(&ctx, 2);
    let scale = r.width() / 4000.;
    let from = r.center();
    render(&ctx, &mut e, press(from, true));
    for delta in [800, 400, 1200, 300, 1900, 500] {
        let to = from + egui::vec2(delta as f32 * scale, 0.);
        render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
        for _ in 0..3 {
            render(&ctx, &mut e, vec![]);
            let p = e.video.timing_preview.as_ref().unwrap();
            assert_eq!(
                (p.objects[&2].start, p.objects[&4].start),
                (4000 + delta, 9000 + delta)
            );
        }
    }
    render(
        &ctx,
        &mut e,
        press(from + egui::vec2(500. * scale, 0.), false),
    );
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&4].start,
        9500
    );
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&4].start,
        9000
    );
}

#[test]
fn still_dialog_and_muted_track_production_layout_block_canvas_input() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.set_timeline_expanded(true);
    let p = e.session.document.video.as_mut().unwrap();
    p.segments[0]
        .audio
        .push(capture_core::video::editor::Audio {
            path: "voice.wav".into(),
            label: "System audio".into(),
            volume: 1.,
            muted: false,
            fade_in: 0,
            fade_out: 0,
        });
    p.track_settings_mut(Layer::Media(1)).muted = true;
    p.zooms
        .push(capture_core::video::zoom::Zoom::new(1, 2000, 8000));
    for _ in 0..24 {
        render(&ctx, &mut e, vec![]);
    }
    let first = render(&ctx, &mut e, vec![]);
    let out = render(&ctx, &mut e, vec![]);
    if let Some(root) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
        crate::property_layout::tests::dump_meshes(
            &ctx,
            &root.to_string_lossy(),
            "flow-timeline",
            first,
            out,
        );
    }
    e.open_still_frame(1);
    for _ in 0..24 {
        render(&ctx, &mut e, vec![]);
    }
    let first = render(&ctx, &mut e, vec![]);
    let out = render(&ctx, &mut e, vec![]);
    if let Some(root) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
        crate::property_layout::tests::dump_meshes(
            &ctx,
            &root.to_string_lossy(),
            "flow-still-dialog",
            first,
            out,
        );
    }
    e.tool = Some(crate::Kind::Step);
    let before = e.session.document.annotations.clone();
    let at = e
        .view
        .current_transform()
        .unwrap()
        .screen_point([200., 180.]);
    render(&ctx, &mut e, press(at, true));
    render(&ctx, &mut e, press(at, false));
    assert_eq!(e.session.document.annotations, before);
}
#[cfg(windows)]
#[test]
fn still_frame_decodes_source_and_is_portable_undoable_media() {
    let root = tempfile::tempdir().unwrap();
    let _runtime = media::Runtime::new().unwrap();
    let path = root.path().join("video.mp4");
    let mut encoder = media::Encoder::new(&path, [320, 180], 30, false).unwrap();
    for i in 0..60 {
        let pixels: Vec<u8> = (0..320 * 180)
            .flat_map(|_| [0, 0, if i < 30 { 60 } else { 220 }, 255])
            .collect();
        encoder.video(i * 10_000_000 / 30, &pixels).unwrap();
    }
    encoder.finish().unwrap();
    let doc = workers::load(&path).unwrap();
    let p = doc.video.as_ref().unwrap();
    let hash = std::fs::read(&path).unwrap();
    let still = still_frame::generate_frame(p, 1, 1200, root.path()).unwrap();
    assert!(image::open(&still).unwrap().to_rgba8().get_pixel(100, 100)[0] > 200);
    let next = p
        .insert_still(1, 1200, 3000, still.to_string_lossy().into_owned(), false)
        .unwrap();
    let mut session = Session::new(doc.clone()).unwrap();
    session.execute(Command::Video { project: next }).unwrap();
    let mut visuals = media::Visuals::new();
    let frame = visuals
        .frame(session.document.video.as_ref().unwrap(), [320, 180], 3000)
        .unwrap();
    assert!(frame.get_pixel(100, 100)[0] > 200);
    let export = root.path().join("freeze.vclip");
    session.document.save(&export).unwrap();
    let reopened = capture_core::project_file::open(&export).unwrap();
    let portable_still = reopened
        .video
        .as_ref()
        .unwrap()
        .segments
        .last()
        .unwrap()
        .source
        .as_ref()
        .unwrap();
    assert_ne!(Path::new(&portable_still.path), still);
    assert!(Path::new(&portable_still.path).is_file());
    let reopened_frame = media::Visuals::new()
        .frame(reopened.video.as_ref().unwrap(), [320, 180], 3000)
        .unwrap();
    assert_eq!(reopened_frame, frame);
    session.execute(Command::Undo).unwrap();
    assert_eq!(session.document.video, doc.video);
    assert_eq!(session.document.annotations, doc.annotations);
    assert_eq!(std::fs::read(path).unwrap(), hash);
}
