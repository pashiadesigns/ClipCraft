use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn automatic_simplify_detects_underlying_text_colors_previews_and_undoes_as_one_edit() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let mut d = capture_core::Document::blank(640, 400).unwrap();
    let mut text = e.new_annotation(Kind::Text, [80., 65.], [400., 125.]);
    text.text.as_mut().unwrap().content = "Account settings".into();
    text.color = [25, 60, 95, 255];
    d.annotations.push(text);
    d.promote_features();
    let pixels = d.render().unwrap();
    e.session =
        capture_core::Session::new(capture_core::Document::from_image(pixels.clone()).unwrap())
            .unwrap();
    e.choose_tool(
        crate::shell::TOOLS
            .iter()
            .position(|t| t.name == "Simplify")
            .unwrap(),
    );
    canvas(&ctx, &mut e, vec![]);
    canvas(&ctx, &mut e, vec![]);
    let t = e.view.current_transform().unwrap();
    let from = t.screen_point([55., 50.]);
    let to = t.screen_point([420., 145.]);
    canvas(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(from), pointer(from, true)],
    );
    canvas(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    let preview = e.creation.simplified.clone();
    assert!(!preview.is_empty());
    assert!(
        preview.len() < 6,
        "glyphs should merge into text blocks: {preview:?}"
    );
    assert!(preview.iter().any(|a| a.color[0] < 70 && a.color[2] > 70));
    assert!(e.session.document.annotations.is_empty());
    canvas(&ctx, &mut e, vec![pointer(to, false)]);
    let committed: Vec<_> = e
        .session
        .document
        .annotations
        .iter()
        .filter(|a| a.analysis.is_none())
        .cloned()
        .map(|mut a| {
            a.group = None;
            a
        })
        .collect();
    assert_eq!(committed, preview);
    assert_eq!(
        e.session
            .document
            .annotations
            .iter()
            .filter(|a| a.analysis.is_some())
            .count(),
        1
    );
    assert_eq!(e.session.document.base_image().unwrap(), pixels);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("auto-simplify.capture.json");
    e.session.save(&file).unwrap();
    assert_eq!(
        capture_core::Document::load(&file)
            .unwrap()
            .render()
            .unwrap(),
        e.session.document.render().unwrap()
    );
    if let Ok(root) = std::env::var("CLIPCRAFT_SIMPLIFY_DIR") {
        std::fs::create_dir_all(&root).unwrap();
        pixels
            .save(std::path::Path::new(&root).join("auto-before.png"))
            .unwrap();
        e.session
            .document
            .export_png(&std::path::Path::new(&root).join("auto-after.png"))
            .unwrap();
    }
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.render().unwrap(), pixels);
    assert!(!e.session.can_undo());
}
fn canvas(ctx: &egui::Context, e: &mut Editor, events: Vec<egui::Event>) {
    let _ = PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640., 400.),
            )),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| e.show_canvas(ui),
    ));
}
fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn simplify_tool_creates_live_blocks_selects_them_and_preserves_last_format() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.choose_tool(
        crate::shell::TOOLS
            .iter()
            .position(|t| t.name == "Simplify")
            .unwrap(),
    );
    assert_eq!(e.tool, Some(Kind::Simplify));
    assert_eq!(e.simplify_palette, vec![[255; 4]]);
    e.simplify_auto = false;
    e.color = [45, 95, 155];
    canvas(&ctx, &mut e, vec![]);
    canvas(&ctx, &mut e, vec![]);
    let transform = e.view.current_transform().unwrap();
    let from = transform.screen_point([80., 80.]);
    let to = transform.screen_point([280., 160.]);
    canvas(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(from), pointer(from, true)],
    );
    canvas(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    assert!(e.session.document.annotations.is_empty());
    let preview = e.creation.annotation.clone().unwrap();
    assert_eq!(preview.kind, Kind::Simplify);
    assert_eq!(preview.color, [45, 95, 155, 255]);
    canvas(&ctx, &mut e, vec![pointer(to, false)]);
    assert_eq!(
        e.session.document.annotations,
        std::slice::from_ref(&preview)
    );
    assert_eq!(e.current_tool().name, "Simplify");
    assert!(e.text_draft.is_none());
    let point = transform.screen_point([180., 120.]);
    canvas(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(point), pointer(point, true)],
    );
    canvas(&ctx, &mut e, vec![pointer(point, false)]);
    assert_eq!(e.selected, Some(preview.id));
    assert_eq!(e.session.document.annotations.len(), 1);
    let mut rounded = preview.clone();
    rounded.corner_radius = 18.;
    rounded.color = [120, 150, 190, 255];
    e.execute(crate::Command::Update {
        annotation: rounded,
    });
    e.select_only(None);
    let next = e.new_annotation(Kind::Simplify, [10., 20.], [160., 70.]);
    assert_eq!(next.corner_radius, 18.);
    assert_eq!(next.color, [120, 150, 190, 255]);
    e.execute(crate::Command::Undo);
    e.execute(crate::Command::Undo);
    assert!(e.session.document.annotations.is_empty());
    assert!(!e.session.can_undo());
}

#[test]
fn simplify_sidebar_is_passive_and_fits_shared_width() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.choose_tool(
        crate::shell::TOOLS
            .iter()
            .position(|t| t.name == "Simplify")
            .unwrap(),
    );
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let mut render = || {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(262., 740.),
                )),
                ..Default::default()
            },
            |ui| {
                e.properties(ui);
                assert!(ui.min_rect().width() <= 262.1);
            },
        ))
    };
    let first = render();
    let second = render();
    if let Ok(root) = std::env::var("CLIPCRAFT_SIMPLIFY_UI_DIR") {
        dump_meshes(&ctx, &root, "simplify", first, second);
    }
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
}
