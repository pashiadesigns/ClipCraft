use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use capture_core::video::{links::Layer, zoom::Zoom};

pub(super) fn editor(ctx: &egui::Context) -> Editor {
    crate::shell::configure(ctx);
    crate::text_layout::configure(ctx);
    let mut e = crate::canvas::tests::editor(ctx);
    e.session = Session::new(tests::fixture()).unwrap();
    e.tool = None;
    e.select_only(None);
    e.video.selected = Selection::Objects;
    let source = Arc::new(image::RgbaImage::from_fn(960, 540, |x, y| {
        image::Rgba(if (x / 80 + y / 60) % 2 == 0 {
            [40, 64, 73, 255]
        } else {
            [49, 81, 93, 255]
        })
    }));
    let bitmap = Arc::new(capture_core::bitmap::Bitmap::transient(source).unwrap());
    e.video.frame = Some((4500, Arc::new(image::RgbaImage::new(1, 1))));
    e.video.frame_layers = vec![(1, bitmap)];
    e
}
pub(super) fn press(pos: egui::Pos2, down: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: down,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
pub(super) fn render(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<egui::Event>,
) -> PaintedOutput {
    render_modified(ctx, e, events, egui::Modifiers::NONE)
}
pub(super) fn render_modified(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
) -> PaintedOutput {
    let mut events = events;
    events.insert(0, egui::Event::ModifiersChanged(modifiers));
    let mut output = PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 960.),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            crate::controls::prepare_dialog_keys(ui.ctx());
            e.show_shell(ui);
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(crate::shell::CANVAS)
                        .inner_margin(12),
                )
                .show(ui, |ui| e.show_workspace(ui));
            e.show_effects_workstation(ui.ctx());
            e.show_video_export(ui.ctx());
            e.show_still_frame(ui.ctx());
        },
    ));
    if std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE").is_some() {
        let accumulated = ctx.data_mut(|d| {
            let values = d
                .get_temp_mut_or_default::<Vec<(egui::TextureId, Vec<egui::epaint::ImageDelta>)>>(
                    egui::Id::unique("repair-textures"),
                );
            values.extend(
                output
                    .0
                    .textures_delta
                    .set
                    .iter()
                    .map(|(id, deltas)| (*id, deltas.to_vec())),
            );
            values.clone()
        });
        output.0.textures_delta.set.clear();
        for (id, deltas) in accumulated {
            output
                .0
                .textures_delta
                .set
                .entry(id)
                .or_default()
                .extend(deltas);
        }
    }
    output
}
#[test]
fn zoom_draw_redraw_properties_and_canvas_mapping_are_editable() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let t = e.view.current_transform().unwrap();
    let from = t.screen_point([210., 120.]);
    let to = t.screen_point([600., 340.]);
    e.start_zoom_insert();
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    render(&ctx, &mut e, press(to, false));
    assert_eq!(
        e.session.document.video.as_ref().unwrap().zooms.len(),
        1,
        "{}",
        e.status
    );
    let id = e.video.zoom_selected.unwrap();
    let before = e.session.document.video.as_ref().unwrap().zooms[0].clone();
    e.video.zoom_draw = true;
    e.video.zoom_from = None;
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, press(t.screen_point([500., 290.]), false));
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.zooms.len(), 1);
    assert_eq!(p.zooms[0].id, id);
    assert_ne!(p.zooms[0].scale, before.scale);
    e.video_seek(5500);
    let p = e.session.document.video.as_ref().unwrap();
    let a = &e.session.document.annotations[1];
    let v = p.layer_view(
        Layer::Annotation(p.objects[&a.id].track),
        [960, 540],
        p.cursor,
    );
    let point = [(a.start[0] + a.end[0]) / 2., (a.start[1] + a.end[1]) / 2.];
    assert_eq!(e.tool_hit(v.point(point), 2.), Some(a.id));
    let transform = e.video_layer_transform(t, Layer::Annotation(p.objects[&a.id].track));
    assert!(
        transform
            .screen_point(point)
            .distance(t.screen_point(v.point(point)))
            < 0.01
    );
    for tab in [PropertyTab::Style, PropertyTab::Timing, PropertyTab::Motion] {
        e.video.tab = tab;
        e.video.zoom_selected = Some(id);
        let first = render(&ctx, &mut e, vec![]);
        let final_frame = render(&ctx, &mut e, vec![]);
        if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
            dump_meshes(
                &ctx,
                &folder.to_string_lossy(),
                &format!("zoom-{}", tab.label()),
                first,
                final_frame,
            );
        }
    }
}
#[test]
fn selecting_and_reordering_track_headers_never_moves_objects() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.select_only(Some(2));
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let p = e.session.document.video.clone().unwrap();
    let layer = Layer::Annotation(p.tracks[0]);
    let header = ctx
        .read_response(egui::Id::unique(("track-header", layer)))
        .unwrap()
        .rect;
    render(&ctx, &mut e, press(header.center(), true));
    render(&ctx, &mut e, press(header.center(), false));
    assert_eq!(e.video.active_track, Some(layer));
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects,
        p.objects
    );
    assert_eq!(e.selected, Some(2));
    let grip = ctx
        .read_response(egui::Id::unique(("track-grip", layer)))
        .unwrap()
        .rect
        .center();
    render(&ctx, &mut e, press(grip, true));
    render(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(grip + egui::vec2(0., 45.))],
    );
    render(&ctx, &mut e, press(grip + egui::vec2(0., 45.), false));
    assert_ne!(
        e.session.document.video.as_ref().unwrap().layers(),
        p.layers()
    );
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects,
        p.objects
    );
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().layers(),
        p.layers()
    );
}
#[test]
fn zoom_properties_fit_narrow_panels_and_gain_feedback_is_symmetric() {
    for width in [240., 302., 420.] {
        for tab in [PropertyTab::Style, PropertyTab::Timing, PropertyTab::Motion] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.video.tab = tab;
            e.session
                .document
                .video
                .as_mut()
                .unwrap()
                .zooms
                .push(Zoom::new(1, 0, 12000));
            e.video.zoom_selected = Some(1);
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 720.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        e.video_properties_pane(ui);
                        assert!(
                            ui.min_rect().right() <= width + 1.,
                            "{width} {tab:?}: {:?}",
                            ui.min_rect()
                        );
                    },
                );
                output.textures_delta.clear();
            }
        }
    }
    assert_eq!(
        property_design::volume_color(0.5),
        property_design::volume_color(1.5)
    );
    assert_ne!(
        property_design::volume_color(1.),
        property_design::volume_color(2.)
    );
}
#[test]
fn video_canvas_accepts_icons_and_steps_over_existing_video() {
    for (icon, empty) in [(false, false), (true, false), (false, true), (true, true)] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        if empty {
            e.session.document.annotations.clear();
            e.session.document.video.as_mut().unwrap().sync(&[]);
            assert!(e.session.document.video.as_ref().unwrap().tracks.is_empty());
        }
        e.tool = if icon { None } else { Some(crate::Kind::Step) };
        e.icon_library.active = icon;
        render(&ctx, &mut e, vec![]);
        render(&ctx, &mut e, vec![]);
        let count = e.session.document.annotations.len();
        let pos = e
            .view
            .current_transform()
            .unwrap()
            .screen_point([830., 420.]);
        render(&ctx, &mut e, press(pos, true));
        render(&ctx, &mut e, press(pos, false));
        assert_eq!(
            e.session.document.annotations.len(),
            count + 1,
            "Icon={icon}: {}",
            e.status
        );
        let object = e.session.document.annotations.last().unwrap();
        assert_eq!(
            e.session.document.video.as_ref().unwrap().objects[&object.id].start,
            4500
        );
        assert_eq!(object.image.as_ref().is_some_and(|b| b.is_icon()), icon);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations.len(), count);
    }
}
#[cfg(windows)]
#[test]
fn zoom_video_export_uses_the_live_layer_transform_and_preserves_originals() {
    let _runtime = media::Runtime::new().unwrap();
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mp4");
    let mut encoder = media::Encoder::new(&source, [320, 180], 30, false).unwrap();
    let pixels: Vec<_> = (0..320 * 180)
        .flat_map(|i| {
            if i % 320 < 160 {
                [20, 30, 220, 255]
            } else {
                [220, 30, 20, 255]
            }
        })
        .collect();
    for i in 0..12 {
        encoder.video(i * 10_000_000 / 30, &pixels).unwrap();
    }
    encoder.finish().unwrap();
    drop(encoder);
    let mut doc = workers::load(&source).unwrap();
    let mut z = Zoom::new(1, 0, 400);
    z.entrance_ms = 0;
    z.exit_ms = 0;
    z.center = [0.25, 0.5];
    doc.video.as_mut().unwrap().zooms.push(z);
    let original = doc.video.as_ref().unwrap().segments.clone();
    let mut visual = media::Visuals::new();
    let p = doc.video.as_ref().unwrap();
    let layers = visual.layers(p, [320, 180], 100).unwrap();
    let expected = capture_core::video::frame::composite(&doc, &layers, 100).unwrap();
    let live_sources = visual.sources(p, 100).unwrap();
    let objects = capture_core::video::frame::source_objects(&doc, &live_sources, 100, false);
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].end[0] - objects[0].start[0], 640.);
    let export = root.path().join("zoom.mp4");
    workers::export_sync(&doc, &export, &AtomicBool::new(false), |_| ()).unwrap();
    let mut decoded = media::Video::open(&export).unwrap();
    let frame = decoded.frame(100).unwrap();
    for (x, y) in [(50, 50), (250, 80), (160, 130)] {
        let a = frame.get_pixel(x, y);
        let b = expected.get_pixel(x, y);
        assert!((0..3).all(|c| (a[c] as i32 - b[c] as i32).abs() < 22));
    }
    assert_eq!(doc.video.as_ref().unwrap().segments, original);
    let saved = root.path().join("zoom.vclip");
    doc.save(&saved).unwrap();
    let reopened = workers::load(&saved).unwrap();
    assert_eq!(
        reopened.video.as_ref().unwrap().zooms,
        doc.video.as_ref().unwrap().zooms
    );
}
