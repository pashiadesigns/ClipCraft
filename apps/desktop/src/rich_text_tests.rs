use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

pub(crate) fn editor(ctx: &egui::Context) -> Editor {
    crate::shell::configure(ctx);
    crate::text_layout::configure(ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(ctx);
    let mut a = e.new_annotation(crate::Kind::Text, [25., 25.], [610., 360.]);
    a.text = Some(TextStyle {
        content: "Review the guide\nContact alex@example.com\nShare with your team".into(),
        color: Some([40, 60, 80, 255]),
        horizontal: capture_core::text::HorizontalAlignment::Left,
        vertical: capture_core::text::VerticalAlignment::Top,
        ..Default::default()
    });
    e.session.document.annotations = vec![a.clone()];
    e.select_only(Some(a.id));
    e
}
pub(crate) fn frame(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<egui::Event>,
) -> PaintedOutput {
    frame_size(ctx, e, events, egui::vec2(1200., 760.))
}
fn frame_size(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<egui::Event>,
    size: egui::Vec2,
) -> PaintedOutput {
    let output = PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| {
            crate::controls::prepare_dialog_keys(ctx);
            egui::CentralPanel::default().show(ui, |ui| {
                e.show_canvas(ui);
            });
            e.show_rich_text(ctx);
            e.show_privacy_review(ctx);
        },
    ));
    ctx.data_mut(|d| {
        let all = d.get_temp_mut_or_default::<Vec<(egui::TextureId, egui::epaint::ImageDelta)>>(
            egui::Id::unique("rich-textures"),
        );
        for (id, patches) in &output.0.textures_delta.set {
            all.extend(patches.iter().map(|patch| (*id, patch.clone())));
        }
    });
    output
}
pub(crate) fn screenshot(ctx: &egui::Context, e: &mut Editor, name: &str) {
    let mut first = frame(ctx, e, vec![]);
    for _ in 0..4 {
        let warm = frame(ctx, e, vec![]);
        first.0.textures_delta.append(warm.0.textures_delta.clone());
    }
    first.0.textures_delta.clear();
    for (id, patch) in ctx
        .data(|d| {
            d.get_temp::<Vec<(egui::TextureId, egui::epaint::ImageDelta)>>(egui::Id::unique(
                "rich-textures",
            ))
        })
        .unwrap()
    {
        first.0.textures_delta.push(id, patch);
    }
    let output = frame(ctx, e, vec![]);
    if let Ok(root) = std::env::var("CLIPDESIGNER_RICH_UI_DIR") {
        dump_meshes(ctx, &root, name, first, output);
    }
}
fn click(ctx: &egui::Context, e: &mut Editor, label: &str) {
    // Modal layout and newly registered font faces settle before hit testing.
    frame(ctx, e, vec![]);
    frame(ctx, e, vec![]);
    let o = frame(ctx, e, vec![]);
    let p = crate::color_widgets::tests::text_rect(&o.shapes, label)
        .unwrap_or_else(|| panic!("Missing click target: {label}"))
        .center();
    for pressed in [true, false] {
        let _ = frame(
            ctx,
            e,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
#[test]
fn paragraph_dialog_apply_cancel_and_link_validation_are_transactional() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let original = e.session.document.clone();
    e.open_rich_text(Tab::Paragraph);
    e.rich_text.paragraph.list = List::Numbered;
    e.rich_text.paragraph.line_spacing = 1.5;
    screenshot(&ctx, &mut e, "Paragraph");
    click(&ctx, &mut e, "Cancel");
    assert_eq!(
        serde_json::to_value(&e.session.document).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    e.open_rich_text(Tab::Paragraph);
    e.rich_text.paragraph.list = List::Numbered;
    e.rich_text.paragraph.line_spacing = 1.5;
    click(&ctx, &mut e, "Apply");
    assert!(!e.rich_text.open);
    assert_eq!(
        e.session.document.annotations[0]
            .text
            .as_ref()
            .unwrap()
            .rich
            .as_ref()
            .unwrap()
            .paragraphs[0]
            .list,
        List::Numbered
    );
    e.execute(Command::Undo);
    assert_eq!(
        serde_json::to_value(&e.session.document).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    e.open_rich_text(Tab::Link);
    e.rich_text.link = "javascript:alert(1)".into();
    click(&ctx, &mut e, "Apply");
    assert!(e.rich_text.open && !e.rich_text.error.is_empty());
    assert_eq!(
        serde_json::to_value(&e.session.document).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    e.rich_text.link = "https://example.com/guide".into();
    screenshot(&ctx, &mut e, "Link");
    click(&ctx, &mut e, "Apply");
    assert_eq!(
        rich::format_at(e.session.document.annotations[0].text.as_ref().unwrap(), 0)
            .link
            .as_deref(),
        Some("https://example.com/guide")
    );
    for tab in [Tab::Font, Tab::Styles, Tab::Find, Tab::Spelling] {
        e.open_rich_text(tab);
        screenshot(
            &ctx,
            &mut e,
            match tab {
                Tab::Font => "Font",
                Tab::Styles => "Styles",
                Tab::Find => "Find",
                _ => "Spelling",
            },
        );
        e.rich_text.open = false;
    }
}

#[test]
fn categories_share_pending_edits_and_preview_without_changing_document() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let before = serde_json::to_value(&e.session.document).unwrap();
    e.open_rich_text(Tab::Font);
    e.rich_text.format.size = 13.;
    e.rich_text.format.bold = true;
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, "Paragraph");
    e.rich_text.paragraph.after = 12.;
    e.rich_text.paragraph.list = List::Bullet;
    frame(&ctx, &mut e, vec![]);
    let preview = e.rich_text.preview.as_ref().unwrap().1.id();
    frame(&ctx, &mut e, vec![]);
    assert_eq!(
        preview,
        e.rich_text.preview.as_ref().unwrap().1.id(),
        "Unchanged preview reuses its texture"
    );
    click(&ctx, &mut e, "Link");
    e.rich_text.link = "https://example.com".into();
    click(&ctx, &mut e, "Styles");
    assert_eq!(before, serde_json::to_value(&e.session.document).unwrap());
    click(&ctx, &mut e, "Apply");
    let text = e.session.document.annotations[0].text.as_ref().unwrap();
    let format = rich::format_at(text, 0);
    assert_eq!(format.size, 13.);
    assert!(format.bold);
    assert_eq!(format.link.as_deref(), Some("https://example.com"));
    assert!(
        text.rich
            .as_ref()
            .unwrap()
            .paragraphs
            .iter()
            .all(|p| p.list == List::Bullet && p.after == 12.)
    );
    e.execute(Command::Undo);
    assert_eq!(before, serde_json::to_value(&e.session.document).unwrap());
}

#[test]
fn style_choices_match_their_applied_format_and_dialog_geometry_is_stable() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.open_rich_text(Tab::Styles);
    for (label, size, bold, italic) in [
        ("Body", 13., false, false),
        ("Heading", 20., true, false),
        ("Subheading", 16., true, false),
        ("Caption", 12., false, true),
    ] {
        click(&ctx, &mut e, label);
        let f = rich::format_at(&e.rich_text.working, 0);
        assert_eq!((f.size, f.bold, f.italic), (size, bold, italic));
    }
    let mut geometries = vec![];
    for tab in [
        Tab::Font,
        Tab::Paragraph,
        Tab::Styles,
        Tab::Link,
        Tab::Find,
        Tab::Spelling,
    ] {
        click(&ctx, &mut e, tab.label());
        geometries.push(ctx.data(|d| {
            ["body", "preview", "footer"].map(|name| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("text-dialog", name)))
                    .unwrap()
            })
        }));
    }
    assert!(geometries.windows(2).all(|r| r[0] == r[1]));
    assert_eq!(geometries[0][0].height(), 500.);
    assert_eq!(geometries[0][1].width(), 222.);
    assert!(geometries[0][2].bottom() < 760.);
    e.rich_text.autocorrect = true;
    click(&ctx, &mut e, "Cancel");
    assert!(!e.rich_text.autocorrect);
}

#[test]
fn responsive_dialog_keeps_actions_visible_and_enter_applies() {
    for size in [
        egui::vec2(800., 700.),
        egui::vec2(600., 600.),
        egui::vec2(320., 480.),
    ] {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.open_rich_text(Tab::Paragraph);
        for _ in 0..4 {
            frame_size(&ctx, &mut e, vec![], size);
        }
        for tab in [
            Tab::Font,
            Tab::Paragraph,
            Tab::Styles,
            Tab::Link,
            Tab::Find,
            Tab::Spelling,
        ] {
            e.rich_text.tab = tab;
            let o = frame_size(&ctx, &mut e, vec![], size);
            let apply = crate::color_widgets::tests::text_rect(&o.shapes, "Apply").unwrap();
            assert!(
                apply.min.x >= 0.
                    && apply.max.x <= size.x
                    && apply.min.y >= 0.
                    && apply.max.y <= size.y,
                "{size:?} {apply:?}"
            );
        }
    }
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    e.open_rich_text(Tab::Paragraph);
    frame(&ctx, &mut e, vec![]);
    e.rich_text.paragraph.indent = 16.;
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(!e.rich_text.open);
    assert_eq!(
        e.session.document.annotations[0]
            .text
            .as_ref()
            .unwrap()
            .rich
            .as_ref()
            .unwrap()
            .paragraphs[0]
            .indent,
        16.
    );
}
