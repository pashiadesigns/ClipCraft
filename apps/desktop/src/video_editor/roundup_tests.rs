use super::*;

#[test]
fn image_frame_layers_and_tool_defaults_keep_controls_inside_the_pane() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    for width in [260., 390.] {
        for scene in [
            "frame-empty",
            "frame-image",
            "frame-layout",
            "layers",
            "next-shape",
        ] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            crate::text_layout::configure(&ctx);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.session = Session::new(
                capture_core::templates::build(
                    "two-steps",
                    "Image guide",
                    &[],
                    capture_core::templates::Scale::Fill,
                )
                .unwrap(),
            )
            .unwrap();
            let a = e
                .session
                .document
                .annotations
                .iter_mut()
                .find(|a| a.is_image_frame())
                .unwrap();
            let id = a.id;
            if scene == "frame-image" {
                let payload = capture_core::clipboard_payload::ImagePayload::from_rgba(
                    64,
                    48,
                    vec![180; 64 * 48 * 4],
                )
                .unwrap();
                a.image = Some(std::sync::Arc::new(
                    capture_core::bitmap::Bitmap::new(payload.image).unwrap(),
                ));
            }
            e.select_only(Some(id));
            e.tool = None;
            if scene == "layers" {
                e.shell.property_tab = crate::shell::PropertyTab::Objects;
            }
            if scene == "next-shape" {
                e.select_only(None);
                e.tool = Some(crate::Kind::Rectangle);
            }
            let draw = |e: &mut Editor, events| {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        e.image_properties_pane(ui);
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::unique("image-scene-bounds"), ui.min_rect())
                        });
                    },
                ))
            };
            let mut first = draw(&mut e, vec![]);
            let mut merge = |out: &PaintedOutput| {
                for (id, delta) in &out.0.textures_delta.set {
                    first
                        .0
                        .textures_delta
                        .set
                        .entry(*id)
                        .or_default()
                        .extend(delta.clone());
                }
            };
            for _ in 0..3 {
                merge(&draw(&mut e, vec![]));
            }
            if scene.starts_with("frame-") {
                let tab = if scene == "frame-layout" {
                    "Layout"
                } else {
                    "Image"
                };
                let point = ctx
                    .data(|d| {
                        d.get_temp::<egui::Rect>(egui::Id::unique(("image-property-tab", tab)))
                    })
                    .unwrap()
                    .center();
                for down in [true, false] {
                    merge(&draw(&mut e, super::repair_tests::press(point, down)));
                }
            }
            for _ in 0..3 {
                merge(&draw(&mut e, vec![]));
            }
            let last = draw(&mut e, vec![]);
            let bounds = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("image-scene-bounds")))
                .unwrap();
            assert!(bounds.width() <= width + 0.1, "{scene} {width}: {bounds:?}");
            let delete = ctx
                .data(|d| {
                    d.get_temp::<(egui::Rect, egui::Color32)>(egui::Id::unique(
                        "delete-object-button",
                    ))
                })
                .unwrap()
                .0;
            assert!(delete.bottom() <= 900.1, "{scene} {width}: {delete:?}");
            if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
                dump_meshes(
                    &ctx,
                    &folder.to_string_lossy(),
                    &format!("image-{scene}-{width}"),
                    first,
                    last,
                );
            }
        }
    }
}

#[test]
fn image_properties_contexts_fit_and_render_without_changing_objects() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    for width in [260., 390., 460.] {
        for kind in [
            crate::Kind::Rectangle,
            crate::Kind::Text,
            crate::Kind::Step,
            crate::Kind::Arrow,
            crate::Kind::Redact,
            crate::Kind::Blur,
            crate::Kind::Image,
            crate::Kind::Callout,
            crate::Kind::Highlighter,
            crate::Kind::Eraser,
        ] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            crate::text_layout::configure(&ctx);
            let mut e = crate::canvas::tests::editor(&ctx);
            let mut a = capture_core::steps::marker(
                1,
                [180., 150.],
                80,
                [20, 155, 194, 255],
                (1, false),
                [960, 540],
            )
            .unwrap();
            a.kind = kind;
            if kind == crate::Kind::Image {
                a = capture_core::icon_library::builtin::find("builtin-five-color")
                    .unwrap()
                    .payload()
                    .unwrap()
                    .objects
                    .remove(0);
                a.id = 1;
            }
            if matches!(
                kind,
                crate::Kind::Blur | crate::Kind::Redact | crate::Kind::Arrow
            ) {
                a.text = None;
            }
            e.session = Session::new(Document::blank(960, 540).unwrap()).unwrap();
            e.session.document.annotations = vec![a.clone()];
            e.select_only(Some(1));
            e.tool = None;
            let mut draw = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 900.),
                        )),
                        focused: true,
                        ..Default::default()
                    },
                    |ui| {
                        e.image_properties_pane(ui);
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::unique("image-test-bounds"), ui.min_rect())
                        });
                    },
                ))
            };
            let mut first = draw();
            let middle = draw();
            for (id, deltas) in &middle.0.textures_delta.set {
                first
                    .0
                    .textures_delta
                    .set
                    .entry(*id)
                    .or_default()
                    .extend(deltas.clone());
            }
            let last = draw();
            assert_eq!(
                e.session.document.annotations[0], a,
                "Passive {:?} changed",
                kind
            );
            if let Some(folder) = std::env::var_os("CLIPCRAFT_VIDEO_EDITOR_EVIDENCE") {
                dump_meshes(
                    &ctx,
                    &folder.to_string_lossy(),
                    &format!("image-properties-{kind:?}-{width}"),
                    first,
                    last,
                );
            }
            let bounds = ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("image-test-bounds")))
                .unwrap();
            let delete = ctx
                .data(|d| {
                    d.get_temp::<(egui::Rect, egui::Color32)>(egui::Id::unique(
                        "delete-object-button",
                    ))
                })
                .unwrap()
                .0;
            assert!(delete.bottom() <= 900.1, "{kind:?} {width}: {delete:?}");
            if let Some(layout) = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("image-property-tab", "Layout")))
            }) {
                assert!(layout.right() <= width + 0.1, "{kind:?}: {layout:?}");
            }
            assert!(
                bounds.width() <= width + 0.1,
                "{kind:?} at {width}: {bounds:?}"
            );
        }
    }
}

#[test]
fn video_zoom_buttons_own_repeated_press_release_without_moving_the_frame() {
    let ctx = egui::Context::default();
    let mut e = super::repair_tests::editor(&ctx);
    e.select_only(None);
    e.video.selected = Selection::Recording(0);
    e.tool = None;
    let rect = e.session.document.video.as_ref().unwrap().segments[0].rect;
    for _ in 0..3 {
        super::repair_tests::render(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(egui::pos2(400., 400.))],
        );
    }
    for i in [1usize, 1, 0, 1, 0, 0] {
        let before = e.session.document.video.as_ref().unwrap().segments[0]
            .frame
            .clone()
            .unwrap_or_default()
            .view;
        let point = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("video-frame-zoom-rect", i))))
            .unwrap()
            .center();
        for down in [true, false] {
            super::repair_tests::render(&ctx, &mut e, super::repair_tests::press(point, down));
            assert!(e.video.frame_drag.is_none());
        }
        let after = e.session.document.video.as_ref().unwrap().segments[0]
            .frame
            .clone()
            .unwrap()
            .view;
        assert_ne!(before, after);
        assert_eq!(
            e.session.document.video.as_ref().unwrap().segments[0].rect,
            rect
        );
    }
}

#[test]
fn image_tabs_keep_context_and_all_pages_fit_the_pane() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session.document.annotations = vec![
        capture_core::steps::marker(
            1,
            [150., 150.],
            80,
            [0, 150, 200, 255],
            (1, false),
            [640, 400],
        )
        .unwrap(),
    ];
    e.select_only(Some(1));
    e.tool = None;
    let mut draw = |events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(260., 900.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                e.image_properties_pane(ui);
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("image-pane-size"), ui.min_rect())
                });
            },
        );
        output.textures_delta.clear();
    };
    for _ in 0..3 {
        draw(vec![]);
    }
    for tab in ["Text", "Layout", "Style", "Text"] {
        let pos = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("image-property-tab", tab))))
            .unwrap()
            .center();
        for down in [true, false] {
            draw(super::repair_tests::press(pos, down));
        }
        draw(vec![]);
        let bounds = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("image-pane-size")))
            .unwrap();
        assert!(bounds.width() <= 260.1, "{tab}: {bounds:?}");
        let delete = ctx
            .data(|d| {
                d.get_temp::<(egui::Rect, egui::Color32)>(egui::Id::unique("delete-object-button"))
            })
            .unwrap()
            .0;
        assert!(delete.bottom() <= 900.1);
    }
}

#[test]
fn every_builtin_icon_opens_style_and_motion_without_panicking() {
    let ctx = egui::Context::default();
    let mut e = super::repair_tests::editor(&ctx);
    for icon in capture_core::icon_library::builtin::ALL {
        let mut a = icon.payload().unwrap().objects.remove(0);
        a.id = 100;
        a.start = [100., 100.];
        a.end = [220., 220.];
        e.session.document.annotations = vec![a.clone()];
        let p = e.session.document.video.as_mut().unwrap();
        p.objects.clear();
        p.place(100, capture_core::video::editor::Timing::new(0, 0, 12000));
        p.cursor = 500;
        e.select_only(Some(100));
        e.video.selected = Selection::Objects;
        for tab in [PropertyTab::Style, PropertyTab::Motion] {
            e.video.tab = tab;
            super::repair_tests::render(&ctx, &mut e, vec![]);
            super::repair_tests::render(&ctx, &mut e, vec![]);
        }
        assert_eq!(
            e.session.document.annotations[0], a,
            "Passive pane changed {}",
            icon.label
        );
    }
}

#[test]
fn playing_timeline_context_menus_repeatedly_open_without_seeking_or_stopping() {
    let ctx = egui::Context::default();
    let mut e = super::repair_tests::editor(&ctx);
    e.set_timeline_expanded(true);
    e.video.playing = true;
    for i in 0..80 {
        let cursor = 1000 + i * 80;
        e.session.document.video.as_mut().unwrap().cursor = cursor;
        super::repair_tests::render(&ctx, &mut e, vec![]);
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("timeline-media-block", 1u64))))
            .unwrap();
        let point = rect.center();
        for down in [true, false] {
            super::repair_tests::render(
                &ctx,
                &mut e,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Secondary,
                        pressed: down,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        super::repair_tests::render(&ctx, &mut e, vec![]);
        assert!(e.video.playing);
        assert_eq!(e.session.document.video.as_ref().unwrap().cursor, cursor);
        egui::Popup::close_all(&ctx);
    }
}

#[test]
fn scrolling_combines_with_each_spatial_mode_and_countdown_defaults_off() {
    use capture_core::capture_options::{Selection, Settings};
    for selection in [Selection::Region, Selection::Window, Selection::Fullscreen] {
        let mut s = Settings::for_selection(selection);
        s.scrolling = true;
        s.validate().unwrap();
        assert!(s.is_scrolling());
        let saved: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(saved, s);
    }
    assert!(!capture_core::video::Options::default().countdown);
}

#[test]
fn audio_output_preferences_are_independent_persistent_and_not_project_edits() {
    use capture_core::quick_preferences::{AudioOutputs, Choice, Store};
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let routing = AudioOutputs {
        speakers: "speakers-device".into(),
        headphones: "headphones-device".into(),
        use_headphones: true,
    };
    store
        .remember(Choice::AudioOutputs(routing.clone()))
        .unwrap();
    assert_eq!(
        Store::open(dir.path())
            .unwrap()
            .read()
            .unwrap()
            .audio_outputs,
        routing
    );
    assert_eq!(routing.selected(), "headphones-device");
}

#[test]
fn animation_channels_preserve_body_outline_text_and_save_as_a_preset() {
    use capture_core::video::editor::{Animation, Timing, animated_parts};
    let mut d = super::tests::fixture();
    let a = &mut d.annotations[1];
    a.kind = crate::Kind::Rectangle;
    a.fill = Some([20, 160, 200, 255]);
    a.color = [0, 0, 0, 255];
    a.text.as_mut().unwrap().content = "Explain this action".into();
    let mut t = Timing::new(0, 0, 3000);
    t.entrance = Animation::Fade;
    t.entrance_ms = 1000;
    t.entrance_stack = vec![Animation::Path, Animation::Typewriter];
    let parts = animated_parts(a, &t, 500);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].width, 0);
    assert!(parts[0].fill.unwrap()[3] > 0 && parts[0].fill.unwrap()[3] < 255);
    assert!(parts[0].text.as_ref().unwrap().content.len() < a.text.as_ref().unwrap().content.len());
    assert_eq!(parts[1].kind, crate::Kind::Pen);
    assert!(parts[1].text.is_none());
    let preset = capture_core::reusable_presets::Motion::from(&t);
    let saved: capture_core::reusable_presets::Motion =
        serde_json::from_str(&serde_json::to_string(&preset).unwrap()).unwrap();
    let mut next = Timing::new(0, 0, 3000);
    saved.apply(a, &mut next).unwrap();
    assert_eq!(next, t);
    assert!(!Timing::stack_compatible(
        Animation::Fade,
        &[],
        Animation::Float
    ));
}

#[cfg(windows)]
#[test]
#[ignore = "Measured 21-second 1080p export; run explicitly for performance evidence"]
fn full_hd_export_profile() {
    let root = tempfile::tempdir().unwrap();
    let _runtime = media::Runtime::new().unwrap();
    let source = root.path().join("source.mp4");
    let mut encoder = media::Encoder::new(&source, [1920, 1080], 30, false).unwrap();
    let mut bytes = vec![0; 1920 * 1080 * 4];
    for i in 0..630u64 {
        for (j, p) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *p = [
                ((j % 1920 + i as usize) % 256) as u8,
                (j / 1920 % 256) as u8,
                140,
                255,
            ];
        }
        encoder.video(i * 10_000_000 / 30, &bytes).unwrap();
    }
    encoder.finish().unwrap();
    drop(encoder);
    let d = workers::load(&source).unwrap();
    let p = d.video.as_ref().unwrap();
    let mut reader = media::Visuals::new();
    let mut decode_ms = 0.;
    let mut composite_ms = 0.;
    for i in 0..30 {
        let start = Instant::now();
        let layers = reader.layers(p, [1920, 1080], i * 1000 / 30).unwrap();
        decode_ms += start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        std::hint::black_box(
            capture_core::video::frame::composite(&d, &layers, i * 1000 / 30).unwrap(),
        );
        composite_ms += start.elapsed().as_secs_f64() * 1000.;
    }
    let started = Instant::now();
    let output = root.path().join("export.mp4");
    workers::export_sync(&d, &output, &AtomicBool::new(false), |_| ()).unwrap();
    let export_seconds = started.elapsed().as_secs_f64();
    let mut variants = vec![];
    for name in ["crossfade", "shaped frame"] {
        let mut variant = d.clone();
        let p = variant.video.as_mut().unwrap();
        if name == "crossfade" {
            let mut clip = p.segments[0].clone();
            clip.id = 2;
            clip.at = 500;
            clip.end = 20000;
            p.segments.push(clip);
        } else {
            p.segments[0].rect = [0.25, 0.25, 0.5, 0.5];
            p.segments[0].frame = Some(capture_core::video::frame::Frame {
                shape: crate::Kind::Ellipse,
                ..Default::default()
            });
        }
        let mut reader = media::Visuals::new();
        let start = Instant::now();
        for i in 0..30 {
            let at = 1000 + i * 1000 / 30;
            let layers = reader
                .layers(variant.video.as_ref().unwrap(), [1920, 1080], at)
                .unwrap();
            std::hint::black_box(
                capture_core::video::frame::composite(&variant, &layers, at).unwrap(),
            );
        }
        variants.push(serde_json::json!({"variant":name,"decode_and_composite_ms":start.elapsed().as_secs_f64()*1000./30.}));
    }
    let result = serde_json::json!({"duration_seconds":21,"size":[1920,1080],"fps":30,"export_seconds":export_seconds,"layer_ms_per_frame":decode_ms/30.,"composite_ms_per_frame":composite_ms/30.,"variants":variants});
    println!("{result}");
    if let Some(path) = std::env::var_os("CLIPCRAFT_EXPORT_PROFILE") {
        std::fs::write(path, result.to_string()).unwrap();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "Read-only enumeration of the current Windows output devices"]
fn native_endpoint_inventory() {
    let _runtime = media::Runtime::new().unwrap();
    let choices = crate::capture::windows::video_audio::endpoint_devices(true).unwrap();
    println!("Native output choices: {}", choices.len());
    for (id, name) in choices {
        println!(
            "{}: {name}",
            if id.is_empty() { "default" } else { "endpoint" }
        );
    }
}
#[cfg(windows)]
#[test]
#[ignore = "Read-only representative user project profiling; original media is never changed"]
fn representative_export_profile() {
    let path = std::env::var_os("CLIPCRAFT_PROFILE_PROJECT").expect("project path");
    let d = Document::load(std::path::Path::new(&path)).unwrap();
    let root = tempfile::tempdir().unwrap();
    let start = Instant::now();
    workers::export_sync(
        &d,
        &root.path().join("profile.mp4"),
        &AtomicBool::new(false),
        |_| (),
    )
    .unwrap();
    let result = serde_json::json!({"project_duration_seconds":d.video.as_ref().unwrap().duration() as f64/1000.,"export_seconds":start.elapsed().as_secs_f64(),"size":[d.width,d.height],"objects":d.annotations.len()});
    println!("{result}");
    if let Some(out) = std::env::var_os("CLIPCRAFT_EXPORT_PROFILE") {
        std::fs::write(out, result.to_string()).unwrap();
    }
}

#[cfg(windows)]
#[test]
#[ignore = "Read-only representative crossfade preparation benchmark"]
fn representative_crossfade_profile() {
    let _runtime = media::Runtime::new().unwrap();
    let path = std::env::var_os("CLIPCRAFT_PROFILE_PROJECT").expect("project path");
    let d = Document::load(std::path::Path::new(&path)).unwrap();
    let p = d.video.as_ref().unwrap();
    let pair = p
        .segments
        .iter()
        .find_map(|a| {
            p.segments
                .iter()
                .find(|b| {
                    a.id < b.id
                        && a.track == b.track
                        && a.at < b.until()
                        && b.at < a.until()
                        && a.kind() != capture_core::video::editor::MediaKind::Audio
                })
                .map(|b| (a, b))
        })
        .expect("overlap");
    let start = pair.0.at.max(pair.1.at);
    let end = pair.0.until().min(pair.1.until());
    let mut reader = media::Visuals::new();
    let first = reader.sources(p, start).unwrap();
    let gpu = capture_core::video::frame::texture_dissolve_sources(
        p,
        [d.width, d.height],
        start,
        pair.0.track,
        &first,
    );
    let began = Instant::now();
    let count = ((end - start) * p.fps as u64 / 1000).clamp(30, 180);
    let ctx = egui::Context::default();
    let mut editor = super::repair_tests::editor(&ctx);
    editor.session = Session::new(d.clone()).unwrap();
    editor.video.worker = None;
    editor.video.source = p.source.clone();
    editor.video.frame = Some((
        start,
        Arc::new(image::RgbaImage::new(
            if gpu { 1 } else { d.width },
            if gpu { 1 } else { d.height },
        )),
    ));
    let mut ui_ms = 0.;
    for i in 0..count {
        let at = start + (end - start) * i / count;
        let frames = if gpu {
            reader.sources(p, at)
        } else {
            reader.layers(p, [d.width, d.height], at)
        }
        .unwrap();
        if std::env::var_os("CLIPCRAFT_PROFILE_CANVAS").is_some() {
            editor.video.frame_layers = frames.clone();
            editor.session.document.video.as_mut().unwrap().cursor = at;
            editor.video.frame_revision += 1;
            let tick = Instant::now();
            let output = super::repair_tests::render(&ctx, &mut editor, vec![]);
            let primitives = ctx.tessellate(output.shapes.clone(), 1.);
            std::hint::black_box(primitives);
            ui_ms += tick.elapsed().as_secs_f64() * 1000.;
        }
        std::hint::black_box(if gpu {
            capture_core::video::frame::source_objects(&d, &frames, at, false)
        } else {
            capture_core::video::frame::ordered_objects(&d, &frames, at, false)
        });
    }
    let result = serde_json::json!({"sampled_frames":count,"overlap_ms":end-start,"gpu_dissolve":gpu,"average_total_ms":began.elapsed().as_secs_f64()*1000./count as f64,"average_ui_ms":ui_ms/count as f64,"native_playback_verified":false});
    println!("{result}");
    if let Some(out) = std::env::var_os("CLIPCRAFT_EXPORT_PROFILE") {
        std::fs::write(out, result.to_string()).unwrap();
    }
}
