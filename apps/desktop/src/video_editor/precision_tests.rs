use super::repair_tests::{editor, press, render};
use super::*;
use capture_core::video::{
    editing::Item,
    editor::Audio,
    effects::{Chain, Effect, Kind, Target},
    links::Layer,
};
use std::collections::BTreeSet;

#[derive(Debug)]
struct DropFile(PathBuf);
impl egui::DroppedFile for DropFile {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}
#[test]
fn timeline_file_hover_previews_real_duration_and_drop_imports_at_target() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    image::RgbaImage::from_pixel(80, 60, image::Rgba([10, 90, 170, 255]))
        .save(&path)
        .unwrap();
    let pos = egui::pos2(470., 90.);
    let draw = |e: &mut Editor, hover: bool, drop: bool| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 500.),
            )),
            events: vec![egui::Event::PointerMoved(pos)],
            hovered_files: if hover {
                vec![egui::HoveredFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }]
            } else {
                vec![]
            },
            dropped_files: if drop {
                vec![Arc::new(DropFile(path.clone()))]
            } else {
                vec![]
            },
            ..Default::default()
        };
        crate::property_layout::tests::PaintedOutput(ctx.run_ui(input, |ui| {
            let p = e.session.document.video.clone().unwrap();
            let whole = egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1000., 400.));
            e.video_media_rows(ui, &p, whole, 210., 0.05, 36.);
        }))
    };
    draw(&mut e, true, false);
    let until = Instant::now() + Duration::from_secs(3);
    while e.video.drop_clips.is_empty() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
        draw(&mut e, true, false);
    }
    assert_eq!(e.video.drop_clips.len(), 1, "{}", e.status);
    assert!(e.video.drop_duration.unwrap() > 0);
    let output = draw(&mut e, true, false);
    assert!(
        crate::color_widgets::tests::text_rect(
            &output.0.shapes,
            &format!(
                "Drop · {:.1}s",
                e.video.drop_duration.unwrap() as f64 / 1000.
            )
        )
        .is_some()
    );
    draw(&mut e, false, true);
    assert!(e.video.importing.is_some());
    let until = Instant::now() + Duration::from_secs(3);
    while e.video.importing.is_some() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
        e.poll_media_import(&ctx);
    }
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.segments.len(), 2, "{}", e.status);
    assert_eq!(p.segments[1].at, 5200);
    assert!(e.session.can_undo());
}

#[test]
fn review_screenshots_show_selection_track_scope_and_editable_focus() {
    let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") else {
        return;
    };
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    p.segments[0].audio.push(Audio {
        path: "voice.wav".into(),
        label: "Microphone".into(),
        volume: 1.,
        muted: false,
        fade_in: 0,
        fade_out: 0,
    });
    p.track_effects.insert(1, Chain::narration());
    p.track_settings_mut(Layer::Media(1)).name = "Screen & narration".into();
    e.set_timeline_expanded(true);
    e.video.selection = BTreeSet::from([Item::Annotation(1), Item::Annotation(2)]);
    for name in [
        "precision-selection",
        "precision-track-effects",
        "precision-focus",
    ] {
        if name == "precision-track-effects" {
            e.timeline_focus(Item::Media(1), false);
            e.open_effects_workstation();
            e.video.fx.track_scope = true;
            e.video.fx.position = Some(egui::pos2(150., 130.));
        }
        if name == "precision-focus" {
            e.video.fx.open = false;
            let p = e.session.document.video.as_mut().unwrap();
            let mut z = capture_core::video::zoom::Zoom::new(1, 0, 10000);
            z.scale = 2.;
            p.zooms.push(z);
            e.timeline_focus(Item::Zoom(1), false);
            e.video.zoom_edit = true;
            e.video.tab = PropertyTab::Style;
        }
        // Capture settled floating windows, rather than their opening fade.
        for _ in 0..24 {
            render(&ctx, &mut e, vec![]);
        }
        let first = render(&ctx, &mut e, vec![]);
        let last = render(&ctx, &mut e, vec![]);
        crate::property_layout::tests::dump_meshes(
            &ctx,
            &folder.to_string_lossy(),
            name,
            first,
            last,
        );
    }
}
fn block(ctx: &egui::Context, id: u64) -> egui::Rect {
    ctx.data(|d| d.get_temp(egui::Id::unique(("timeline-block", id))))
        .unwrap()
}
#[test]
fn timeline_group_drag_changes_tracks_atomically_and_marquee_selects_intersections() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    let target = p.add_track();
    let before = p.clone();
    e.set_timeline_expanded(true);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let row = before.layer_index(Layer::Annotation(target));
    let oldrow = before.layer_index(Layer::Annotation(before.objects[&1].track));
    let a = block(&ctx, 1);
    let from = a.center();
    let to = from + egui::vec2(0., (row as f32 - oldrow as f32) * 39.);
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    render(&ctx, &mut e, press(to, false));
    assert_eq!(
        e.session.document.video.as_ref().unwrap().objects[&1].track,
        target,
        "{}",
        e.status
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video.as_ref().unwrap(), &before);
    e.video.selection = BTreeSet::from([Item::Annotation(1), Item::Annotation(2)]);
    render(&ctx, &mut e, vec![]);
    let a = block(&ctx, 1);
    let from = a.center();
    let to = from + egui::vec2(12., 0.);
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    render(&ctx, &mut e, press(to, false));
    let after = e.session.document.video.as_ref().unwrap();
    let delta = after.objects[&1].start - before.objects[&1].start;
    assert!(delta > 0);
    assert_eq!(after.objects[&2].start - before.objects[&2].start, delta);
    assert_eq!(after.objects[&2].end - before.objects[&2].end, delta);
    e.execute(Command::Undo);
    render(&ctx, &mut e, vec![]);
    let a = block(&ctx, 1);
    let b = block(&ctx, 2);
    let panel: egui::Rect = ctx
        .data(|d| d.get_temp(egui::Id::unique("timeline-panel")))
        .unwrap();
    let from = egui::pos2(a.left() + 4., panel.bottom() - 24.);
    let to = b.right_top() + egui::vec2(-10., 4.);
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    render(&ctx, &mut e, press(to, false));
    assert!(e.video.selection.contains(&Item::Annotation(1)));
    assert!(e.video.selection.contains(&Item::Annotation(2)));
}
#[test]
fn cursor_zoom_keeps_pointer_time_and_drag_cursor_survives_other_handles() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.set_timeline_expanded(true);
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let a = block(&ctx, 1);
    let cursor = a.center();
    let fraction = (cursor.x - a.left()) / a.width();
    render(
        &ctx,
        &mut e,
        vec![
            egui::Event::PointerMoved(cursor),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., 100.),
                modifiers: egui::Modifiers::CTRL,
                source: egui::MouseWheelSource::Unknown,
                phase: egui::TouchPhase::Move,
            },
        ],
    );
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(cursor)]);
    assert!(
        e.video.zoom > 1.,
        "The wheel event must actually zoom the timeline"
    );
    let a = block(&ctx, 1);
    assert!(((cursor.x - a.left()) / a.width() - fraction).abs() < 0.002);
    let edge = a.right_center() - egui::vec2(2., 0.);
    render(&ctx, &mut e, press(edge, true));
    let out = render(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(edge + egui::vec2(22., -80.))],
    );
    assert_eq!(
        out.0.platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal
    );
    render(&ctx, &mut e, press(edge, false));
}
#[test]
fn clip_track_effects_survive_split_move_ripple_and_link_without_sharing() {
    let mut p = Project::new("same-source.mp4".into(), [960, 540], 12000, 30);
    p.clip_effects.insert(1, Chain::narration());
    p.split_clip(0, 6000).unwrap();
    let other = p.segments[1].id;
    let track = p.add_media_track();
    p.segments[1].track = track;
    p.link_items(&BTreeSet::from([Item::Media(1), Item::Media(other)]))
        .unwrap();
    p.clip_effects.get_mut(&other).unwrap().effects[1].values[0] = -35.;
    assert_eq!(p.clip_effects[&1].effects[1].values[0], -21.);
    p.track_effects.insert(track, Chain::narration());
    p.track_effects.get_mut(&track).unwrap().effects[1].values[0] = -12.;
    assert_eq!(p.clip_effects[&other].effects[1].values[0], -35.);
    p.remove_range(7000, 8000).unwrap();
    for c in p.segments.iter().filter(|c| c.track == track) {
        assert_eq!(p.clip_effects[&c.id].effects[1].values[0], -35.);
    }
    p.track_settings_mut(Layer::Media(track)).name = "Narrator".into();
    p.track_settings_mut(Layer::Media(track)).muted = true;
    let reopened: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(p, reopened);
    p.remove_layer(Layer::Media(track));
    assert!(!p.track_effects.contains_key(&track));
    assert!(p.track_settings.is_empty());
}
#[test]
fn workstation_selection_is_clip_scoped_and_like_clip_properties_stay_put() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    p.segments[0].audio.push(Audio {
        path: "voice.wav".into(),
        label: "Microphone".into(),
        volume: 1.,
        muted: false,
        fade_in: 0,
        fade_out: 0,
    });
    p.clip_effects.insert(1, Chain::narration());
    p.split_clip(0, 6000).unwrap();
    let id = p.segments[1].id;
    p.segments[1].track = p.add_media_track();
    p.clip_effects.get_mut(&id).unwrap().effects[0].values[0] = 3.;
    e.timeline_focus(Item::Media(1), false);
    e.open_effects_workstation();
    e.video.tab = PropertyTab::Audio;
    e.timeline_focus(Item::Media(id), false);
    let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            ..Default::default()
        },
        |ui| {
            e.show_effects_workstation(ui.ctx());
        },
    ));
    assert_eq!(e.video.fx.clip, id);
    assert_eq!(
        e.video
            .fx
            .target(e.session.document.video.as_ref().unwrap()),
        Target::Clip(id)
    );
    assert_eq!(e.video.tab, PropertyTab::Audio);
    assert!(e.tool.is_none());
    e.video.fx.track_scope = true;
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(e.video.fx.target(p), Target::Track(p.segments[1].track));
}
#[test]
fn focus_overlay_moves_live_without_seeking_and_commits_one_undo() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let p = e.session.document.video.as_mut().unwrap();
    let mut z = capture_core::video::zoom::Zoom::new(1, 2000, 9000);
    z.scale = 2.;
    p.zooms.push(z);
    let before = p.clone();
    e.video.zoom_selected = Some(1);
    e.video.zoom_edit = true;
    render(&ctx, &mut e, vec![]);
    let output = render(&ctx, &mut e, vec![]);
    let label = output
        .0
        .shapes
        .iter()
        .position(|s| {
            matches!(&s.shape,
        egui::Shape::Text(t) if t.galley.job.text == "Zoom focus · 2.0×")
        })
        .unwrap();
    let last_image = output
        .0
        .shapes
        .iter()
        .rposition(|s| {
            matches!(&s.shape,
        egui::Shape::Mesh(m) if m.texture_id != egui::TextureId::default())
        })
        .unwrap();
    assert!(
        label > last_image,
        "focus boundary must paint above the video image"
    );
    let t = e.view.current_transform().unwrap();
    let from = t.screen_point([480., 270.]);
    let to = t.screen_point([580., 300.]);
    render(&ctx, &mut e, press(from, true));
    render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    assert_ne!(
        e.video.timing_preview.as_ref().unwrap().zooms[0].center,
        before.zooms[0].center
    );
    assert_eq!(e.session.document.video.as_ref().unwrap(), &before);
    render(&ctx, &mut e, press(to, false));
    let p = e.session.document.video.as_ref().unwrap();
    assert_eq!(p.cursor, before.cursor);
    assert_eq!((p.zooms[0].start, p.zooms[0].end), (2000, 9000));
    e.execute(Command::Undo);
    assert_eq!(e.session.document.video.as_ref().unwrap(), &before);
}

#[test]
fn floating_workstation_and_export_block_same_frame_canvas_clicks() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.session.document.video.as_mut().unwrap().segments[0]
        .audio
        .push(Audio {
            path: "voice.wav".into(),
            label: "Voice".into(),
            volume: 1.,
            muted: false,
            fade_in: 0,
            fade_out: 0,
        });
    e.video.selected = Selection::Recording(0);
    e.open_effects_workstation();
    e.video.fx.position = Some(egui::pos2(20., 170.));
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    e.tool = Some(crate::Kind::Step);
    let before = e.session.document.annotations.clone();
    let r: egui::Rect = ctx
        .data(|d| d.get_temp(egui::Id::unique("effects-window")))
        .unwrap();
    let pos = r.center();
    let events = press(pos, true)
        .into_iter()
        .chain(press(pos, false))
        .collect();
    render(&ctx, &mut e, events);
    assert_eq!(e.session.document.annotations, before);
    e.video.fx.open = false;
    e.video.export_dialog = true;
    render(&ctx, &mut e, vec![]);
    render(&ctx, &mut e, vec![]);
    let events = press(pos, true)
        .into_iter()
        .chain(press(pos, false))
        .collect();
    render(&ctx, &mut e, events);
    assert_eq!(e.session.document.annotations, before);
}
#[cfg(windows)]
#[test]
fn independent_clip_and_track_dsp_mono_and_mute_match_export_audio() {
    let _runtime = media::Runtime::new().unwrap();
    let root = tempfile::tempdir().unwrap();
    let samples: Vec<i16> = (0..96000)
        .flat_map(|i| {
            let a = (4000. * (i as f64 * 440. * std::f64::consts::TAU / 48000.).sin()) as i16;
            [a, 0]
        })
        .collect();
    let mut writer = capture_core::video::wav::Writer::new(root.path()).unwrap();
    writer.append(&samples).unwrap();
    let file = writer.finish().unwrap();
    let original = std::fs::read(&file).unwrap();
    let mut p = Project::new(file.to_string_lossy().into(), [320, 180], 2000, 30);
    p.segments = imports::import_files(std::slice::from_ref(&file)).unwrap();
    p.segments[0].id = 1;
    p.segments[0].track = 1;
    p.split_clip(0, 1000).unwrap();
    let second = p.segments[1].id;
    let track = p.add_media_track();
    p.segments[1].track = track;
    let gain = |db| {
        let mut e = Effect::new(1, Kind::Equalizer);
        e.bands.clear();
        e.values[0] = db;
        Chain {
            mix: 1.,
            bypass: false,
            effects: vec![e],
        }
    };
    p.clip_effects.insert(1, gain(6.));
    p.clip_effects.insert(second, gain(-6.));
    p.track_effects.insert(track, gain(6.));
    let mut active = media::Mixer::new(&p).unwrap();
    active.samples(&p, 4800, 2000).unwrap();
    assert!(active.meters.contains_key(&Target::Clip(1)));
    active.samples(&p, 52800, 2000).unwrap();
    assert!(!active.meters.contains_key(&Target::Clip(1)));
    assert!(active.meters.contains_key(&Target::Clip(second)));
    let a = media::Mixer::new(&p)
        .unwrap()
        .samples(&p, 4800, 2000)
        .unwrap();
    let b = media::Mixer::new(&p)
        .unwrap()
        .samples(&p, 52800, 2000)
        .unwrap();
    let peak = |v: &[i16]| v.iter().map(|s| s.unsigned_abs()).max().unwrap() as f64;
    assert!((peak(&a) / peak(&b) - 10f64.powf(6. / 20.)).abs() < 0.02);
    p.segments[1].mono = true;
    let mono = media::Mixer::new(&p)
        .unwrap()
        .samples(&p, 52800, 2000)
        .unwrap();
    assert!(mono.as_chunks::<2>().0.iter().all(|c| c[0] == c[1]));
    assert!(peak(&mono) > 1900.);
    p.track_settings_mut(Layer::Media(track)).muted = true;
    assert!(
        media::Mixer::new(&p)
            .unwrap()
            .samples(&p, 52800, 2000)
            .unwrap()
            .iter()
            .all(|v| *v == 0)
    );
    assert_eq!(std::fs::read(file).unwrap(), original);
}

/// Approved option A: a PDF dropped on the timeline opens PDF pages that go
/// in where it was dropped; the hover says so instead of reading it as media.
#[test]
fn a_pdf_dropped_on_the_timeline_opens_pdf_pages_at_that_spot() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let temp = tempfile::tempdir().unwrap();
    let history = tempfile::tempdir().unwrap();
    e.history = crate::history::HistoryPanel::isolated(history.path());
    let path = temp.path().join("pages.pdf");
    std::fs::write(&path, b"%PDF-1.4\n").unwrap();
    let pos = egui::pos2(470., 90.);
    let draw = |e: &mut Editor, hover: bool, drop: bool| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000., 500.),
            )),
            events: vec![egui::Event::PointerMoved(pos)],
            hovered_files: if hover {
                vec![egui::HoveredFile {
                    path: Some(path.clone()),
                    ..Default::default()
                }]
            } else {
                vec![]
            },
            dropped_files: if drop {
                vec![Arc::new(DropFile(path.clone()))]
            } else {
                vec![]
            },
            ..Default::default()
        };
        crate::property_layout::tests::PaintedOutput(ctx.run_ui(input, |ui| {
            let p = e.session.document.video.clone().unwrap();
            let whole = egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(1000., 400.));
            e.video_media_rows(ui, &p, whole, 210., 0.05, 36.);
        }))
    };
    let output = draw(&mut e, true, false);
    assert!(
        crate::color_widgets::tests::text_rect(&output.0.shapes, "Drop to add PDF pages here")
            .is_some()
    );
    assert!(e.video.drop_probe.is_none(), "a PDF isn't read as media");
    assert_eq!(e.notice.kind, crate::guided::Kind::Plain, "{}", e.status);
    draw(&mut e, false, true);
    let draft = e.pdf_import.as_ref().expect("PDF pages opened");
    assert_eq!(draft.video_at_for_test(), Some(Some(5200)));
    assert!(e.video.importing.is_none());
}

/// Insert › Media fits its labels inside the compact buttons.
#[test]
fn the_media_group_labels_fit_their_buttons() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = editor(&ctx);
    let output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600., 200.),
            )),
            ..Default::default()
        },
        |ui| {
            ui.horizontal(|ui| e.video_insert_ribbon(ui, 58.));
        },
    ));
    for label in ["Video/audio", "Pictures", "PDF pages", "Record more"] {
        let r = crate::color_widgets::tests::text_rect(&output.0.shapes, label)
            .unwrap_or_else(|| panic!("{label}"));
        assert!(r.width() + 25. <= 100., "{label}: {r:?}");
    }
    // BZ8: Media is the last group, after Audio and Presets.
    let at = |label| {
        crate::color_widgets::tests::text_rect(&output.0.shapes, label)
            .unwrap_or_else(|| panic!("{label}"))
    };
    assert!(at("Pictures").left() > at("Blocks").right());
    assert!(at("Pictures").left() > at("Narration").right());
}
