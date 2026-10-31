use super::*;
use crate::{Command, Kind, property_layout::tests::PaintedOutput};
use egui::{Event, Modifiers, PointerButton};
fn click(pos: egui::Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        },
    ]
}
#[test]
fn canvas_picker_uses_exact_rgba_at_zoom_and_applies_batch_once_without_drawing() {
    for (zoom, opaque) in [(0.75, false), (1.75, false), (1.0, true)] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let pixels = image::RgbaImage::from_fn(640, 400, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 180, 128])
        });
        let mut d = crate::Document::from_image(pixels).unwrap();
        for (id, x) in [(1, 410.), (2, 510.)] {
            let mut a = e.new_annotation(Kind::Rectangle, [x, 250.], [x + 80., 330.]);
            a.id = id;
            d.annotations.push(a);
        }
        d.promote_features();
        e.session = crate::Session::new(d).unwrap();
        e.select_only(Some(1));
        e.click_selection(Some(2), true);
        e.view.apply(
            crate::canvas_view::ZoomAction::Set(crate::canvas_view::ZoomMode::Manual(zoom)),
            None,
        );
        let original = serde_json::to_vec(&e.session.document).unwrap();
        let frame = |e: &mut Editor, events| {
            let mut swatch = egui::Id::NULL;
            let mut t = None;
            let _ = PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.prepare_property_input(&ctx);
                    let mut edited = e.selected_annotation().unwrap();
                    ui.scope_builder(
                        egui::UiBuilder::new().id_salt("picker").max_rect(
                            egui::Rect::from_min_size(egui::pos2(840., 15.), egui::vec2(100., 35.)),
                        ),
                        |ui| swatch = rgba_options(ui, &mut edited.color, false, opaque).id,
                    );
                    e.preview_property_edit(edited);
                    e.settle_property_preview(&ctx);
                    if e.needs_render {
                        e.refresh_canvas_display(&ctx);
                    }
                    ui.scope_builder(
                        egui::UiBuilder::new().id_salt("sample-canvas").max_rect(
                            egui::Rect::from_min_size(egui::pos2(0., 70.), egui::vec2(700., 700.)),
                        ),
                        |ui| {
                            e.show_canvas(ui);
                            t = e.view.current_transform();
                        },
                    );
                },
            ));
            (swatch, t.unwrap())
        };
        let (id, _) = frame(&mut e, vec![]);
        egui::Popup::open_id(&ctx, id.with("rgba-popup"));
        frame(&mut e, vec![]);
        let (_, t) = frame(&mut e, vec![]);
        let pick = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("eyedropper-button")))
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(&mut e, click(pick, pressed));
        }
        assert!(active(&ctx));
        assert!(!dismissing_live_popup(&ctx));
        let sample = [300.5, 130.5];
        let p = t.screen_point(sample);
        let mut expected = e.preview_document().render().unwrap().get_pixel(300, 130).0;
        if opaque {
            expected[3] = 255;
        }
        frame(&mut e, click(p, true));
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        for _ in 0..3 {
            frame(&mut e, vec![]);
        }
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        frame(&mut e, click(p, false));
        frame(&mut e, vec![]);
        assert!(!active(&ctx));
        assert_eq!(e.selected_annotation().unwrap().color, expected);
        let outside = egui::pos2(975., 760.);
        for pressed in [true, false] {
            frame(&mut e, click(outside, pressed));
        }
        assert_eq!(e.session.document.annotations.len(), 2);
        assert!(
            e.session
                .document
                .annotations
                .iter()
                .all(|a| a.color == expected)
        );
        assert!(e.session.can_undo());
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert!(!e.session.can_undo());
        // Starting another sample and pressing Escape leaves the current color and popup intact.
        let (id, _) = frame(&mut e, vec![]);
        egui::Popup::open_id(&ctx, id.with("rgba-popup"));
        frame(&mut e, vec![]);
        frame(&mut e, vec![]);
        let pick = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("eyedropper-button")))
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(&mut e, click(pick, pressed));
        }
        assert!(active(&ctx));
        frame(
            &mut e,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(!active(&ctx));
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert!(!e.session.can_undo());
    }
}
