use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use egui::{Event, Modifiers, PointerButton};

struct Layout {
    header: Vec<(&'static str, Response)>,
    tabs: [egui::Rect; 4],
}

#[test]
fn step_press_hold_release_keeps_one_property_set_and_stable_canvas() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.tool = Some(crate::Kind::Step);
    e.session = crate::Session::new(crate::Document::blank(640, 400).unwrap()).unwrap();
    e.add_step([100., 100.]);
    e.needs_render = true;
    let render = |e: &mut Editor, events| {
        let mut viewport = egui::Rect::NOTHING;
        let out = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                e.show_shell(ui);
                if e.needs_render {
                    e.refresh_canvas_display(&ctx);
                }
                egui::CentralPanel::default().show(ui, |ui| {
                    viewport = ui.available_rect_before_wrap();
                    e.show_workspace(ui);
                });
            },
        ));
        let transform = e
            .view
            .transform(viewport, egui::vec2(640., 400.), ctx.pixels_per_point());
        (transform, out)
    };
    for _ in 0..4 {
        render(&mut e, vec![]);
    }
    let (original, _) = render(&mut e, vec![]);
    let pos = original.screen_point([350., 220.]);
    let button = |pressed| Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
    };
    render(&mut e, vec![Event::PointerMoved(pos), button(true)]);
    assert!(e.step_placement.is_some());
    for _ in 0..4 {
        let (t, out) = render(&mut e, vec![]);
        // The Marker group follows Appearance (properties audit), so it may sit
        // below the fold; whatever is shown must appear only once.
        for label in ["Marker size", "Thickness", "Fill opacity"] {
            let count = out
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.text()==label))
                .count();
            assert!(
                count <= 1,
                "a pending marker must not duplicate the default and selected controls ({label})"
            );
        }
        assert_eq!(
            t.image, original.image,
            "holding a new marker must not shift the canvas"
        );
    }
    render(&mut e, vec![button(false)]);
    assert!(e.step_placement.is_none());
    assert_eq!(e.session.document.annotations.len(), 2);
    let (t, _) = render(&mut e, vec![]);
    assert_eq!(t.image, original.image);
}

#[test]
fn ribbons_only_allocate_scrollbar_space_for_overflow_and_advanced_replaces_old_tabs() {
    for width in [900., 1920.] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        for tab in [RibbonTab::Insert, RibbonTab::Formatting, RibbonTab::Export] {
            e.shell.ribbon_tab = tab;
            for _ in 0..4 {
                frame(&ctx, &mut e, vec![], width);
            }
            let (_, output) = frame(&ctx, &mut e, vec![], width);
            assert!(
                crate::color_widgets::tests::text_rect(&output.shapes, "Advanced Settings")
                    .is_none()
            );
            assert!(
                crate::color_widgets::tests::text_rect(&output.shapes, "Tool Settings").is_none()
            );
            let (content, inner, outer) = ctx
                .data(|d| {
                    d.get_temp::<(egui::Vec2, egui::Rect, egui::Rect)>(egui::Id::unique(
                        "ribbon-overflow-geometry",
                    ))
                })
                .unwrap();
            let overflow = content.x > inner.width().ceil();
            let rail = outer.height() - inner.height();
            assert_eq!(
                rail > 5.,
                overflow,
                "{tab:?} {width}: content {content:?}, inner {inner:?}, outer {outer:?}"
            );
            if width == 1920. {
                assert!(!overflow, "wide {tab:?} must not show an idle scrollbar");
            }
        }
    }
}

#[test]
fn page_controls_review_shell_reserves_scrollbar_rails_in_restored_and_wide_windows() {
    for width in [960., 1440.] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let d = capture_core::templates::build(
            "three-steps",
            "Guide review",
            &[],
            capture_core::templates::Scale::Fill,
        )
        .unwrap()
        .edit_guide(&capture_core::guide_sections::Action::AddPage {
            page: 0,
            copy: true,
        })
        .unwrap();
        if width == 960.
            && let Ok(root) = std::env::var("CLIPCRAFT_FRAME_PAGES_FIXTURES")
        {
            std::fs::create_dir_all(&root).unwrap();
            d.save(&std::path::Path::new(&root).join("review-guide.capture.json"))
                .unwrap();
        }
        e.session = capture_core::Session::new(d).unwrap();
        e.tool = None;
        e.needs_render = true;
        e.shell.properties = true;
        e.shell.property_tab = crate::shell::PropertyTab::Objects;
        e.shell.ribbon_tab = RibbonTab::Formatting;
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique(("objects-page-expanded", 0usize)), true));
        let render = |e: &mut crate::Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 740.),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.show_shell(ui);
                    e.show_canvas(ui);
                },
            ))
        };
        let mut first = render(&mut e);
        for _ in 0..2 {
            let output = render(&mut e);
            first
                .0
                .textures_delta
                .append(output.0.textures_delta.clone());
        }
        let output = render(&mut e);
        let content = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("properties-scroll-content")))
            .unwrap();
        for a in &e.session.document.annotations {
            if let Some((row, eye, lock, _)) = ctx.data(|d| {
                d.get_temp::<(egui::Rect, egui::Rect, egui::Rect, String)>(egui::Id::unique((
                    "object-row",
                    a.id,
                )))
            }) {
                assert!(
                    row.right() <= content.right() + 0.1
                        && eye.right() <= content.right() + 0.1
                        && lock.right() <= content.right() + 0.1
                );
            }
        }
        let ribbon = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("ribbon-scroll-content")))
            .unwrap();
        for caption in ["Font", "Justification", "Alignment", "Distribution"] {
            if let Some(rect) = crate::color_widgets::tests::text_rect(&output.shapes, caption) {
                assert!(
                    rect.bottom() <= ribbon.bottom() + 0.1,
                    "{caption}: {rect:?} {ribbon:?}"
                );
            }
        }
        if let Ok(root) = std::env::var("CLIPCRAFT_PAGE_CONTROLS_UI") {
            dump_meshes(&ctx, &root, &format!("shell-{width:.0}"), first, output);
        }
    }
}

#[test]
#[ignore = "Production UI image export; synthetic content and no desktop access"]
fn website_ui_previews() {
    let root = std::env::var("CLIPCRAFT_WEBSITE_UI").expect("Set output directory");
    for scene in ["editor", "icons", "export"] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let storage = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(storage.path());
        e.session = capture_core::Session::new(
            capture_core::Document::canvas(640, 400, [255, 255, 255, 255]).unwrap(),
        )
        .unwrap();
        e.shell.properties = true;
        e.tool = None;
        for (id, kind, start, end, text) in [
            (
                1,
                crate::Kind::Text,
                [30., 25.],
                [610., 90.],
                "Make every step clear",
            ),
            (
                2,
                crate::Kind::Rectangle,
                [65., 135.],
                [295., 280.],
                "Review the details",
            ),
            (
                3,
                crate::Kind::Callout,
                [350., 145.],
                [580., 275.],
                "Share with confidence",
            ),
            (
                4,
                crate::Kind::Text,
                [65., 320.],
                [580., 370.],
                "Capture. Annotate. Explain.",
            ),
        ] {
            let mut a = e.new_annotation(kind, start, end);
            a.id = id;
            a.color = [24, 145, 193, 255];
            if kind == crate::Kind::Rectangle {
                a.fill = Some([224, 242, 249, 255]);
                a.corner_radius = 16.;
            }
            let t = a.text.get_or_insert_with(Default::default);
            t.content = text.into();
            t.size_points = if id == 1 { 28. } else { 17. };
            t.color = Some([37, 39, 41, 255]);
            t.bold = id == 1;
            e.execute(crate::Command::Add { annotation: a });
        }
        assert_eq!(e.session.document.annotations.len(), 4, "{}", e.status);
        e.session.document.validate().unwrap();
        e.status = "Synthetic onboarding guide".into();
        e.select_only(Some(2));
        if scene == "icons" {
            e.choose_tool(
                crate::shell::TOOLS
                    .iter()
                    .position(|t| t.id == capture_core::toolbar::ToolId::Icons)
                    .unwrap(),
            );
        }
        if scene == "export" {
            e.begin_export(false);
        }
        e.needs_render = true;
        let mut frame_time = 0.0;
        let mut render = || {
            frame_time += 1.0;
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    time: Some(frame_time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 800.),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.show_shell(ui);
                    if e.needs_render {
                        e.refresh_canvas_display(&ctx);
                    }
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(crate::shell::CANVAS)
                                .inner_margin(12),
                        )
                        .show(ui, |ui| e.show_workspace(ui));
                    e.show_export(&ctx, None);
                },
            ))
        };
        let mut first = render();
        for _ in 0..4 {
            let settled = render();
            first
                .0
                .textures_delta
                .append(settled.textures_delta.clone());
        }
        let output = render();
        dump_meshes(&ctx, &root, scene, first, output);
    }
}

#[test]
fn library_tab_reclaims_ribbon_space_with_connected_full_workspace_border() {
    for width in [900., 1280., 1920.] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.library = true;
        let library = tempfile::tempdir().unwrap();
        let mut store = capture_core::history::History::open(library.path()).unwrap();
        store
            .create(
                &e.session.document,
                "Synthetic capture",
                capture_core::history::Origin::Capture,
            )
            .unwrap();
        let mut guide = capture_core::step_capture::Guide::new([160, 90]).unwrap();
        guide
            .push(
                image::RgbaImage::from_pixel(160, 90, image::Rgba([125, 195, 225, 255])),
                capture_core::step_capture::Gesture {
                    start: [50, 40],
                    end: [50, 40],
                    button: capture_core::step_capture::Button::Left,
                },
            )
            .unwrap();
        store
            .create(
                &guide.document().unwrap(),
                "Synthetic step guide",
                capture_core::history::Origin::Capture,
            )
            .unwrap();
        e.history = crate::history::HistoryPanel::isolated(library.path());
        let mut render = || {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 720.),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.show_shell(ui);
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(crate::shell::SURROUND)
                                .inner_margin(egui::Margin {
                                    left: 5,
                                    right: 5,
                                    top: 0,
                                    bottom: 3,
                                }),
                        )
                        .show(ui, |ui| e.show_workspace(ui));
                },
            ))
        };
        let first = render();
        let output = render();
        let rect = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("library-surround")))
            .unwrap();
        let tab = e.shell.library_tab_rect.unwrap();
        assert!(
            rect.top() - tab.bottom() <= 12.,
            "ribbon space was not reclaimed: {rect:?} {tab:?}"
        );
        assert!(rect.width() >= width - 14. && rect.bottom() <= 720.);
        assert!(output.0.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Rect(r) if r.stroke.color == crate::tabs::ACTIVE_BLUE && r.rect == rect)));
        assert!(
            !output.0.shapes.iter().any(
                |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text == "Clipboard")
            )
        );
        if let Ok(root) = std::env::var("CLIPCRAFT_GUIDE_FIX_UI") {
            dump_meshes(&ctx, &root, &format!("library-{width:.0}"), first, output);
        }
    }
}

#[test]
fn actual_format_button_double_click_locks_and_single_click_turns_it_off() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.session.document.annotations = (1..=3)
        .map(|id| {
            capture_core::steps::marker(
                id,
                [100.0 * id as f32, 100.0],
                44,
                [id as u8 * 50, 40, 100, 255],
                (id as u32, false),
                [640, 400],
            )
            .unwrap()
        })
        .collect();
    e.select_only(Some(1));
    let before = e.session.document.annotations.clone();
    frame(&ctx, &mut e, vec![], 1280.0);
    frame(&ctx, &mut e, vec![], 1280.0);
    let button = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("format-painter-button")))
        .unwrap();
    click(&ctx, &mut e, button, 1280.0);
    assert!(e.quick_controls.format_source.is_some());
    assert!(!e.quick_controls.format_locked);
    click(&ctx, &mut e, button, 1280.0);
    assert!(e.quick_controls.format_locked);
    let source = e.quick_controls.format_source.clone();
    e.paint_format_to(2);
    let after_first = e.session.document.annotations.clone();
    assert_eq!(e.quick_controls.format_source, source);
    assert!(e.quick_controls.format_locked);
    e.paint_format_to(3);
    assert_eq!(e.quick_controls.format_source, source);
    assert!(e.quick_controls.format_locked);
    for a in e.session.document.annotations.iter().skip(1) {
        assert_eq!(a.color, before[0].color);
    }
    for _ in 0..40 {
        frame(&ctx, &mut e, vec![], 1280.0);
    }
    click(&ctx, &mut e, button, 1280.0);
    assert!(e.quick_controls.format_source.is_none());
    assert!(!e.quick_controls.format_locked);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, after_first);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, before);
    assert!(!e.session.can_undo());
}

#[test]
fn more_tools_appears_only_when_the_actual_row_cannot_fit_every_tool() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    let total = e.quick_controls.toolbar_order().len();
    let all_width = e.tool_sections_width(total, 3.0);
    let before = e.quick_controls.toolbar_order().to_vec();
    // Resize down/up across the exact boundary in the same context.
    for width in [
        all_width + 50.0,
        all_width,
        all_width - 1.0,
        330.0,
        all_width,
    ] {
        let mut layout = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(all_width + 100.0, 300.0),
                )),
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    let left = ui.cursor().left();
                    layout = Some((left, e.ribbon_tools(ui, width, 58.0)));
                });
            },
        );
        output.textures_delta.clear();
        let (left, (visible, last, more)) = layout.unwrap();
        assert_eq!(more.is_some(), width < all_width);
        if let Some(more) = more {
            assert!(visible < total);
            assert!(more.rect.right() <= left + width + 0.01);
            assert_eq!(more.rect.height(), last.height());
            egui::Popup::open_id(&ctx, egui::Popup::default_response_id(&more));
        } else {
            assert_eq!(visible, total);
            assert!(!egui::Popup::is_any_open(&ctx));
        }
        assert!(last.right() <= left + width + 0.01);
        assert_eq!(e.quick_controls.toolbar_order(), before);
        assert!(!e.session.can_undo());
    }
}

#[test]
fn more_tools_matches_tool_height_and_hover_has_visible_background() {
    {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut tool = egui::Rect::NOTHING;
        let mut more = egui::Rect::NOTHING;
        let mut render = |events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| {
                        tool = crate::shell::tool_button(ui, crate::shell::TOOLS[2], false).rect;
                        more = crate::split_button::dropdown_sized(
                            ui,
                            "More tools",
                            egui::vec2(24.0, 58.0),
                            |ui| ui.label("Other tools"),
                        )
                        .response
                        .rect;
                    });
                },
            )
        };
        let mut initial = render(vec![]);
        initial.textures_delta.clear();
        let mut resting = render(vec![]);
        resting.textures_delta.clear();
        drop(render);
        assert_eq!(tool.height(), more.height());
        assert!((tool.center().y - more.center().y).abs() < 0.1);
        let mut hovered = ctx.run_ui(
            egui::RawInput {
                events: vec![Event::PointerMoved(more.center())],
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    crate::shell::tool_button(ui, crate::shell::TOOLS[2], false);
                    crate::split_button::dropdown_sized(
                        ui,
                        "More tools",
                        egui::vec2(24.0, more.height()),
                        |ui| ui.label("Other tools"),
                    );
                });
            },
        );
        hovered.textures_delta.clear();
        assert!(hovered.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Rect(r) if r.rect.contains(more.center())&&r.fill==ctx.style_of(egui::Theme::Dark).visuals.widgets.hovered.weak_bg_fill)));
    }
}
fn frame(
    ctx: &egui::Context,
    e: &mut Editor,
    events: Vec<Event>,
    width: f32,
) -> (Layout, PaintedOutput) {
    let mut l = Layout {
        header: vec![],
        tabs: [egui::Rect::NOTHING; 4],
    };
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 720.0),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            l.header = e.show_quick_access(ui);
            l.tabs = e.show_ribbon(ui).map(|r| r.rect);
        },
    );
    (l, PaintedOutput(output))
}
fn click(ctx: &egui::Context, e: &mut Editor, rect: egui::Rect, width: f32) {
    for pressed in [true, false] {
        frame(
            ctx,
            e,
            vec![
                Event::PointerMoved(rect.center()),
                Event::PointerButton {
                    pos: rect.center(),
                    pressed,
                    button: PointerButton::Primary,
                    modifiers: Modifiers::NONE,
                },
            ],
            width,
        );
    }
}
fn header<'a>(l: &'a Layout, name: &str) -> &'a Response {
    &l.header.iter().find(|(n, _)| *n == name).unwrap().1
}
#[test]
fn header_stays_fixed_between_tabs_and_library_with_working_undo_redo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.tool = Some(crate::Kind::Step);
    e.add_step([100.0, 100.0]);
    frame(&ctx, &mut e, vec![], 1280.0);
    let (l, _) = frame(&ctx, &mut e, vec![], 1280.0);
    let original = serde_json::to_vec(&e.session.document).unwrap();

    let fixed = header(&l, "Save As").rect;
    click(&ctx, &mut e, l.tabs[3], 1280.0);
    assert_eq!(e.shell.ribbon_tab, RibbonTab::Export);
    let (l, _) = frame(&ctx, &mut e, vec![], 1280.0);
    assert_eq!(header(&l, "Save As").rect, fixed);
    assert!(header(&l, "Undo").enabled());
    click(&ctx, &mut e, header(&l, "Undo").rect, 1280.0);
    assert!(e.session.document.annotations.is_empty());
    let (l, _) = frame(&ctx, &mut e, vec![], 1280.0);
    assert!(header(&l, "Redo").enabled());
    click(&ctx, &mut e, header(&l, "Redo").rect, 1280.0);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    let (l, _) = frame(&ctx, &mut e, vec![], 1280.0);
    assert!(!l.header.iter().any(|(name, _)| *name == "Library"));
    let library = e.shell.library_tab_rect.unwrap();
    click(&ctx, &mut e, library, 1280.0);
    assert!(e.shell.library);
    let (l, _) = frame(&ctx, &mut e, vec![], 1280.0);
    assert_eq!(header(&l, "Save As").rect, fixed);
    assert!(!header(&l, "Undo").enabled());
    click(&ctx, &mut e, l.tabs[3], 1280.0);
    assert!(!e.shell.library);
    assert_eq!(e.shell.ribbon_tab, RibbonTab::Export);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
}
#[test]
fn compact_header_fits_and_ribbon_rendering_does_not_edit_documents() {
    use capture_core::toolbar::ToolId;
    assert_eq!(
        &capture_core::toolbar::default_order()[..7],
        &[
            ToolId::Move,
            ToolId::Text,
            ToolId::Shape,
            ToolId::Steps,
            ToolId::Lines,
            ToolId::Icons,
            ToolId::Penlighter
        ]
    );
    for width in [900.0, 1280.0] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let mut first_footer = None;
        for tab in [RibbonTab::Insert, RibbonTab::Formatting, RibbonTab::Export] {
            e.shell.ribbon_tab = tab;
            frame(&ctx, &mut e, vec![], width);
            let (l, output) = frame(&ctx, &mut e, vec![], width);
            let labels: &[&str] = match tab {
                RibbonTab::Insert => &["Clipboard", "Favorites", "General Tools"],
                RibbonTab::Formatting => &[
                    "Alignment",
                    "Distribution",
                    "Arrange",
                    "Grouping",
                    "Rotate & flip",
                    "Size",
                    "Guides",
                    "Tools",
                ],
                RibbonTab::Export => &["Image", "Editable project", "Printing"],
                RibbonTab::Advanced => &[],
            };
            let footers: Vec<_> = output
                .0
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t)
                        if labels.contains(&t.galley.job.text.as_str())
                            && t.galley
                                .job
                                .sections
                                .iter()
                                .all(|s| s.format.font_id.size == 10.0) =>
                    {
                        Some(t.pos.y)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(footers.len(), labels.len());
            let first = *first_footer.get_or_insert(footers[0]);
            assert!(
                (footers[0] - first).abs() < 0.1,
                "Ribbon height changed on {tab:?}"
            );
            assert!(
                footers.iter().all(|y| (*y - footers[0]).abs() < 0.1),
                "{footers:?}"
            );
            assert!(
                output
                    .0
                    .shapes
                    .iter()
                    .any(|s| matches!(&s.shape, egui::Shape::Rect(r)
                if r.fill==Color32::from_rgb(50,53,55) && r.rect.width() >= width-12.0))
            );
            for (_, r) in l.header {
                assert!(
                    r.rect.min.x >= 0.0
                        && r.rect.max.x < width
                        && r.rect.min.y >= 0.0
                        && r.rect.max.y < 40.0
                );
            }
        }
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert!(!e.session.can_undo());
    }
}
#[test]
fn headless_full_workspace_ribbon_fixtures() {
    let fixture: capture_core::Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    for (tab, library, name, width) in [
        (RibbonTab::Insert, false, "Insert", 1280.0),
        (RibbonTab::Insert, false, "Shape", 1280.0),
        (RibbonTab::Insert, false, "Pen", 1280.0),
        (RibbonTab::Insert, false, "Obfuscate", 1280.0),
        (RibbonTab::Insert, false, "Eraser", 1280.0),
        (RibbonTab::Insert, false, "Icons", 1280.0),
        (RibbonTab::Advanced, false, "Crop", 1280.0),
        (RibbonTab::Advanced, false, "Cropped", 1280.0),
        (RibbonTab::Formatting, false, "Typography", 1280.0),
        (RibbonTab::Formatting, false, "Formatting", 1280.0),
        (RibbonTab::Export, false, "Export", 1280.0),
        (RibbonTab::Advanced, false, "Advanced", 1280.0),
        (RibbonTab::Insert, true, "Library", 1280.0),
        (RibbonTab::Insert, false, "HomeWide", 1920.0),
        (RibbonTab::Insert, false, "HomeCompact", 900.0),
        (RibbonTab::Formatting, false, "SettingsCompact", 900.0),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        crate::text_layout::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.ribbon_tab = tab;
        e.shell.library = library;
        let mut fixture = fixture.clone();
        if name == "Typography" {
            for a in &mut fixture.annotations {
                if let Some(t) = &mut a.text {
                    t.underline = true;
                    t.strikethrough = true;
                }
            }
            fixture.promote_features();
        }
        e.session = capture_core::Session::new(fixture.clone()).unwrap();
        e.selected = Some(
            fixture
                .annotations
                .iter()
                .find(|a| a.kind == crate::Kind::Text)
                .unwrap()
                .id,
        );
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        if let Some(id) = match name {
            "Shape" => Some(capture_core::toolbar::ToolId::Shape),
            "Pen" => Some(capture_core::toolbar::ToolId::Penlighter),
            "Obfuscate" => Some(capture_core::toolbar::ToolId::Obfuscate),
            "Eraser" => Some(capture_core::toolbar::ToolId::Eraser),
            "Icons" => Some(capture_core::toolbar::ToolId::Icons),
            _ => None,
        } {
            e.select_only(None);
            e.choose_tool(crate::shell::TOOLS.iter().position(|t| t.id == id).unwrap());
            if name == "Obfuscate" {
                e.tool = Some(crate::Kind::Redact);
            }
        }
        if name == "Cropped" {
            e.execute(capture_core::Command::CropImage {
                x: 80,
                y: 45,
                width: 400,
                height: 250,
                mode: capture_core::crop::CropMode::Preserve,
                page: 0,
            });
            fixture = e.session.document.clone();
            e.select_only(None);
        }
        if name == "Crop" {
            e.begin_crop();
            e.crop.as_mut().unwrap().bounds = [80, 45, 480, 295];
        }
        e.needs_render = true;
        let mut render = || {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 720.0),
                    )),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.show_shell(ui);
                    if e.needs_render {
                        e.refresh_canvas_display(&ctx);
                    }
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(crate::shell::CANVAS)
                                .inner_margin(12),
                        )
                        .show(ui, |ui| e.show_workspace(ui));
                },
            ))
        };
        let mut first = render();
        for _ in 0..4 {
            let settled = render();
            first
                .0
                .textures_delta
                .append(settled.textures_delta.clone());
        }
        let output = render();
        if name == "Crop" {
            let apply = crate::color_widgets::tests::text_rect(&output.shapes, "Apply crop")
                .expect("Crop dock must be painted");
            assert!(
                apply.bottom() < 665.,
                "Crop dock overlaps the footer: {apply:?}"
            );
            assert!(apply.top() > e.view.current_transform().unwrap().viewport.bottom());
        }
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_RIBBON_DIR") {
            dump_meshes(&ctx, &root, name, first, output);
        }
        assert_eq!(e.session.document.annotations, fixture.annotations);
        assert_eq!(e.session.can_undo(), name == "Cropped");
    }
}

#[test]
fn advanced_restore_reverts_with_one_undo() {
    let ctx = egui::Context::default();
    let mut e = crate::canvas::tests::editor(&ctx);
    e.add_step([100.0, 100.0]);
    let original = serde_json::to_vec(&e.session.document).unwrap();
    let render = |e: &mut Editor, events| {
        let mut responses = vec![];
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    responses = e.restore_ribbon(ui, 58.0);
                });
            },
        );
        output.textures_delta.clear();
        responses
    };
    let click = |e: &mut Editor, name| {
        render(e, vec![]);
        let rs = render(e, vec![]);
        let r = &rs.iter().find(|(n, _)| *n == name).unwrap().1;
        let pos = r.rect.center();
        for pressed in [true, false] {
            render(
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
    };
    click(&mut e, "Revert");
    assert!(e.session.document.annotations.is_empty());
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
}

#[test]
fn ribbon_frame_edges_and_advanced_action_tiles_match() {
    for scale in [1., 1.25, 1.5] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.set_pixels_per_point(scale);
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut expected: Option<(f32, f32)> = None;
        for tab in [
            RibbonTab::Insert,
            RibbonTab::Formatting,
            RibbonTab::Export,
            RibbonTab::Advanced,
        ] {
            e.shell.ribbon_tab = tab;
            for _ in 0..3 {
                frame(&ctx, &mut e, vec![], 1920.);
            }
            let (_, out) = frame(&ctx, &mut e, vec![], 1920.);
            let border = out
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Rect(r)
                        if r.stroke.color == crate::tabs::ACTIVE_BLUE && r.rect.width() > 1000. =>
                    {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .unwrap();
            if let Some((top, height)) = expected {
                assert!((border.top() - top).abs() < 0.1, "{tab:?}: {border:?}");
                assert!(
                    (border.height() - height).abs() < 0.1,
                    "{tab:?}: {border:?} expected {height}"
                );
            } else {
                expected = Some((border.top(), border.height()));
            }
            if tab == RibbonTab::Advanced {
                let get = |label| {
                    ctx.data(|d| {
                        d.get_temp::<egui::Rect>(egui::Id::unique(("command-tile", label)))
                    })
                    .unwrap()
                };
                assert_eq!(get("Grab Text").size(), get("Revert to Original").size());
            }
        }
    }
}

#[test]
fn header_capture_starts_current_profile_without_switching_tabs() {
    use capture_core::capture_options::{Fullscreen, Selection, Settings};
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.shell.ribbon_tab = RibbonTab::Insert;
    let settings = Settings {
        selection: Selection::Fullscreen,
        clipboard: true,
        find_sensitive: false,
        preview: false,
        cursor: true,
        delay_seconds: 3,
        fullscreen: Fullscreen::All,
        scroll_automatic: false,
        scrolling: false,
    };
    e.quick_controls.remember_capture(settings.clone());
    // With its settings window turned off, the header button captures straight away.
    e.quick_controls.set_ask_before_clip(false);
    frame(&ctx, &mut e, vec![], 1280.);
    let (layout, _) = frame(&ctx, &mut e, vec![], 1280.);
    click(&ctx, &mut e, header(&layout, "Capture").rect, 1280.);
    assert_eq!(e.clip.active, Some(settings));
    assert_eq!(e.capture_requested, crate::CAPTURE_ENABLED);
    assert_eq!(e.shell.ribbon_tab, RibbonTab::Insert);
    let (layout, _) = frame(&ctx, &mut e, vec![], 1280.);
    assert!(!header(&layout, "Capture").enabled());
}
