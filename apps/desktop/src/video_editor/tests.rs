use super::*;
use capture_core::video::editor::{Animation, Timing};
pub(super) fn fixture() -> Document {
    let mut d = Document::blank(960, 540).unwrap();
    d.video = Some(Project::new("fixture.mp4".into(), [960, 540], 12000, 30));
    d.canvas_behavior.dynamic_expansion = false;
    for (id, start, end) in [
        (1, 0, 4000),
        (2, 4000, 8000),
        (3, 3000, 9000),
        (4, 9000, 12000),
    ] {
        let mut a = capture_core::steps::marker(
            id,
            [140. + id as f32 * 110., 180. + (id % 2) as f32 * 80.],
            64,
            [20, 155, 194, 255],
            (id as u32, false),
            [960, 540],
        )
        .unwrap();
        if id == 3 {
            a.kind = crate::Kind::Text;
            a.start = [160., 70.];
            a.end = [750., 150.];
            a.text.as_mut().unwrap().content = "Build a clearer walkthrough".into();
            a.text.as_mut().unwrap().size_points = 34.;
        }
        d.annotations.push(a);
        let v = d.video.as_mut().unwrap();
        v.place(id, Timing::new(0, start, end));
    }
    d.video.as_mut().unwrap().cursor = 4500;
    d
}
#[test]
fn playback_multipress_seek_rules_and_session_state() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.select_only(Some(2));
    e.video_playback_key();
    assert!(e.video.playing);
    assert_eq!(e.video_time(), 4500);
    e.video_playback_key();
    assert_eq!(e.video_time(), 4000);
    assert!(e.video.playing);
    e.video_playback_key();
    assert_eq!(e.video_time(), 0);
    assert!(e.video.playing);
    e.video.space_last = None;
    e.video_playback_key();
    assert!(!e.video.playing);
    e.select_only(None);
    e.video_seek(6500);
    assert_eq!(e.video_clip_start(), 4000);
    e.video.collapsed = true;
    e.video.zoom = 2.;
    e.switch_session(
        Session::new(Document::blank(320, 180).unwrap()).unwrap(),
        None,
        Some(Path::new("image.png")),
    );
    assert!(!e.is_video());
}
#[cfg(windows)]
#[test]
fn video_export_roundtrip_edits_audio_seek_and_portable_project() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.mp4");
    let runtime = media::Runtime::new().unwrap();
    let mut enc = media::Encoder::new(&source, [320, 180], 30, true).unwrap();
    for i in 0..60u64 {
        let mut p = vec![0u8; 320 * 180 * 4];
        for c in p.as_chunks_mut::<4>().0 {
            c.copy_from_slice(if i < 30 {
                &[20, 30, 210, 255]
            } else {
                &[210, 30, 20, 255]
            });
        }
        enc.video(i * 10_000_000 / 30, &p).unwrap();
        let sound: Vec<i16> = (0..1600)
            .flat_map(|j| {
                let value = (((i * 1600 + j) as f64 * 440. * std::f64::consts::TAU / 48000.).sin()
                    * 9000.) as i16;
                [value, value]
            })
            .collect();
        enc.audio(i * 1600, &sound).unwrap();
    }
    enc.finish().unwrap();
    drop(enc);
    drop(runtime);
    let mut d = workers::load(&source).unwrap();
    assert_eq!(d.video.as_ref().unwrap().segments[0].audio.len(), 1);
    let mut a =
        capture_core::steps::marker(1, [90., 90.], 44, [0, 0, 0, 255], (1, false), [320, 180])
            .unwrap();
    a.kind = crate::Kind::Redact;
    a.start = [40., 40.];
    a.end = [140., 140.];
    a.text = None;
    d.annotations.push(a);
    let v = d.video.as_mut().unwrap();
    v.sync(&d.annotations);
    v.split(1000).unwrap();
    v.remove_range(500, 1000).unwrap();
    let output = root.path().join("edited.mp4");
    workers::export_sync(&d, &output, &AtomicBool::new(false), |_| ()).unwrap();
    let _runtime = media::Runtime::new().unwrap();
    let mut decoder = media::Video::open(&output).unwrap();
    assert!((1400..=1550).contains(&decoder.duration));
    let first = decoder.frame(100).unwrap();
    let last = decoder.frame(1000).unwrap();
    assert!(first.get_pixel(10, 10)[0] > 150);
    assert!(last.get_pixel(10, 10)[2] > 150);
    assert!(last.get_pixel(80, 80).0[..3].iter().all(|v| *v < 15));
    let again = decoder.frame(100).unwrap();
    assert!(again.get_pixel(10, 10)[0] > 150);
    let mut audio = media::Sound::open(&output).unwrap();
    assert!((0..480).any(|i| audio.value(i).unwrap()[0].abs() > 100));
    let protected = root.path().join("keep.mp4");
    std::fs::write(&protected, b"unchanged destination").unwrap();
    assert!(workers::export_sync(&d, &protected, &AtomicBool::new(true), |_| ()).is_err());
    assert_eq!(std::fs::read(&protected).unwrap(), b"unchanged destination");
    assert!(workers::export_sync(&d, &source, &AtomicBool::new(false), |_| ()).is_err());
    let mut wav = capture_core::video::wav::Writer::new(root.path()).unwrap();
    wav.append(&vec![8000; 4800]).unwrap();
    let wav_path = wav.finish().unwrap();
    let mut sound = media::Sound::open(&wav_path).unwrap();
    assert!(sound.value(100).unwrap()[0].abs() > 7000);
    d.video
        .as_mut()
        .unwrap()
        .objects
        .values_mut()
        .next()
        .unwrap()
        .label = "Review highlight".into();
    let project = root.path().join("Saved.vclip");
    d.save(&project).unwrap();
    let loaded = workers::load(&project).unwrap();
    let mut expected = d.video.as_ref().unwrap().segments.clone();
    for c in &mut expected {
        for a in &mut c.audio {
            a.path = loaded.video.as_ref().unwrap().source.clone();
        }
    }
    assert_eq!(loaded.video.as_ref().unwrap().segments, expected);
    assert_eq!(loaded.annotations, d.annotations);
    assert_eq!(
        loaded.video.as_ref().unwrap().objects,
        d.video.as_ref().unwrap().objects
    );
    if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(output, folder.join("edited-roundtrip.mp4")).unwrap();
        first.save(folder.join("export-first.png")).unwrap();
        last.save(folder.join("export-last.png")).unwrap();
    }
}
#[test]
fn production_video_workspace_renders_all_contexts() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    let mut ribbon_height: Option<f32> = None;
    let mut canvas_top: Option<f32> = None;
    for (name, selected, tab) in [
        ("workspace", Some(3), crate::ribbon::RibbonTab::Insert),
        (
            "voiceover-workstation",
            None,
            crate::ribbon::RibbonTab::Insert,
        ),
        ("voiceover-record", None, crate::ribbon::RibbonTab::Insert),
        ("track-types", None, crate::ribbon::RibbonTab::Insert),
        ("camera-frame", None, crate::ribbon::RibbonTab::Insert),
        ("camera-crop", None, crate::ribbon::RibbonTab::Insert),
        ("camera-behind", None, crate::ribbon::RibbonTab::Insert),
        ("media-timeline", None, crate::ribbon::RibbonTab::Insert),
        ("media-menu", None, crate::ribbon::RibbonTab::Insert),
        ("media-fades", None, crate::ribbon::RibbonTab::Formatting),
        ("media-audio", None, crate::ribbon::RibbonTab::Formatting),
        ("motion", Some(3), crate::ribbon::RibbonTab::Formatting),
        ("timing", Some(3), crate::ribbon::RibbonTab::Insert),
        ("video-settings", None, crate::ribbon::RibbonTab::Advanced),
        ("video-export", None, crate::ribbon::RibbonTab::Export),
        ("audio", None, crate::ribbon::RibbonTab::Formatting),
        (
            "timeline-collapsed",
            Some(3),
            crate::ribbon::RibbonTab::Insert,
        ),
        (
            "recent-from-timeline",
            Some(3),
            crate::ribbon::RibbonTab::Insert,
        ),
        (
            "recent-from-collapsed",
            Some(3),
            crate::ribbon::RibbonTab::Insert,
        ),
        (
            "timeline-reopened",
            Some(3),
            crate::ribbon::RibbonTab::Insert,
        ),
        ("object-menu", Some(3), crate::ribbon::RibbonTab::Insert),
        (
            "properties-priority",
            Some(3),
            crate::ribbon::RibbonTab::Formatting,
        ),
        (
            "properties-recent",
            Some(3),
            crate::ribbon::RibbonTab::Formatting,
        ),
        (
            "properties-collapsed",
            Some(3),
            crate::ribbon::RibbonTab::Formatting,
        ),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = Session::new(fixture()).unwrap();
        e.select_only(selected);
        e.tool = None;
        e.shell.ribbon_tab = tab;
        e.shell.properties = true;
        e.shell.backstage = None;
        e.shell.library = false;
        let history_root = tempfile::tempdir().unwrap();
        let mut store = capture_core::history::History::open(history_root.path()).unwrap();
        for (i, color) in [
            [233, 242, 247, 255],
            [232, 241, 232, 255],
            [245, 235, 224, 255],
            [235, 230, 244, 255],
            [230, 242, 242, 255],
        ]
        .into_iter()
        .enumerate()
        {
            let mut doc = Document::canvas(320, 180, color).unwrap();
            let mut a = fixture().annotations[2].clone();
            a.start = [22., 35.];
            a.end = [298., 130.];
            a.text.as_mut().unwrap().size_points = 20.;
            a.text.as_mut().unwrap().content = [
                "Prepare workspace",
                "Capture settings",
                "Review your steps",
                "Add annotations",
                "Share your guide",
            ][i]
                .into();
            doc.annotations.push(a);
            store
                .create(
                    &doc,
                    &format!("Walkthrough {}", i + 1),
                    capture_core::history::Origin::Capture,
                )
                .unwrap();
        }
        e.history = crate::history::HistoryPanel::isolated(history_root.path());
        e.history.recent = false;
        let mut pixels = image::RgbaImage::from_pixel(960, 540, image::Rgba([235, 239, 241, 255]));
        for y in 165..490 {
            for x in 70..890 {
                pixels.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        // Synthetic media samples exercise production filmstrip and waveform painting.
        for (label, frequency) in [("Microphone", 220.), ("System audio", 660.)] {
            e.session.document.video.as_mut().unwrap().segments[0]
                .audio
                .push(capture_core::video::editor::Audio {
                    path: "sample.wav".into(),
                    label: label.into(),
                    volume: 1.,
                    muted: false,
                    fade_in: 350,
                    fade_out: 500,
                });
            e.video.waves.push(
                (0..256)
                    .map(|i| {
                        let envelope = (i as f32 * 0.13).sin().abs() * 0.7 + 0.1;
                        let start = i * 2250;
                        (start..start + 480)
                            .map(|n| {
                                (n as f32 * frequency * std::f32::consts::TAU / 48000.)
                                    .sin()
                                    .abs()
                                    * envelope
                            })
                            .fold(0f32, f32::max)
                    })
                    .collect(),
            );
        }
        let thumbnail = image::imageops::thumbnail(&pixels, 160, 90);
        let texture = ctx.load_texture(
            "sample-filmstrip",
            egui::ColorImage::from_rgba_unmultiplied(
                [thumbnail.width() as usize, thumbnail.height() as usize],
                thumbnail.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        e.video.thumbnails = (0..16).map(|n| (n * 750, texture.clone())).collect();
        if name == "audio" {
            e.video.selected = Selection::Audio(0);
        }
        e.video.tab = match name {
            "motion" | "properties-priority" | "properties-recent" | "properties-collapsed" => {
                PropertyTab::Motion
            }
            "timing" => PropertyTab::Timing,
            "audio" => PropertyTab::Audio,
            _ => PropertyTab::Style,
        };
        if e.video.tab != PropertyTab::Audio {
            e.video.object_tab = e.video.tab;
        }
        if name.starts_with("media-") {
            use capture_core::video::editor::{MediaKind, Segment, Source};
            let v = e.session.document.video.as_mut().unwrap();
            v.segments[0].label = "Screen walkthrough.mp4".into();
            v.segments[0].end = 7000;
            v.segments[0].fade_in = 650;
            let mut second = v.segments[0].clone();
            second.id = 2;
            second.at = 5500;
            second.start = 1000;
            second.end = 7500;
            second.label = "Follow-up recording.mp4".into();
            second.fade_in = 0;
            second.fade_out = 850;
            v.segments.push(second);
            let track = v.add_media_track();
            v.media_levels.insert(track, 0.7);
            let mut audio = Segment::new(
                3,
                0,
                track,
                Some(Source {
                    path: "Narration.wav".into(),
                    size: [1, 1],
                    duration: 12000,
                    kind: MediaKind::Audio,
                }),
                12000,
            );
            audio.fade_in = 400;
            audio.fade_out = 750;
            audio.audio = v.segments[0].audio[..1].to_vec();
            audio.audio[0].label = "Narration".into();
            audio.audio[0].volume = 0.85;
            v.segments.push(audio);
            let first = e.video.thumbnails.clone();
            let wave = e.video.waves[0].clone();
            e.video
                .media_overviews
                .insert("Narration.wav".into(), (first, wave));
            e.video.selected = if name == "media-audio" {
                Selection::Audio(2)
            } else {
                Selection::Recording(1)
            };
            e.video.tab = if name == "media-audio" {
                PropertyTab::Audio
            } else if name == "media-fades" {
                PropertyTab::Motion
            } else {
                PropertyTab::Timing
            };
            e.video.property_selection = Some((e.video.selected, None));
        }
        e.video.frame = Some((4500, Arc::new(pixels)));
        e.video.frame_revision = 1;
        if let Some(t) = e
            .session
            .document
            .video
            .as_mut()
            .unwrap()
            .objects
            .get_mut(&3)
        {
            t.entrance = Animation::Typewriter;
            t.exit = Animation::Typewriter;
        }
        if name == "voiceover-workstation" {
            e.session
                .document
                .video
                .as_mut()
                .unwrap()
                .clip_effects
                .insert(1, capture_core::video::effects::Chain::narration());
            e.open_effects_workstation();
            e.video.fx.position = Some(egui::pos2(30., 180.));
            e.video.tab = PropertyTab::Audio;
        }
        if name == "voiceover-record" {
            e.video.voiceover = voiceover::State {
                open: true,
                ready: true,
                countdown: 3,
                devices: vec![(String::new(), "Default input device".into())],
                ..Default::default()
            };
            e.video.tab = PropertyTab::Audio;
        }
        if name.starts_with("camera-") {
            use capture_core::video::{editor::*, frame::Frame, links::Layer};
            let p = e.session.document.video.as_mut().unwrap();
            let track = p.add_typed_track(TrackKind::Video);
            let mut camera = Segment::new(
                2,
                0,
                track,
                Some(Source {
                    path: "camera-preview.mp4".into(),
                    size: [640, 360],
                    duration: 12000,
                    kind: MediaKind::Video,
                }),
                12000,
            );
            camera.label = "Camera · Take 1".into();
            camera.rect = [0.57, 0.23, 0.36, 0.57];
            camera.frame = Some(Frame {
                shape: capture_core::Kind::Ellipse,
                ..Default::default()
            });
            camera.link = Some(1);
            let raw = Arc::new(image::RgbaImage::from_fn(640, 360, |x, y| {
                image::Rgba([
                    (40 + ((x / 80 + y / 60) % 2) * 16) as u8,
                    (85 + y / 8) as u8,
                    (120 + x / 8) as u8,
                    255,
                ])
            }));
            let rendered = camera
                .frame
                .as_ref()
                .unwrap()
                .render(raw, camera.rect, [960, 540], 2)
                .unwrap();
            let mut narration = camera.clone();
            narration.id = 3;
            narration.track = p.add_typed_track(TrackKind::Voiceover);
            narration.label = "Narration · Take 1".into();
            narration.source = Some(Source {
                path: "narration.wav".into(),
                size: [1, 1],
                duration: 12000,
                kind: MediaKind::Audio,
            });
            narration.frame = None;
            narration.audio = p.segments[0].audio.clone();
            p.segments.extend([camera, narration]);
            p.move_layer(Layer::Media(track), -100);
            if name == "camera-behind" {
                p.move_layer(Layer::Media(track), 1);
            }
            let base = e.video.frame.as_ref().unwrap().1.clone();
            e.video.frame_layers = vec![
                (
                    1,
                    Arc::new(capture_core::bitmap::Bitmap::transient(base).unwrap()),
                ),
                (
                    track,
                    Arc::new(capture_core::bitmap::Bitmap::transient(Arc::new(rendered)).unwrap()),
                ),
            ];
            e.video.frame = Some((
                4500,
                Arc::new(image::RgbaImage::from_pixel(
                    960,
                    540,
                    image::Rgba(p.background),
                )),
            ));
            e.video.selected = Selection::Recording(1);
            e.video.tab = PropertyTab::Style;
            e.video.property_selection = Some((e.video.selected, None));
            e.video.frame_crop = name == "camera-crop";
            e.video.properties_priority = true;
        }
        let textures = std::cell::RefCell::new(Vec::new());
        let render = |e: &mut Editor, events| {
            let out = PaintedOutput(ctx.run_ui(
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
                    e.show_shell(ui);
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::unique("video-workspace-top"),
                            ui.available_rect_before_wrap().top(),
                        )
                    });
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(crate::shell::CANVAS)
                                .inner_margin(12),
                        )
                        .show(ui, |ui| e.show_workspace(ui));
                    e.show_effects_workstation(&ctx);
                    e.show_voiceover_recorder(&ctx);
                },
            ));
            textures
                .borrow_mut()
                .extend(out.0.textures_delta.set.clone());
            out
        };
        let mut first = render(&mut e, vec![]);
        render(&mut e, vec![]);
        let click = |e: &mut Editor, key: &str, button: egui::PointerButton| {
            let r = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(key)))
                .unwrap();
            for pressed in [true, false] {
                render(
                    e,
                    vec![
                        egui::Event::PointerMoved(r.center()),
                        egui::Event::PointerButton {
                            pos: r.center(),
                            button,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            render(e, vec![]);
        };
        if name.starts_with("properties-") {
            click(&mut e, "footer-priority", egui::PointerButton::Primary);
        }
        if [
            "timeline-collapsed",
            "recent-from-collapsed",
            "properties-collapsed",
        ]
        .contains(&name)
        {
            click(&mut e, "timeline-toggle", egui::PointerButton::Primary);
        }
        if [
            "recent-from-timeline",
            "recent-from-collapsed",
            "timeline-reopened",
            "properties-recent",
        ]
        .contains(&name)
        {
            click(&mut e, "footer-recent", egui::PointerButton::Primary);
            assert!(e.history.recent && e.video.collapsed);
            if name == "timeline-reopened" {
                click(&mut e, "timeline-toggle", egui::PointerButton::Primary);
                assert!(!e.history.recent && !e.video.collapsed);
            }
        }
        if name == "track-types" {
            click(&mut e, "timeline-add-track", egui::PointerButton::Primary);
        }
        if name == "object-menu" || name == "media-menu" {
            let r = ctx
                .read_response(egui::Id::unique((
                    if name == "media-menu" {
                        "media-clip"
                    } else {
                        "video-object"
                    },
                    3u64,
                )))
                .unwrap()
                .rect;
            for pressed in [true, false] {
                render(
                    &mut e,
                    vec![
                        egui::Event::PointerMoved(r.center()),
                        egui::Event::PointerButton {
                            pos: r.center(),
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        }
        for _ in 0..20 {
            render(&mut e, vec![]);
            std::thread::sleep(Duration::from_millis(5));
        }
        let output = render(&mut e, vec![]);
        let dock = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("timeline-panel")))
            .unwrap();
        if e.video.collapsed {
            assert!((dock.height() - 44.).abs() < 1.);
        } else {
            assert!(
                (dock.height() - 260.).abs() < 1.,
                "{name}: expanded Timeline must restore its height"
            );
        }
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("ribbon-scroll-content")))
            .unwrap();
        if let Some(before) = ribbon_height {
            assert!(
                (rect.height() - before).abs() < 1.,
                "{name}: {} vs {before}",
                rect.height()
            );
        }
        ribbon_height = Some(rect.height());
        let top = ctx
            .data(|d| d.get_temp::<f32>(egui::Id::unique("video-workspace-top")))
            .unwrap();
        if let Some(before) = canvas_top {
            assert!(
                (top - before).abs() < 1.,
                "{name} ribbon moved workspace: {top} vs {before}"
            );
        }
        canvas_top = Some(top);
        if name == "motion" {
            let preview = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("video-motion-preview")))
                .unwrap();
            assert!(
                preview.bottom()
                    < ctx
                        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("properties-panel")))
                        .unwrap()
                        .bottom()
            );
        }
        if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
            first.0.textures_delta.set.clear();
            for (id, deltas) in textures.into_inner() {
                first
                    .0
                    .textures_delta
                    .set
                    .entry(id)
                    .or_default()
                    .extend(deltas);
            }
            dump_meshes(&ctx, &folder.to_string_lossy(), name, first, output);
        }
    }
}

#[test]
fn video_properties_tabs_fit_and_remember_selection_context() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    for width in [240., 302., 420.] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = Session::new(fixture()).unwrap();
        e.select_only(Some(3));
        e.video.tab = PropertyTab::Motion;
        e.video.object_tab = PropertyTab::Motion;
        let render = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 620.),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| e.video_properties_pane(ui),
            ))
        };
        let first = render(&mut e, vec![]);
        let second = render(&mut e, vec![]);
        let rects: Vec<_> = PropertyTab::ALL
            .iter()
            .filter(|t| **t != PropertyTab::Audio)
            .map(|tab| {
                ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("video-tab", tab.label()))))
                    .unwrap()
            })
            .collect();
        assert!(rects.iter().all(|r| r.right() <= width + 0.1));
        assert!(
            rects
                .windows(2)
                .all(|r| r[0].right() < r[1].left() && (r[0].width() - r[1].width()).abs() < 0.1)
        );
        let labels = ctx
            .data(|d| d.get_temp::<bool>(egui::Id::unique("video-tabs-labels")))
            .unwrap();
        if width == 420. {
            assert!(labels);
        }
        let preview = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("video-motion-preview")))
            .unwrap();
        assert!(preview.bottom() <= 620. && preview.height() >= 160. && preview.height() < 260.);
        if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
            dump_meshes(
                &ctx,
                &folder.to_string_lossy(),
                &format!("properties-{width}"),
                first,
                second,
            );
        }
        let pos = rects[1].center();
        let press = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        render(&mut e, vec![egui::Event::PointerMoved(pos), press(true)]);
        render(&mut e, vec![press(false)]);
        assert_eq!(e.video.tab, PropertyTab::Timing);
        e.select_only(None);
        e.video.selected = Selection::Audio(0);
        render(&mut e, vec![]);
        assert_eq!(
            e.video.tab,
            PropertyTab::Style,
            "Silent media must not expose another track's audio"
        );
        e.session.document.video.as_mut().unwrap().segments[0]
            .audio
            .push(capture_core::video::editor::Audio {
                path: "fixture.wav".into(),
                label: "Audio".into(),
                volume: 1.,
                muted: false,
                fade_in: 0,
                fade_out: 0,
            });
        e.video.tab = PropertyTab::Audio;
        render(&mut e, vec![]);
        assert_eq!(e.video.tab, PropertyTab::Audio);
        e.select_only(Some(2));
        render(&mut e, vec![]);
        assert_eq!(e.video.tab, PropertyTab::Timing);
        assert_eq!(e.shell.property_tab, crate::shell::PropertyTab::Tool);
    }
}
#[test]
fn timeline_edge_drag_previews_then_commits_one_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
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
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let rect = ctx
        .read_response(egui::Id::unique(("video-object", 3u64)))
        .unwrap()
        .rect;
    let start = rect.right_center() - egui::vec2(2., 0.);
    let finish = start - egui::vec2(100., 0.);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let before = e.session.document.video.clone().unwrap();
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(start), button(start, true)],
    );
    frame(&mut e, vec![egui::Event::PointerMoved(finish)]);
    assert!(e.video.timing_preview.is_some());
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&3],
        before.objects[&3]
    );
    frame(&mut e, vec![button(finish, false)]);
    assert!(e.video.drag.is_none());
    assert!(e.session.document.video.as_ref().unwrap().objects[&3].end < before.objects[&3].end);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video.unwrap().objects, before.objects);
}

#[test]
fn timeline_object_actions_preserve_source_and_undo_atomically() {
    use super::object_menu::Action;
    for action in [
        Action::Duplicate,
        Action::TrimStart,
        Action::TrimEnd,
        Action::MoveStart,
        Action::Track(None),
        Action::ClearMotion,
        Action::Visible(false),
        Action::Locked(true),
        Action::Delete,
    ] {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut doc = fixture();
        doc.video
            .as_mut()
            .unwrap()
            .objects
            .get_mut(&3)
            .unwrap()
            .entrance = Animation::Fade;
        e.session = Session::new(doc).unwrap();
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let original = e.session.document.video.as_ref().unwrap().objects[&3].clone();
        e.video_object_action(3, action);
        let after = serde_json::to_vec(&e.session.document).unwrap();
        assert_ne!(after, before, "{action:?}");
        e.session.document.validate().unwrap();
        if matches!(action, Action::Duplicate) {
            let v = e.session.document.video.as_ref().unwrap();
            assert_eq!(v.objects[&3], original);
            let copy = &v.objects[&5];
            assert_eq!(
                (copy.start, copy.end, copy.entrance),
                (original.start, original.end, original.entrance)
            );
            assert_ne!(copy.track, original.track);
            let mut a = e.session.document.annotations.last().unwrap().clone();
            a.id = 3;
            assert_eq!(a, e.session.document.annotations[2]);
        }
        e.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&e.session.document).unwrap(),
            before,
            "Undo {action:?}"
        );
        assert!(!e.session.can_undo());
        e.execute(Command::Redo);
        assert_eq!(
            serde_json::to_vec(&e.session.document).unwrap(),
            after,
            "Redo {action:?}"
        );
    }
}
#[test]
fn timeline_object_actions_reject_locks_collisions_and_invalid_trims() {
    use super::object_menu::Action;
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let track = e.session.document.video.as_ref().unwrap().objects[&1].track;
    e.video_object_action(3, Action::Track(Some(track)));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    e.session.document.video.as_mut().unwrap().cursor = 11000;
    let before = serde_json::to_vec(&e.session.document).unwrap();
    for action in [Action::TrimStart, Action::TrimEnd, Action::MoveStart] {
        e.video_object_action(3, action);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    }
    e.video_object_action(3, Action::Locked(true));
    let before = serde_json::to_vec(&e.session.document).unwrap();
    for action in [
        Action::Duplicate,
        Action::Delete,
        Action::Track(None),
        Action::ClearMotion,
        Action::Rename,
    ] {
        e.video_object_action(3, action);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    }
    assert!(
        e.session
            .execute(Command::VideoDuplicate {
                id: 3,
                timing: None
            })
            .is_err()
    );
    e.video_object_action(3, Action::Seek(false));
    assert_eq!(
        e.session.document.video.as_ref().unwrap().cursor,
        e.session.document.video.as_ref().unwrap().objects[&3].start
    );
}
#[test]
fn timeline_alt_drag_keeps_original_cancel_and_single_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.shell.library = false;
    e.shell.backstage = None;
    e.video.snap = false;
    let frame = |e: &mut Editor, mut events: Vec<egui::Event>| {
        events.insert(0, egui::Event::ModifiersChanged(egui::Modifiers::ALT));
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
        modifiers: egui::Modifiers::ALT,
    };
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let rect = ctx
        .read_response(egui::Id::unique(("video-object", 3u64)))
        .unwrap()
        .rect;
    let start = rect.center();
    let end = start + egui::vec2(110., 0.);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(start), button(start, true)],
    );
    frame(&mut e, vec![egui::Event::PointerMoved(end)]);
    let preview = e.video.timing_preview.as_ref().unwrap();
    assert!(preview.objects.contains_key(&5));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert_eq!(
        preview.objects[&3],
        e.session.document.video.as_ref().unwrap().objects[&3]
    );
    let expected = preview.objects[&5].clone();
    frame(&mut e, vec![button(end, false)]);
    assert_eq!(e.session.document.annotations.len(), 5);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&5],
        expected
    );
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
    frame(&mut e, vec![]);
    let rect = ctx
        .read_response(egui::Id::unique(("video-object", 3u64)))
        .unwrap()
        .rect;
    frame(
        &mut e,
        vec![
            egui::Event::PointerMoved(rect.center()),
            button(rect.center(), true),
        ],
    );
    frame(
        &mut e,
        vec![egui::Event::PointerMoved(
            rect.center() + egui::vec2(75., 0.),
        )],
    );
    frame(
        &mut e,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        }],
    );
    frame(&mut e, vec![button(end, false)]);
    assert!(e.video.drag.is_none());
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
}
#[test]
fn object_context_menu_right_click_targets_clicked_block() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.shell.library = false;
    e.shell.backstage = None;
    e.select_only(Some(1));
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
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let r = ctx
        .read_response(egui::Id::unique(("video-object", 3u64)))
        .unwrap()
        .rect;
    for pressed in [true, false] {
        frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(r.center()),
                egui::Event::PointerButton {
                    pos: r.center(),
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    frame(&mut e, vec![]);
    assert_eq!(e.selected, Some(3));
    let r = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("timeline-menu", "Duplicate"))))
        .unwrap();
    for pressed in [true, false] {
        frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(r.center()),
                egui::Event::PointerButton {
                    pos: r.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!(e.session.document.annotations.len(), 5);
    assert_eq!(
        e.session.document.annotations.last().unwrap().kind,
        crate::Kind::Text
    );
}

#[test]
fn video_canvas_uses_image_navigation_without_changing_timeline_or_document() {
    use crate::canvas::tests::frame;
    use crate::canvas_view::{ZoomAction, ZoomMode};
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.tool = None;
    e.view.apply(ZoomAction::Set(ZoomMode::Manual(1.)), None);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let timeline_zoom = e.video.zoom;
    for delta in [100., -100.] {
        let initial = frame(&ctx, &mut e, vec![]);
        let anchor = initial.viewport.center();
        let point = initial.image_point(anchor);
        let changed = frame(
            &ctx,
            &mut e,
            vec![
                egui::Event::PointerMoved(anchor),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0., delta),
                    modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
                },
            ],
        );
        assert!((changed.zoom - initial.zoom) * delta > 0.);
        assert!((changed.screen_point(point) - anchor).length() < 0.001);
    }
    for key in [egui::Key::Plus, egui::Key::Minus, egui::Key::Num0] {
        let mut action = None;
        let _ = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ui| {
                action = ui.input_mut(|i| crate::canvas_view::shortcut(i, true));
            },
        );
        e.view
            .apply(action.expect("same image shortcut must work"), None);
    }
    assert_eq!(e.video.zoom, timeline_zoom);
    assert_eq!(e.view.mode, ZoomMode::Manual(1.));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
}
