use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
fn pointer_selection_ranges_marquee_context_and_double_open() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    e.shell.library = true;
    for name in ["Install", "Configure", "Review", "Approve", "Publish"] {
        e.history
            .store
            .as_mut()
            .unwrap()
            .create(
                &capture_core::Document::canvas(80, 50, [40, 130, 170, 255]).unwrap(),
                name,
                Origin::Canvas,
            )
            .unwrap();
    }
    let unchanged = e.session.document.clone();
    let mut time = 0.;
    let deltas = std::cell::RefCell::new(Vec::new());

    let mut frame = |e: &mut Editor, mut events: Vec<egui::Event>, modifiers| {
        time += 0.08;
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        let output = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 700.),
                )),
                time: Some(time),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| e.show_library(ui),
        ));
        deltas
            .borrow_mut()
            .extend(output.0.textures_delta.set.clone());
        output
    };
    let mut first = frame(&mut e, vec![], egui::Modifiers::NONE);
    let warm = frame(&mut e, vec![], egui::Modifiers::NONE);
    for removed in ["Select visible", "Clear", "Delete…"] {
        assert!(crate::color_widgets::tests::text_rect(&warm.shapes, removed).is_none());
    }
    first.0.textures_delta.append(warm.0.textures_delta.clone());
    let cards: Vec<(String, egui::Rect)> = ctx
        .data(|d| d.get_temp(egui::Id::unique("library-cards")))
        .unwrap();
    let click = |p, button, modifiers| {
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button,
                pressed: true,
                modifiers,
            },
            egui::Event::PointerButton {
                pos: p,
                button,
                pressed: false,
                modifiers,
            },
        ]
    };
    let primary = egui::PointerButton::Primary;
    let _ = frame(
        &mut e,
        click(cards[0].1.center(), primary, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    assert_eq!(e.history.library.selected, [cards[0].0.clone()].into());
    assert_eq!(
        serde_json::to_vec(&e.session.document).unwrap(),
        serde_json::to_vec(&unchanged).unwrap()
    );
    assert!(e.shell.library);
    let _ = frame(
        &mut e,
        click(cards[2].1.center(), primary, egui::Modifiers::SHIFT),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(
        e.history.library.selected,
        cards[..3].iter().map(|c| c.0.clone()).collect()
    );
    let _ = frame(
        &mut e,
        click(cards[4].1.center(), primary, egui::Modifiers::ALT),
        egui::Modifiers::ALT,
    );
    assert_eq!(e.history.library.selected.len(), 4);
    let start = cards[1].1.left_top() + egui::vec2(-3., 3.);
    let end = cards[2].1.right_bottom() + egui::vec2(3., 3.);
    let _ = frame(
        &mut e,
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        egui::Modifiers::NONE,
    );
    let _ = frame(
        &mut e,
        vec![egui::Event::PointerMoved(end)],
        egui::Modifiers::NONE,
    );
    assert!(
        e.history.library.marquee.is_some(),
        "drag did not start within Library grid"
    );
    let _ = frame(
        &mut e,
        vec![egui::Event::PointerButton {
            pos: end,
            button: primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        e.history.library.selected,
        cards[1..3].iter().map(|c| c.0.clone()).collect()
    );
    let p = cards[1].1.center();
    let _ = frame(
        &mut e,
        click(p, egui::PointerButton::Secondary, egui::Modifiers::NONE),
        egui::Modifiers::NONE,
    );
    let menu = frame(&mut e, vec![], egui::Modifiers::NONE);
    for label in [
        "Save All As…",
        "Duplicate All",
        "Rename All…",
        "Edit tags…",
        "Delete from Library…",
    ] {
        assert!(
            crate::color_widgets::tests::text_rect(&menu.shapes, label).is_some(),
            "missing {label}"
        );
    }
    assert_eq!(e.history.library.selected.len(), 2);
    if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_LIBRARY_DIR") {
        first.0.textures_delta.clear();
        for (id, patches) in deltas.borrow().iter() {
            first
                .0
                .textures_delta
                .set
                .entry(*id)
                .or_default()
                .extend(patches.clone());
        }
        dump_meshes(&ctx, &dir, "library-bulk", first, menu);
    }
    egui::Popup::close_all(&ctx);
    let _ = frame(&mut e, vec![], egui::Modifiers::NONE);
    let p = cards[3].1.center();
    for _ in 0..2 {
        let _ = frame(
            &mut e,
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
        let _ = frame(
            &mut e,
            vec![egui::Event::PointerButton {
                pos: p,
                button: primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
    }
    assert!(
        !e.shell.library,
        "status: {}; selected: {:?}; pending: {}",
        e.status,
        e.history.library.selected,
        e.autosave.action.is_some()
    );
    assert_eq!(e.history.active.as_deref(), Some(cards[3].0.as_str()));
}

#[test]
fn thumbnails_default_to_names_and_details_are_opt_in() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.shell.library = true;
    e.history = HistoryPanel::isolated(root.path());
    let entry = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(
            &capture_core::Document::canvas(80, 50, [40, 130, 170, 255]).unwrap(),
            "Onboarding",
            Origin::Canvas,
        )
        .unwrap();
    let frame = |e: &mut Editor| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 700.),
                )),
                ..Default::default()
            },
            |ui| e.show_library(ui),
        ))
    };
    frame(&mut e);
    let output = frame(&mut e);
    let has = |output: &PaintedOutput, s: &str| {
        crate::color_widgets::tests::text_rect(&output.shapes, s).is_some()
    };
    assert!(has(&output, &entry.label));
    assert!(!has(&output, "80 × 50 · Image"));
    assert!(!has(&output, &entry.created_utc()));
    assert!(has(&output, "IMAGE"));
    let template = capture_core::templates::build(
        "overview",
        "Quick reference",
        &[],
        capture_core::templates::Scale::Fit,
    )
    .unwrap();
    e.history
        .store
        .as_mut()
        .unwrap()
        .create(&template, "Reference", Origin::Canvas)
        .unwrap();
    e.history.refresh();
    e.quick_controls.set_library_pinned(true);
    for _ in 0..20 {
        frame(&mut e);
    }
    let output = frame(&mut e);
    assert!(has(&output, "Template"));
    assert!(has(&output, "From Template  (1)"));
    e.quick_controls.set_thumbnail_details(true);
    let output = frame(&mut e);
    assert!(has(&output, "80 × 50 · Image"));
    assert!(has(&output, &entry.created_utc()));
    assert!(has(&output, "IMAGE"));
}

#[test]
fn library_hover_pushes_grid_pin_keeps_it_open_and_tags_show_counts() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    e.shell.library = true;
    for name in ["Review", "Publish"] {
        let store = e.history.store.as_mut().unwrap();
        let entry = store
            .create(
                &capture_core::Document::canvas(80, 50, [40, 130, 170, 255]).unwrap(),
                name,
                Origin::Canvas,
            )
            .unwrap();
        store
            .edit_metadata(&entry, name, &["Business".into()])
            .unwrap();
    }
    e.history.refresh();
    let clock = std::cell::Cell::new(0f64);
    let deltas = std::cell::RefCell::new(Vec::new());
    let render = |e: &mut Editor, p: egui::Pos2, click: bool| {
        let mut events = vec![egui::Event::PointerMoved(p)];
        if click {
            for pressed in [true, false] {
                events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        let output = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                time: Some({
                    clock.set(clock.get() + 0.05);
                    clock.get()
                }),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 760.),
                )),
                events,
                ..Default::default()
            },
            |ui| e.show_library(ui),
        ));
        deltas
            .borrow_mut()
            .extend(output.0.textures_delta.set.clone());
        output
    };
    let width = || {
        ctx.data(|d| d.get_temp::<f32>(egui::Id::unique("library-sidebar-width")))
            .unwrap()
    };
    let away = egui::pos2(1000., 600.);
    let mut first = render(&mut e, away, false);
    let out = render(&mut e, away, false);
    assert_eq!(width(), 52.);
    assert!(
        crate::color_widgets::tests::text_rect(&out.shapes, "Tags").is_none(),
        "collapsed rail contains only icons"
    );
    if let Ok(path) = std::env::var("CLIPCRAFT_RICH_UI_DIR") {
        dump_meshes(&ctx, &path, "LibraryCollapsed", first, out);
    }
    first = render(&mut e, egui::pos2(20., 200.), false);
    let warm = render(&mut e, egui::pos2(20., 200.), false);
    first.0.textures_delta.append(warm.0.textures_delta.clone());
    assert!(
        width() > 52. && width() < 280.,
        "opening has an intermediate width"
    );
    for _ in 0..5 {
        let _ = render(&mut e, egui::pos2(20., 200.), false);
    }
    assert_eq!(width(), 280.);
    first.0.textures_delta.clear();
    for (id, patches) in deltas.borrow().iter() {
        for patch in patches {
            first.0.textures_delta.push(*id, patch.clone());
        }
    }
    let out = render(&mut e, egui::pos2(20., 200.), false);
    let has_tag = crate::color_widgets::tests::text_rect(&out.shapes, "Business  (2)").is_some();
    if let Ok(path) = std::env::var("CLIPCRAFT_RICH_UI_DIR") {
        dump_meshes(&ctx, &path, "LibraryExpanded", first, out);
    }
    assert!(has_tag);
    let pin = ctx
        .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique("library-pin")))
        .unwrap();
    let _ = render(&mut e, pin.center(), true);
    assert!(e.quick_controls.library_pinned());
    let _ = render(&mut e, away, false);
    assert_eq!(width(), 280.);
    let _ = render(&mut e, pin.center(), true);
    assert!(!e.quick_controls.library_pinned());
    for _ in 0..12 {
        let _ = render(&mut e, away, false);
    }
    assert_eq!(width(), 52.);
}
#[test]
fn ctrl_wheel_zooms_tiles_without_reloading_cached_master_or_scrolling() {
    let root = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.history = HistoryPanel::isolated(root.path());
    let entry = e
        .history
        .store
        .as_mut()
        .unwrap()
        .create(
            &capture_core::Document::blank(1200, 800).unwrap(),
            "Crisp",
            Origin::Canvas,
        )
        .unwrap();
    let frame = |e: &mut Editor, delta: f32, pos: egui::Pos2| {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if delta != 0. {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                delta: egui::vec2(0., delta),
                modifiers: egui::Modifiers::CTRL,
            });
        }
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200., 760.),
                )),
                events,
                ..Default::default()
            },
            |ui| e.show_library(ui),
        ))
    };
    let pointer = egui::pos2(500., 150.);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while e
        .history
        .textures
        .get(&entry.id)
        .and_then(|t| t.as_ref())
        .is_none()
    {
        frame(&mut e, 0., pointer);
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let texture = e.history.textures[&entry.id].as_ref().unwrap().id();
    for _ in 0..15 {
        frame(&mut e, 1., pointer);
    }
    assert_eq!(e.history.library.zoom, 200.);
    assert_eq!(
        e.history.textures[&entry.id].as_ref().unwrap().id(),
        texture
    );
    assert!(e.history.thumbnail_jobs.is_empty());
    frame(&mut e, -1., egui::pos2(20., 150.));
    assert_eq!(
        e.history.library.zoom, 200.,
        "sidebar wheel is not Library zoom"
    );
    for _ in 0..20 {
        frame(&mut e, -1., pointer);
    }
    assert_eq!(e.history.library.zoom, 50.);
    assert_eq!(
        e.history.textures[&entry.id].as_ref().unwrap().id(),
        texture
    );
}
