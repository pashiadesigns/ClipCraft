use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
fn frame(
    ctx: &egui::Context,
    e: &mut Editor,
    width: f32,
    mut events: Vec<egui::Event>,
) -> PaintedOutput {
    let releases: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } = e
            {
                Some(egui::Event::Key {
                    key: *key,
                    physical_key: None,
                    modifiers: *modifiers,
                    pressed: false,
                    repeat: false,
                })
            } else {
                None
            }
        })
        .collect();
    events.extend(releases);
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 720.),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            e.process_keybind_input(ui.ctx());
            e.show_shell(ui);
        },
    ))
}
fn text(o: &PaintedOutput, s: &str) -> egui::Rect {
    o.0.shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Text(t) = &shape.shape {
                (t.galley.text() == s).then_some(egui::Rect::from_min_size(t.pos, t.galley.size()))
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("missing {s}"))
}
fn click(ctx: &egui::Context, e: &mut Editor, r: egui::Rect) {
    for pressed in [true, false] {
        frame(
            ctx,
            e,
            980.,
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
}
fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}
#[test]
fn recording_conflicts_and_cancel_are_isolated_from_document_and_capture() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.open_preferences(crate::preferences::Section::Shortcuts);
    let document = serde_json::to_vec(&e.session.document).unwrap();
    frame(&ctx, &mut e, 980., vec![]);
    let o = frame(&ctx, &mut e, 980., vec![]);
    click(&ctx, &mut e, text(&o, "Print Screen"));
    assert_eq!(
        e.options.recording,
        Some(ShortcutAction::Capture(Action::Saved))
    );
    assert!(!e.accepts_global_capture(&ctx));
    frame(
        &ctx,
        &mut e,
        980.,
        vec![key(egui::Key::F12, egui::Modifiers::ALT)],
    );
    assert!(e.options.error);
    assert!(e.options.message.contains("F12"));
    frame(
        &ctx,
        &mut e,
        980.,
        vec![key(egui::Key::F10, egui::Modifiers::ALT)],
    );
    assert!(e.options.recording.is_none());
    assert_eq!(
        e.options
            .draft
            .as_ref()
            .unwrap()
            .get(Action::Saved)
            .unwrap()
            .label(),
        "Alt+F10"
    );
    assert_eq!(
        e.quick_controls.keybinds(),
        Keybinds::default(),
        "recording must not save without submission"
    );
    e.options.recording = Some(ShortcutAction::Capture(Action::Region));
    frame(
        &ctx,
        &mut e,
        980.,
        vec![key(egui::Key::F10, egui::Modifiers::ALT)],
    );
    assert!(e.options.error);
    assert!(e.options.message.contains("already assigned"));
    assert_eq!(
        e.options
            .draft
            .as_ref()
            .unwrap()
            .get(Action::Region)
            .unwrap()
            .label(),
        "Alt+F11"
    );
    frame(
        &ctx,
        &mut e,
        980.,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(e.options.recording.is_none());
    assert_eq!(
        e.preferences.section(),
        Some(crate::preferences::Section::Shortcuts)
    );
    e.options.recording = Some(ShortcutAction::Capture(Action::Fullscreen));
    let o = frame(&ctx, &mut e, 980., vec![]);
    click(&ctx, &mut e, text(&o, "Use Print Screen"));
    assert_eq!(
        e.options
            .draft
            .as_ref()
            .unwrap()
            .get(Action::Fullscreen)
            .unwrap()
            .key,
        0x2c
    );
    let o = frame(&ctx, &mut e, 980., vec![]);
    click(&ctx, &mut e, text(&o, "Discard changes"));
    assert_eq!(e.options.draft, Some(Keybinds::default()));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), document);
    assert!(!e.session.can_undo());
    assert!(!e.capture_requested);
    assert!(e.capture_result.is_none());
}
#[test]
fn preferences_sections_fit_compact_and_wide_views() {
    use crate::preferences::Section as S;
    for width in [900., 1280.] {
        for section in [
            S::General,
            S::Editor,
            S::Capture,
            S::Library,
            S::Shortcuts,
            S::Toolbar,
        ] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            crate::text_layout::configure(&ctx);
            ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.open_preferences(section);
            frame(&ctx, &mut e, width, vec![]);
            let o = frame(&ctx, &mut e, width, vec![]);
            for label in ["General", "Toolbar", "Apply", "Cancel"] {
                assert!(text(&o, label).max.x < width, "{section:?} {label}");
            }
            assert!(text(&o, "Apply").max.y < 690., "{section:?}");
            if section == S::Shortcuts {
                assert!(text(&o, "Save shortcuts").max.y < text(&o, "Apply").min.y);
            }
        }
    }
}

#[test]
fn preferences_apply_saves_cancel_restores_and_defaults_stay_in_their_section() {
    use crate::preferences::Section as S;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    let before = e.quick_controls.workspace();
    e.open_preferences(S::Editor);
    frame(&ctx, &mut e, 1280., vec![]);
    let o = frame(&ctx, &mut e, 1280., vec![]);
    let _ = &o;
    e.edit_preferences_draft(|d| d.workspace.show_grid = true);
    assert_eq!(
        e.quick_controls.workspace(),
        before,
        "nothing saves until Apply"
    );
    let o = frame(&ctx, &mut e, 1280., vec![]);
    click(&ctx, &mut e, text(&o, "Cancel"));
    assert!(e.preferences.section().is_none());
    assert_eq!(e.quick_controls.workspace(), before);
    e.open_preferences(S::Editor);
    frame(&ctx, &mut e, 1280., vec![]);
    let o = frame(&ctx, &mut e, 1280., vec![]);
    let _ = &o;
    e.edit_preferences_draft(|d| d.workspace.show_grid = true);
    let o = frame(&ctx, &mut e, 1280., vec![]);
    click(&ctx, &mut e, text(&o, "Apply"));
    assert!(e.quick_controls.workspace().show_grid);
    assert_eq!(e.quick_controls.snap_grid(), Some(20));
    // Restore defaults here leaves the Library switch alone.
    let mut w = e.quick_controls.workspace();
    w.keep_versions = false;
    e.quick_controls.remember_workspace_for_test(w);
    e.open_preferences(S::Editor);
    frame(&ctx, &mut e, 1280., vec![]);
    let o = frame(&ctx, &mut e, 1280., vec![]);
    click(&ctx, &mut e, text(&o, "Restore defaults"));
    let o = frame(&ctx, &mut e, 1280., vec![]);
    click(&ctx, &mut e, text(&o, "Apply"));
    assert!(!e.quick_controls.workspace().show_grid);
    assert!(!e.quick_controls.workspace().keep_versions);
}

#[test]
fn keybind_categories_keep_navigation_fixed_and_restore_their_scroll_positions() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.open_preferences(crate::preferences::Section::Shortcuts);
    e.options.category = 2;
    let first = frame(&ctx, &mut e, 980., vec![]);
    let before = frame(&ctx, &mut e, 980., vec![]);
    let nav = text(&before, "Functions");
    let footer = text(&before, "Save shortcuts");
    let state = |ctx: &egui::Context| {
        ctx.data(|d| {
            d.get_temp::<(egui::Rect, egui::Vec2)>(egui::Id::unique("keybinds-scroll"))
                .unwrap()
        })
    };
    let (rect, _) = state(&ctx);
    let mut input = vec![
        egui::Event::PointerMoved(rect.center()),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0., -550.),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
        },
    ];
    frame(&ctx, &mut e, 980., std::mem::take(&mut input));
    let output = frame(&ctx, &mut e, 980., vec![]);
    let offset = state(&ctx).1.y;
    assert!(offset > 0.);
    assert_eq!(text(&output, "Functions"), nav);
    assert_eq!(text(&output, "Save shortcuts"), footer);
    assert!(rect.top() > nav.bottom() && rect.bottom() < footer.top());
    if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_KEYBINDS_DIR") {
        dump_meshes(&ctx, &root, "Functions-scrolled", first, output);
    }
    e.options.category = 0;
    frame(
        &ctx,
        &mut e,
        980.,
        vec![egui::Event::PointerMoved(nav.center())],
    );
    assert_eq!(state(&ctx).1.y, 0.);
    e.options.category = 2;
    frame(&ctx, &mut e, 980., vec![]);
    assert!(state(&ctx).1.y >= offset);
    assert!(!e.capture_requested && !e.session.can_undo());
}
#[test]
fn global_keybinds_work_from_options_but_wait_for_recording_and_ocr_dialogs() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    assert!(e.accepts_global_capture(&ctx));
    e.ocr
        .present(Err("synthetic recognition failure".into()), true, false);
    assert!(!e.accepts_global_capture(&ctx));
    e.ocr = Default::default();
    e.open_preferences(crate::preferences::Section::Shortcuts);
    assert!(e.accepts_global_capture(&ctx));
    e.options.recording = Some(ShortcutAction::Capture(Action::Saved));
    assert!(!e.accepts_global_capture(&ctx));
    e.options.recording = None;
    e.shell.backstage = None;
    assert!(e.accepts_global_capture(&ctx));
}

#[test]
fn global_capture_dispatch_from_options_and_stale_input_focus_reaches_capture_request() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.open_preferences(crate::preferences::Section::Shortcuts);
    let _output = PaintedOutput(ctx.run_ui(Default::default(), |ui| {
        let mut text = String::from("synthetic field");
        ui.text_edit_singleline(&mut text).request_focus();
    }));
    assert!(ctx.egui_wants_keyboard_input());
    assert!(e.accepts_global_capture(&ctx));
    e.request_keybind_capture(Action::Region, &ctx);
    assert!(e.capture_requested || e.autosave.action.is_some());
    assert!(e.pending.is_none());
    e.capture_requested = true;
    assert!(!e.accepts_global_capture(&ctx));
}

#[test]
fn inset_clear_has_its_own_hit_target_and_category_rows_do_not_overlap() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    crate::text_layout::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.open_preferences(crate::preferences::Section::Shortcuts);
    e.options.category = 1;
    frame(&ctx, &mut e, 980., vec![]);
    frame(&ctx, &mut e, 980., vec![]);
    let rects = |name| {
        ctx.data(|d| {
            d.get_temp::<(egui::Rect, egui::Rect, egui::Rect)>(egui::Id::unique((
                "shortcut-field",
                name,
            )))
            .unwrap()
        })
    };
    let (whole, record, clear) = rects("Lines");
    let (next, _, _) = rects("Text");
    assert!(whole.contains_rect(clear));
    assert!(record.right() <= clear.left());
    assert!(next.top() >= whole.bottom() + 1.);
    click(&ctx, &mut e, clear);
    assert!(e.options.recording.is_none());
    assert!(
        e.options
            .draft
            .as_ref()
            .unwrap()
            .editor
            .get(capture_core::editor_keybinds::Action::Tool(
                capture_core::toolbar::ToolId::Lines
            ))
            .is_none()
    );
    click(&ctx, &mut e, record);
    assert!(e.options.recording.is_some());
}
