use super::*;
use crate::{Command, Kind, property_layout::tests::PaintedOutput};
use egui::{Event, Modifiers, PointerButton};
fn button(pos: egui::Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn editor(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    e.quick_controls.set_snapping(false);
    let mut d = crate::Document::blank(640, 400).unwrap();
    for (id, min, max) in [
        (1, [80., 90.], [180., 160.]),
        (2, [220., 170.], [320., 240.]),
        (3, [450., 290.], [510., 350.]),
    ] {
        let mut a = e.new_annotation(Kind::Rectangle, min, max);
        a.id = id;
        a.fill = Some([50, 160, 210, 160]);
        d.annotations.push(a);
    }
    d.promote_features();
    e.session = crate::Session::new(d).unwrap();
    e.tool = None;
    e
}
fn frame(ctx: &egui::Context, e: &mut Editor, events: Vec<Event>) -> Transform {
    let mut viewport = egui::Rect::NOTHING;
    let _ = PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            viewport = ui.available_rect_before_wrap();
            e.show_canvas(ui);
        },
    ));
    e.view.transform(
        viewport,
        egui::vec2(
            e.session.document.width as f32,
            e.session.document.height as f32,
        ),
        ctx.pixels_per_point(),
    )
}
#[test]
fn group_ribbon_actions_and_member_selection_persist_without_changing_ink() {
    use crate::ribbon_settings::Action;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = editor(&ctx);
    e.click_selection(Some(1), false);
    e.click_selection(Some(2), true);
    let pixels = e.session.document.render().unwrap();
    let draw = |e: &mut Editor, events| {
        let mut buttons = vec![];
        let _ = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(262., 160.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| buttons = e.settings_grid(ui, &[Action::Group, Action::Ungroup], 2),
        ));
        buttons
    };
    draw(&mut e, vec![]);
    let responses = draw(&mut e, vec![]);
    assert!(responses[0].1.enabled());
    assert!(!responses[1].1.enabled());
    let pos = responses[0].1.rect.center();
    for pressed in [true, false] {
        draw(&mut e, vec![Event::PointerMoved(pos), button(pos, pressed)]);
    }
    assert_eq!(e.session.document.group_members(1), vec![1, 2]);
    assert_eq!(e.session.document.render().unwrap(), pixels);
    e.select_only(None);
    e.click_selection(Some(1), false);
    assert_eq!(e.selected_objects().len(), 2);
    e.click_selection(Some(2), true);
    assert!(e.selected_objects().is_empty());
    e.select_only(Some(1));
    e.tool = Some(Kind::Rectangle);
    e.select_all_matching();
    assert_eq!(e.selected_objects().len(), 3);
    e.select_only(Some(1));
    let responses = draw(&mut e, vec![]);
    assert!(!responses[0].1.enabled());
    assert!(responses[1].1.enabled());
    let pos = responses[1].1.rect.center();
    for pressed in [true, false] {
        draw(&mut e, vec![Event::PointerMoved(pos), button(pos, pressed)]);
    }
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.group.is_none())
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.group_members(1), vec![1, 2]);
}

#[test]
fn group_corner_and_rotation_nodes_preview_commit_and_undo_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = editor(&ctx);
    e.execute(Command::GroupObjects { ids: vec![1, 2] });
    e.select_only(Some(1));
    for rotate in [false, true] {
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let objects = e.selected_objects();
        let (min, max) = groups::bounds(&objects).unwrap();
        let center = [0, 1].map(|i| (min[i] + max[i]) * 0.5);
        let t = frame(&ctx, &mut e, vec![]);
        frame(&ctx, &mut e, vec![]);
        let nodes = nodes(&objects, t).unwrap();
        let from = nodes[if rotate { 4 } else { 2 }];
        let to = if rotate {
            let c = t.screen_point(center);
            c + egui::vec2(c.y - from.y, 0.)
        } else {
            let anchor = t.screen_point(min);
            anchor + (from - anchor) * 1.25
        };
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(from), button(from, true)],
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(to)]);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        let preview = e.group_drag.as_ref().unwrap();
        assert!(preview.transform.is_some());
        assert_ne!(preview.preview, objects);
        if rotate {
            assert!(
                preview
                    .preview
                    .iter()
                    .all(|a| (a.rotation_degrees - 90.).abs() < 0.01)
            );
        }
        frame(&ctx, &mut e, vec![button(to, false)]);
        assert!(e.group_drag.is_none());
        assert_ne!(serde_json::to_vec(&e.session.document).unwrap(), before);
        if let Ok(root) = std::env::var("CLIPDESIGNER_GROUP_UI_DIR") {
            let v = egui::Context::default();
            crate::shell::configure(&v);
            let mut demo = editor(&v);
            demo.session = crate::Session::new(e.session.document.clone()).unwrap();
            demo.select_only(Some(1));
            demo.refresh_canvas_display(&v);
            let draw = |demo: &mut Editor| {
                PaintedOutput(v.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 600.),
                        )),
                        focused: true,
                        ..Default::default()
                    },
                    |ui| demo.show_canvas(ui),
                ))
            };
            let first = draw(&mut demo);
            let last = draw(&mut demo);
            crate::property_layout::tests::dump_meshes(
                &v,
                &root,
                if rotate { "rotate" } else { "resize" },
                first,
                last,
            );
        }
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert_eq!(e.selected_objects().len(), 2);
    }
}
