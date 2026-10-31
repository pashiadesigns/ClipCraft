use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2};
#[test]
fn typography_ribbon_actions_batch_preview_and_inline_drafts_commit_coherently() {
    use capture_core::text::typography::Script;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    let before = e.session.document.annotations.clone();
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let button_rect = |label| {
        ctx.data(|d| {
            d.get_temp::<(egui::Rect, bool)>(egui::Id::unique(("quick_text_button", label)))
                .unwrap()
                .0
        })
    };
    click(&ctx, &mut e, button_rect("Superscript").center());
    assert!(
        e.selected_objects()
            .iter()
            .all(|a| a.text.as_ref().unwrap().typography().script == Script::Superscript)
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before);
    assert!(!e.session.can_undo());
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, button_rect("Subscript").center());
    assert!(
        e.selected_objects()
            .iter()
            .all(|a| a.text.as_ref().unwrap().typography().script == Script::Subscript)
    );
    let before_spacing = e.session.document.annotations.clone();
    let menu = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("spacing-menu"))
            .unwrap()
    });
    click(&ctx, &mut e, menu.center());
    let rail = ctx.data(|d| {
        d.get_temp::<(egui::Rect, egui::Rect)>(egui::Id::unique(("property-slider", "Spacing")))
            .unwrap()
            .1
    });
    frame(&ctx, &mut e, button(rail.center(), true));
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerMoved(rail.center() + egui::vec2(15., 0.))],
    );
    assert_eq!(e.session.document.annotations, before_spacing);
    assert!(
        e.property_preview
            .as_ref()
            .unwrap()
            .batch_preview
            .iter()
            .all(|a| a.text.as_ref().unwrap().typography().spacing > 0.)
    );
    frame(
        &ctx,
        &mut e,
        button(rail.center() + egui::vec2(15., 0.), false),
    );
    click(&ctx, &mut e, egui::pos2(850., 600.));
    assert!(
        e.selected_objects()
            .iter()
            .all(|a| a.text.as_ref().unwrap().typography().spacing > 0.)
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before_spacing);
    let menu = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("case-menu"))
            .unwrap()
    });
    click(&ctx, &mut e, menu.center());
    let upper = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("case-choice", "UPPERCASE")))
            .unwrap()
    });
    click(&ctx, &mut e, upper.center());
    assert!(e.selected_objects().iter().all(|a| {
        let c = &a.text.as_ref().unwrap().content;
        *c == c.to_uppercase()
    }));
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before_spacing);
    // Live palette affects the label background on every selected object.
    frame(&ctx, &mut e, vec![]);
    let swatch = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("text-highlight-color"))
            .unwrap()
    });
    click(&ctx, &mut e, swatch.center());
    let output = frame(&ctx, &mut e, vec![]);
    // Palette presets expose their swatch rectangles for interaction tests.
    let yellow = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("preset-swatch", "Yellow")))
            .unwrap()
    });
    drop(output);
    click(&ctx, &mut e, yellow.center());
    assert_eq!(e.session.document.annotations, before_spacing);
    click(&ctx, &mut e, egui::pos2(850., 600.));
    assert!(
        e.selected_objects().iter().all(|a| a
            .text
            .as_ref()
            .unwrap()
            .typography()
            .highlight
            .is_some())
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before_spacing);
    let id = e.selected_objects()[0].id;
    e.select_only(Some(id));
    e.begin_text_edit();
    assert!(e.text_draft.is_some());
    let saved = e.session.document.annotations.clone();
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, button_rect("Superscript").center());
    assert_eq!(
        e.text_draft
            .as_ref()
            .unwrap()
            .preview
            .text
            .as_ref()
            .unwrap()
            .typography()
            .script,
        Script::Superscript
    );
    assert_eq!(e.session.document.annotations, saved);
    assert!(e.commit_text());
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, saved);
}

fn fixture(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    e.session = capture_core::Session::new(
        serde_json::from_str(include_str!(
            "../../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let ids: Vec<_> = e
        .session
        .document
        .annotations
        .iter()
        .filter(|a| a.text.is_some())
        .map(|a| a.id)
        .collect();
    e.select_only(Some(ids[0]));
    for id in ids.into_iter().skip(1) {
        e.click_selection(Some(id), true);
    }
    e
}

fn frame(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<Event>,
) -> crate::property_layout::tests::PaintedOutput {
    let output = ctx.run_ui(
        egui::RawInput {
            events,
            focused: true,
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            ..Default::default()
        },
        |ui| {
            e.prepare_property_input(ctx);
            e.show_quick_font_controls(ui);
            e.settle_property_preview(ctx);
        },
    );
    crate::property_layout::tests::PaintedOutput(output)
}
fn button(pos: Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
    ]
}
fn click(ctx: &egui::Context, e: &mut Editor, pos: Pos2) {
    frame(ctx, e, button(pos, true));
    frame(ctx, e, button(pos, false));
    frame(ctx, e, vec![]);
}

#[test]
fn ribbon_font_color_live_palette_changes_every_selected_kind_and_undoes_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    let original = e.session.document.clone();
    let first = frame(&ctx, &mut e, vec![]);
    let second = frame(&ctx, &mut e, vec![]);
    if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_FONT_DIR") {
        crate::property_layout::tests::dump_meshes(&ctx, &root, "FontControls", first, second);
    }
    let swatch = ctx.data(|d| {
        d.get_temp::<egui::Response>(egui::Id::unique("ribbon_font_color"))
            .unwrap()
    });
    click(&ctx, &mut e, swatch.rect.center());
    for (label, chosen) in [
        ("Red", [237, 32, 36, 255]),
        ("Transparent", [0; 4]),
        ("Blue", [29, 160, 231, 255]),
    ] {
        frame(&ctx, &mut e, vec![]);
        let pos = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("preset-swatch", label)))
                    .unwrap()
            })
            .center();
        click(&ctx, &mut e, pos);
        for (a, b) in e
            .preview_document()
            .annotations
            .iter()
            .zip(&original.annotations)
        {
            if e.is_selected(a.id) {
                assert_eq!(a.text.as_ref().unwrap().color, Some(chosen), "{:?}", a.kind);
                let mut expected = b.clone();
                expected.text.as_mut().unwrap().color = Some(chosen);
                assert_eq!(a, &expected, "only label ink may change");
            } else {
                assert_eq!(a, b);
            }
        }
        assert_eq!(e.session.document.annotations, original.annotations);
        assert!(!e.session.can_undo());
    }
    click(&ctx, &mut e, egui::pos2(850.0, 650.0));
    assert!(e.session.can_undo());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("colors.capture.json");
    e.session.save(&path).unwrap();
    let reopened = capture_core::Document::load(&path).unwrap();
    assert_eq!(
        reopened.render().unwrap(),
        e.session.document.render().unwrap()
    );
    if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_FONT_DIR") {
        let root = std::path::Path::new(&root);
        original.export_png(&root.join("original.png")).unwrap();
        reopened
            .export_png(&root.join("all-labels-blue.png"))
            .unwrap();
    }
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(!e.session.can_undo());
}

#[test]
fn ribbon_size_real_drag_updates_mixed_text_live_and_commits_one_undo() {
    for inline in [false, true] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = fixture(&ctx);
        if inline {
            e.select_only(e.selected);
            e.begin_text_edit();
        }
        let original = e.session.document.annotations.clone();
        let initial = e.selected_objects()[0].text.as_ref().unwrap().size_points;
        frame(&ctx, &mut e, vec![]);
        frame(&ctx, &mut e, vec![]);
        let pos = ctx
            .data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique("ribbon_font_size"))
                    .unwrap()
            })
            .center();
        frame(&ctx, &mut e, button(pos, true));
        let mut previous = initial;
        for (delta, growing) in [(24.0, true), (44.0, true), (28.0, false)] {
            frame(
                &ctx,
                &mut e,
                vec![Event::PointerMoved(pos + egui::vec2(delta, 0.0))],
            );
            let preview = e.preview_document();
            let sizes: Vec<_> = preview
                .annotations
                .iter()
                .filter(|a| e.is_selected(a.id))
                .map(|a| a.text.as_ref().unwrap().size_points)
                .collect();
            let current = sizes[0];
            assert!(
                if growing {
                    current > previous
                } else {
                    current < previous
                },
                "{sizes:?}, previous {previous}"
            );
            assert!(sizes.iter().all(|s| *s == current));
            assert_eq!(e.session.document.annotations, original);
            assert!(!e.session.can_undo());
            previous = current;
        }
        frame(&ctx, &mut e, button(pos + egui::vec2(28.0, 0.0), false));
        if inline {
            assert!(e.text_draft.is_some());
            assert!(e.commit_text());
        }
        assert_ne!(e.session.document.annotations, original);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        assert!(!e.session.can_undo());
    }
}
