use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use egui::{Event, Modifiers, PointerButton};

struct Layout {
    header: Vec<(&'static str, Response)>,
    tabs: [egui::Rect; 4],
}

#[test]
fn library_tab_reclaims_ribbon_space_with_connected_full_workspace_border() {
    for width in [900., 1280., 1920.] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.badges = false;
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
                                .fill(crate::tabs::BACKGROUND)
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
    e.shell.badges = false;
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
    e.shell.badges = false;
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
    for badges in [false, true] {
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
                        tool = crate::shell::tool_button(ui, crate::shell::TOOLS[2], false, badges)
                            .rect;
                        more = crate::split_button::dropdown_sized(
                            ui,
                            "More tools",
                            egui::vec2(24.0, if badges { 76.0 } else { 58.0 }),
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
                    crate::shell::tool_button(ui, crate::shell::TOOLS[2], false, badges);
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
    e.shell.badges = false;
    e.tool = Some(crate::Kind::Step);
    e.add_step([100.0, 100.0]);
    frame(&ctx, &mut e, vec![], 1280.0);
    let (l, _) = frame(&ctx, &mut e, vec![], 1280.0);
    let original = serde_json::to_vec(&e.session.document).unwrap();
    assert!(!header(&l, "Record").enabled());
    let fixed = header(&l, "Save As").rect;
    click(&ctx, &mut e, l.tabs[2], 1280.0);
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
    click(&ctx, &mut e, l.tabs[2], 1280.0);
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
            ToolId::Text,
            ToolId::Arrow,
            ToolId::Shape,
            ToolId::Callout,
            ToolId::Step,
            ToolId::Selection,
            ToolId::Move
        ]
    );
    for (width, badges) in [
        (900.0, false),
        (1280.0, false),
        (900.0, true),
        (1280.0, true),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.badges = badges;
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let mut first_footer = None;
        for tab in [
            RibbonTab::Home,
            RibbonTab::ToolSettings,
            RibbonTab::Export,
            RibbonTab::AdvancedSettings,
        ] {
            e.shell.ribbon_tab = tab;
            frame(&ctx, &mut e, vec![], width);
            let (l, output) = frame(&ctx, &mut e, vec![], width);
            let labels: &[&str] = match tab {
                RibbonTab::Home => &["Clipboard", "Favorites", "General Tools"],
                RibbonTab::ToolSettings => &[
                    "Font",
                    "Justification",
                    "Alignment",
                    "Distribution",
                    "Arrange",
                    "Objects",
                    "Image",
                    "Canvas",
                ],
                RibbonTab::Export => &["Image", "Editable project", "Printing"],
                RibbonTab::AdvancedSettings => &[
                    "Image recovery",
                    "Text recognition",
                    "Selection behavior",
                    "Preferences",
                ],
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
    for (tab, library, name, width, badges) in [
        (RibbonTab::Home, false, "Home", 1280.0, false),
        (RibbonTab::ToolSettings, false, "Typography", 1280.0, false),
        (
            RibbonTab::ToolSettings,
            false,
            "ToolSettings",
            1280.0,
            false,
        ),
        (RibbonTab::Export, false, "Export", 1280.0, false),
        (
            RibbonTab::AdvancedSettings,
            false,
            "AdvancedSettings",
            1280.0,
            false,
        ),
        (RibbonTab::Home, true, "Library", 1280.0, false),
        (RibbonTab::Home, false, "HomeWide", 1920.0, false),
        (RibbonTab::Home, false, "HomeCompact", 900.0, true),
        (
            RibbonTab::ToolSettings,
            false,
            "SettingsCompact",
            900.0,
            true,
        ),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.badges = badges;
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
                },
            ))
        };
        let first = render();
        let output = render();
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_RIBBON_DIR") {
            dump_meshes(&ctx, &root, name, first, output);
        }
        assert_eq!(e.session.document.annotations, fixture.annotations);
        assert!(!e.session.can_undo());
    }
}

#[test]
fn advanced_settings_toggles_tool_selection_and_reverts_with_one_undo() {
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
                    responses = e.advanced_controls(ui, 58.0);
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
    let controls = render(&mut e, vec![]);
    assert_eq!(
        controls
            .iter()
            .find(|(n, _)| *n == "Grab Text")
            .unwrap()
            .1
            .enabled(),
        cfg!(windows)
    );
    assert!(
        controls
            .iter()
            .find(|(n, _)| *n == "More Preferences")
            .unwrap()
            .1
            .enabled()
    );
    click(&mut e, "More Preferences");
    assert_eq!(e.shell.backstage, Some(crate::backstage::Page::Options));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    e.shell.backstage = None;
    let initial = e.quick_controls.auto_switch_tool();
    click(&mut e, "Switch tools");
    assert_eq!(e.quick_controls.auto_switch_tool(), !initial);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    click(&mut e, "Revert");
    assert!(e.session.document.annotations.is_empty());
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
}
