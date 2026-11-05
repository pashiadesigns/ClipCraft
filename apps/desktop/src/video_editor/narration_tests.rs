use super::*;
#[cfg(windows)]
#[test]
fn webcam_layers_frame_styles_preview_and_export_agree_and_sources_roundtrip() {
    use capture_core::video::{frame, links::Layer};
    let _runtime = media::Runtime::new().unwrap();
    let root = tempfile::tempdir().unwrap();
    let camera = root.path().join("camera.mp4");
    let mut encoder = media::Encoder::new(&camera, [320, 180], 30, false).unwrap();
    let pixels: Vec<_> = (0..320 * 180).flat_map(|_| [220, 90, 30, 255]).collect();
    for n in 0..30 {
        encoder.video(n * 10_000_000 / 30, &pixels).unwrap();
    }
    encoder.finish().unwrap();
    drop(encoder);
    let mut doc = workers::load(&camera).unwrap();
    let a = capture_core::steps::marker(
        1,
        [160., 90.],
        70,
        [230, 20, 30, 255],
        (1, false),
        [320, 180],
    )
    .unwrap();
    doc.annotations.push(a);
    doc.video.as_mut().unwrap().sync(&doc.annotations);
    let p = doc.video.as_mut().unwrap();
    p.segments[0].frame = Some(frame::Frame {
        shape: capture_core::Kind::Ellipse,
        ..Default::default()
    });
    p.segments[0].rect = [0.25, 0.1, 0.5, 0.8];
    p.cursor = 100;
    let mut visuals = media::Visuals::new();
    let layers = visuals
        .layers(doc.video.as_ref().unwrap(), [320, 180], 100)
        .unwrap();
    let before = frame::composite(&doc, &layers, 100).unwrap();
    let sources = visuals.sources(doc.video.as_ref().unwrap(), 100).unwrap();
    let mut live = image::RgbaImage::from_pixel(
        320,
        180,
        image::Rgba(doc.video.as_ref().unwrap().background),
    );
    for object in frame::source_objects(&doc, &sources, 100, false) {
        let rendered = capture_core::display::render_annotation_preview(
            &object,
            capture_core::display::Viewport {
                size: [320, 180],
                origin: [0., 0.],
                scale: 1.,
            },
        )
        .unwrap();
        image::imageops::overlay(&mut live, &rendered, 0, 0);
    }
    let error: f64 = live
        .as_raw()
        .iter()
        .zip(before.as_raw())
        .map(|(a, b)| (*a as f64 - *b as f64).abs())
        .sum::<f64>()
        / live.as_raw().len() as f64;
    assert!(
        error < 0.05,
        "Raw-source preview differs from export: {error}"
    );
    assert!(before.get_pixel(160, 90)[0] > 200);
    doc.video.as_mut().unwrap().move_layer(Layer::Media(1), -1);
    let expected = frame::composite(&doc, &layers, 100).unwrap();
    assert!(expected.get_pixel(160, 90)[2] > 190);
    let out = root.path().join("result.mp4");
    workers::export_sync(&doc, &out, &AtomicBool::new(false), |_| ()).unwrap();
    let mut video = media::Video::open(&out).unwrap();
    let rendered = video.frame(100).unwrap();
    for (x, y) in [(160, 90), (2, 2), (80, 18), (160, 35)] {
        let a = expected.get_pixel(x, y);
        let b = rendered.get_pixel(x, y);
        assert!(
            (0..3).all(|c| (a[c] as i32 - b[c] as i32).abs() < 22),
            "{x},{y}: {a:?} {b:?}"
        );
    }
    let file = root.path().join("Narration.vclip");
    doc.save(&file).unwrap();
    let reopened = workers::load(&file).unwrap();
    assert_eq!(
        reopened.video.as_ref().unwrap().layer_order,
        doc.video.as_ref().unwrap().layer_order
    );
    assert_eq!(
        reopened.video.as_ref().unwrap().segments[0].frame,
        doc.video.as_ref().unwrap().segments[0].frame
    );
    let copy = frame::composite(
        &reopened,
        &visuals
            .layers(reopened.video.as_ref().unwrap(), [320, 180], 100)
            .unwrap(),
        100,
    )
    .unwrap();
    assert_eq!(copy, expected);
    let encoded_before = std::fs::read(&camera).unwrap();
    doc.video.as_mut().unwrap().segments[0].hidden = true;
    let hidden = visuals
        .layers(doc.video.as_ref().unwrap(), [320, 180], 100)
        .unwrap();
    assert!(hidden.is_empty());
    assert_eq!(std::fs::read(&camera).unwrap(), encoded_before);
}

#[test]
fn frame_drag_and_crop_use_canvas_coordinates_one_undo_and_preserve_narration() {
    use capture_core::video::editor::*;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(tests::fixture()).unwrap();
    e.tool = None;
    let p = e.session.document.video.as_mut().unwrap();
    p.cursor = 0;
    p.objects.clear();
    e.session.document.annotations.clear();
    let p = e.session.document.video.as_mut().unwrap();
    p.segments[0].rect = [0.25, 0.25, 0.5, 0.5];
    p.segments[0].frame = Some(Default::default());
    p.segments[0].link = Some(1);
    let mut audio = p.segments[0].clone();
    audio.id = 2;
    audio.track = p.add_typed_track(TrackKind::Voiceover);
    audio.source = Some(Source {
        path: "narration.wav".into(),
        size: [1, 1],
        duration: 12000,
        kind: MediaKind::Audio,
    });
    audio.frame = None;
    p.segments.push(audio);
    let before = p.clone();
    let transform = crate::canvas_view::Transform {
        image: egui::Rect::from_min_size(egui::pos2(10., 10.), egui::vec2(480., 270.)),
        viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 500.)),
        scale: 0.5,
        zoom: 0.5,
    };
    let run = |e: &mut Editor, pos: egui::Pos2, button: Option<bool>| {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = button {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(transform.viewport),
                events,
                ..Default::default()
            },
            |ui| {
                e.video_frame_input(ui, transform, true);
            },
        );
        output.textures_delta.clear();
    };
    run(&mut e, egui::pos2(250., 145.), Some(true));
    run(&mut e, egui::pos2(270., 155.), None);
    run(&mut e, egui::pos2(270., 155.), Some(false));
    let after = e.session.document.video.as_ref().unwrap();
    assert!((after.segments[0].rect[0] - before.segments[0].rect[0] - 40. / 960.).abs() < 0.001);
    assert_eq!(after.segments[1], before.segments[1]);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video.as_ref().unwrap(), &before);
    e.video.frame_crop = true;
    e.session.document.video.as_mut().unwrap().segments[0]
        .frame
        .as_mut()
        .unwrap()
        .view
        .rect = [0.2, 0.2, 0.5, 0.5];
    let rect = e.session.document.video.as_ref().unwrap().segments[0].rect;
    run(&mut e, egui::pos2(250., 145.), Some(true));
    run(&mut e, egui::pos2(280., 145.), None);
    run(&mut e, egui::pos2(280., 145.), Some(false));
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.segments[0].rect, rect);
    assert!(p.segments[0].frame.as_ref().unwrap().view.rect[0] < 0.2);
}
