use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use egui::{Event, Modifiers, PointerButton, Pos2};

fn frame(ctx: &egui::Context, e: &mut Editor, width: f32, events: Vec<Event>) -> PaintedOutput {
    let time = ctx
        .data(|d| d.get_temp::<f64>(egui::Id::unique("footer-test-time")))
        .unwrap_or(0.)
        + 0.1;
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("footer-test-time"), time));
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 120.))),
            time: Some(time),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| e.show_footer(ui),
    ))
}
fn layout(ctx: &egui::Context) -> (Rect, Rect, Rect, Rect) {
    ctx.data(|d| d.get_temp(egui::Id::unique("footer-layout")).unwrap())
}
fn message(ctx: &egui::Context) -> (Rect, f32, f32) {
    ctx.data(|d| d.get_temp(egui::Id::unique("footer-message")).unwrap())
}
fn click(ctx: &egui::Context, e: &mut Editor, pos: Pos2) {
    for pressed in [true, false] {
        frame(
            ctx,
            e,
            1920.,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    pressed,
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    frame(ctx, e, 1920., vec![]);
}

#[test]
fn centered_single_row_survives_widths_views_and_scrolling_messages() {
    for width in [880., 981., 1280., 1920.] {
        for library in [false, true] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.shell.library = library;
            e.status = "Copied the complete image, including all editable annotations, to the clipboard. Original source pixels and vector annotations remain saved in the Library.".into();
            let original = serde_json::to_vec(&e.session.document).unwrap();
            let first = frame(&ctx, &mut e, width, vec![]);
            let initial = frame(&ctx, &mut e, width, vec![]);
            if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_FOOTER_DIR") {
                dump_meshes(
                    &ctx,
                    &root,
                    &format!(
                        "Footer{width}-{}",
                        if library { "Library" } else { "Editor" }
                    ),
                    first,
                    initial,
                );
            }
            for _ in 0..4 {
                frame(&ctx, &mut e, width, vec![]);
            }
            let (area, status, controls, content) = layout(&ctx);
            assert!(
                status.expand(0.1).contains_rect(message(&ctx).0),
                "status={status:?} message={:?}",
                message(&ctx).0
            );
            assert!((controls.center().x - area.center().x).abs() < 0.1);
            assert!(status.right() < controls.left());
            assert!(
                controls.height() <= 48.1 && area.height() <= 48.1,
                "{area:?}"
            );
            assert!(content.height() <= 38.1, "{content:?}");
            if width == 1920. {
                assert!(
                    (controls.width() - content.width()).abs() < 0.6,
                    "{controls:?} {content:?}"
                );
            }
            for _ in 0..35 {
                frame(&ctx, &mut e, width, vec![]);
            }
            assert_eq!(
                message(&ctx).1 > 0.,
                message(&ctx).2 > 0.,
                "scroll only overflowing text"
            );
            assert_eq!(layout(&ctx).2, controls, "long text cannot move controls");
            let pos = message(&ctx).0.center();
            frame(&ctx, &mut e, width, vec![Event::PointerMoved(pos)]);
            let paused = message(&ctx).1;
            for _ in 0..12 {
                frame(&ctx, &mut e, width, vec![]);
            }
            assert_eq!(message(&ctx).1, paused);
            e.status = "Saved".into();
            frame(&ctx, &mut e, width, vec![Event::PointerGone]);
            assert_eq!(message(&ctx).1, 0.);
            assert_eq!(message(&ctx).2, 0.);
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        }
    }
}

#[test]
fn centered_footer_keeps_actions_and_status_details_interactive() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let original = serde_json::to_vec(&e.session.document).unwrap();
    for _ in 0..5 {
        frame(&ctx, &mut e, 1920., vec![]);
    }
    let recent = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique("footer-recent"))
            .unwrap()
    });
    let before = e.history.recent;
    click(&ctx, &mut e, recent.center());
    assert_eq!(e.history.recent, !before);
    let annotations = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique("footer-annotations"))
            .unwrap()
    });
    click(&ctx, &mut e, annotations.center());
    assert!(e.shell.annotations_hidden);
    click(&ctx, &mut e, message(&ctx).0.center());
    assert!(egui::Popup::is_any_open(&ctx));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    assert!(!e.session.can_undo());
}
