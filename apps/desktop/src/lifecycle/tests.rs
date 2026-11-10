use super::*;

#[test]
fn tray_capture_actions_use_saved_settings_and_video_opens_the_same_options() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.lifecycle.available = true;
    e.close_editor_window(&ctx);
    let settings = e.quick_controls.video_settings();
    let before = serde_json::to_vec(&e.session.document).unwrap();
    e.tray_action(tray::Action::CaptureVideo, &ctx);
    assert!(!e.lifecycle.hidden && e.clip.video_dialog);
    assert_eq!(e.clip.video_options, Some(settings));
    assert!(!e.capture_requested);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    e.clip.video_dialog = false;
    e.tray_action(tray::Action::CaptureClip, &ctx);
    assert!(e.capture_requested || e.autosave.action.is_some());
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
}
use crate::canvas::tests::editor;
#[test]
fn capture_tray_toggles_preserve_document_and_do_not_start_capture() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let original = e.session.document.clone();
    let preview = e.quick_controls.capture_settings().preview;
    for expected in [!preview, preview] {
        e.tray_action(tray::Action::ToggleOpenAfter, &ctx);
        assert_eq!(e.quick_controls.capture_settings().preview, expected);
        assert!(!e.capture_requested);
    }
    for expected in [true, false] {
        e.tray_action(tray::Action::ToggleScrolling, &ctx);
        assert_eq!(e.quick_controls.capture_settings().is_scrolling(), expected);
    }
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&original).unwrap()
    );
}
#[test]
fn cancelled_capture_refocuses_even_when_preview_is_off_and_preserves_project() {
    for result in [Ok(None), Err("Synthetic capture failure".to_owned())] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let settings = capture_core::capture_options::Settings {
            preview: false,
            ..Default::default()
        };
        e.clip.active = Some(settings);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(result).unwrap();
        e.capture_result = Some(rx);
        e.receive_capture();
        assert_eq!(e.clip.restore_focus, Some(true));
        assert!(e.capture_result.is_none());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    }
}

#[test]
fn post_capture_show_preserves_normal_and_maximized_placement() {
    for (minimized, maximized) in [(false, false), (false, true), (true, false)] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let mut input = egui::RawInput::default();
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.minimized = Some(minimized);
        viewport.maximized = Some(maximized);
        let output = ctx.run_logic(&input, |ctx| e.show_editor_window(ctx));
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert_eq!(
            commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Minimized(false))),
            minimized
        );
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Maximized(_)))
        );
    }
}

#[test]
fn close_hides_without_destroying_document_or_capture_shortcuts_and_exit_is_explicit() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.lifecycle.available = true; // Inject tray capability; never touch the real desktop.
    let before = serde_json::to_vec(&e.session.document).unwrap();
    e.close_editor_window(&ctx);
    assert!(e.lifecycle.hidden);
    assert!(!e.allow_close);
    assert!(e.accepts_global_capture(&ctx));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    e.tray_action(tray::Action::Editor, &ctx);
    assert!(!e.lifecycle.hidden);
    e.tray_action(tray::Action::Clip, &ctx);
    assert!(e.clip.clip_dialog);
    e.close_editor_window(&ctx);
    assert!(e.lifecycle.hidden);
    e.tray_action(tray::Action::Exit, &ctx);
    assert!(e.allow_close);
}

#[test]
fn hidden_logic_dispatches_capture_without_a_ui_pass_and_restores_on_tray_failure() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.lifecycle.available = true;
    e.close_editor_window(&ctx);
    e.global_capture = Some(crate::hotkey::CaptureHotkey::test_pressed(
        capture_core::keybinds::Keybinds::default().bindings[0],
    ));
    let _ = ctx.run_logic(&egui::RawInput::default(), |ctx| e.background_logic(ctx));
    assert!(e.capture_requested);
    assert!(e.global_capture.is_some());
    assert!(!e.allow_close);
    e.capture_requested = false;
    e.tray_action(tray::Action::Unavailable, &ctx);
    assert!(!e.lifecycle.hidden);
    assert!(!e.lifecycle.available);
    assert!(!e.lifecycle.error.is_empty());
    e.close_editor_window(&ctx);
    assert!(e.allow_close);
}

#[test]
fn startup_preference_file_override_and_tray_failure_have_accessible_windows() {
    for available in [true, false] {
        for explicit_file in [true, false] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.lifecycle.available = available;
            e.quick_controls.test_lifecycle(true, StartupWindow::Clip);
            e.apply_startup_window(&ctx, explicit_file, e.quick_controls.startup_window());
            assert!(!e.lifecycle.hidden);
            assert!(
                !e.clip.clip_dialog,
                "startup never opens the Capture Clip window"
            );
        }
    }
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.lifecycle.available = true;
    e.quick_controls
        .test_lifecycle(false, StartupWindow::Editor);
    e.close_editor_window(&ctx);
    assert!(e.allow_close);
    assert!(!e.lifecycle.hidden);
}

#[test]
fn exit_from_hidden_keeps_unsaved_confirmation_accessible() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let doc = e.session.document.clone();
    e.session = capture_core::Session::unsaved(doc).unwrap();
    e.lifecycle.available = true;
    e.close_editor_window(&ctx);
    e.tray_action(tray::Action::Exit, &ctx);
    e.background_logic(&ctx);
    assert!(e.pending.is_some());
    assert!(!e.allow_close);
    assert!(!e.lifecycle.hidden);
}
