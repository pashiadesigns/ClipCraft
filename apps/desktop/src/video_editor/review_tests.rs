use super::*;
use capture_core::video::editor::{Animation, MediaKind};

#[test]
fn properties_auto_fit_respects_session_overrides_and_recovers_when_space_returns() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let frame = |height: f32, events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(390., height),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                property_design::begin(ui, "fit-test");
                let available = ui.available_height();
                let top = ui.next_widget_position().y;
                for title in ["Crop", "Outline"] {
                    property_design::group(ui, "Style", title, "", |ui| {
                        ui.allocate_exact_size(egui::vec2(200., 100.), egui::Sense::hover());
                    });
                }
                property_design::finish(ui, ui.next_widget_position().y - top, available);
            },
        );
        output.textures_delta.clear();
    };
    let state = |title| {
        ctx.data(|d| {
            d.get_temp::<(egui::Rect, bool)>(egui::Id::unique(("property-group-state", title)))
                .unwrap()
        })
    };
    for _ in 0..3 {
        frame(500., vec![]);
    }
    assert!(state("Crop").1 && state("Outline").1);
    for _ in 0..3 {
        frame(180., vec![]);
    }
    assert!(!state("Crop").1 && !state("Outline").1);
    let pos = state("Crop").0.center();
    for pressed in [true, false] {
        frame(
            180.,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    assert!(state("Crop").1);
    for _ in 0..3 {
        frame(500., vec![]);
    }
    assert!(state("Crop").1 && state("Outline").1);
    for _ in 0..3 {
        frame(180., vec![]);
    }
    assert!(state("Crop").1 && !state("Outline").1);
}

#[test]
fn video_text_outline_retains_selection_and_commits_in_properties() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(tests::fixture()).unwrap();
    e.select_only(Some(3));
    e.tool = Some(crate::Kind::Text);
    e.video.tab = PropertyTab::Style;
    e.video.object_tab = PropertyTab::Style;
    let frame = |e: &mut Editor, events| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(390., 1200.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                e.prepare_property_input(&ctx);
                e.video_properties_pane(ui);
                e.settle_property_preview(&ctx);
            },
        );
        out.textures_delta.clear();
    };
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    let rect = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("text-outline-size"))
            .unwrap()
    });
    for pressed in [true, false] {
        frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
    frame(&mut e, vec![]);
    assert_eq!(e.selected, Some(3));
    assert!(
        e.selected_annotation()
            .unwrap()
            .text
            .unwrap()
            .outline
            .is_some(),
        "{}",
        e.status
    );
    assert_eq!(e.session.document.video.as_ref().unwrap().cursor, 4500);
}

#[test]
fn audio_style_edits_keep_playback_clock_and_separation_keeps_linked_timing() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(tests::fixture()).unwrap();
    e.session.document.video.as_mut().unwrap().segments[0]
        .audio
        .push(capture_core::video::editor::Audio {
            path: "source.wav".into(),
            label: "System audio".into(),
            volume: 1.,
            muted: false,
            fade_in: 0,
            fade_out: 0,
        });
    e.video.playing = true;
    e.video.clock = Some((Instant::now(), 4500));
    let clock = e.video.clock;
    let mut p = e.session.document.video.clone().unwrap();
    p.segments[0].audio[0].volume = 0.5;
    e.commit_video(p);
    assert!(e.video.playing);
    assert_eq!(e.video.clock, clock);
    let mut p = e.session.document.video.clone().unwrap();
    let original = p.segments[0].clone();
    p.separate_audio(0).unwrap();
    assert!(p.segments[0].audio.is_empty());
    let audio = p.segments.last().unwrap();
    assert_eq!(audio.kind(), MediaKind::Audio);
    assert_eq!(
        (audio.at, audio.start, audio.end),
        (original.at, original.start, original.end)
    );
    assert_eq!(audio.link, p.segments[0].link);
    assert_eq!(audio.audio, original.audio);
}

#[test]
fn faded_text_inks_and_closed_path_directions_are_consistent() {
    let d = tests::fixture();
    let mut a = d.annotations[2].clone();
    let mut t = d.video.as_ref().unwrap().objects[&a.id].clone();
    a.text.as_mut().unwrap().outline = Some(Default::default());
    t.entrance = Animation::Fade;
    t.entrance_ms = 1000;
    t.smooth = false;
    let faded = capture_core::video::editor::animated(&a, &t, t.start + 500).unwrap();
    assert_eq!(faded.text.unwrap().outline.unwrap().color[3], 128);
    assert_eq!(faded.color[3], 128);
    a.kind = crate::Kind::Rectangle;
    a.text = None;
    a.start = [20., 20.];
    a.end = [100., 100.];
    a.corner_radius = 0.;
    t.entrance = Animation::Path;
    let cw = capture_core::video::editor::animated(&a, &t, t.start + 200).unwrap();
    t.path_reverse = true;
    let ccw = capture_core::video::editor::animated(&a, &t, t.start + 200).unwrap();
    assert_ne!(cw.points, ccw.points);
    assert_eq!(cw.points[0], ccw.points[0]);
    assert_eq!(cw.kind, crate::Kind::Pen);
    a.kind = crate::Kind::Highlighter;
    a.points = vec![[0., 0.], [100., 0.], [100., 100.]];
    t.direction = 0;
    let partial = capture_core::video::editor::animated(&a, &t, t.start + 500).unwrap();
    assert_eq!(partial.points.last(), Some(&[100., 0.]));
}

#[cfg(windows)]
#[test]
fn extended_local_narration_path_opens_and_camera_preview_preserves_aspect() {
    let _runtime = media::Runtime::new().unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut wav = capture_core::video::wav::Writer::new(root.path()).unwrap();
    wav.append(&vec![1000; 9600]).unwrap();
    let path = wav.finish().unwrap();
    let canonical = path.canonicalize().unwrap();
    assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
    assert!(media::Sound::open(&canonical).is_ok());
    for size in [[1280, 720], [1920, 1080], [640, 480]] {
        let data: Vec<_> = (0..size[0] * size[1])
            .flat_map(|_| [20, 40, 80, 255])
            .collect();
        let preview = camera::preview_pixels(&data, size);
        assert!(preview.width() <= 640 && preview.height() <= 360);
        assert!(
            (preview.width() as f64 / preview.height() as f64 - size[0] as f64 / size[1] as f64)
                .abs()
                < 0.005
        );
        assert_eq!(preview.get_pixel(0, 0).0, [80, 40, 20, 255]);
    }
}

#[test]
fn approved_properties_layouts_render_with_adaptive_groups_without_editing_documents() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    for (name, tab, recording, width, height) in [
        ("review-camera-style", PropertyTab::Style, true, 390., 780.),
        (
            "review-camera-timing",
            PropertyTab::Timing,
            true,
            390.,
            780.,
        ),
        ("review-audio", PropertyTab::Audio, true, 390., 780.),
        ("review-text-style", PropertyTab::Style, false, 390., 780.),
        ("review-text-motion", PropertyTab::Motion, false, 390., 780.),
        (
            "review-camera-expanded",
            PropertyTab::Style,
            true,
            390.,
            1800.,
        ),
        ("review-text-narrow", PropertyTab::Style, false, 260., 780.),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = Session::new(tests::fixture()).unwrap();
        let p = e.session.document.video.as_mut().unwrap();
        p.segments[0].label = "Camera · Take 1".into();
        p.segments[0].rect = [0.65, 0.6, 0.3, 0.3];
        p.segments[0].frame = Some(Default::default());
        p.segments[0]
            .audio
            .push(capture_core::video::editor::Audio {
                path: "narration.wav".into(),
                label: "Narration · Take 1".into(),
                volume: 1.,
                muted: false,
                fade_in: 150,
                fade_out: 300,
            });
        if recording {
            e.select_only(None);
            e.video.selected = if tab == PropertyTab::Audio {
                Selection::Audio(0)
            } else {
                Selection::Recording(0)
            };
        } else {
            e.select_only(Some(3));
            e.video.object_tab = tab;
        }
        e.video.tab = tab;
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let render = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, height),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.video_properties_pane(ui);
                    e.settle_property_preview(&ctx);
                    assert!(
                        ui.min_rect().right() < width + 1.,
                        "{name}: {:?}",
                        ui.min_rect()
                    );
                },
            ))
        };
        let mut first = render(&mut e);
        let second = render(&mut e);
        let third = render(&mut e);
        first
            .0
            .textures_delta
            .append(second.0.textures_delta.clone());
        assert_eq!(
            serde_json::to_vec(&e.session.document).unwrap(),
            before,
            "{name} passive layout changed document"
        );
        if let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
            dump_meshes(&ctx, &folder.to_string_lossy(), name, first, third);
        }
    }
}

#[test]
fn live_sources_keep_pixels_shared_apply_geometry_immediately_and_coalesce_seek_requests() {
    use capture_core::video::frame;
    let mut d = tests::fixture();
    d.width = 1920;
    d.height = 1080;
    d.annotations.clear();
    let p = d.video.as_mut().unwrap();
    p.objects.clear();
    p.segments[0].frame = Some(frame::Frame {
        width: 4,
        ..Default::default()
    });
    let pixels = Arc::new(image::RgbaImage::from_pixel(
        1920,
        1080,
        image::Rgba([30, 120, 220, 255]),
    ));
    let sources = vec![(
        p.segments[0].id,
        Arc::new(capture_core::bitmap::Bitmap::transient(pixels.clone()).unwrap()),
    )];
    let started = Instant::now();
    for _ in 0..120 {
        let objects = frame::source_objects(&d, &sources, 500, false);
        assert!(Arc::ptr_eq(
            objects[0].image.as_ref().unwrap().pixels().unwrap(),
            &pixels
        ));
    }
    let elapsed = started.elapsed().as_secs_f64() * 1000.;
    let before = frame::source_objects(&d, &sources, 500, false);
    let c = &mut d.video.as_mut().unwrap().segments[0];
    c.rect = [0.1, 0.2, 0.6, 0.6];
    let f = c.frame.as_mut().unwrap();
    f.shape = crate::Kind::Ellipse;
    f.width = 12;
    f.opacity = 128;
    f.view.rect = [0.2, 0.1, 0.5, 0.6];
    let live = frame::source_objects(&d, &sources, 500, false);
    assert_ne!(before[0].start, live[0].start);
    assert_eq!(live[0].kind, crate::Kind::Ellipse);
    assert_eq!(live[0].width, 12);
    assert!(Arc::ptr_eq(
        live[0].image.as_ref().unwrap().pixels().unwrap(),
        &pixels
    ));
    let requests = workers::Latest::default();
    for n in 0..100 {
        requests.send(n).unwrap();
    }
    assert_eq!(requests.take(), Some(99));
    assert_eq!(requests.take(), None);
    if let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            PathBuf::from(folder).join("live-preview-performance.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "source_size": [1920,1080], "iterations":120, "geometry_total_ms":elapsed,
                "source_pixels_shared":true,"latest_seek_replaces_pending":true,
                "native_display_frame_rate_measured":false
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn save_group_uses_a_timeline_multi_selection() {
    use capture_core::video::editing::Item;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(tests::fixture()).unwrap();
    assert!(e.group_preset_ids().len() < 2);
    e.timeline_focus(Item::Annotation(1), false);
    e.timeline_focus(Item::Annotation(3), true);
    e.timeline_focus(
        Item::Media(e.session.document.video.as_ref().unwrap().segments[0].id),
        true,
    );
    assert!(
        e.selected_objects().len() < 2,
        "the canvas holds one object"
    );
    assert_eq!(e.group_preset_ids(), vec![1, 3]);
    e.save_group_preset();
    assert!(e.status.is_empty(), "{}", e.status);
    match e.presets.reusable.pending_save() {
        Some(capture_core::reusable_presets::Payload::Group(c)) => {
            assert_eq!(c.objects.objects.len(), 2)
        }
        _ => panic!("a group preset is being saved"),
    }
}
#[test]
fn timeline_selection_context_menu_and_zoom_preserve_project_playback_position() {
    use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(tests::fixture()).unwrap();
    e.session.document.video.as_mut().unwrap().cursor = 4500;
    e.video.playing = true;
    e.video.clock = Some((Instant::now(), 4500));
    e.history.recent = false;
    e.tool = None;
    let clock = e.video.clock;
    let render = |e: &mut Editor, mut events: Vec<Event>, modifiers| {
        events.insert(0, Event::ModifiersChanged(modifiers));
        let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440., 960.))),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| e.show_shell(ui),
        ));
    };
    for _ in 0..3 {
        render(&mut e, vec![], Modifiers::NONE);
    }
    let id = e.session.document.video.as_ref().unwrap().segments[0].id;
    let rect = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique(("timeline-media-block", id)))
            .unwrap()
    });
    for button in [PointerButton::Primary, PointerButton::Secondary] {
        for pressed in [true, false] {
            render(
                &mut e,
                vec![
                    Event::PointerMoved(rect.center()),
                    Event::PointerButton {
                        pos: rect.center(),
                        button,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                Modifiers::NONE,
            );
        }
        assert_eq!(e.video_time(), 4500);
        assert_eq!(e.video.clock, clock);
        assert!(e.video.playing);
    }
    // Dismiss only the menu, then zoom over the timeline and outside it.
    egui::Popup::close_all(&ctx);
    let zoom = e.video.zoom;
    render(
        &mut e,
        vec![Event::PointerMoved(rect.center()), Event::Zoom(1.25)],
        Modifiers::CTRL,
    );
    assert!(e.video.zoom > zoom);
    let zoom = e.video.zoom;
    render(
        &mut e,
        vec![
            Event::PointerMoved(egui::pos2(400., 300.)),
            Event::Zoom(1.25),
        ],
        Modifiers::CTRL,
    );
    assert_eq!(e.video.zoom, zoom);
    assert_eq!(e.video_time(), 4500);
}
