use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect};

#[test]
fn track_menu_has_four_types_and_keeps_undo_and_persistence() {
    use capture_core::video::editor::TrackKind;
    for (label, kind) in [
        ("Annotation track", None),
        ("Audio track", Some(TrackKind::Audio)),
        ("Voiceover track", Some(TrackKind::Voiceover)),
        ("Video track", Some(TrackKind::Video)),
    ] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let original = e.session.document.video.clone().unwrap();
        for _ in 0..3 {
            render(&ctx, &mut e, 1440., vec![]);
        }
        click(&ctx, &mut e, 1440., "timeline-add-track");
        render(&ctx, &mut e, 1440., vec![]);
        let r: Rect = ctx.data(|d| d.get_temp(egui::Id::unique(("add-track", label))).unwrap());
        for pressed in [true, false] {
            render(
                &ctx,
                &mut e,
                1440.,
                vec![
                    Event::PointerMoved(r.center()),
                    Event::PointerButton {
                        pos: r.center(),
                        pressed,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        let p = e.session.document.video.as_ref().unwrap();
        if let Some(kind) = kind {
            assert_eq!(p.media_tracks.len(), original.media_tracks.len() + 1);
            assert_eq!(p.track_kind(*p.media_tracks.last().unwrap()), kind);
            let copy: Project = serde_json::from_str(&serde_json::to_string(p).unwrap()).unwrap();
            assert_eq!(copy.media_kinds, p.media_kinds);
        } else {
            assert_eq!(p.tracks.len(), original.tracks.len() + 1);
            assert_eq!(p.annotation_track, p.tracks.last().copied());
        }
        if kind == Some(TrackKind::Voiceover) {
            assert!(e.video.voiceover.open);
            assert_eq!(e.video.voiceover.target, p.media_tracks.last().copied());
        }
        e.video.voiceover = Default::default();
        e.execute(Command::Undo);
        assert_eq!(
            e.session.document.video.as_ref().unwrap().media_tracks,
            original.media_tracks
        );
        assert_eq!(
            e.session.document.video.as_ref().unwrap().tracks,
            original.tracks
        );
    }
}

#[test]
fn insert_audio_buttons_match_annotation_tile_geometry() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440., 200.))),
            ..Default::default()
        },
        |ui| {
            ui.horizontal(|ui| e.video_insert_ribbon(ui, 58.));
        },
    ));
    let tile = |name: &str| {
        ctx.data(|d| {
            d.get_temp::<(Rect, bool, bool)>(egui::Id::unique(("ribbon-tile", name)))
                .unwrap()
                .0
        })
    };
    let a = tile("Text");
    for name in ["Focus", "Narration", "Audio effects"] {
        let b = tile(name);
        assert_eq!(a.size(), b.size());
        assert!((a.top() - b.top()).abs() < 1.);
        assert!(b.left() > tile("Obfuscate").right());
    }
    let focus = tile("Focus").center();
    for pressed in [true, false] {
        let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440., 200.))),
                events: vec![
                    Event::PointerMoved(focus),
                    Event::PointerButton {
                        pos: focus,
                        pressed,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| e.video_insert_ribbon(ui, 58.));
            },
        ));
    }
    assert_eq!(e.current_tool().id, capture_core::toolbar::ToolId::Focus);
}

fn render(ctx: &egui::Context, e: &mut Editor, width: f32, events: Vec<Event>) {
    let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 960.))),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| e.show_shell(ui),
    ));
}
fn rect(ctx: &egui::Context, key: &str) -> Rect {
    ctx.data(|d| d.get_temp(egui::Id::unique(key)).unwrap())
}
fn click(ctx: &egui::Context, e: &mut Editor, width: f32, key: &str) {
    let pos = rect(ctx, key).center();
    for pressed in [true, false] {
        render(
            ctx,
            e,
            width,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    pressed,
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    for _ in 0..3 {
        render(ctx, e, width, vec![]);
    }
}
fn editor(ctx: &egui::Context) -> Editor {
    crate::shell::configure(ctx);
    let mut e = crate::canvas::tests::editor(ctx);
    e.session = Session::new(super::tests::fixture()).unwrap();
    e.select_only(Some(3));
    e.tool = None;
    e.shell.properties = true;
    e.history.recent = false;
    e
}

#[test]
fn footer_docks_preserve_geometry_documents_and_view_state() {
    for width in [880., 1100., 1440., 1920.] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let original = serde_json::to_vec(&e.session.document).unwrap();
        for _ in 0..5 {
            render(&ctx, &mut e, width, vec![]);
        }
        let timeline = rect(&ctx, "timeline-panel");
        let properties = rect(&ctx, "properties-panel");
        let (area, _, controls, content): (Rect, Rect, Rect, Rect) =
            ctx.data(|d| d.get_temp(egui::Id::unique("footer-layout")).unwrap());
        assert!((controls.center().x - area.center().x).abs() < 1.);
        assert!(content.height() <= 38.1);
        assert!(properties.bottom() <= timeline.top());
        assert!(timeline.right() > properties.left());
        for key in [
            "footer-recent",
            "footer-snap",
            "footer-follow",
            "footer-fit",
            "footer-video-setup",
        ] {
            assert!(
                controls.expand(1.).contains_rect(rect(&ctx, key)),
                "{width}: {key} outside center controls"
            );
        }
        e.video.properties_priority = !e.video.properties_priority;
        render(&ctx, &mut e, width, vec![]);
        render(&ctx, &mut e, width, vec![]);
        assert!(e.video.properties_priority);
        let dock = rect(&ctx, "timeline-panel");
        let pane = rect(&ctx, "properties-panel");
        assert!(dock.right() <= pane.left());
        assert!(pane.bottom() > dock.top() + 200.);
        assert!((dock.height() - timeline.height()).abs() < 1.);
        assert!((pane.width() - properties.width()).abs() < 1.);
        e.set_timeline_expanded(e.video.collapsed);
        render(&ctx, &mut e, width, vec![]);
        render(&ctx, &mut e, width, vec![]);
        assert!(e.video.collapsed);
        assert!(rect(&ctx, "timeline-panel").height() < 60.);
        click(&ctx, &mut e, width, "footer-recent");
        assert!(e.history.recent && e.video.collapsed);
        assert!(rect(&ctx, "recent-panel").right() <= rect(&ctx, "properties-panel").left());
        e.set_timeline_expanded(e.video.collapsed);
        render(&ctx, &mut e, width, vec![]);
        render(&ctx, &mut e, width, vec![]);
        assert!(!e.history.recent && !e.video.collapsed);
        assert!((rect(&ctx, "timeline-panel").height() - timeline.height()).abs() < 1.);
        e.video.properties_priority = !e.video.properties_priority;
        render(&ctx, &mut e, width, vec![]);
        render(&ctx, &mut e, width, vec![]);
        assert!(!e.video.properties_priority);
        assert!((rect(&ctx, "timeline-panel").right() - timeline.right()).abs() < 1.);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert!(!e.session.can_undo());
    }
}

#[test]
fn footer_controls_edit_only_their_intended_target() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    for _ in 0..5 {
        render(&ctx, &mut e, 1440., vec![]);
    }
    let original = serde_json::to_vec(&e.session.document).unwrap();
    click(&ctx, &mut e, 1440., "footer-snap");
    assert!(!e.video.snap);
    click(&ctx, &mut e, 1440., "footer-annotations");
    assert!(e.shell.annotations_hidden);
    click(&ctx, &mut e, 1440., "footer-annotations");
    click(&ctx, &mut e, 1440., "footer-zoom-in");
    assert!(matches!(
        e.view.mode,
        crate::canvas_view::ZoomMode::Manual(_)
    ));
    click(&ctx, &mut e, 1440., "footer-fit");
    assert_eq!(e.view.mode, crate::canvas_view::ZoomMode::Fit);
    assert_eq!(e.video.zoom, 1.);
    click(&ctx, &mut e, 1440., "footer-video-setup");
    assert!(e.selected.is_none());
    assert_eq!(e.video.tab, PropertyTab::Style);
    for fps in [15u32, 30, 60] {
        let control: Rect = ctx.data(|d| {
            d.get_temp(egui::Id::unique(("video-setup-fps", fps)))
                .expect("Video Setup frame-rate choice")
        });
        assert!(rect(&ctx, "properties-panel").contains_rect(control));
    }
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    assert!(!e.session.can_undo());
    let tracks = e.session.document.video.as_ref().unwrap().tracks.len();
    click(&ctx, &mut e, 1440., "timeline-add-track");
    click(&ctx, &mut e, 1440., "add-object-track");
    assert_eq!(
        e.session.document.video.as_ref().unwrap().tracks.len(),
        tracks + 1
    );
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    e.shell.preview_ui = true;
    for _ in 0..3 {
        render(&ctx, &mut e, 1440., vec![]);
    }
    click(&ctx, &mut e, 1440., "footer-follow");
    assert!(!e.video.follow_playhead);
    e.shell.preview_ui = false;
    e.video.properties_priority = true;
    e.path = Some(PathBuf::from("footer-video.vclip"));
    e.switch_session(
        Session::new(Document::blank(320, 180).unwrap()).unwrap(),
        None,
        Some(Path::new("footer-image.iclip")),
    );
    assert!(!e.is_video());
    render(&ctx, &mut e, 1440., vec![]);
    e.switch_session(
        Session::new(super::tests::fixture()).unwrap(),
        None,
        Some(Path::new("footer-video.vclip")),
    );
    assert!(e.is_video() && e.video.properties_priority);
}

#[test]
fn approved_footer_glyphs_render_at_multiple_pixel_densities() {
    for name in [
        "images",
        "panel-bottom",
        "panel-right-open",
        "panel-bottom-open",
        "eye",
        "eye-off",
        "magnet",
        "list-plus",
        "sparkles",
        "zoom-out",
        "zoom-in",
        "scan",
        "monitor-cog",
    ] {
        for px in [18, 27, 36] {
            let image = crate::ui_glyphs::render(name, px).unwrap();
            assert!(
                image.pixels().any(|p| p[3] > 0 && p[3] < 255),
                "{name}: antialiased edges"
            );
            assert!(image.pixels().any(|p| p[3] > 200), "{name}: visible stroke");
        }
    }
}
