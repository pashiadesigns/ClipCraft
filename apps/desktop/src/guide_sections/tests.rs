use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use capture_core::{
    Session,
    step_capture::{Button, Gesture, Guide},
};

fn fixture() -> Document {
    let mut g = Guide::new([400, 180]).unwrap();
    for c in [[210, 235, 255, 255], [255, 235, 190, 255]] {
        g.push(
            image::RgbaImage::from_pixel(400, 180, image::Rgba(c)),
            Gesture {
                start: [70, 40],
                end: [70, 40],
                button: Button::Left,
            },
        )
        .unwrap();
    }
    g.document().unwrap()
}

#[test]
fn chosen_image_file_inserts_visible_editable_section_and_undo_restores_guide() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    frame(&ctx, &mut e, vec![]);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let transform = e.view.current_transform().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.png");
    let image = image::RgbaImage::from_pixel(80, 40, image::Rgba([40, 180, 200, 255]));
    image.save(&path).unwrap();
    e.insert_image_section_from_path(Some(1), transform, &path);
    let section = &e.session.document.guide.as_ref().unwrap().sections[1];
    assert_eq!(section.kind, SectionKind::Image);
    assert_eq!(section.objects.len(), 1);
    let a = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.id == section.objects[0])
        .unwrap();
    assert_eq!(a.kind, crate::Kind::Image);
    assert_eq!(**a.image.as_ref().unwrap().pixels().unwrap(), image);
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    e.insert_image_section_from_path(Some(1), transform, &dir.path().join("missing.png"));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
}
fn frame(ctx: &egui::Context, e: &mut Editor, events: Vec<egui::Event>) -> PaintedOutput {
    if e.needs_render {
        e.refresh_canvas_display(ctx);
    }
    PaintedOutput(ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640., 700.),
            )),
            focused: true,
            events,
            ..Default::default()
        },
        |ui| e.show_canvas(ui),
    ))
}
fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn rail_drag_moves_section_between_pages_with_live_landing_and_one_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let d = capture_core::templates::build(
        "two-steps",
        "Drag guide",
        &[],
        capture_core::templates::Scale::Fill,
    )
    .unwrap()
    .edit_guide(&Action::AddPage {
        page: 0,
        copy: false,
    })
    .unwrap();
    let id = d.guide.as_ref().unwrap().sections[1].id;
    e.session = Session::new(d).unwrap();
    e.tool = None;
    e.needs_render = true;
    e.view.mode = crate::canvas_view::ZoomMode::Manual(0.65);
    let draw = |e: &mut Editor, events| {
        if e.needs_render {
            e.refresh_canvas_display(&ctx);
        }
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| e.show_canvas(ui),
        ))
    };
    let mut atlas = draw(&mut e, vec![]);
    let warm = draw(&mut e, vec![]);
    atlas.0.textures_delta.append(warm.textures_delta.clone());
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let transform = e.view.current_transform().unwrap();
    let from = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("section-grip-test", id, 0usize))))
        .unwrap()
        .center();
    let to = pages::page_rect(&e.session.document, 1, transform).center_top() + egui::vec2(0., 20.);
    let press = draw(
        &mut e,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    atlas.0.textures_delta.append(press.textures_delta.clone());
    let started = draw(&mut e, vec![egui::Event::PointerMoved(to)]);
    atlas
        .0
        .textures_delta
        .append(started.textures_delta.clone());
    assert!(reorder::active(&ctx));
    assert!(e.edit_drag.is_none());
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    let output = draw(&mut e, vec![]);
    assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Moving section").is_some());
    assert!(
        crate::color_widgets::tests::text_rect(&output.shapes, "Page 2 · position 1").is_some(),
        "target {:?}, pointer {to:?}, transform {:?}",
        reorder::destination(
            &e.session.document,
            e.view.current_transform().unwrap(),
            id,
            to
        ),
        e.view.current_transform().unwrap().image
    );
    if let Ok(root) = std::env::var("CLIPDESIGNER_PAGE_POLISH_UI") {
        dump_meshes(&ctx, &root, "section-drag", atlas, output);
    }
    draw(&mut e, vec![button(to, false)]);
    assert!(!reorder::active(&ctx));
    assert_eq!(
        e.session
            .document
            .guide
            .as_ref()
            .unwrap()
            .sections
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .page,
        1
    );
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
}

#[test]
fn placeholder_position_follows_section_resize_preview_before_release() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(
        capture_core::templates::build(
            "two-steps",
            "Resize guide",
            &[],
            capture_core::templates::Scale::Fill,
        )
        .unwrap(),
    )
    .unwrap();
    e.tool = None;
    e.needs_render = true;
    e.view.mode = crate::canvas_view::ZoomMode::Manual(0.5);
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let g = e.session.document.guide.as_ref().unwrap();
    let section = g.sections[0].id;
    let id = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.image.as_ref().is_some_and(|b| b.is_placeholder()))
        .unwrap()
        .id;
    let read = || {
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("choose-frame-image", id))))
            .unwrap()
    };
    let before = read();
    let grab = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("guide-resize-test", section))))
        .unwrap()
        .center();
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(grab), button(grab, true)],
    );
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(grab + egui::vec2(0., 35.))],
    );
    frame(&ctx, &mut e, vec![]);
    assert!(e.guide_drag.is_some());
    assert!((read().center().y - before.center().y - 35.).abs() < 0.1);
    let transform = e.guide_transform(e.view.current_transform().unwrap());
    let image = e
        .guide_drag
        .as_ref()
        .unwrap()
        .preview
        .annotations
        .iter()
        .find(|a| a.id == id)
        .unwrap();
    assert!(
        (read().center()
            - transform.screen_point([
                (image.start[0] + image.end[0]) * 0.5,
                (image.start[1] + image.end[1]) * 0.5
            ]))
        .length()
            < 0.1
    );
}

#[test]
fn page_layout_and_section_destination_menus_are_visible() {
    let frame = |ctx: &egui::Context, e: &mut Editor, events| {
        if e.needs_render {
            e.refresh_canvas_display(ctx);
        }
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| e.show_canvas(ui),
        ))
    };

    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let d = capture_core::templates::build(
        "two-steps",
        "Synthetic guide",
        &[],
        capture_core::templates::Scale::Fill,
    )
    .unwrap()
    .edit_guide(&Action::PageSize {
        width: 900,
        height: 550,
        auto_overflow: true,
    })
    .unwrap();
    e.session = Session::new(d).unwrap();
    e.view.mode = crate::canvas_view::ZoomMode::Manual(0.65);
    e.tool = None;
    e.needs_render = true;
    let first = frame(&ctx, &mut e, vec![]);
    let output = frame(&ctx, &mut e, vec![]);
    assert!(
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("page-button", 0usize, 1usize))))
            .is_some()
    );
    assert!(
        ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("page-button", 0usize, 2usize))))
            .is_some()
    );
    if let Ok(dir) = std::env::var("CLIPDESIGNER_FRAME_PAGES_UI") {
        dump_meshes(&ctx, &dir, "guide-pages", first, output);
    }
    let t = e.view.current_transform().unwrap();
    let p = controls(&e.session.document, t)[0].menu.center();
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut e,
            vec![egui::Event::PointerMoved(p), button(p, pressed)],
        );
    }
    let output = frame(&ctx, &mut e, vec![]);
    for label in ["Move to", "Copy to"] {
        assert!(
            crate::color_widgets::tests::text_rect(&output.shapes, label).is_some(),
            "{label}"
        );
    }
}

#[test]
fn page_buttons_add_blank_confirm_delete_and_restore_with_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.tool = None;
    e.needs_render = true;
    let initial = frame(&ctx, &mut e, vec![]);
    let textures = std::cell::RefCell::new(initial.0.textures_delta.clone());
    let click = |e: &mut Editor, label: &str| {
        let output = frame(&ctx, e, vec![]);
        textures
            .borrow_mut()
            .append(output.0.textures_delta.clone());
        let p = if matches!(label, "Add Page" | "Delete Page") {
            ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique((
                    "page-button",
                    0usize,
                    if label == "Add Page" { 1usize } else { 2usize },
                )))
            })
            .unwrap()
        } else {
            crate::color_widgets::tests::text_rect(&output.shapes, label)
                .unwrap_or_else(|| panic!("missing {label}"))
        }
        .center();
        for pressed in [true, false] {
            let painted = frame(
                &ctx,
                e,
                vec![egui::Event::PointerMoved(p), button(p, pressed)],
            );
            textures
                .borrow_mut()
                .append(painted.0.textures_delta.clone());
        }
    };
    click(&mut e, "Add Page");
    click(&mut e, "Blank Page");
    assert_eq!(e.session.document.guide.as_ref().unwrap().page_count(), 2);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    click(&mut e, "Delete Page");
    let mut first = frame(&ctx, &mut e, vec![]);
    for _ in 0..12 {
        let settled = frame(&ctx, &mut e, vec![]);
        first
            .0
            .textures_delta
            .append(settled.0.textures_delta.clone());
    }
    let output = frame(&ctx, &mut e, vec![]);
    first
        .0
        .textures_delta
        .append(output.0.textures_delta.clone());
    let popup = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("delete-page-popup")))
        .unwrap();
    assert!(popup.width() <= 255. && popup.height() <= 140., "{popup:?}");
    if let Ok(root) = std::env::var("CLIPDESIGNER_PAGE_CONTROLS_UI") {
        textures.borrow_mut().append(first.0.textures_delta.clone());
        first.0.textures_delta.clear();
        first.0.textures_delta = textures.borrow().clone();
        dump_meshes(&ctx, &root, "delete-page", first, output);
    }
    click(&mut e, "Cancel");
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    click(&mut e, "Delete Page");
    click(&mut e, "Delete");
    assert_eq!(e.session.document.guide.as_ref().unwrap().page_count(), 1);
    // Deleting the original content page keeps the remaining blank page editable.
    assert!(e.session.document.annotations.is_empty());
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    textures.borrow_mut().clear();
}

#[test]
fn bottom_handle_previews_reflow_commits_on_release_and_undoes_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.tool = None;
    e.needs_render = true;
    let mut initial = frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let original_height = e.session.document.height;
    let first = e.session.document.annotations[..4].to_vec();
    let handle = ctx
        .data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique(("guide-resize-test", 1u64)))
                .unwrap()
        })
        .center();
    let original_transform = e.view.current_transform().unwrap();
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(handle), button(handle, true)],
    );
    let to = handle + egui::vec2(0., 70.);
    frame(&ctx, &mut e, vec![egui::Event::PointerMoved(to)]);
    assert!(e.edit_drag.is_none());
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert_eq!(e.preview_document().height, original_height + 70);
    assert_eq!(e.preview_document().annotations[..4], first);
    let first_paint = frame(&ctx, &mut e, vec![]);
    let second_paint = frame(&ctx, &mut e, vec![]);
    if let Ok(dir) = std::env::var("CLIPDESIGNER_GUIDE_UI_DIR") {
        initial
            .0
            .textures_delta
            .append(first_paint.0.textures_delta.clone());
        dump_meshes(&ctx, &dir, "sections", initial, second_paint);
    }
    frame(&ctx, &mut e, vec![button(to, false)]);
    assert!(e.guide_drag.is_none());
    assert_eq!(e.session.document.height, original_height + 70);
    frame(&ctx, &mut e, vec![]);
    assert_eq!(
        e.view.current_transform().unwrap().image.min,
        original_transform.image.min
    );
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    assert!(!e.session.can_undo());
    e.shell.annotations_hidden = true;
    let hidden = e.canvas_document();
    hidden.validate().unwrap();
    assert!(hidden.guide.is_none());
    assert!(e.session.document.guide.is_some());
}

#[test]
fn plus_menu_inserts_real_editable_text_and_preserves_other_sections() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.tool = None;
    e.needs_render = true;
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let before = e.session.document.annotations.clone();
    let transform = e.view.current_transform().unwrap();
    let plus = controls(&e.session.document, transform)[0].add.center();
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(plus), button(plus, true)],
    );
    frame(&ctx, &mut e, vec![button(plus, false)]);
    frame(&ctx, &mut e, vec![]);
    assert!(egui::Popup::is_any_open(&ctx));
    let choice = ctx
        .data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique((
                "guide-insert-choice",
                1u64,
                "Text Section",
            )))
            .unwrap()
        })
        .center();
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(choice), button(choice, true)],
    );
    frame(&ctx, &mut e, vec![button(choice, false)]);
    assert_eq!(
        e.session.document.guide.as_ref().unwrap().sections[1].kind,
        SectionKind::Text
    );
    let section = &e.session.document.guide.as_ref().unwrap().sections[1];
    let id = section.objects[0];
    e.select_only(Some(id));
    e.begin_text_edit();
    e.text_draft
        .as_mut()
        .unwrap()
        .preview
        .text
        .as_mut()
        .unwrap()
        .content = "Editable section text".into();
    assert!(e.commit_text());
    assert_eq!(
        e.session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
            .unwrap()
            .text
            .as_ref()
            .unwrap()
            .content,
        "Editable section text"
    );
    e.execute(Command::Undo);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before);
}

#[test]
fn plus_menu_inserts_blank_section_without_text_or_image() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session = Session::new(fixture()).unwrap();
    e.tool = None;
    e.needs_render = true;
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    let count = e.session.document.annotations.len();
    let transform = e.view.current_transform().unwrap();
    let plus = controls(&e.session.document, transform)[0].add.center();
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(plus), button(plus, true)],
    );
    frame(&ctx, &mut e, vec![button(plus, false)]);
    frame(&ctx, &mut e, vec![]);
    let choice = ctx
        .data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique((
                "guide-insert-choice",
                1u64,
                "Blank section",
            )))
        })
        .unwrap()
        .center();
    frame(
        &ctx,
        &mut e,
        vec![egui::Event::PointerMoved(choice), button(choice, true)],
    );
    frame(&ctx, &mut e, vec![button(choice, false)]);
    let section = &e.session.document.guide.as_ref().unwrap().sections[1];
    assert_eq!(section.kind, SectionKind::Blank);
    assert!(section.objects.is_empty());
    assert_eq!(e.session.document.annotations.len(), count);
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
}

#[test]
fn section_action_buttons_stay_in_workspace_gutters_at_fit_and_zoom() {
    let document = fixture();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 800.));
    for scale in [0.5, 1., 2.] {
        let image = egui::Rect::from_center_size(
            viewport.center(),
            egui::vec2(document.width as f32, document.height as f32) * scale,
        );
        let t = Transform {
            image,
            viewport,
            scale,
            zoom: scale,
        };
        for c in controls(&document, t) {
            assert_eq!(c.add.center().y, c.menu.center().y);
            assert!(c.add.right() < image.left());
            assert!(c.menu.left() > image.right());
            assert_eq!(c.resize.center().y, t.screen_point([0., c.bottom]).y);
        }
        assert!(top_add(&document, t).unwrap().right() < image.left());
    }
}

#[test]
fn page_and_section_buttons_hide_at_collision_and_return_without_a_zoom_preset() {
    let document = fixture()
        .edit_guide(&Action::AddPage {
            page: 0,
            copy: true,
        })
        .unwrap();
    let mut previous = None;
    for percent in 10..=1600 {
        let scale = percent as f32 / 100.;
        let t = Transform {
            image: egui::Rect::from_min_size(
                egui::pos2(50., 50.),
                egui::vec2(document.width as f32, document.height as f32) * scale,
            ),
            viewport: egui::Rect::EVERYTHING,
            scale,
            zoom: scale,
        };
        let visible = visible_buttons(&document, t);
        assert!(crate::canvas_buttons::visible(&visible).iter().all(|v| *v));
        let rows = controls(&document, t);
        let pair = &rows[1];
        assert_eq!(pair.add.center().y, pair.menu.center().y);
        if percent == 10 {
            assert!(!visible.contains(&pair.menu));
        }
        if percent == 100 {
            assert!(visible.contains(&pair.menu));
        }
        if let Some(was) = previous
            && was
        {
            assert!(visible.contains(&pair.menu));
        }
        previous = Some(visible.contains(&pair.menu));
    }
}
