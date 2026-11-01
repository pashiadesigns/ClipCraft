use super::*;
#[test]
fn tag_and_rename_dialogs_focus_once_for_immediate_typing_and_preserve_tab_navigation() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    let entry = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(
            &Document::blank(40, 30).unwrap(),
            "Example",
            Origin::Capture,
        )
        .unwrap();
    let frame = |e: &mut Editor, events| {
        crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                focused: true,
                events,
                ..Default::default()
            },
            |ui| e.show_history_dialog(ui.ctx()),
        ))
    };
    for action in [Action::Tags, Action::Rename] {
        e.history_action(&entry, action, &ctx);
        frame(&mut e, vec![]);
        frame(&mut e, vec![]);
        let focused = ctx
            .memory(|m| m.focused())
            .expect("primary text field is focused");
        frame(&mut e, vec![egui::Event::Text("Typed".into())]);
        let d = e.history.dialog.as_ref().unwrap();
        assert!(
            if action == Action::Tags {
                &d.tags
            } else {
                &d.name
            }
            .contains("Typed")
        );
        let other = egui::Id::unique("focus-elsewhere");
        ctx.memory_mut(|m| {
            m.surrender_focus(focused);
            m.request_focus(other);
        });
        frame(&mut e, vec![]);
        assert_ne!(ctx.memory(|m| m.focused()), Some(focused));
        e.history.dialog = None;
    }
    e.begin_library_management(super::super::manage::Action::AddTags, vec![entry]);
    frame(&mut e, vec![]);
    frame(&mut e, vec![egui::Event::Text("Business".into())]);
    // The bulk modal is separately covered by its visible typed text.
    let out = frame(&mut e, vec![]);
    assert!(crate::color_widgets::tests::text_rect(&out.shapes, "Business").is_some());
}
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use capture_core::{Document, Kind};

#[test]
fn rename_modal_sizes_to_content_and_actions_stay_bottom_right() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    let entry = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(
            &Document::blank(40, 30).unwrap(),
            "Test capture",
            Origin::Capture,
        )
        .unwrap();
    let frame = |e: &mut Editor| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 800.),
                )),
                ..Default::default()
            },
            |ui| e.show_history_dialog(ui.ctx()),
        ))
    };
    e.history_action(&entry, Action::Details, &ctx);
    let mut first = frame(&mut e);
    frame(&mut e);
    e.history_action(&entry, Action::Rename, &ctx);
    let out = frame(&mut e);
    first.0.textures_delta.append(out.0.textures_delta.clone());
    let output = frame(&mut e);
    let text = |s| crate::color_widgets::tests::text_rect(&output.shapes, s).unwrap();
    let title = text("Rename");
    let save = text("Save");
    let cancel = text("Cancel");
    assert!(save.top() > title.bottom());
    assert!(
        save.bottom() - title.top() < 160.,
        "Rename footer left a huge gap"
    );
    assert!(save.left() > title.left() + 180. && cancel.left() > save.right());
    if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_LIBRARY_DIR") {
        dump_meshes(&ctx, &dir, "library-rename", first, output);
    }
}

#[test]
fn details_show_real_file_properties_without_changing_the_project() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    let entry = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(
            &Document::canvas(640, 400, [255; 4]).unwrap(),
            "Install guide",
            Origin::Capture,
        )
        .unwrap();
    let entry = e
        .history
        .store
        .as_mut()
        .unwrap()
        .edit_metadata(&entry, "Install guide", &["Onboarding".into()])
        .unwrap();
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let values = details(e.history.store.as_ref().unwrap(), &entry);
    assert!(values.contains(&("Dimensions", "640 × 400 pixels".into())));
    assert!(values.contains(&("Tags", "Onboarding".into())));
    let path = values
        .iter()
        .find(|(k, _)| *k == "Project file")
        .unwrap()
        .1
        .clone();
    assert!(values.contains(&(
        "Project size",
        format!("{} bytes", std::fs::metadata(path).unwrap().len())
    )));
    e.history_action(&entry, Action::Details, &ctx);
    let frame = |e: &mut Editor| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                ..Default::default()
            },
            |ui| e.show_history_dialog(ui.ctx()),
        ))
    };
    let first = frame(&mut e);
    let output = frame(&mut e);
    for text in [
        "Details",
        "Dimensions",
        "640 × 400 pixels",
        "Project size",
        "Close",
    ] {
        assert!(
            crate::color_widgets::tests::text_rect(&output.shapes, text).is_some(),
            "missing {text}"
        );
    }
    if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_LIBRARY_DIR") {
        dump_meshes(&ctx, &dir, "library-details", first, output);
    }
    assert_eq!(before, serde_json::to_vec(&e.session.document).unwrap());
    e.history_action(&entry, Action::SaveAs, &ctx);
    assert!(e.history.manage.is_some());
}

#[test]
fn shared_library_and_recent_menu_actions_preserve_canvas_and_use_selected_entry() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    let mut doc = Document::canvas(50, 40, [20, 40, 90, 255]).unwrap();
    doc.annotations.push(
        capture_core::steps::marker(1, [20., 20.], 24, [255, 0, 0, 255], (1, false), [50, 40])
            .unwrap(),
    );
    let original = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(&doc, "First", Origin::Capture)
        .unwrap();
    let other = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(&Document::blank(60, 40).unwrap(), "Other", Origin::Canvas)
        .unwrap();
    let canvas = serde_json::to_vec(&e.session.document).unwrap();
    e.copy_history_with(&original, |d| {
        assert_eq!(d.width, 50);
        assert_eq!(d.annotations[0].kind, Kind::Step);
        Ok(())
    });
    assert!(e.status.starts_with("Copied"));
    e.history_action(&original, Action::Rename, &ctx);
    let mut dialog = e.history.dialog.take().unwrap();
    dialog.name = "Onboarding".into();
    e.submit_history_dialog(&dialog).unwrap();
    let renamed = e.history.entry(&original.id).unwrap();
    e.history_action(&renamed, Action::Tags, &ctx);
    let mut dialog = e.history.dialog.take().unwrap();
    dialog.tags = "guide, Support, guide".into();
    e.submit_history_dialog(&dialog).unwrap();
    let tagged = e.history.entry(&original.id).unwrap();
    assert_eq!(tagged.tags, ["guide", "Support"]);
    e.history_action(&tagged, Action::Duplicate, &ctx);
    assert_eq!(e.history.entries().len(), 3);
    let duplicate = e
        .history
        .entries()
        .into_iter()
        .find(|a| a.id != tagged.id && a.id != other.id)
        .unwrap();
    assert_eq!(duplicate.tags, tagged.tags);
    e.history_action(&duplicate, Action::Delete, &ctx);
    assert!(
        e.history.entry(&duplicate.id).is_some(),
        "delete requires its confirmation"
    );
    let dialog = e.history.dialog.take().unwrap();
    e.submit_history_dialog(&dialog).unwrap();
    assert!(e.history.entry(&duplicate.id).is_none());
    assert!(e.history.entry(&other.id).is_some());
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), canvas);

    e.session = capture_core::Session::new(doc.clone()).unwrap();
    e.history.active = Some(tagged.id.clone());
    e.autosave
        .reset(&doc, Some(tagged.clone()), &tagged.label, tagged.origin);
    e.history_action(&tagged, Action::Delete, &ctx);
    let dialog = e.history.dialog.take().unwrap();
    e.submit_history_dialog(&dialog).unwrap();
    e.autosave_tick(&ctx, true);
    assert!(!e.library_saved());
    assert_eq!(e.autosave_summary().0, "Removed from Library");
    assert!(e.history.active.is_none());
    assert!(e.history.entry(&tagged.id).is_none());
    assert_eq!(e.session.document.annotations, doc.annotations);

    let mut guide = capture_core::step_capture::Guide::new([80, 50]).unwrap();
    guide
        .push(
            image::RgbaImage::from_pixel(80, 50, image::Rgba([60, 110, 160, 255])),
            capture_core::step_capture::Gesture {
                start: [25, 25],
                end: [25, 25],
                button: capture_core::step_capture::Button::Left,
            },
        )
        .unwrap();
    e.history
        .store
        .as_mut()
        .unwrap()
        .create(&guide.document().unwrap(), "Step capture", Origin::Capture)
        .unwrap();

    // Both views open the same real context menu via secondary pointer events.
    for compact in [true, false] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        e.history.textures.clear();
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1050., 650.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| e.show_history(ui, compact),
            ))
        };
        let mut first = frame(&mut e, vec![]);
        let warm = frame(&mut e, vec![]);
        first.0.textures_delta.append(warm.textures_delta.clone());
        let tile = crate::color_widgets::tests::text_rect(&warm.shapes, "IMAGE")
            .unwrap()
            .center()
            - egui::vec2(0., 30.);
        let event = |pressed| egui::Event::PointerButton {
            pos: tile,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let press = frame(&mut e, vec![egui::Event::PointerMoved(tile), event(true)]);
        first.0.textures_delta.append(press.textures_delta.clone());
        let release = frame(&mut e, vec![event(false)]);
        first
            .0
            .textures_delta
            .append(release.textures_delta.clone());
        let output = frame(&mut e, vec![]);
        for label in [
            "Open in Editor",
            "Copy to Clipboard",
            "Save As…",
            "Details",
            "Duplicate",
            "Rename",
            "Edit Tags",
            "Delete from Library",
        ] {
            assert!(
                crate::color_widgets::tests::text_rect(&output.shapes, label).is_some(),
                "{compact}: {label}"
            );
        }
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_LIBRARY_DIR") {
            dump_meshes(
                &ctx,
                &dir,
                if compact {
                    "Recent-menu"
                } else {
                    "Library-menu"
                },
                first,
                output,
            );
        }
    }
}
