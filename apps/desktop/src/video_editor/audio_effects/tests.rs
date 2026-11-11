use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use capture_core::video::effects::{Band, BandKind, Effect};
fn editor(ctx: &egui::Context) -> Editor {
    crate::shell::configure(ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    crate::text_layout::configure(ctx);
    let mut e = crate::canvas::tests::editor(ctx);
    e.session = Session::new(super::super::tests::fixture()).unwrap();
    e.session.document.video.as_mut().unwrap().segments[0]
        .audio
        .push(capture_core::video::editor::Audio {
            path: "voice.wav".into(),
            label: "Microphone".into(),
            muted: false,
            volume: 1.,
            fade_in: 0,
            fade_out: 0,
        });
    e.select_only(None);
    e.video.selected = Selection::Recording(0);
    e.open_effects_workstation();
    let chain = Chain::narration();
    e.session
        .document
        .video
        .as_mut()
        .unwrap()
        .clip_effects
        .insert(1, chain);
    e.video.fx.selected = 1;
    e.video.fx.position = Some(egui::pos2(20., 20.));
    e
}
fn render(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<egui::Event>,
    width: f32,
) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 800.),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            ui.painter()
                .rect_filled(ui.max_rect(), 0., egui::Color32::from_rgb(17, 22, 23));
            e.show_effects_workstation(ctx);
        },
    ))
}
#[test]
fn effects_edits_keep_audio_worker_clock_and_originals_while_undoing() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let original = e.session.document.video.clone().unwrap();
    let tx = workers::Latest::default();
    let rx = tx.clone();
    e.video.audio_updates = Some(tx);
    let stop = Arc::new(AtomicBool::new(false));
    e.video.audio_stop = Some(stop.clone());
    e.video.playing = true;
    let clock = (Instant::now(), 4500);
    e.video.clock = Some(clock);
    let mut chain = Chain::narration();
    chain.effects[0]
        .bands
        .push(Band::new(BandKind::HighPass, 90.));
    chain.effects[1].values[0] = -28.;
    e.commit_chain(chain.clone());
    assert!(!stop.load(Ordering::Relaxed));
    assert_eq!(e.video.clock, Some(clock));
    assert!(e.video.playing);
    assert_eq!(rx.take().unwrap().clip_effects[&1], chain);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().source,
        original.source
    );
    e.execute_audio_edit(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().clip_effects,
        original.clip_effects
    );
    assert!(!stop.load(Ordering::Relaxed));
    assert_eq!(e.video.clock, Some(clock));
    e.execute_audio_edit(Command::Redo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().clip_effects[&1],
        chain
    );
    assert_eq!(e.video.clock, Some(clock));
    assert!(original.same_visual(e.session.document.video.as_ref().unwrap()));
}
#[test]
fn workstation_geometry_icons_and_real_spectrum_screenshots() {
    for width in [880., 1024., 1440.] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let mut bounds = None;
        let mut textures =
            std::collections::HashMap::<egui::TextureId, Vec<egui::epaint::ImageDelta>>::new();
        for (i, kind) in Kind::ALL.into_iter().enumerate() {
            if i == 6 {
                e.session
                    .document
                    .video
                    .as_mut()
                    .unwrap()
                    .clip_effects
                    .get_mut(&1)
                    .unwrap()
                    .add(kind)
                    .unwrap();
            }
            e.video.fx.selected = i as u64 + 1;
            if width == 1440. {
                let chain = &e.session.document.video.as_ref().unwrap().clip_effects[&1];
                let mut processor = capture_core::video::effects::Processor::new(chain).unwrap();
                processor.set_visualization(Some(e.video.fx.selected));
                for block in 0..100 {
                    let mut samples: Vec<_> = (0..2400)
                        .map(|n| {
                            let t = (block * 2400 + n) as f64 / 48000.;
                            let env = 0.25 + 0.22 * (t * 3.).sin();
                            let v = ((t * std::f64::consts::TAU * 185.).sin() * 0.8
                                + (t * std::f64::consts::TAU * 6500.).sin() * 0.2)
                                * env;
                            [v, v]
                        })
                        .collect();
                    processor.process(&mut samples);
                }
                e.video.meters.insert(Target::Clip(1), processor.meters);
            }
            let mut first = render(&ctx, &mut e, vec![], width);
            for (id, deltas) in &first.textures_delta.set {
                textures.entry(*id).or_default().extend(deltas.clone());
            }
            for _ in 0..3 {
                let out = render(&ctx, &mut e, vec![], width);
                for (id, deltas) in &out.textures_delta.set {
                    textures.entry(*id).or_default().extend(deltas.clone());
                }
            }
            let output = render(&ctx, &mut e, vec![], width);
            first.0.textures_delta.set = textures
                .iter()
                .map(|(id, v)| (*id, v.iter().cloned().collect()))
                .collect();
            for (id, deltas) in &output.textures_delta.set {
                textures.entry(*id).or_default().extend(deltas.clone());
            }
            let r: egui::Rect =
                ctx.data(|d| d.get_temp(egui::Id::unique("effects-window")).unwrap());
            assert!(r.right() <= width && r.bottom() <= 800., "{kind:?}: {r:?}");
            let size = egui::vec2(r.width(), r.height());
            if let Some(b) = bounds {
                assert!(
                    (size - b).length() < 0.1,
                    "unstable effect window for {kind:?}: {size:?} / {b:?}"
                );
            } else {
                bounds = Some(size);
            }
            if width == 1440.
                && let Some(root) = std::env::var_os("CLIPDESIGNER_VOICEOVER_EVIDENCE")
            {
                dump_meshes(
                    &ctx,
                    &root.to_string_lossy(),
                    &format!("effect-{kind:?}"),
                    first,
                    output,
                );
            }
        }
        e.video.fx.analysis_open = true;
        let mut first = render(&ctx, &mut e, vec![], width);
        let output = render(&ctx, &mut e, vec![], width);
        for (id, deltas) in &first.textures_delta.set {
            textures.entry(*id).or_default().extend(deltas.clone());
        }
        first.0.textures_delta.set = textures
            .into_iter()
            .map(|(id, v)| (id, v.into_iter().collect()))
            .collect();
        let analysis_rect: egui::Rect =
            ctx.data(|d| d.get_temp(egui::Id::unique("effects-window")).unwrap());
        assert!(
            (analysis_rect.size() - bounds.unwrap()).length() < 0.1,
            "Analysis changed window size: {:?} / {:?}",
            analysis_rect.size(),
            bounds
        );
        if width == 1440.
            && let Some(root) = std::env::var_os("CLIPDESIGNER_VOICEOVER_EVIDENCE")
        {
            dump_meshes(
                &ctx,
                &root.to_string_lossy(),
                "analyze-setup",
                first,
                output,
            );
        }
    }
    for name in [
        "sliders-horizontal",
        "scan-line",
        "power",
        "trash-2",
        "chart-no-axes-combined",
        "minimize-2",
        "audio-lines",
        "equal",
        "shield-check",
        "list-filter",
    ] {
        for px in [18, 27, 36] {
            let image = crate::ui_glyphs::render(name, px).unwrap();
            assert!(image.pixels().any(|p| p[3] > 0 && p[3] < 255), "{name}");
        }
    }
}

#[test]
fn every_vocal_preset_is_valid_and_preserves_effect_identity() {
    for kind in Kind::ALL {
        let choices = presets::choices(kind, 17);
        assert!(choices.len() >= 3);
        for (_, effect) in choices {
            assert_eq!(effect.id, 17);
            assert_eq!(effect.kind, kind);
            Chain {
                bypass: false,
                mix: 1.,
                effects: vec![effect],
            }
            .validate()
            .unwrap();
        }
    }
}

#[test]
fn adaptive_strength_updates_applied_clip_chain_and_undo() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let pcm: Vec<i16> = (0..96000)
        .flat_map(|i| {
            let t = i as f64 / 48000.;
            let v = 0.25 * (t * 340. * std::f64::consts::TAU).sin()
                + 0.05 * (t * 960. * std::f64::consts::TAU).sin()
                + 0.012 * (t * 2720. * std::f64::consts::TAU).sin();
            [(v * 32767.) as i16; 2]
        })
        .collect();
    let mut analyzer = Analyzer::default();
    analyzer.push(&pcm);
    e.video.fx.report = Some(analyzer.report());
    e.video.fx.analysis_open = true;
    e.video.fx.strength = 10.;
    e.update_audio_suggestion();
    let gentle = e.video.fx.proposal.clone().unwrap();
    e.video.fx.strength = 85.;
    e.update_audio_suggestion();
    let strong = e.video.fx.proposal.clone().unwrap();
    assert_ne!(gentle.effects[0].bands, strong.effects[0].bands);
    let before = e.session.document.video.clone();
    let first = render(&ctx, &mut e, vec![], 1440.);
    let out = render(&ctx, &mut e, vec![], 1440.);
    fn text_pos(s: &egui::Shape) -> Option<egui::Pos2> {
        match s {
            egui::Shape::Text(t) if t.galley.text() == "Apply to clip" => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            egui::Shape::Vec(v) => v.iter().find_map(text_pos),
            _ => None,
        }
    }
    let at = out
        .0
        .shapes
        .iter()
        .find_map(|s| text_pos(&s.shape))
        .unwrap();
    if let Some(root) = std::env::var_os("CLIPDESIGNER_VOICEOVER_EVIDENCE") {
        dump_meshes(
            &ctx,
            &root.to_string_lossy(),
            "adaptive-analysis",
            first,
            out,
        );
    }
    for pressed in [true, false] {
        render(
            &ctx,
            &mut e,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            1440.,
        );
    }
    assert_eq!(
        e.session.document.video.as_ref().unwrap().clip_effects[&1],
        strong
    );
    assert!(!e.video.fx.analysis_open);
    e.execute_audio_edit(Command::Undo);
    assert_eq!(e.session.document.video, before);
}

#[test]
fn suggested_chain_comparison_never_commits_and_cancel_restores_track() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let stored = e.session.document.video.as_ref().unwrap().clip_effects[&1].clone();
    let mut suggested = stored.clone();
    suggested.effects[1].values[0] = -30.;
    let tx = workers::Latest::default();
    let rx = tx.clone();
    e.video.audio_updates = Some(tx);
    e.video.fx.proposal = Some(suggested.clone());
    e.video.fx.analysis_open = true;
    e.video.fx.auditioning = true;
    e.preview_chain(&stored);
    assert_eq!(rx.take().unwrap().clip_effects[&1], suggested);
    e.video.fx.original = true;
    e.preview_chain(&stored);
    assert!(rx.take().unwrap().clip_effects[&1].bypass);
    e.video.fx.original = false;
    e.preview_chain(&stored);
    assert_eq!(rx.take().unwrap().clip_effects[&1], suggested);
    e.video.fx.analysis_open = false;
    e.video.fx.auditioning = false;
    e.preview_chain(&stored);
    assert_eq!(rx.take().unwrap().clip_effects[&1], stored);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().clip_effects[&1],
        stored
    );
}

#[test]
fn measured_spectrum_and_analysis_results_keep_workstation_geometry() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let mut chain = Chain::narration();
    chain.effects[0].bands[0].gain = -3.;
    chain.effects[0].bands[2].gain = 2.;
    chain.effects[0]
        .bands
        .push(Band::new(BandKind::HighPass, 65.));
    chain.effects[0]
        .bands
        .push(Band::new(BandKind::LowPass, 15000.));
    let mut pcm = Vec::new();
    let mut samples = Vec::new();
    for i in 0..48000 {
        let x = [120., 360., 1100., 3500., 6500.]
            .iter()
            .enumerate()
            .map(|(j, f)| {
                (std::f64::consts::TAU * f * i as f64 / 48000.).sin() * 0.08 / (j + 1) as f64
            })
            .sum::<f64>();
        samples.push([x, x]);
        pcm.extend([(x * 32768.) as i16; 2]);
    }
    let mut processor = capture_core::video::effects::Processor::new(&chain).unwrap();
    processor.set_visualization(Some(1));
    processor.process(&mut samples);
    e.video
        .meters
        .insert(Target::Clip(1), processor.meters.clone());
    e.session
        .document
        .video
        .as_mut()
        .unwrap()
        .clip_effects
        .insert(1, chain);
    let first = render(&ctx, &mut e, vec![], 1440.);
    let output = render(&ctx, &mut e, vec![], 1440.);
    let bounds: egui::Rect = ctx.data(|d| d.get_temp(egui::Id::unique("effects-window")).unwrap());
    if let Some(root) = std::env::var_os("CLIPDESIGNER_VOICEOVER_EVIDENCE") {
        dump_meshes(
            &ctx,
            &root.to_string_lossy(),
            "eq-live-spectrum",
            first,
            output,
        );
    }
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let mut analyzer = Analyzer::default();
    analyzer.push(&pcm);
    e.video.fx.report = Some(analyzer.report());
    e.update_audio_suggestion();
    e.video.fx.analysis_open = true;
    let first = render(&ctx, &mut e, vec![], 1440.);
    let output = render(&ctx, &mut e, vec![], 1440.);
    let r: egui::Rect = ctx.data(|d| d.get_temp(egui::Id::unique("effects-window")).unwrap());
    assert!((r.size() - bounds.size()).length() < 0.1);
    if let Some(root) = std::env::var_os("CLIPDESIGNER_VOICEOVER_EVIDENCE") {
        dump_meshes(
            &ctx,
            &root.to_string_lossy(),
            "analyze-results",
            first,
            output,
        );
    }
}
#[test]
fn eq_curve_creates_and_moves_bands_without_changing_original_audio() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    for _ in 0..5 {
        render(&ctx, &mut e, vec![], 1440.);
    }
    let plot: egui::Rect = ctx.data(|d| d.get_temp(egui::Id::unique("effects-plot")).unwrap());
    let pos = egui::pos2(plot.left() + plot.width() * 0.85, plot.center().y);
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    for pressed in [true, false] {
        render(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(pos), event(pos, pressed)],
            1440.,
        );
    }
    assert_eq!(
        e.session.document.video.as_ref().unwrap().clip_effects[&1].effects[0]
            .bands
            .len(),
        4
    );
    render(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(pos), event(pos, true)],
        1440.,
    );
    let end = pos + egui::vec2(-25., -22.);
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(end)], 1440.);
    render(&ctx, &mut e, vec![event(end, false)], 1440.);
    let edited = &e.session.document.video.as_ref().unwrap().clip_effects[&1].effects[0];
    assert!(edited.bands[3].gain > 2.);
    assert_eq!(edited.bands.len(), 4);
    e.execute_audio_edit(Command::Undo);
    assert_eq!(
        e.session.document.video.as_ref().unwrap().clip_effects[&1].effects[0].bands[3].gain,
        0.
    );
}
#[cfg(windows)]
#[test]
fn track_effects_preview_export_portable_save_and_analysis_use_original_audio() {
    let root = tempfile::tempdir().unwrap();
    let _runtime = media::Runtime::new().unwrap();
    let samples: Vec<i16> = (0..96000)
        .flat_map(|i| {
            let x = ((std::f64::consts::TAU * 300. * i as f64 / 48000.).sin() * 7000.) as i16;
            [x, x]
        })
        .collect();
    let mut w = capture_core::video::wav::Writer::new(root.path()).unwrap();
    w.append(&samples).unwrap();
    let file = w.finish().unwrap();
    let original = std::fs::read(&file).unwrap();
    let clips = super::super::imports::import_files(std::slice::from_ref(&file)).unwrap();
    let mut d = Document::blank(320, 180).unwrap();
    let mut p = Project::new(file.to_string_lossy().into_owned(), [320, 180], 2000, 30);
    p.segments = clips;
    p.segments[0].track = 1;
    p.segments[0].id = 1;
    let mut eq = Effect::new(1, Kind::Equalizer);
    eq.bands = vec![Band::new(BandKind::HighPass, 1500.)];
    let c = Chain {
        bypass: false,
        mix: 1.,
        effects: vec![eq],
    };
    p.clip_effects.insert(1, c.clone());
    d.video = Some(p.clone());
    let mut mixer = media::Mixer::new(&p).unwrap();
    let processed = mixer.samples(&p, 0, 48000).unwrap();
    let rms =
        |s: &[i16]| (s.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / s.len() as f64).sqrt();
    assert!(rms(&processed) < rms(&samples) * 0.06);
    p.clip_effects.get_mut(&1).unwrap().bypass = true;
    let bypass = mixer.samples(&p, 48000, 48000).unwrap();
    assert!(rms(&bypass) > rms(&samples) * 0.99);
    let export = root.path().join("voice.mp4");
    workers::export_sync(&d, &export, &AtomicBool::new(false), |_| ()).unwrap();
    let mut read = media::Sound::open(&export).unwrap();
    let decoded: Vec<i16> = (24000..48000)
        .flat_map(|i| read.value(i).unwrap())
        .collect();
    assert!((rms(&decoded) / rms(&processed) - 1.).abs() < 0.2);
    let project = root.path().join("Voice.vclip");
    d.save(&project).unwrap();
    let reopened = capture_core::project_file::open(&project).unwrap();
    assert_eq!(reopened.video.as_ref().unwrap().clip_effects[&1], c);
    assert_eq!(std::fs::read(&file).unwrap(), original);
    let mut raw = p.clone();
    raw.clip_effects.clear();
    let job = analyze(raw);
    let report = job
        .result
        .recv_timeout(Duration::from_secs(20))
        .unwrap()
        .unwrap();
    assert!(report.usable);
    assert!(report.rms_db > -20.);
    assert_eq!(std::fs::read(&file).unwrap(), original);
}
