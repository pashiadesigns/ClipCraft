fn legacy_editor(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    for tool in capture_core::toolbar::default_favorites() {
        e.apply_toolbar_choice(Choice::FavoriteTool {
            tool,
            favorite: false,
        });
    }
    e
}
use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2};

fn favorite_order() -> Vec<ToolId> {
    capture_core::toolbar::default_order()
        .into_iter()
        .filter(|t| !matches!(t, ToolId::Favorites | ToolId::CutOut))
        .collect()
}
fn favorite_editor(ctx: &egui::Context) -> Editor {
    let mut e = legacy_editor(ctx);
    for tool in favorite_order() {
        e.apply_toolbar_choice(Choice::FavoriteTool {
            tool,
            favorite: true,
        });
    }
    e
}

#[test]
fn overflow_context_favorites_and_combined_crop_choices_remain_interactive() {
    let ctx = egui::Context::default();
    let mut e = legacy_editor(&ctx);
    frame(&ctx, &mut e, vec![], 2, true);
    let layout = frame(&ctx, &mut e, vec![], 2, true);
    click(&ctx, &mut e, layout.more.center(), 2);
    frame(&ctx, &mut e, vec![], 2, true);
    let layout = frame(&ctx, &mut e, vec![], 2, true);
    let p = pos(&layout.menu, ToolId::Text);
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut e,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Secondary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            2,
            true,
        );
    }
    frame(&ctx, &mut e, vec![], 2, true);
    frame(&ctx, &mut e, vec![], 2, true);
    let action = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique("favorite-context-action"))
            .unwrap()
    });
    click(&ctx, &mut e, action.center(), 2);
    assert_eq!(e.quick_controls.tool_group(ToolId::Text), Group::Favorites);
    // Put the combined button near the beginning of overflow to avoid scrolling.
    for id in [ToolId::Text, ToolId::Move, ToolId::Crop] {
        e.apply_toolbar_choice(Choice::FavoriteTool {
            tool: id,
            favorite: true,
        });
    }
    egui::Popup::close_all(&ctx);
    frame(&ctx, &mut e, vec![], 2, true);
    let layout = frame(&ctx, &mut e, vec![], 2, true);
    click(&ctx, &mut e, layout.more.center(), 2);
    frame(&ctx, &mut e, vec![], 2, true);
    let layout = frame(&ctx, &mut e, vec![], 2, true);
    click(&ctx, &mut e, pos(&layout.menu, ToolId::Crop), 2);
    frame(&ctx, &mut e, vec![], 2, true);
    frame(&ctx, &mut e, vec![], 2, true);
    let choice = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("canvas-tool-choice", ToolId::CutOut)))
            .unwrap()
    });
    click(&ctx, &mut e, choice.center(), 2);
    assert_eq!(e.quick_controls.canvas_tool(), ToolId::CutOut);
    assert!(e.cut_out.is_some());
}

#[test]
fn real_context_menu_adds_and_removes_favorites_without_editing_document() {
    let ctx = egui::Context::default();
    let mut e = legacy_editor(&ctx);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    for favorite in [true, false] {
        frame(&ctx, &mut e, vec![], 8, true);
        let layout = frame(&ctx, &mut e, vec![], 8, true);
        let p = pos(&layout.tools, ToolId::Text);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut e,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Secondary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                8,
                true,
            );
        }
        frame(&ctx, &mut e, vec![], 8, true);
        let action = ctx.data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique("favorite-context-action"))
                .unwrap()
        });
        click(&ctx, &mut e, action.center(), 8);
        assert_eq!(
            e.quick_controls.tool_group(ToolId::Text) == Group::Favorites,
            favorite
        );
        assert_eq!(
            e.quick_controls
                .toolbar_order()
                .iter()
                .filter(|id| **id == ToolId::Text)
                .count(),
            1
        );
    }
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
}

#[test]
fn crop_cut_button_opens_stacked_full_tools_and_only_choice_activates() {
    let ctx = egui::Context::default();
    let mut e = legacy_editor(&ctx);
    e.apply_toolbar_choice(Choice::FavoriteTool {
        tool: ToolId::Crop,
        favorite: true,
    });
    frame(&ctx, &mut e, vec![], 4, true);
    let layout = frame(&ctx, &mut e, vec![], 4, true);
    let before = e.current_tool().id;
    click(&ctx, &mut e, pos(&layout.tools, ToolId::Crop), 4);
    assert_eq!(e.current_tool().id, before);
    frame(&ctx, &mut e, vec![], 4, true);
    let crop = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("canvas-tool-choice", ToolId::Crop)))
            .unwrap()
    });
    let choice = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("canvas-tool-choice", ToolId::CutOut)))
            .unwrap()
    });
    assert_eq!(crop.size(), egui::vec2(72.0, 58.0));
    assert_eq!(choice.size(), crop.size());
    assert!(choice.top() > crop.bottom());
    click(&ctx, &mut e, choice.center(), 4);
    assert_eq!(e.quick_controls.canvas_tool(), ToolId::CutOut);
    assert!(e.cut_out.is_some());
    e.choose_tool(TOOLS.iter().position(|t| t.id == ToolId::Text).unwrap());
    let layout = frame(&ctx, &mut e, vec![], 4, true);
    click(&ctx, &mut e, pos(&layout.tools, ToolId::Crop), 4);
    assert_eq!(e.current_tool().id, ToolId::Text);
    frame(&ctx, &mut e, vec![], 4, true);
    let crop = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("canvas-tool-choice", ToolId::Crop)))
            .unwrap()
    });
    click(&ctx, &mut e, crop.center(), 4);
    assert!(e.crop.is_some());
    assert_eq!(e.quick_controls.canvas_tool(), ToolId::Crop);
    assert!(!e.session.can_undo());
}

#[test]
fn cross_section_pointer_drag_leaves_membership_and_order_unchanged() {
    let ctx = egui::Context::default();
    let mut e = legacy_editor(&ctx);
    e.apply_toolbar_choice(Choice::FavoriteTool {
        tool: ToolId::Text,
        favorite: true,
    });
    let before = e.quick_controls.toolbar_order();
    frame(&ctx, &mut e, vec![], 4, true);
    let layout = frame(&ctx, &mut e, vec![], 4, true);
    let source = pos(&layout.tools, ToolId::Text);
    let target = pos(&layout.tools, ToolId::Move);
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerMoved(source), button(source, true)],
        4,
        true,
    );
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerMoved(source + egui::vec2(14.0, 0.0))],
        4,
        true,
    );
    frame(&ctx, &mut e, vec![Event::PointerMoved(target)], 4, true);
    frame(&ctx, &mut e, vec![button(target, false)], 4, true);
    assert_eq!(e.quick_controls.toolbar_order(), before);
    assert_eq!(e.quick_controls.tool_group(ToolId::Text), Group::Favorites);
}

struct Layout {
    tools: Vec<(ToolId, egui::Response)>,
    more: egui::Rect,
    menu: Vec<(ToolId, egui::Response)>,
}
fn frame(
    ctx: &egui::Context,
    editor: &mut Editor,
    events: Vec<Event>,
    visible: usize,
    focused: bool,
) -> Layout {
    let mut layout = Layout {
        tools: vec![],
        more: egui::Rect::NOTHING,
        menu: vec![],
    };
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(1100.0, 850.0),
            )),
            focused,
            events,
            ..Default::default()
        },
        |ui| {
            ui.horizontal(|ui| {
                layout.tools = editor.toolbar_buttons(ui, visible);
                layout.more = ui
                    .menu_button("More", |ui| {
                        layout.menu = editor.draggable_tools_menu(ui, visible);
                    })
                    .response
                    .rect;
            });
        },
    );
    output.textures_delta.clear();
    layout
}
fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn pos(rows: &[(ToolId, egui::Response)], id: ToolId) -> Pos2 {
    rows.iter().find(|(t, _)| *t == id).unwrap().1.rect.center()
}
fn click(ctx: &egui::Context, editor: &mut Editor, p: Pos2, visible: usize) {
    frame(
        ctx,
        editor,
        vec![Event::PointerMoved(p), button(p, true)],
        visible,
        true,
    );
    frame(ctx, editor, vec![button(p, false)], visible, true);
}

#[test]
fn toolbar_catalog_has_unique_stable_ids_and_reordered_click_selects_correct_tool() {
    assert_eq!(
        TOOLS.iter().map(|t| t.id).collect::<Vec<_>>(),
        capture_core::toolbar::ALL
    );
    let ctx = egui::Context::default();
    let mut editor = favorite_editor(&ctx);
    editor.apply_toolbar_choice(Choice::MoveTool {
        tool: ToolId::Step,
        before: Some(ToolId::Text),
    });
    frame(&ctx, &mut editor, vec![], 4, true);
    let layout = frame(&ctx, &mut editor, vec![], 4, true);
    click(&ctx, &mut editor, pos(&layout.tools, ToolId::Step), 4);
    assert_eq!(editor.tool, Some(crate::Kind::Step));
    assert_eq!(editor.current_tool().id, ToolId::Step);
    editor.apply_toolbar_choice(Choice::MoveTool {
        tool: ToolId::Move,
        before: Some(ToolId::Step),
    });
    let layout = frame(&ctx, &mut editor, vec![], 2, true);
    click(&ctx, &mut editor, pos(&layout.tools, ToolId::Move), 2);
    assert_eq!(editor.tool, None);
    assert_eq!(editor.current_tool().id, ToolId::Move);
}

#[test]
fn dragging_toolbar_reorders_only_on_release_without_selecting_or_editing() {
    let ctx = egui::Context::default();
    let mut editor = favorite_editor(&ctx);
    let bytes = serde_json::to_vec(&editor.session.document).unwrap();
    frame(&ctx, &mut editor, vec![], 4, true);
    let layout = frame(&ctx, &mut editor, vec![], 4, true);
    let source = pos(&layout.tools, ToolId::Shape);
    let target = layout.tools[0].1.rect.left_center() + egui::vec2(5.0, 0.0);
    frame(
        &ctx,
        &mut editor,
        vec![Event::PointerMoved(source), button(source, true)],
        4,
        true,
    );
    frame(
        &ctx,
        &mut editor,
        vec![Event::PointerMoved(source + egui::vec2(12.0, 0.0))],
        4,
        true,
    );
    frame(
        &ctx,
        &mut editor,
        vec![Event::PointerMoved(target)],
        4,
        true,
    );
    assert_eq!(editor.quick_controls.toolbar_order(), favorite_order());
    assert_eq!(editor.tool, Some(crate::Kind::Arrow));
    frame(&ctx, &mut editor, vec![button(target, false)], 4, true);
    assert_eq!(
        &editor.quick_controls.toolbar_order()[..3],
        &[ToolId::Shape, ToolId::Text, ToolId::Arrow]
    );
    assert_eq!(editor.tool, Some(crate::Kind::Arrow));
    assert_eq!(serde_json::to_vec(&editor.session.document).unwrap(), bytes);
    assert!(!editor.session.can_undo());
    assert!(egui::DragAndDrop::payload::<ToolDrag>(&ctx).is_none());
}

#[test]
fn actual_more_popup_drag_promotes_hidden_tool_even_at_last_slot_edge() {
    let ctx = egui::Context::default();
    let mut editor = favorite_editor(&ctx);
    frame(&ctx, &mut editor, vec![], 2, true);
    let layout = frame(&ctx, &mut editor, vec![], 2, true);
    click(&ctx, &mut editor, layout.more.center(), 2);
    let menu = frame(&ctx, &mut editor, vec![], 2, true);
    assert_eq!(
        menu.menu.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        editor.quick_controls.toolbar_order()[2..]
    );
    assert!(
        menu.tools
            .iter()
            .all(|(id, _)| !menu.menu.iter().any(|(hidden, _)| id == hidden))
    );
    // Line starts below the popup's visible clip. Exercise the real scroll
    // path before grabbing it instead of clicking an offscreen layout rect.
    let inside = pos(&menu.menu, ToolId::Callout);
    frame(
        &ctx,
        &mut editor,
        vec![
            Event::PointerMoved(inside),
            Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(0.0, -240.0),
                modifiers: Modifiers::NONE,
            },
        ],
        2,
        true,
    );
    frame(&ctx, &mut editor, vec![], 2, true);
    let menu = frame(&ctx, &mut editor, vec![], 2, true);
    let source = pos(&menu.menu, ToolId::Line);
    assert!(
        menu.menu
            .iter()
            .find(|(id, _)| *id == ToolId::Line)
            .unwrap()
            .1
            .interact_rect
            .contains(source)
    );
    let target = menu.tools[1].1.rect.right_center() - egui::vec2(3.0, 0.0);
    frame(
        &ctx,
        &mut editor,
        vec![Event::PointerMoved(source), button(source, true)],
        2,
        true,
    );
    frame(
        &ctx,
        &mut editor,
        vec![Event::PointerMoved(source + egui::vec2(-14.0, 0.0))],
        2,
        true,
    );
    assert!(
        egui::DragAndDrop::payload::<ToolDrag>(&ctx).is_some(),
        "More entry did not start dragging"
    );
    frame(
        &ctx,
        &mut editor,
        vec![Event::PointerMoved(target)],
        2,
        true,
    );
    frame(&ctx, &mut editor, vec![button(target, false)], 2, true);
    assert_eq!(
        &editor.quick_controls.toolbar_order()[..2],
        &[ToolId::Text, ToolId::Line]
    );
    assert_eq!(editor.tool, Some(crate::Kind::Arrow));
    let layout = frame(&ctx, &mut editor, vec![], 2, true);
    assert!(layout.tools.iter().any(|(id, _)| *id == ToolId::Line));
    click(&ctx, &mut editor, pos(&layout.tools, ToolId::Line), 2);
    assert_eq!(editor.tool, Some(crate::Kind::Line));
}

#[test]
fn focus_loss_and_drop_outside_leave_order_unchanged() {
    for lose_focus in [false, true] {
        let ctx = egui::Context::default();
        let mut editor = favorite_editor(&ctx);
        frame(&ctx, &mut editor, vec![], 4, true);
        let layout = frame(&ctx, &mut editor, vec![], 4, true);
        let source = pos(&layout.tools, ToolId::Text);
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(source), button(source, true)],
            4,
            true,
        );
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(source + egui::vec2(20.0, 0.0))],
            4,
            true,
        );
        let outside = egui::pos2(600.0, 600.0);
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(outside), button(outside, false)],
            4,
            !lose_focus,
        );
        assert_eq!(editor.quick_controls.toolbar_order(), favorite_order());
        assert!(egui::DragAndDrop::payload::<ToolDrag>(&ctx).is_none());
    }
}

#[test]
fn focused_button_keyboard_order_and_reset_keep_tool_and_document_unchanged() {
    let ctx = egui::Context::default();
    let mut editor = favorite_editor(&ctx);
    frame(&ctx, &mut editor, vec![], 4, true);
    let layout = frame(&ctx, &mut editor, vec![], 4, true);
    layout
        .tools
        .iter()
        .find(|(id, _)| *id == ToolId::Shape)
        .unwrap()
        .1
        .request_focus();
    frame(
        &ctx,
        &mut editor,
        vec![Event::Key {
            key: egui::Key::Home,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        }],
        4,
        true,
    );
    assert_eq!(editor.quick_controls.toolbar_order()[0], ToolId::Shape);
    assert_eq!(editor.tool, Some(crate::Kind::Arrow));
    assert!(!editor.session.can_undo());
    editor.apply_toolbar_choice(Choice::ResetToolbar);
    assert_eq!(editor.quick_controls.toolbar_order(), favorite_order());
}
