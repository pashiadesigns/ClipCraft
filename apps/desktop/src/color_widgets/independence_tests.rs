use super::*;
use crate::property_layout::tests::PaintedOutput;
use crate::{Annotation, Command, Editor, Kind};
use egui::{Event, Modifiers, PointerButton};

fn frame(ctx: &egui::Context, editor: &mut Editor, events: Vec<Event>) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 1000.),
            )),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| {
            editor.prepare_property_input(ctx);
            ui.set_width(262.);
            editor.properties(ui);
            editor.settle_property_preview(ctx);
        },
    ))
}

fn click(ctx: &egui::Context, editor: &mut Editor, point: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            ctx,
            editor,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

fn choose(ctx: &egui::Context, editor: &mut Editor, role: Preview, preset: &str) {
    let (_, rect, _, _, _) = ctx.data(|d| {
        d.get_temp::<(egui::Id, egui::Rect, egui::Rect, [u8; 4], bool)>(egui::Id::unique((
            "color-preview",
            role.label(),
        )))
        .unwrap()
    });
    click(ctx, editor, rect.center());
    frame(ctx, editor, vec![]);
    let rect = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("preset-swatch", preset)))
            .unwrap()
    });
    click(ctx, editor, rect.center());
    assert!(egui::Popup::is_any_open(ctx));
}

fn text_color(a: &Annotation) -> [u8; 4] {
    a.text.as_ref().unwrap().color.unwrap_or_else(|| {
        if a.kind == Kind::Step {
            capture_core::steps::foreground(a).color
        } else {
            a.color
        }
    })
}

fn outline(a: &Annotation) -> [u8; 4] {
    if let Some(callout) = &a.callout {
        callout.outline
    } else if a.kind == Kind::Step {
        a.step_outline.as_ref().unwrap().color
    } else {
        a.color
    }
}

#[test]
fn shape_fill_uses_color_and_transparency_without_a_separate_enable_control() {
    for kind in [Kind::Rectangle, Kind::Ellipse, Kind::Polygon] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.all_styles_mut(|s| s.animation_time = 0.);
        let mut editor = crate::canvas::tests::editor(&ctx);
        editor.tool = Some(kind);
        let mut a = editor.new_annotation(kind, [60., 60.], [260., 160.]);
        a.id = 1;
        a.fill = None;
        editor.session.document.annotations = vec![a];
        editor.select_only(Some(1));
        let original = editor.session.document.annotations.clone();
        frame(&ctx, &mut editor, vec![]);
        let out = frame(&ctx, &mut editor, vec![]);
        assert!(super::tests::text_rect(&out.shapes, "Fill shape").is_none());
        assert_eq!(editor.session.document.annotations, original);
        choose(&ctx, &mut editor, Preview::Fill, "White");
        assert_eq!(
            editor.preview_document().annotations[0].fill,
            Some([255; 4])
        );
        assert_eq!(editor.session.document.annotations, original);
        click(&ctx, &mut editor, egui::pos2(870., 950.));
        let filled = editor.session.document.annotations.clone();
        frame(&ctx, &mut editor, vec![]);
        choose(&ctx, &mut editor, Preview::Fill, "Transparent");
        assert_eq!(editor.preview_document().annotations[0].fill.unwrap()[3], 0);
        click(&ctx, &mut editor, egui::pos2(870., 950.));
        let transparent = editor.session.document.annotations.clone();
        frame(&ctx, &mut editor, vec![]);
        assert_eq!(editor.session.document.annotations, transparent);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, filled);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, original);
        assert!(!editor.session.can_undo());
    }
}

#[test]
fn production_outline_text_and_stroke_colors_are_independent_live_in_batches_and_after_reopen() {
    for kind in [
        Kind::Rectangle,
        Kind::Ellipse,
        Kind::Polygon,
        Kind::Callout,
        Kind::Step,
    ] {
        for batch in [false, true] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            ctx.all_styles_mut(|s| s.animation_time = 0.);
            let mut editor = crate::canvas::tests::editor(&ctx);
            editor.tool = Some(kind);
            let mut primary = if kind == Kind::Step {
                capture_core::steps::marker(
                    1,
                    [100., 100.],
                    60,
                    [29, 55, 83, 210],
                    (1, false),
                    [640, 400],
                )
                .unwrap()
            } else {
                editor.new_annotation(kind, [60., 60.], [260., 160.])
            };
            primary.id = 1;
            primary.color = [29, 55, 83, 210];
            primary.text = Some(capture_core::text::TextStyle {
                content: "Label".into(),
                ..Default::default()
            });
            if kind == Kind::Step {
                primary.step_outline = Some(Default::default());
            }
            let mut inherited = primary.clone();
            inherited.id = 2;
            inherited.color = [96, 23, 88, 180];
            let mut explicit = primary.clone();
            explicit.id = 3;
            explicit.text.as_mut().unwrap().color = Some([26, 120, 92, 200]);
            editor.session.document.annotations = vec![primary, inherited, explicit];
            editor.select_only(Some(if batch { 3 } else { 1 }));
            if batch {
                editor.click_selection(Some(2), true);
                editor.click_selection(Some(1), true);
            }
            let original = editor.session.document.clone();
            frame(&ctx, &mut editor, vec![]);
            frame(&ctx, &mut editor, vec![]);
            assert_eq!(
                editor.session.document.annotations, original.annotations,
                "passive display must not mutate fallback colors"
            );
            choose(&ctx, &mut editor, Preview::Outline, "White");
            let preview = editor.preview_document();
            for (a, b) in original.annotations.iter().zip(&preview.annotations) {
                assert_eq!(
                    text_color(a),
                    text_color(b),
                    "outline changed text: {kind:?}, batch={batch}, id={}",
                    a.id
                );
                assert_eq!(a.fill, b.fill);
                if a.id == 1 || batch {
                    assert_eq!(outline(b), [255; 4]);
                } else {
                    assert_eq!(a, b);
                }
            }
            assert_eq!(editor.session.document.annotations, original.annotations);
            assert!(!editor.session.can_undo());
            click(&ctx, &mut editor, egui::pos2(870., 950.));
            assert_eq!(editor.session.document.annotations, preview.annotations);
            editor.execute(Command::Undo);
            assert_eq!(editor.session.document.annotations, original.annotations);
            assert!(!editor.session.can_undo());
            editor.execute(Command::Redo);
            frame(&ctx, &mut editor, vec![]);
            choose(&ctx, &mut editor, Preview::Text, "Black");
            let text_preview = editor.preview_document();
            for (a, b) in preview.annotations.iter().zip(&text_preview.annotations) {
                assert_eq!(outline(a), outline(b), "text changed outline");
                assert_eq!(a.fill, b.fill);
                if a.id == 1 || batch {
                    assert_eq!(text_color(b), [0, 0, 0, 255]);
                } else {
                    assert_eq!(a, b);
                }
            }
            click(&ctx, &mut editor, egui::pos2(870., 950.));
            frame(&ctx, &mut editor, vec![]);
            choose(&ctx, &mut editor, Preview::Stroke, "White");
            let stroke_preview = editor.preview_document();
            for (a, b) in text_preview
                .annotations
                .iter()
                .zip(&stroke_preview.annotations)
            {
                assert_eq!(outline(a), outline(b));
                assert_eq!(text_color(a), text_color(b));
                if a.id == 1 || batch {
                    assert_eq!(b.text.as_ref().unwrap().outline.unwrap().color, [255; 4]);
                } else {
                    assert_eq!(a, b);
                }
            }
            click(&ctx, &mut editor, egui::pos2(870., 950.));
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("independent-colors.capture.json");
            editor.session.save(&path).unwrap();
            let reopened = capture_core::Document::load(&path).unwrap();
            assert_eq!(reopened.annotations, stroke_preview.annotations);
            assert_eq!(reopened.render().unwrap(), stroke_preview.render().unwrap());
            editor.execute(Command::Undo);
            assert_eq!(
                editor.session.document.annotations,
                text_preview.annotations
            );
            editor.execute(Command::Undo);
            assert_eq!(editor.session.document.annotations, preview.annotations);
        }
    }
}
