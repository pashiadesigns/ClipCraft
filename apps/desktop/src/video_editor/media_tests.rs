use super::*;
use capture_core::video::editor::{MediaKind, Segment, Source};

#[cfg(windows)]
#[test]
fn imported_media_crossfades_match_pixels_audio_export_and_portable_sources() {
    let root = tempfile::tempdir().unwrap();
    let _runtime = media::Runtime::new().unwrap();
    let red = root.path().join("red.mp4");
    let blue = root.path().join("blue.mp4");
    for (path, color) in [(&red, [0, 0, 240, 255]), (&blue, [240, 0, 0, 255])] {
        let mut e = media::Encoder::new(path, [320, 180], 30, false).unwrap();
        let pixels: Vec<u8> = (0..320 * 180).flat_map(|_| color).collect();
        for i in 0..30 {
            e.video(i * 10_000_000 / 30, &pixels).unwrap();
        }
        e.finish().unwrap();
    }
    let mut wav = capture_core::video::wav::Writer::new(root.path()).unwrap();
    wav.append(&vec![10000; 96000]).unwrap();
    let wav = wav.finish().unwrap();
    let image = root.path().join("green.png");
    image::RgbaImage::from_pixel(80, 80, image::Rgba([0, 240, 0, 255]))
        .save(&image)
        .unwrap();
    let mut doc = workers::load(&red).unwrap();
    let imported = imports::import_files(&[blue.clone(), wav.clone(), image.clone()]).unwrap();
    let p = doc.video.as_mut().unwrap();
    let mut blue_clip = imported[0].clone();
    blue_clip.id = 2;
    blue_clip.at = 500;
    blue_clip.end = 1000;
    p.segments.push(blue_clip);
    let track = p.add_media_track();
    let mut a = imported[1].clone();
    a.id = 3;
    a.track = track;
    a.end = 1000;
    let mut b = a.clone();
    b.id = 4;
    b.at = 500;
    b.audio[0].volume = 0.4;
    p.segments.extend([a, b]);
    let mut card = imported[2].clone();
    card.id = 5;
    card.at = 1500;
    card.end = 500;
    card.rect = [0.25, 0.25, 0.5, 0.5];
    p.segments.push(card);
    doc.validate().unwrap();
    let p = doc.video.as_ref().unwrap();
    let mut visuals = media::Visuals::new();
    let first = visuals.frame(p, [320, 180], 100).unwrap();
    let mixed = visuals.frame(p, [320, 180], 750).unwrap();
    let last = visuals.frame(p, [320, 180], 1250).unwrap();
    assert!(first.get_pixel(30, 30)[0] > 200);
    assert!(last.get_pixel(30, 30)[2] > 200);
    let rgb = mixed.get_pixel(30, 30);
    assert!((rgb[0] as i32 - 120).abs() < 15);
    assert!((rgb[2] as i32 - 120).abs() < 15);
    let card = visuals.frame(p, [320, 180], 1750).unwrap();
    assert_eq!(card.get_pixel(160, 90)[1], 240);
    assert_eq!(card.get_pixel(0, 0).0, p.background);
    let mut mixer = media::Mixer::new(p).unwrap();
    let values = mixer.samples(p, 750 * 48, 16).unwrap();
    assert!(
        values.iter().all(|v| (6800..=7200).contains(v)),
        "{values:?}"
    );
    let output = root.path().join("mix.mp4");
    workers::export_sync(&doc, &output, &AtomicBool::new(false), |_| ()).unwrap();
    let mut decoder = media::Video::open(&output).unwrap();
    let pixels = decoder.frame(750).unwrap();
    let rgb = pixels.get_pixel(30, 30);
    assert!((rgb[0] as i32 - 120).abs() < 25 && (rgb[2] as i32 - 120).abs() < 25);
    for path in [&red, &blue, &wav, &image] {
        assert!(workers::export_sync(&doc, path, &AtomicBool::new(false), |_| ()).is_err());
    }
    let file = root.path().join("Share.vclip");
    doc.save(&file).unwrap();
    let reopened = workers::load(&file).unwrap();
    for path in reopened.video.as_ref().unwrap().media_paths() {
        assert!(Path::new(path).is_file());
        assert!(path.contains("vclip-media"));
    }
    let mut visuals = media::Visuals::new();
    assert_eq!(
        visuals
            .frame(reopened.video.as_ref().unwrap(), [320, 180], 750)
            .unwrap(),
        mixed
    );
    if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        mixed.save(folder.join("crossfade-midpoint.png")).unwrap();
        std::fs::copy(output, folder.join("multitrack-roundtrip.mp4")).unwrap();
    }
}

#[test]
fn media_corner_fades_move_duplicate_cancel_and_undo_are_atomic() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(super::tests::fixture()).unwrap();
    e.shell.library = false;
    e.shell.backstage = None;
    e.video.snap = false;
    let frame = |e: &mut Editor, mut events: Vec<egui::Event>, modifiers| {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 960.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| e.show_video_timeline(ui),
        );
        out.textures_delta.clear();
    };
    let button = |pos, pressed, modifiers| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    frame(&mut e, vec![], egui::Modifiers::NONE);
    frame(&mut e, vec![], egui::Modifiers::NONE);
    let rect = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("media-block", 1u64))))
        .unwrap();
    let start = rect.left_top() + egui::vec2(3., 4.);
    let end = start + egui::vec2(110., 0.);
    let before = e.session.document.clone();
    frame(
        &mut e,
        vec![
            egui::Event::PointerMoved(start),
            button(start, true, egui::Modifiers::NONE),
        ],
        egui::Modifiers::NONE,
    );
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(end)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    assert!(e.video.timing_preview.as_ref().unwrap().segments[0].fade_in > 0);
    frame(
        &mut e,
        vec![button(end, false, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        e.session.document.video.as_ref().unwrap().segments[0].start,
        0
    );
    assert!(e.session.document.video.as_ref().unwrap().segments[0].fade_in > 0);
    e.execute(Command::Undo);
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    let start = rect.center();
    let end = start + egui::vec2(140., 0.);
    frame(
        &mut e,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::ALT),
            egui::Event::PointerMoved(start),
            button(start, true, egui::Modifiers::ALT),
        ],
        egui::Modifiers::ALT,
    );
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(end)],
        egui::Modifiers::ALT,
    );
    assert_eq!(e.video.timing_preview.as_ref().unwrap().segments.len(), 2);
    frame(
        &mut e,
        vec![button(end, false, egui::Modifiers::ALT)],
        egui::Modifiers::ALT,
    );
    assert_eq!(e.session.document.video.as_ref().unwrap().segments.len(), 2);
    e.execute(Command::Undo);
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    frame(
        &mut e,
        vec![
            egui::Event::PointerMoved(start),
            button(start, true, egui::Modifiers::NONE),
        ],
        egui::Modifiers::NONE,
    );
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(end)],
        egui::Modifiers::NONE,
    );
    frame(
        &mut e,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    assert!(e.video.timing_preview.is_none());
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
}

/// Reported: adding a webcam recording failed with "layer order", because the
/// screen and webcam tracks mapped to one layer on either side of the objects.
#[test]
fn a_recording_with_a_webcam_appends_with_the_webcam_on_its_own_track() {
    use capture_core::video::links::Layer;
    use capture_core::video::webcam::Webcam;
    let ctx = egui::Context::default();
    let mut recording = super::tests::fixture();
    let camera = Webcam {
        enabled: true,
        ..Default::default()
    };
    let project = recording.video.as_mut().unwrap();
    let camera_track = camera.place(project, "camera.mp4".into(), [640, 360], 12000);
    assert_eq!(project.layers()[0], Layer::Media(camera_track));
    for new_track in [false, true] {
        for empty in [true, false] {
            let mut e = crate::canvas::tests::editor(&ctx);
            let mut target = super::tests::fixture();
            if empty {
                target.annotations.clear();
                let v = target.video.as_mut().unwrap();
                v.objects.clear();
                v.segments.clear();
            }
            e.session = Session::new(target).unwrap();
            let before = e.session.document.video.as_ref().unwrap().segments.len();
            e.append_recording(&recording, new_track);
            let p = e.session.document.video.as_ref().unwrap();
            e.session.document.validate().unwrap();
            assert_eq!(p.segments.len(), before + 2, "new track {new_track}");
            let webcam = p.segments.iter().find(|c| c.label == "Webcam").unwrap();
            let screen = p.segments.iter().rfind(|c| c.label != "Webcam").unwrap();
            assert_ne!(webcam.track, screen.track);
            assert_eq!(webcam.rect, camera.rect([960, 540]));
            assert_eq!(webcam.frame.as_ref().unwrap().shape, crate::Kind::Ellipse);
            let layers = p.layers();
            assert!(layers.contains(&Layer::Media(webcam.track)));
        }
    }
}
#[test]
fn recording_append_and_new_track_preserve_project_and_single_undo() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(super::tests::fixture()).unwrap();
    let before = e.session.document.clone();
    let mut recording = Document::blank(320, 180).unwrap();
    recording.video = Some(Project::new("second.mp4".into(), [320, 180], 2000, 30));
    recording.canvas_behavior.dynamic_expansion = false;
    e.append_recording(&recording, false);
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.segments.len(), 2);
    assert_eq!(p.segments[1].at, 12000);
    assert_eq!(p.segments[1].source.as_ref().unwrap().path, "second.mp4");
    assert_eq!(e.session.document.annotations, before.annotations);
    e.execute(Command::Undo);
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    e.append_recording(&recording, true);
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.media_tracks.len(), 2);
    assert_eq!(p.segments[1].at, before.video.as_ref().unwrap().cursor);
    e.execute(Command::Undo);
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
    let source = Source {
        path: "image.png".into(),
        size: [80, 80],
        duration: 3_600_000,
        kind: MediaKind::Image,
    };
    let clip = Segment::new(1, 0, 1, Some(source), 5000);
    e.append_media_clips(vec![clip], 12000, 1);
    assert_eq!(e.session.document.video.as_ref().unwrap().duration(), 17000);
    e.execute(Command::Undo);
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&before).unwrap()
    );
}

#[test]
fn track_volume_drag_uses_total_motion_and_commits_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let mut doc = super::tests::fixture();
    doc.video.as_mut().unwrap().segments[0]
        .audio
        .push(capture_core::video::editor::Audio {
            path: "audio.wav".into(),
            label: "Audio".into(),
            volume: 1.,
            muted: false,
            fade_in: 0,
            fade_out: 0,
        });
    e.session = Session::new(doc).unwrap();
    e.shell.library = false;
    e.shell.backstage = None;
    let frame = |e: &mut Editor, events| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 960.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| e.show_video_timeline(ui),
        );
        out.textures_delta.clear();
    };
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let rect = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("track-volume", 1u64))))
        .unwrap();
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let start = rect.center();
    let end = start + egui::vec2(40., 0.);
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(start), button(start, true)],
    );
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(start + egui::vec2(10., 0.))],
    );
    frame(&mut e, vec![egui::Event::PointerMoved(end)]);
    assert!((e.video.timing_preview.as_ref().unwrap().media_levels[&1] - 1.4).abs() < 0.001);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    frame(&mut e, vec![button(end, false)]);
    assert!((e.session.document.video.as_ref().unwrap().media_levels[&1] - 1.4).abs() < 0.001);
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
}
