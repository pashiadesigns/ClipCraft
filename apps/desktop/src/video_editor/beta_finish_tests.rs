use super::repair_tests::{editor, press, render};
use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn zoom_motion_handles_preview_latch_and_commit_one_undo() {
    for exit in [false, true] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let mut z = capture_core::video::zoom::Zoom::new(1, 2000, 8000);
        z.entrance_ms = 0;
        z.exit_ms = 0;
        e.session
            .document
            .video
            .as_mut()
            .unwrap()
            .zooms
            .push(z.clone());
        e.set_timeline_expanded(true);
        e.video.snap = false;
        render(&ctx, &mut e, vec![]);
        render(&ctx, &mut e, vec![]);
        let r = ctx
            .read_response(egui::Id::unique(("zoom-block", 1u64)))
            .unwrap()
            .rect;
        let from = if exit {
            r.right_top() + egui::vec2(-4., 4.)
        } else {
            r.left_top() + egui::vec2(4., 4.)
        };
        let to = from + egui::vec2(if exit { -45. } else { 45. }, 22.);
        render(&ctx, &mut e, press(from, true));
        let out = render(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
        assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::None);
        let preview = e.video.timing_preview.as_ref().unwrap().zooms[0].clone();
        assert_eq!((preview.start, preview.end), (2000, 8000));
        assert!(if exit {
            preview.exit_ms > 0
        } else {
            preview.entrance_ms > 0
        });
        render(&ctx, &mut e, vec![]);
        assert_eq!(e.video.timing_preview.as_ref().unwrap().zooms[0], preview);
        render(&ctx, &mut e, press(to, false));
        e.execute(Command::Undo);
        assert_eq!(e.session.document.video.as_ref().unwrap().zooms[0], z);
    }
}

#[test]
fn capture_defaults_append_and_scrolling_is_region_only() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.open_video_capture();
    assert_eq!(e.clip.recording_target, 1);
    e.clip.recording_target = 2;
    let _out = PaintedOutput(ctx.run_ui(Default::default(), |ui| e.video_capture_dialog(ui.ctx())));
    assert_eq!(e.clip.recording_target, 2);
    e.clip.video_dialog = false;
    e.session.document.video = None;
    e.open_video_capture();
    assert_eq!(e.clip.recording_target, 0);
    use capture_core::capture_options::{Selection, Settings};
    for selection in [
        Selection::Region,
        Selection::Window,
        Selection::Fullscreen,
        Selection::Scrolling,
    ] {
        let s = Settings {
            selection,
            scrolling: true,
            ..Default::default()
        }
        .request()
        .unwrap();
        assert_eq!(s.selection, Selection::Region);
        assert!(s.is_scrolling());
    }
}

#[test]
fn focus_insert_is_editable_and_properties_fit_image_and_video() {
    for video in [false, true] {
        for width in [260., 390.] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            if !video {
                e.session.document.video = None;
            }
            e.choose_tool(
                crate::shell::TOOLS
                    .iter()
                    .position(|t| t.id == capture_core::toolbar::ToolId::Focus)
                    .unwrap(),
            );
            let a = e.new_annotation(crate::Kind::Rectangle, [100., 100.], [450., 350.]);
            assert!(a.focus.is_some() && a.fill.is_none() && a.text.is_none());
            let id = a.id;
            e.execute(Command::Add { annotation: a });
            e.select_only(Some(id));
            let before = e.session.document.annotations.clone();
            let mut draw = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 850.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        if video {
                            e.video_properties_pane(ui);
                        } else {
                            e.image_properties_pane(ui);
                        }
                    },
                ))
            };
            let mut first = draw();
            for _ in 0..3 {
                let out = draw();
                for (id, delta) in &out.textures_delta.set {
                    first
                        .0
                        .textures_delta
                        .set
                        .entry(*id)
                        .or_default()
                        .extend(delta.clone());
                }
            }
            let last = draw();
            for s in &last.shapes {
                if let egui::Shape::Text(t) = &s.shape {
                    assert!(
                        t.pos.x + t.galley.size().x <= width + 1.,
                        "Focus text overflow {width}: {:?}",
                        t.galley.text()
                    );
                }
            }
            if let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
                dump_meshes(
                    &ctx,
                    &folder.to_string_lossy(),
                    &format!("focus-{}-{width}", if video { "video" } else { "image" }),
                    first,
                    last,
                );
            }
            assert_eq!(before, e.session.document.annotations);
            e.begin_text_edit();
            assert!(e.text_draft.is_none());
            e.execute(Command::Undo);
            assert!(!e.session.document.annotations.iter().any(|a| a.id == id));
        }
    }
}

#[test]
fn preset_columns_use_available_width_without_stretching_tiles() {
    assert_eq!(crate::style_tile::columns(236., true), 2);
    assert_eq!(crate::style_tile::columns(366., true), 3);
    assert_eq!(crate::style_tile::columns(236., false), 2);
    assert_eq!(crate::style_tile::columns(366., false), 3);
}

#[test]
fn clipboard_only_capture_retains_editable_project_without_switching_editor() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let root = tempfile::tempdir().unwrap();
    e.history = crate::history::HistoryPanel::isolated(root.path());
    let original = serde_json::to_vec(&e.session.document).unwrap();
    let mut d = Document::canvas(80, 60, [220, 230, 240, 255]).unwrap();
    d.annotations.push(
        capture_core::steps::marker(1, [30., 30.], 20, [30, 80, 200, 255], (1, false), [80, 60])
            .unwrap(),
    );
    let copied = std::cell::Cell::new(false);
    e.accept_capture(
        d.clone(),
        capture_core::capture_options::Settings {
            preview: false,
            clipboard: true,
            ..Default::default()
        },
        |_| {
            copied.set(true);
            Ok(())
        },
    );
    assert!(copied.get());
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    let entries = e.history.entries();
    assert_eq!(entries.len(), 1);
    let store = capture_core::history::History::open(root.path()).unwrap();
    let restored = store.load(&entries[0].id).unwrap();
    assert_eq!(restored.annotations, d.annotations);
    assert_eq!(restored.render().unwrap(), d.render().unwrap());
}

#[test]
fn opened_image_capture_starts_on_the_insert_tab() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let root = tempfile::tempdir().unwrap();
    e.history = crate::history::HistoryPanel::isolated(root.path());
    e.shell.ribbon_tab = crate::ribbon::RibbonTab::Advanced;
    let d = Document::canvas(80, 60, [220, 230, 240, 255]).unwrap();
    e.accept_capture(
        d,
        capture_core::capture_options::Settings {
            preview: true,
            clipboard: false,
            ..Default::default()
        },
        |_| Ok(()),
    );
    assert_eq!(e.shell.ribbon_tab, crate::ribbon::RibbonTab::Insert);
}

#[test]
#[ignore = "Measured synthetic full-HD overlap preparation; not native playback FPS"]
fn framed_overlap_preparation_profile() {
    let source = Arc::new(image::RgbaImage::from_fn(1920, 1080, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 180, 255])
    }));
    let mut results = vec![];
    for (name, shape, rotation) in [
        ("rounded", crate::Kind::Rectangle, 0.),
        ("circle", crate::Kind::Ellipse, 0.),
        ("rotated", crate::Kind::Rectangle, 15.),
    ] {
        let style = capture_core::video::frame::Frame {
            shape,
            rotation,
            width: 3,
            shadow: Some(Default::default()),
            ..Default::default()
        };
        let rect = [0.08, 0.08, 0.84, 0.84];
        let start = Instant::now();
        for _ in 0..12 {
            std::hint::black_box(style.render(source.clone(), rect, [1920, 1080], 1).unwrap());
        }
        let original_ms = start.elapsed().as_secs_f64() * 1000. / 12.;
        let mut cache = capture_core::video::frame_cache::PreviewCache::default();
        let mut reduced = style.clone();
        reduced.radius *= 0.5;
        reduced.width = 2;
        if let Some(shadow) = &mut reduced.shadow {
            shadow.blur *= 0.5;
            shadow.distance *= 0.5;
        }
        cache
            .render(&reduced, source.clone(), rect, [960, 540], 1)
            .unwrap();
        let start = Instant::now();
        for _ in 0..30 {
            std::hint::black_box(
                cache
                    .render(&reduced, source.clone(), rect, [960, 540], 1)
                    .unwrap(),
            );
        }
        let preview_ms = start.elapsed().as_secs_f64() * 1000. / 30.;
        results.push(serde_json::json!({"frame":name,"old_full_resolution_ms":original_ms,"display_resolution_ms":preview_ms,"display_pixels":[960,540],"project_pixels":[1920,1080]}));
    }
    let data = serde_json::json!({"frames":results,"native_playback_verified":false,"export_resolution_unchanged":true});
    println!("{data}");
    if let Some(folder) = std::env::var_os("CLIPDESIGNER_VIDEO_EDITOR_EVIDENCE") {
        std::fs::write(
            PathBuf::from(folder).join("framed-overlap-performance.json"),
            data.to_string(),
        )
        .unwrap();
    }
}
