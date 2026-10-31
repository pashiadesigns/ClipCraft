use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use egui::{Event, Modifiers, PointerButton, Pos2};

fn editor(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    e.session = capture_core::Session::new(
        serde_json::from_str(include_str!(
            "../../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap(),
    )
    .unwrap();
    e
}
fn frame(ctx: &egui::Context, e: &mut Editor, events: Vec<Event>) -> PaintedOutput {
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            events,
            focused: true,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(262., 540.))),
            ..Default::default()
        },
        |ui| {
            e.show_objects(ui);
            delete_button(
                ui,
                !e.selected_objects().is_empty(),
                "Delete object",
                ui.available_width(),
            );
        },
    ))
}
fn click(ctx: &egui::Context, e: &mut Editor, pos: Pos2) {
    for pressed in [true, false] {
        frame(
            ctx,
            e,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    frame(ctx, e, vec![]);
}
fn row(ctx: &egui::Context, id: u64) -> (Rect, Rect, Rect, String) {
    ctx.data(|d| d.get_temp(egui::Id::unique(("object-row", id))).unwrap())
}

#[test]
fn group_header_expands_selects_hides_locks_and_undoes_as_a_unit() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = editor(&ctx);
    let ids: Vec<_> = e
        .session
        .document
        .annotations
        .iter()
        .rev()
        .take(2)
        .map(|a| a.id)
        .collect();
    e.execute(Command::GroupObjects { ids: ids.clone() });
    let group = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.id == ids[0])
        .unwrap()
        .group
        .unwrap();
    let header = |ctx: &egui::Context| {
        ctx.data(|d| {
            d.get_temp::<(Rect, Rect, Rect, Rect)>(egui::Id::unique(("object-group", group)))
                .unwrap()
        })
    };
    let first = frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, header(&ctx).0.center());
    assert_eq!(e.selected_objects().len(), 2);
    click(&ctx, &mut e, header(&ctx).1.center());
    assert_eq!(
        ctx.data(|d| d.get_temp::<bool>(egui::Id::unique(("object-group-expanded", group)))),
        Some(false)
    );
    click(&ctx, &mut e, header(&ctx).1.center());
    click(&ctx, &mut e, header(&ctx).2.center());
    assert!(ids.iter().all(|id| !e.session.document.object_visible(*id)));
    e.execute(Command::Undo);
    assert!(ids.iter().all(|id| e.session.document.object_visible(*id)));
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, header(&ctx).3.center());
    assert!(
        ids.iter()
            .all(|id| e.session.document.object_state(*id).locked)
    );
    e.execute(Command::Undo);
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, row(&ctx, ids[0]).2.center());
    assert!(e.session.document.object_state(ids[0]).locked);
    assert!(!e.session.document.object_state(ids[1]).locked);
    e.execute(Command::Undo);
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, row(&ctx, ids[0]).1.center());
    assert!(!e.session.document.object_visible(ids[0]));
    assert!(e.session.document.object_visible(ids[1]));
    e.execute(Command::Undo);
    if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_INTERACTIONS_DIR") {
        let last = frame(&ctx, &mut e, vec![]);
        dump_meshes(&ctx, &root, "group-objects", first, last);
    }
    e.execute(Command::UngroupObjects { ids });
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.group.is_none())
    );
    e.execute(Command::Undo);
    assert_eq!(
        e.session
            .document
            .annotations
            .iter()
            .filter(|a| a.group == Some(group))
            .count(),
        2
    );
}
#[test]
fn layer_rows_drag_toggle_visibility_lock_and_retain_one_step_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = editor(&ctx);
    let original = e.session.document.annotations.clone();
    let step = original
        .iter()
        .find(|a| a.kind == Kind::Step)
        .unwrap()
        .clone();
    e.select_only(Some(step.id));
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    assert_eq!(row(&ctx, step.id).3, "Step 1");
    click(&ctx, &mut e, row(&ctx, step.id).1.center());
    assert!(e.session.document.object_state(step.id).hidden);
    assert!(e.selected_objects().is_empty());
    assert!(e.selected_annotation().is_none());
    assert_eq!(e.selected, Some(step.id));
    assert_ne!(e.tool_hit(step.start, 3.), Some(step.id));
    e.execute(Command::Undo);
    assert!(e.session.document.object_visible(step.id));
    assert!(!e.session.can_undo());
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, row(&ctx, step.id).2.center());
    assert!(e.session.document.object_state(step.id).locked);
    assert!(e.selected_objects().is_empty());
    assert!(e.selected_annotation().is_none());
    assert_ne!(e.tool_hit(step.start, 3.), Some(step.id));
    e.delete_selection();
    assert_eq!(e.session.document.annotations, original);
    e.execute(Command::Undo);
    assert!(!e.session.document.object_state(step.id).locked);
    assert!(!e.session.can_undo());
    for (name, hidden) in [("Hide All", true), ("Show All", false)] {
        frame(&ctx, &mut e, vec![]);
        let r = ctx.data(|d| {
            d.get_temp::<Rect>(egui::Id::unique(("object-all", name)))
                .unwrap()
        });
        click(&ctx, &mut e, r.center());
        assert!(
            original
                .iter()
                .all(|a| e.session.document.object_state(a.id).hidden == hidden)
        );
        assert_eq!(e.session.document.annotations, original);
    }
    e.execute(Command::Undo);
    e.execute(Command::Undo);
    assert!(!e.session.can_undo());
    frame(&ctx, &mut e, vec![]);
    let top = original.last().unwrap().id;
    let bottom = original.first().unwrap().id;
    let from = row(&ctx, top).0.left_center() + egui::vec2(8., 0.);
    let to = row(&ctx, bottom).0.left_bottom() + egui::vec2(30., -3.);
    frame(
        &ctx,
        &mut e,
        vec![
            Event::PointerMoved(from),
            Event::PointerButton {
                pos: from,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
    );
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerMoved(from + egui::vec2(0., 10.))],
    );
    frame(&ctx, &mut e, vec![Event::PointerMoved(to)]);
    assert_eq!(
        e.session.document.annotations, original,
        "no rearrangement before drop"
    );
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(e.session.document.annotations[0].id, top);
    assert_eq!(
        e.session.document.annotations.last().unwrap().id,
        original[original.len() - 2].id
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
}

#[test]
fn descriptive_labels_and_centered_delete_hover_render_without_document_changes() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = editor(&ctx);
    let original = serde_json::to_vec(&e.session.document).unwrap();
    let text = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Text)
        .unwrap();
    assert!(label(text).starts_with("Text: \"ClipCraft"));
    assert!(label(text).ends_with("…\""));
    let mut shape = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Rectangle)
        .unwrap()
        .clone();
    shape.fill = Some([0, 200, 0, 255]);
    shape.color = [255, 0, 0, 255];
    shape.corner_radius = 0.;
    assert_eq!(label(&shape), "Rectangle (Green, Red)");
    shape.fill = None;
    assert_eq!(label(&shape), "Rectangle (Transparent, Red)");
    assert_eq!(color_name([213, 230, 254, 255]), "Blue");
    assert_eq!(color_name([250, 229, 166, 255]), "Yellow");
    e.select_only(Some(text.id));
    let first = frame(&ctx, &mut e, vec![]);
    let pos = ctx.data(|d| {
        d.get_temp::<(Rect, Color32)>(egui::Id::unique("delete-object-button"))
            .unwrap()
            .0
            .center()
    });
    let last = frame(&ctx, &mut e, vec![Event::PointerMoved(pos)]);
    assert_eq!(
        ctx.data(|d| d
            .get_temp::<(Rect, Color32)>(egui::Id::unique("delete-object-button"))
            .unwrap()
            .1),
        Color32::from_rgb(178, 48, 57)
    );
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_OBJECTS_DIR") {
        dump_meshes(&ctx, &root, "ObjectsPane", first, last);
    }
    let select_type = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique("object-select-type"))
            .unwrap()
    });
    click(&ctx, &mut e, select_type.center());
    assert_eq!(e.selected_objects().len(), 2);
    assert!(e.selected_objects().iter().all(|a| a.kind == Kind::Text));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
}

#[test]
fn expanded_group_rows_select_children_and_header_restores_whole_group() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    let ids: Vec<_> = e
        .session
        .document
        .annotations
        .iter()
        .take(2)
        .map(|a| a.id)
        .collect();
    e.execute(Command::GroupObjects { ids: ids.clone() });
    let group = e.session.document.annotations[0].group.unwrap();
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, row(&ctx, ids[0]).0.center());
    assert_eq!(e.selected_objects().len(), 1);
    assert_eq!(e.selected, Some(ids[0]));
    assert_eq!(e.editing_group, Some(group));
    let header = ctx.data(|d| {
        d.get_temp::<(Rect, Rect, Rect, Rect)>(egui::Id::unique(("object-group", group)))
            .unwrap()
            .0
    });
    click(&ctx, &mut e, header.center());
    assert_eq!(e.selected_objects().len(), 2);
    assert!(e.editing_group.is_none());
}
