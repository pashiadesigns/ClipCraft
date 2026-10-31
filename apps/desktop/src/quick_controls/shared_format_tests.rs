use super::*;
use crate::Kind;
use egui::{Event, Key, Modifiers};

fn setup(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    let d: capture_core::Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    e.session = capture_core::Session::new(d).unwrap();
    e
}

#[test]
fn copy_snapshot_and_shared_paste_preserve_geometry_and_undo_as_one_batch() {
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    let source = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Callout)
        .unwrap()
        .clone();
    e.select_only(Some(source.id));
    e.copy_all_with(|d| {
        assert_eq!(d.annotations.len(), 11);
        Ok(())
    });
    assert_eq!(e.quick_controls.copied_format, Some(source.clone()));
    // Failure leaves the last successful copy intact; the OS is never read here.
    e.select_only(Some(2));
    e.copy_all_with(|_| Err("injected clipboard unavailable".into()));
    assert_eq!(e.quick_controls.copied_format, Some(source.clone()));
    e.session
        .document
        .annotations
        .iter_mut()
        .find(|a| a.id == source.id)
        .unwrap()
        .width = 11;
    let targets: Vec<_> = e
        .session
        .document
        .annotations
        .iter()
        .filter(|a| a.id != source.id)
        .map(|a| a.id)
        .collect();
    e.select_only(Some(targets[0]));
    for id in targets.iter().skip(1) {
        e.click_selection(Some(*id), true);
    }
    let before = e.session.document.clone();
    e.paste_shared_format();
    for id in targets {
        let previous = before.annotations.iter().find(|a| a.id == id).unwrap();
        let expected = capture_core::object_styles::paint_format(&source, previous).unwrap();
        assert_eq!(
            e.session
                .document
                .annotations
                .iter()
                .find(|a| a.id == id)
                .unwrap(),
            &expected
        );
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("paste.capture.json");
    e.session.save(&path).unwrap();
    assert_eq!(
        capture_core::Document::load(&path)
            .unwrap()
            .render()
            .unwrap(),
        e.session.document.render().unwrap()
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before.annotations);
    assert!(!e.session.can_undo());
    e.select_only(None);
    e.copy_all_with(|_| Ok(()));
    assert!(e.quick_controls.copied_format.is_none());
}

#[test]
fn paste_format_chord_owns_canvas_only_and_plain_paste_stays_unhandled() {
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    e.quick_controls.copied_format = Some(
        e.session
            .document
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Callout)
            .unwrap()
            .clone(),
    );
    let id = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Text)
        .unwrap()
        .id;
    e.select_only(Some(id));
    let before = e.session.document.annotations.clone();
    let run = |e: &mut Editor, modifiers, blocked| {
        let mut retained = false;
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![
                    Event::ModifiersChanged(modifiers),
                    Event::Key {
                        key: Key::V,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers,
                    },
                    Event::Key {
                        key: Key::V,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    },
                ],
                ..Default::default()
            },
            |ui| {
                e.shell.library = blocked;
                e.shared_format_shortcuts(ui.ctx());
                retained = ui.input(|i| {
                    i.events.iter().any(|v| {
                        matches!(
                            v,
                            Event::Key {
                                key: Key::V,
                                pressed: true,
                                ..
                            }
                        )
                    })
                });
            },
        );
        out.textures_delta.clear();
        retained
    };
    let chord = Modifiers::COMMAND | Modifiers::SHIFT;
    assert!(run(&mut e, Modifiers::COMMAND, false));
    assert_eq!(e.session.document.annotations, before);
    assert!(run(&mut e, chord, true));
    assert_eq!(e.session.document.annotations, before);
    e.shell.library = false;
    e.begin_text_edit();
    assert!(e.text_draft.is_some());
    assert!(run(&mut e, chord, false));
    assert_eq!(e.session.document.annotations, before);
    e.text_draft = None;
    assert!(!run(&mut e, chord, false));
    assert_ne!(e.session.document.annotations, before);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before);
    assert!(!e.session.can_undo());
}
