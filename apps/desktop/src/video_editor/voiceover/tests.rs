use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
#[test]
fn waveform_overview_preserves_spikes_and_is_bounded_across_long_takes() {
    let mut w = waveform::Overview::default();
    let mut chunk = vec![0; 4800];
    chunk[0] = i16::MIN;
    chunk[4799] = i16::MAX;
    for _ in 0..10000 {
        w.push(&chunk);
    }
    let s = w.snapshot();
    assert!(s.peaks.len() <= 2048);
    assert_eq!(s.frames, 24_000_000);
    assert!(s.bucket > 2400);
    assert!(s.peaks.iter().all(|v| v[0] == -1. && v[1] > 0.99));
    w.push(&[10000, -10000]);
    let tail = w.snapshot();
    assert_eq!(tail.frames, s.frames + 1);
    assert!(tail.peaks.last().unwrap()[1] > 0.3);
}
#[test]
fn take_scrub_is_view_only_and_keep_uses_chosen_voiceover_track() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(super::super::tests::fixture()).unwrap();
    let track = e
        .session
        .document
        .video
        .as_mut()
        .unwrap()
        .add_typed_track(capture_core::video::editor::TrackKind::Voiceover);
    let before = e.session.document.video.clone().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut w = capture_core::video::wav::Writer::new(root.path()).unwrap();
    w.append(&vec![1000; 96000]).unwrap();
    let path = w.finish().unwrap();
    e.video.voiceover = State {
        open: true,
        at: 1500,
        target: Some(track),
        take: Some(Take {
            camera: None,
            path: path.clone(),
            duration: 1000,
            keep: false,
            wave: Default::default(),
        }),
        ..Default::default()
    };
    e.review_voiceover(450, false);
    assert_eq!(e.video.voiceover.review_at, 450);
    assert_eq!(e.video_time(), before.cursor);
    e.video.voiceover.review_with_project = true;
    e.review_voiceover(450, false);
    assert_eq!(e.video_time(), 1950);
    e.video.voiceover.review_with_project = false;
    assert!(before.same_visual(e.session.document.video.as_ref().unwrap()));
    e.review_voiceover(99999, false);
    assert_eq!(e.video.voiceover.review_at, 1000);
    assert!(e.keep_voiceover());
    let v = e.session.document.video.as_ref().unwrap();
    assert_eq!(v.media_tracks.len(), before.media_tracks.len());
    assert_eq!(v.segments.last().unwrap().track, track);
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().segments,
        before.segments
    );
    assert!(path.exists());
}
#[test]
fn keeping_voiceover_is_one_undo_and_discard_never_deletes_kept_audio() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(super::super::tests::fixture()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut w = capture_core::video::wav::Writer::new(root.path()).unwrap();
    w.append(&vec![1000; 48000]).unwrap();
    let path = w.finish().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let original = e.session.document.video.clone().unwrap();
    e.video.voiceover.at = 2000;
    e.video.voiceover.take = Some(Take {
        camera: None,
        path: path.clone(),
        duration: 500,
        keep: false,
        wave: Default::default(),
    });
    assert!(e.keep_voiceover());
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.segments.len(), original.segments.len() + 1);
    assert_eq!(p.segments.last().unwrap().at, 2000);
    assert_eq!(
        p.segments.last().unwrap().source.as_ref().unwrap().kind,
        capture_core::video::editor::MediaKind::Audio
    );
    assert_eq!(p.media_tracks.len(), original.media_tracks.len() + 1);
    e.execute(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().segments,
        original.segments
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    e.execute(Command::Redo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().segments.len(),
        original.segments.len() + 1
    );
    let discard = root.path().join("unkept.wav");
    std::fs::write(&discard, &bytes).unwrap();
    drop(Take {
        camera: None,
        path: discard.clone(),
        duration: 500,
        keep: false,
        wave: Default::default(),
    });
    assert!(!discard.exists());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
#[test]
fn voiceover_recorder_states_render_without_device_access() {
    for (name, state, camera) in [
        ("voiceover-ready", 0, false),
        ("voiceover-countdown", 1, false),
        ("voiceover-recording", 2, false),
        ("voiceover-review", 3, false),
        ("camera-ready", 0, true),
        ("camera-recording", 2, true),
        ("camera-review", 3, true),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = Session::new(super::super::tests::fixture()).unwrap();
        e.video.voiceover = State {
            open: true,
            ready: true,
            devices: vec![(String::new(), "Default input device".into())],
            countdown: 3,
            position: Some(egui::pos2(24., 24.)),
            ..Default::default()
        };
        let mut overview = waveform::Overview::default();
        let samples: Vec<i16> = (0..48_000 * 12)
            .flat_map(|i| {
                let t = i as f64 / 48000.;
                let envelope = 0.25 + 0.2 * (t * 3.).sin();
                let s = (t * std::f64::consts::TAU * 185.).sin() * envelope;
                [(s * 32768.) as i16; 2]
            })
            .collect();
        overview.push(&samples);
        if state >= 2 {
            e.video.voiceover.wave = overview.snapshot();
        }
        let root = tempfile::tempdir().unwrap();
        match state {
            1 => e.video.voiceover.deadline = Some(Instant::now() + Duration::from_secs(3)),
            2 => e.video.voiceover.recording = Some(Instant::now() - Duration::from_secs(23)),
            3 => {
                let mut w = capture_core::video::wav::Writer::new(root.path()).unwrap();
                w.append(&samples).unwrap();
                e.video.voiceover.take = Some(Take {
                    camera: None,
                    path: w.finish().unwrap(),
                    duration: 12000,
                    keep: false,
                    wave: overview.snapshot(),
                });
            }
            _ => (),
        }
        if camera {
            e.video.voiceover.with_camera = true;
            e.video.voiceover.camera_height = 720;
            e.video.voiceover.cameras = vec![("test".into(), "Webcam · synthetic preview".into())];
            e.video.voiceover.camera_device = "test".into();
            let pixels = Arc::new(image::RgbaImage::from_fn(640, 360, |x, y| {
                let grid = ((x / 80 + y / 60) % 2) * 16;
                image::Rgba([
                    (40 + grid) as u8,
                    (85 + y / 8) as u8,
                    (120 + x / 8) as u8,
                    255,
                ])
            }));
            let texture = ctx.load_texture(
                "synthetic-webcam",
                egui::ColorImage::from_rgba_unmultiplied([640, 360], pixels.as_raw()),
                egui::TextureOptions::LINEAR,
            );
            e.video.voiceover.camera_texture = Some((pixels, texture));
        }
        let render = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1024., 800.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.painter().rect_filled(
                        ui.max_rect(),
                        0.,
                        egui::Color32::from_rgb(17, 22, 23),
                    );
                    e.show_voiceover_recorder(&ctx);
                    let r: egui::Rect =
                        ctx.data(|d| d.get_temp(egui::Id::unique("voiceover-window")).unwrap());
                    assert!(r.right() <= 1024. && r.bottom() < 800., "{r:?}");
                },
            ))
        };
        let first = render(&mut e);
        let output = render(&mut e);
        if let Some(root) = std::env::var_os("CLIPCRAFT_VOICEOVER_EVIDENCE") {
            dump_meshes(&ctx, &root.to_string_lossy(), name, first, output);
        }
    }
}

#[test]
fn camera_take_keeps_separate_linked_sources_in_one_undo_and_discard_cleans_both() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(super::super::tests::fixture()).unwrap();
    let root = tempfile::tempdir().unwrap();
    let audio = root.path().join("narration.wav");
    let camera = root.path().join("camera.mp4");
    std::fs::write(&audio, b"fixture").unwrap();
    std::fs::write(&camera, b"fixture").unwrap();
    let before = e.session.document.video.clone().unwrap();
    e.video.voiceover = State {
        open: true,
        at: 1500,
        take: Some(Take {
            camera: Some(voiceover_capture::CameraTake {
                path: camera.clone(),
                size: [1280, 720],
                duration: 2000,
            }),
            path: audio.clone(),
            duration: 2000,
            keep: false,
            wave: Default::default(),
        }),
        ..Default::default()
    };
    assert!(e.keep_voiceover());
    let p = e.session.document.video.as_ref().unwrap();
    let a = p.segments.iter().find(|s| s.label == "Narration").unwrap();
    let c = p.segments.iter().find(|s| s.label == "Camera").unwrap();
    assert_ne!(a.track, c.track);
    assert_eq!(a.link, c.link);
    assert!(a.link.is_some());
    assert_eq!((a.at, a.duration()), (c.at, c.duration()));
    assert!(c.audio.is_empty());
    assert!(c.frame.is_some());
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video.as_ref().unwrap(), &before);
    assert!(audio.exists() && camera.exists());
    e.execute(Command::Redo);
    let pending = Take {
        camera: Some(voiceover_capture::CameraTake {
            path: camera.clone(),
            size: [1280, 720],
            duration: 2000,
        }),
        path: audio.clone(),
        duration: 2000,
        keep: false,
        wave: Default::default(),
    };
    // This isolated fixture owns its files; a real kept take has keep=true.
    drop(pending);
    assert!(!audio.exists() && !camera.exists());
}
