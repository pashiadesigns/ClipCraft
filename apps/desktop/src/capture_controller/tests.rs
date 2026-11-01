use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn capture_hotkey_uses_its_saved_mode_profile_and_clipboard_adapter() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let root = tempfile::tempdir().unwrap();
    e.history = crate::history::HistoryPanel::isolated(root.path());
    let region = Settings {
        clipboard: true,
        delay_seconds: 5,
        ..Default::default()
    };
    e.quick_controls.remember_capture(region.clone());
    e.quick_controls.remember_capture(Settings {
        selection: Selection::Window,
        preview: false,
        ..Default::default()
    });
    e.request_keybind_capture(capture_core::keybinds::Action::Region, &ctx);
    assert_eq!(e.capture_controller.active, Some(region.clone()));
    let mut copied = false;
    e.accept_capture(
        capture_core::Document::blank(50, 30).unwrap(),
        region,
        |_| {
            copied = true;
            Ok(())
        },
    );
    assert!(copied);
    assert_eq!(e.session.document.width, 50);
    assert!(e.status.contains("Copied to clipboard"));
}

#[test]
fn controller_survives_success_cancel_and_error_without_reopening_or_losing_settings() {
    for (preview, success) in [(true, true), (false, true), (true, false), (false, false)] {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let dir = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(dir.path());
        e.open_capture_controller();
        let settings = Settings {
            preview,
            ..Default::default()
        };
        e.perform_action(FileAction::StartCapture(settings.clone()), &ctx);
        assert!(e.capture_controller.open && e.capture_controller.suspended);
        let (tx, rx) = std::sync::mpsc::channel();
        e.capture_result = Some(rx);
        tx.send(if success {
            Ok(Some(capture_core::Document::blank(20, 10).unwrap().into()))
        } else {
            Ok(None)
        })
        .unwrap();
        e.receive_capture();
        assert!(e.capture_controller.open && !e.capture_controller.suspended);
        assert!(e.capture_controller.active.is_none());
        assert_eq!(e.capture_controller.restore_focus, Some(preview && success));
        e.perform_action(FileAction::StartCapture(settings), &ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        e.capture_result = Some(rx);
        tx.send(Err("injected failure".into())).unwrap();
        e.receive_capture();
        assert!(e.capture_controller.open && !e.capture_controller.suspended);
        assert!(e.status.contains("injected failure"));
    }
}

#[test]
fn window_overlay_repaints_only_when_target_changes() {
    let a = crate::capture::Region {
        x: 0,
        y: 0,
        width: 30,
        height: 20,
    };
    let b = crate::capture::Region { x: 30, ..a };
    assert!(!crate::capture::repaint_selection(true, Some(a), Some(a)));
    assert!(!crate::capture::repaint_selection(true, None, None));
    assert!(crate::capture::repaint_selection(true, Some(a), Some(b)));
    assert!(crate::capture::repaint_selection(true, Some(a), None));
    assert!(crate::capture::repaint_selection(false, Some(a), Some(a)));
}

#[test]
fn fullscreen_cog_exposes_all_and_specific_monitor_choices_without_desktop_access() {
    use capture_core::capture_options::{Display, Fullscreen};
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.all_styles_mut(|s| s.animation_time = 0.);
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::unique("capture-display-list"),
            Ok::<_, String>(vec![
                Display {
                    device: "synthetic-left".into(),
                    bounds: [-1920, 0, 0, 1080],
                    primary: false,
                },
                Display {
                    device: "synthetic-main".into(),
                    bounds: [0, 0, 2560, 1440],
                    primary: true,
                },
            ]),
        )
    });
    let mut mode = Mode::Image;
    let mut settings = Settings {
        selection: Selection::Fullscreen,
        ..Default::default()
    };
    let mut first = frame(&ctx, &mut mode, &mut settings, vec![]).1;
    let output = frame(&ctx, &mut mode, &mut settings, vec![]).1;
    first
        .0
        .textures_delta
        .append(output.0.textures_delta.clone());
    let rect = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("fullscreen-settings"))
            .unwrap()
    });
    assert!(rect.right() < 714. && rect.top() >= 0.);
    for pressed in [true, false] {
        let output = frame(
            &ctx,
            &mut mode,
            &mut settings,
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .1;
        first
            .0
            .textures_delta
            .append(output.0.textures_delta.clone());
    }
    let output = frame(&ctx, &mut mode, &mut settings, vec![]).1;
    if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_CAPTURE_DIR") {
        dump_meshes(&ctx, &dir, "FullscreenSettings", first, output);
    }
    let all = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("all-monitors"))
            .unwrap()
    });
    click(&ctx, &mut mode, &mut settings, all);
    assert_eq!(settings.fullscreen, Fullscreen::All);
}

fn frame(
    ctx: &egui::Context,
    mode: &mut Mode,
    settings: &mut Settings,
    events: Vec<egui::Event>,
) -> (Option<Action>, PaintedOutput) {
    let mut action = None;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(714., 190.),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| action = panel(ui, mode, settings, true),
    );
    (action, PaintedOutput(output))
}
fn click(
    ctx: &egui::Context,
    mode: &mut Mode,
    s: &mut Settings,
    rect: egui::Rect,
) -> Option<Action> {
    let event = |pressed| egui::Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        mode,
        s,
        vec![egui::Event::PointerMoved(rect.center()), event(true)],
    );
    frame(ctx, mode, s, vec![event(false)]).0
}
#[test]
fn real_controller_buttons_route_image_settings_and_block_future_modes() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut mode = Mode::Image;
    let mut settings = Settings::default();
    frame(&ctx, &mut mode, &mut settings, vec![]);
    frame(&ctx, &mut mode, &mut settings, vec![]);
    let rect = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("capture-switch", "Copy to Clipboard")))
            .unwrap()
    });
    assert!(settings.clipboard);
    click(&ctx, &mut mode, &mut settings, rect);
    assert!(!settings.clipboard);
    click(&ctx, &mut mode, &mut settings, rect);
    assert!(settings.clipboard);
    let (rect, enabled) = ctx.data(|d| {
        d.get_temp::<(egui::Rect, bool)>(egui::Id::unique("controller-capture"))
            .unwrap()
    });
    assert!(enabled);
    assert_eq!(
        click(&ctx, &mut mode, &mut settings, rect),
        Some(Action::Capture)
    );
    settings.selection = Selection::Scrolling;
    frame(&ctx, &mut mode, &mut settings, vec![]);
    assert!(
        !ctx.data(|d| d
            .get_temp::<(egui::Rect, bool)>(egui::Id::unique("controller-capture"))
            .unwrap())
            .1
    );
    assert_eq!(click(&ctx, &mut mode, &mut settings, rect), None);
    for choice in [Mode::AllInOne, Mode::Image, Mode::Video] {
        let rect = ctx.data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique(("capture-mode", choice.label())))
                .unwrap()
        });
        assert_eq!(click(&ctx, &mut mode, &mut settings, rect), None);
        assert_eq!(mode, choice);
    }
    if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_CAPTURE_DIR") {
        for choice in [Mode::AllInOne, Mode::Image, Mode::Video] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let mut mode = choice;
            settings.selection = Selection::Region;
            let first = frame(&ctx, &mut mode, &mut settings, vec![]).1;
            let output = frame(&ctx, &mut mode, &mut settings, vec![]).1;
            dump_meshes(&ctx, &dir, choice.label(), first, output);
        }
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut mode = Mode::Image;
        settings.selection = Selection::Steps;
        let first = frame(&ctx, &mut mode, &mut settings, vec![]).1;
        let output = frame(&ctx, &mut mode, &mut settings, vec![]).1;
        assert!(ctx.data(|d| {
            d.get_temp::<(egui::Rect, bool)>(egui::Id::unique("controller-capture"))
                .unwrap()
                .1
        }));
        dump_meshes(&ctx, &dir, "Steps", first, output);
    }
}
#[test]
fn opening_controller_preserves_edits_and_failed_retention_keeps_capture_available() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let old = serde_json::to_vec(&e.session.document).unwrap();
    e.request_action(FileAction::Capture, &ctx);
    assert!(e.capture_controller.open);
    assert!(!e.capture_requested);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), old);
    let mut copies = 0;
    let settings = Settings {
        preview: false,
        clipboard: true,
        ..Default::default()
    };
    e.accept_capture(
        capture_core::Document::canvas(12, 8, [30, 50, 80, 255]).unwrap(),
        settings,
        |_| {
            copies += 1;
            Err("injected busy clipboard".into())
        },
    );
    assert_eq!(copies, 1);
    assert_eq!(e.session.document.width, 12);
    assert!(e.status.contains("Clipboard transfer failed"));
    assert!(e.status.contains("Library save failed"));
    assert!(e.session.is_modified());
    e.accept_capture(
        capture_core::Document::blank(15, 8).unwrap(),
        Settings {
            clipboard: false,
            ..Default::default()
        },
        |_| panic!("Clipboard disabled"),
    );
    assert_eq!(e.session.document.width, 15);
    let dir = tempfile::tempdir().unwrap();
    e.history = crate::history::HistoryPanel::isolated(dir.path());
    let before = serde_json::to_vec(&e.session.document).unwrap();
    e.accept_capture(
        capture_core::Document::canvas(30, 20, [17, 80, 140, 255]).unwrap(),
        Settings {
            preview: false,
            clipboard: false,
            ..Default::default()
        },
        |_| panic!("Clipboard disabled"),
    );
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert_eq!(e.history.entries().len(), 1);
    assert!(e.status.contains("saved to Library"));
}

#[test]
fn retained_step_result_reuses_its_library_item_without_duplicating_or_replacing_preview_off_document()
 {
    use capture_core::{
        Document,
        history::{History, Origin},
    };
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let dir = tempfile::tempdir().unwrap();
    let mut h = History::open(dir.path()).unwrap();
    let document = Document::canvas(40, 30, [20, 60, 90, 255]).unwrap();
    let entry = h.create(&document, "Step guide", Origin::Capture).unwrap();
    e.history = crate::history::HistoryPanel::isolated(dir.path());
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let result = || crate::capture::Captured {
        document: document.clone(),
        retained: Some(entry.clone()),
        warning: "Partial recording kept".into(),
        recognized: None,
    };
    e.accept_capture_result(
        result(),
        Settings {
            preview: false,
            ..Settings::for_selection(Selection::Steps)
        },
        |_| panic!("No clipboard"),
    );
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert_eq!(e.history.entries().len(), 1);
    assert!(e.status.contains("Partial recording kept"));
    e.accept_capture_result(result(), Settings::for_selection(Selection::Steps), |_| {
        panic!("No clipboard")
    });
    assert_eq!(e.history.active.as_deref(), Some(entry.id.as_str()));
    assert!(!e.session.is_modified());
    assert_eq!(
        e.session.document.render().unwrap(),
        document.render().unwrap()
    );
    assert_eq!(e.history.entries().len(), 1);
}
