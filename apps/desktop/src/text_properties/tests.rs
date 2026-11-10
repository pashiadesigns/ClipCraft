use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect};

fn fixture(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    let doc: crate::Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    let mut a = doc
        .annotations
        .into_iter()
        .find(|a| a.kind == crate::Kind::Text)
        .unwrap();
    a.text.as_mut().unwrap().size_points = 24.0;
    a.text.as_mut().unwrap().bold = false;
    a.text.as_mut().unwrap().content = "Text formatting".into();
    a.start = [60.0, 60.0];
    a.end = [360.0, 150.0];
    let mut b = a.clone();
    b.id = a.id + 100;
    b.text.as_mut().unwrap().content = "Another label".into();
    e.session.document.annotations = vec![a.clone(), b.clone()];
    e.select_only(Some(a.id));
    e.click_selection(Some(b.id), true);
    e.tool = Some(crate::Kind::Text);
    e
}
fn frame(ctx: &egui::Context, e: &mut Editor, events: Vec<Event>) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(262.0, 980.0))),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| {
            e.prepare_property_input(ctx);
            e.property_tabs(ui);
            e.properties(ui);
            e.settle_property_preview(ctx);
            assert!(
                ui.min_rect().width() <= 262.1,
                "sidebar expanded to {:?}",
                ui.min_rect()
            );
        },
    ))
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
fn text_button_pos(ctx: &egui::Context, label: &str) -> Pos2 {
    ctx.data(|d| {
        d.get_temp::<(Rect, bool)>(egui::Id::unique(("quick_text_button", label)))
            .unwrap()
            .0
            .center()
    })
}

#[test]
fn text_outline_width_and_slider_preview_batch_and_inline_ink_without_extra_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    let original = e.session.document.annotations.clone();
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let toggle = ctx
        .data(|d| {
            d.get_temp::<Rect>(egui::Id::unique("text-outline-size"))
                .unwrap()
        })
        .center();
    click(&ctx, &mut e, toggle);
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.text.as_ref().unwrap().outline.is_some())
    );
    assert_eq!(
        e.session.document.version,
        capture_core::MAX_PROJECT_VERSION
    );
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
    click(&ctx, &mut e, toggle);
    let outlined = e.session.document.annotations.clone();
    let rail = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique("text-outline-size"))
            .unwrap()
    });
    let from = rail.center();
    let to = from + egui::vec2(25., 0.);
    frame(&ctx, &mut e, button(from, true));
    frame(&ctx, &mut e, vec![Event::PointerMoved(to)]);
    assert_eq!(e.session.document.annotations, outlined);
    assert!(
        e.preview_document().annotations.iter().all(|a| a
            .text
            .as_ref()
            .unwrap()
            .outline
            .unwrap()
            .width
            > 1.)
    );
    frame(&ctx, &mut e, button(to, false));
    frame(&ctx, &mut e, vec![]);
    assert!(
        e.session.document.annotations.iter().all(|a| a
            .text
            .as_ref()
            .unwrap()
            .outline
            .unwrap()
            .width
            > 1.)
    );
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, outlined);
    let id = outlined[0].id;
    e.select_only(Some(id));
    e.begin_text_edit();
    e.text_draft
        .as_mut()
        .unwrap()
        .preview
        .text
        .as_mut()
        .unwrap()
        .content = "Outlined typing".into();
    let canvas = e.canvas_document();
    assert_eq!(
        canvas.annotations[0].text.as_ref().unwrap().content,
        "Outlined typing"
    );
    assert!(canvas.annotations[0].text.as_ref().unwrap().decorated());
    assert!(e.commit_text());
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, outlined);
    let paint = egui::Context::default();
    crate::shell::configure(&paint);
    let mut pane = fixture(&paint);
    for a in &mut pane.session.document.annotations {
        a.text.as_mut().unwrap().outline = Some(capture_core::text::outline::Outline {
            width: 2.5,
            ..Default::default()
        });
    }
    pane.session.document.promote_features();
    let first = frame(&paint, &mut pane, vec![]);
    let second = frame(&paint, &mut pane, vec![]);
    if let Ok(dir) = std::env::var("CLIPCRAFT_TEXT_OUTLINE_UI_DIR") {
        dump_meshes(&paint, &dir, "text-outline", first, second);
    }
}

#[test]
fn dedicated_text_sidebar_buttons_batch_justify_fit_and_preserve_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    let original = e.session.document.clone();
    let first = frame(&ctx, &mut e, vec![]);
    let second = frame(&ctx, &mut e, vec![]);
    if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_TEXT_SIDEBAR_DIR") {
        dump_meshes(&ctx, &root, "TextProperties", first, second);
    }
    for label in [
        "Bold",
        "Italic",
        "Underline",
        "Strikethrough",
        "Increase font size",
        "Decrease font size",
    ] {
        frame(&ctx, &mut e, vec![]);
        let pos = text_button_pos(&ctx, label);
        click(&ctx, &mut e, pos);
        for (a, old) in e
            .session
            .document
            .annotations
            .iter()
            .zip(&original.annotations)
        {
            let t = a.text.as_ref().unwrap();
            assert_eq!(t.content, old.text.as_ref().unwrap().content);
            match label {
                "Bold" => assert!(t.bold),
                "Italic" => assert!(t.italic),
                "Underline" => assert!(t.underline),
                "Strikethrough" => assert!(t.strikethrough),
                "Increase font size" => assert_eq!(t.size_points, 25.0),
                _ => assert_eq!(t.size_points, 23.0),
            }
        }
        e.execute(crate::Command::Undo);
        assert_eq!(e.session.document.annotations, original.annotations);
        assert!(!e.session.can_undo());
    }
    frame(&ctx, &mut e, vec![]);
    let pos = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique((
            "format-grid",
            "Text justification: Bottom right",
        )))
        .unwrap()
        .center()
    });
    click(&ctx, &mut e, pos);
    for a in &e.session.document.annotations {
        let t = a.text.as_ref().unwrap();
        assert_eq!(t.horizontal, capture_core::text::HorizontalAlignment::Right);
        assert_eq!(t.vertical, capture_core::text::VerticalAlignment::Bottom);
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.capture.json");
    e.session.save(&path).unwrap();
    assert_eq!(
        crate::Document::load(&path).unwrap().render().unwrap(),
        e.session.document.render().unwrap()
    );
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, original.annotations);
}

#[test]
fn dedicated_text_sidebar_size_drag_is_live_and_inline_changes_commit_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    let original = e.session.document.clone();
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let start = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique("ribbon_font_size"))
            .unwrap()
            .center()
    });
    frame(&ctx, &mut e, button(start, true));
    let end = start + egui::vec2(32.0, 0.0);
    frame(&ctx, &mut e, vec![Event::PointerMoved(end)]);
    let preview = e.preview_document();
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(
        preview
            .annotations
            .iter()
            .all(|a| a.text.as_ref().unwrap().size_points > 24.0)
    );
    frame(&ctx, &mut e, button(end, false));
    assert_eq!(e.session.document.annotations, preview.annotations);
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(!e.session.can_undo());

    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    e.select_only(Some(original.annotations[0].id));
    e.begin_text_edit();
    let first = frame(&ctx, &mut e, vec![]);
    let second = frame(&ctx, &mut e, vec![]);
    if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_TEXT_SIDEBAR_DIR") {
        dump_meshes(&ctx, &root, "InlineTextProperties", first, second);
    }
    for label in ["Bold", "Underline", "Increase font size"] {
        let pos = text_button_pos(&ctx, label);
        click(&ctx, &mut e, pos);
    }
    let t = e
        .text_draft
        .as_ref()
        .unwrap()
        .preview
        .text
        .as_ref()
        .unwrap();
    assert!(t.bold && t.underline);
    assert_eq!(t.size_points, 25.0);
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(!e.session.can_undo());
    assert!(e.commit_text());
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, original.annotations);
    assert!(!e.session.can_undo());
}

#[test]
fn object_text_entry_routes_to_text_properties_and_back_commits_one_edit() {
    use crate::{Kind, shell::PropertyTab};
    let fixture: crate::Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    for kind in [
        Kind::Rectangle,
        Kind::Ellipse,
        Kind::Polygon,
        Kind::Callout,
        Kind::Step,
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut a = fixture
            .annotations
            .iter()
            .find(|a| a.kind == kind)
            .unwrap()
            .clone();
        a.rotation_degrees = 27.0;
        a.text.get_or_insert_with(Default::default).bold = false;
        e.session.document.version = capture_core::MAX_PROJECT_VERSION;
        e.session.document.annotations = vec![a.clone()];
        e.tool = Some(kind);
        e.select_only(Some(a.id));
        e.shell.property_tab = PropertyTab::Objects;
        e.begin_text_edit();
        assert_eq!(e.shell.property_tab, PropertyTab::Tool);
        let first = frame(&ctx, &mut e, vec![]);
        let second = frame(&ctx, &mut e, vec![]);
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_OBJECT_TEXT_DIR") {
            dump_meshes(
                &ctx,
                &root,
                &format!("{kind:?}TextProperties"),
                first,
                second,
            );
        }
        let back = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::unique("text-back-to-object"))
                .unwrap()
        });
        assert!(
            back.top() > 30.0,
            "back belongs in section header below tabs"
        );
        let pos = text_button_pos(&ctx, "Bold");
        click(&ctx, &mut e, pos);
        assert!(
            e.text_draft
                .as_ref()
                .unwrap()
                .preview
                .text
                .as_ref()
                .unwrap()
                .bold
        );
        assert_eq!(e.session.document.annotations, vec![a.clone()]);
        assert!(!e.session.can_undo());
        click(&ctx, &mut e, back.center());
        assert!(e.text_draft.is_none());
        assert_eq!(e.selected, Some(a.id));
        assert_eq!(e.tool, Some(kind));
        assert_eq!(e.shell.property_tab, PropertyTab::Tool);
        let changed = &e.session.document.annotations[0];
        assert!(changed.text.as_ref().unwrap().bold);
        assert_eq!(changed.kind, kind);
        assert_eq!(changed.rotation_degrees, 27.0);
        assert_eq!(
            (
                changed.start,
                changed.end,
                changed.color,
                changed.fill,
                changed.width
            ),
            (a.start, a.end, a.color, a.fill, a.width)
        );
        assert_eq!(changed.callout, a.callout);
        e.execute(crate::Command::Undo);
        assert_eq!(e.session.document.annotations, vec![a.clone()]);
        assert!(!e.session.can_undo());
        // Enter/back without edits must not add a history entry.
        e.begin_text_edit();
        frame(&ctx, &mut e, vec![]);
        frame(&ctx, &mut e, vec![]);
        click(&ctx, &mut e, back.center());
        assert!(e.text_draft.is_none());
        assert!(!e.session.can_undo());
    }
    let ctx = egui::Context::default();
    let mut e = super::tests::fixture(&ctx);
    e.select_only(Some(e.session.document.annotations[0].id));
    e.begin_text_edit();
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    assert!(
        ctx.data(|d| d.get_temp::<Rect>(egui::Id::unique("text-back-to-object")))
            .is_none(),
        "standalone Text has no object back button"
    );
}

#[test]
fn regular_text_resize_fit_can_be_chosen_from_actual_properties_popup() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = fixture(&ctx);
    let id = e.session.document.annotations[0].id;
    e.select_only(Some(id));
    frame(&ctx, &mut e, vec![]);
    let output = frame(&ctx, &mut e, vec![]);
    assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Advanced").is_none());
    let output = frame(&ctx, &mut e, vec![]);
    let combo = crate::color_widgets::tests::text_rect(
        &output.shapes,
        e.selected_annotation()
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .fit
            .label(),
    )
    .unwrap()
    .center();
    click(&ctx, &mut e, combo);
    let output = frame(&ctx, &mut e, vec![]);
    let choice = crate::color_widgets::tests::text_rect(&output.shapes, "Resize shape to fit text")
        .unwrap()
        .center();
    click(&ctx, &mut e, choice);
    assert_eq!(
        e.selected_annotation().unwrap().text.as_ref().unwrap().fit,
        TextFit::ResizeShape,
        "{}",
        e.status
    );
    assert!(e.session.can_undo());
    e.execute(crate::Command::Undo);
    assert_ne!(
        e.selected_annotation().unwrap().text.as_ref().unwrap().fit,
        TextFit::ResizeShape
    );
}

/// BI2: while typing with text selected, the Style swatches show the selected
/// text's format and a change there reaches only the field that was changed,
/// so the swatch does not snap back after the picker closes.
#[test]
fn style_swatches_show_and_edit_the_selected_text_format() {
    use capture_core::text::rich::{format_at, format_range};
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = fixture(&ctx);
    let id = e.session.document.annotations[0].id;
    e.select_only(Some(id));
    e.begin_text_edit();
    let red = [220, 30, 30, 255];
    {
        let d = e.text_draft.as_mut().unwrap();
        d.selection = 0..4;
        d.selection_active = true;
        format_range(d.preview.text.as_mut().unwrap(), 0..4, |f| {
            f.color = Some(red)
        });
    }
    let a = e.selected_annotation().unwrap();
    let shown = e
        .selection_view(&a)
        .expect("the selection's format is shown");
    assert_eq!(shown.text.as_ref().unwrap().color, Some(red));
    let blue = [20, 60, 220, 255];
    let mut edited = shown.clone();
    edited.text.as_mut().unwrap().outline = Some(capture_core::text::outline::Outline {
        color: blue,
        width: 2.,
    });
    e.update_text_sidebar_seen(edited, false, Some(shown));
    let text = e.text_draft.as_ref().unwrap().preview.text.clone().unwrap();
    let first = format_at(&text, 0);
    assert_eq!(first.color, Some(red), "the selection keeps its color");
    assert_eq!(
        first.outline.map(|o| o.color),
        Some(blue),
        "and gains the stroke"
    );
    let rest = format_at(&text, 8);
    assert_ne!(
        rest.color,
        Some(red),
        "text outside the selection keeps its color"
    );
    assert!(rest.outline.is_none_or(|o| o.color != blue));
    let a = e.selected_annotation().unwrap();
    let again = e.selection_view(&a).unwrap();
    assert_eq!(
        again.text.as_ref().unwrap().color,
        Some(red),
        "no snap back"
    );
    assert_eq!(
        again.text.as_ref().unwrap().outline.map(|o| o.color),
        Some(blue)
    );
}
