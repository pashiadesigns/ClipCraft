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
