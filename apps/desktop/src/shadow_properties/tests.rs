use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
type TextureUpdates = Vec<(egui::TextureId, Vec<egui::epaint::ImageDelta>)>;

fn frame(ctx: &egui::Context, e: &mut crate::Editor, events: Vec<Event>) -> PaintedOutput {
    let output = PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(262., 850.))),
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
                "shadow properties widened the sidebar: {:?}",
                ui.min_rect()
            );
        },
    ));
    ctx.data_mut(|d| {
        let key = egui::Id::unique("shadow-test-textures");
        let mut textures = d.get_temp::<TextureUpdates>(key).unwrap_or_default();
        textures.extend(
            output
                .textures_delta
                .set
                .iter()
                .map(|(id, updates)| (*id, updates.to_vec())),
        );
        d.insert_temp(key, textures);
    });
    output
}
fn button(p: Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(p),
        Event::PointerButton {
            pos: p,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
    ]
}
fn click(ctx: &egui::Context, e: &mut crate::Editor, p: Pos2) {
    frame(ctx, e, button(p, true));
    frame(ctx, e, button(p, false));
    frame(ctx, e, vec![]);
}
#[test]
fn direction_settings_live_slider_batch_inheritance_and_one_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session.document.annotations.clear();
    for id in [1, 2] {
        e.session.document.annotations.push(
            capture_core::steps::marker(
                id,
                [100. * id as f32, 100.],
                64,
                [40, 100, 180, 255],
                (id as u32, false),
                [640, 400],
            )
            .unwrap(),
        );
    }
    e.tool = Some(crate::Kind::Step);
    e.select_only(Some(1));
    e.click_selection(Some(2), true);
    let original = e.session.document.annotations.clone();
    let mut first = frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let direction = ctx
        .data(|d| {
            d.get_temp::<Rect>(egui::Id::unique(("shadow-direction", "Bottom right")))
                .unwrap()
        })
        .center();
    click(&ctx, &mut e, direction);
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.shadow.is_some_and(|s| s.angle == 45.))
    );
    assert_eq!(
        e.session.document.version,
        capture_core::MAX_PROJECT_VERSION
    );
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
    click(&ctx, &mut e, direction);
    let selected = e.session.document.annotations.clone();
    let rail = ctx.data(|d| {
        d.get_temp::<(Rect, Rect)>(egui::Id::unique(("property-slider", "Shadow distance")))
            .unwrap()
            .1
    });
    let from = rail.center();
    let to = from + egui::vec2(25., 0.);
    frame(&ctx, &mut e, button(from, true));
    frame(&ctx, &mut e, vec![Event::PointerMoved(to)]);
    assert_eq!(e.session.document.annotations, selected);
    assert!(
        e.preview_document()
            .annotations
            .iter()
            .all(|a| a.shadow.unwrap().distance > 5.)
    );
    let output = frame(&ctx, &mut e, button(to, false));
    if let Ok(root) = std::env::var("CLIPDESIGNER_SHADOW_UI_DIR") {
        let updates: TextureUpdates = ctx.data(|d| {
            d.get_temp(egui::Id::unique("shadow-test-textures"))
                .unwrap()
        });
        first.0.textures_delta.set.clear();
        for (id, updates) in updates {
            first
                .0
                .textures_delta
                .set
                .entry(id)
                .or_default()
                .extend(updates);
        }
        dump_meshes(&ctx, &root, "shadow-settings", first, output);
    }
    click(&ctx, &mut e, egui::pos2(8., 830.));
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.shadow.unwrap().distance > 5.)
    );
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, selected);
    let styled = e.step_marker([60., 60.]).unwrap();
    assert!(styled.shadow.is_some());
}

#[test]
fn shadow_color_opens_nested_picker_and_commits_one_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.all_styles_mut(|s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session.document.annotations.push(
        capture_core::steps::marker(
            1,
            [100., 100.],
            64,
            [40, 100, 180, 255],
            (1, false),
            [640, 400],
        )
        .unwrap(),
    );
    e.tool = Some(crate::Kind::Step);
    e.select_only(Some(1));
    e.session.document.annotations[0].shadow = Some(Default::default());
    let original = e.session.document.annotations.clone();
    let mut first = frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let rect = |key| ctx.data(|d| d.get_temp::<Rect>(egui::Id::unique(key)).unwrap());

    click(&ctx, &mut e, rect("shadow-color").center());
    let output = frame(&ctx, &mut e, vec![]);
    assert!(
        crate::color_widgets::tests::text_rect(&output.shapes, "Saturation / brightness").is_some(),
        "nested color picker vanished"
    );
    let red = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique(("preset-swatch", "Red")))
            .unwrap()
    });
    click(&ctx, &mut e, red.center());
    let output = frame(&ctx, &mut e, vec![]);
    assert_eq!(
        e.preview_document().annotations[0].shadow.unwrap().color,
        [237, 32, 36, 255]
    );
    assert!(
        crate::color_widgets::tests::text_rect(&output.shapes, "Saturation / brightness").is_some()
    );
    if let Ok(root) = std::env::var("CLIPDESIGNER_SHADOW_UI_DIR") {
        let updates: TextureUpdates = ctx.data(|d| {
            d.get_temp(egui::Id::unique("shadow-test-textures"))
                .unwrap()
        });
        first.0.textures_delta.set.clear();
        for (id, updates) in updates {
            first
                .0
                .textures_delta
                .set
                .entry(id)
                .or_default()
                .extend(updates);
        }
        dump_meshes(&ctx, &root, "shadow-color", first, output);
    }
    click(&ctx, &mut e, egui::pos2(8., 830.));
    click(&ctx, &mut e, egui::pos2(8., 830.));
    assert_eq!(
        e.session.document.annotations[0].shadow.unwrap().color,
        [237, 32, 36, 255]
    );
    e.execute(crate::Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
}
