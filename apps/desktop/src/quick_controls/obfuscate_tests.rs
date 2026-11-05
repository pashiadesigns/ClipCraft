use super::*;
use crate::Kind;
use egui::{Event, Modifiers, PointerButton};

#[test]
fn blur_pixelate_family_clicks_convert_batches_remember_choices_and_saved_styles() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let dir = tempfile::tempdir().unwrap();
    e.quick_controls.store = Some(Store::open(&dir.path().join("preferences")).unwrap());
    e.presets.templates =
        Some(capture_core::object_styles::Store::open(&dir.path().join("styles")).unwrap());
    for (id, kind) in [(1, Kind::Blur), (2, Kind::Pixelate), (3, Kind::Redact)] {
        let mut object = e.new_annotation(
            kind,
            [id as f32 * 100.0, 100.0],
            [id as f32 * 100.0 + 60.0, 160.0],
        );
        object.id = id;
        object.width = 4;
        e.session.document.annotations.push(object);
    }
    let original = e.session.document.annotations.clone();
    e.tool = Some(Kind::Blur);
    e.select_only(Some(1));
    e.click_selection(Some(2), true);
    let mut saved = capture_core::object_styles::presets(Kind::Pixelate)[3].clone();
    saved.name = "My effect".into();
    e.presets
        .templates
        .as_mut()
        .unwrap()
        .save(saved.clone())
        .unwrap();
    let frame = |e: &mut Editor, events| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 520.0),
                )),
                ..Default::default()
            },
            |ui| e.object_templates(ui, e.tool.unwrap()),
        );
        out.textures_delta.clear();
    };
    let tile = |kind: Kind, label: &str| {
        ctx.data(|d| {
            d.get_temp::<(egui::Rect, bool, bool)>(egui::Id::unique((
                "style-tile",
                format!("{kind:?}"),
                label,
            )))
            .unwrap()
        })
    };
    let click = |e: &mut Editor, pos| {
        for pressed in [true, false] {
            frame(
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
        frame(e, vec![]);
    };
    frame(&mut e, vec![]);
    frame(&mut e, vec![]);
    assert!(
        tile(Kind::Pixelate, "My effect").2,
        "compatible saved effect visible above both families"
    );
    click(&mut e, tile(Kind::Blur, "Blur").0.center());
    assert!(tile(Kind::Blur, "Blur").1);
    assert!(
        e.selected_objects()
            .iter()
            .all(|a| a.kind == Kind::Blur && a.width == 4)
    );
    assert_eq!(e.session.document.annotations[2], original[2]);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
    frame(&mut e, vec![]);
    let dropdown = ctx.data(|d| {
        d.get_temp::<egui::Rect>(egui::Id::unique(("split-dropdown", "Pixelate variations")))
            .unwrap()
    });
    click(&mut e, dropdown.center());
    click(&mut e, tile(Kind::Pixelate, "Intensity 20").0.center());
    assert!(!egui::Popup::is_any_open(&ctx));
    assert_eq!(e.tool, Some(Kind::Pixelate));
    assert!(tile(Kind::Pixelate, "Pixelate").1);
    assert_eq!(
        e.quick_controls.style_choice("pixelate"),
        Some("Intensity 20")
    );
    for (a, b) in original[..2].iter().zip(e.selected_objects()) {
        assert_eq!((a.id, a.start, a.end), (b.id, b.start, b.end));
        assert_eq!((b.kind, b.width), (Kind::Pixelate, 20));
    }
    let path = dir.path().join("effects.capture.json");
    e.session.document.save(&path).unwrap();
    assert_eq!(
        capture_core::Document::load(&path)
            .unwrap()
            .render()
            .unwrap(),
        e.session.document.render().unwrap()
    );
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
    // Repeating the family button uses the chosen intensity, not its first tile.
    frame(&mut e, vec![]);
    click(&mut e, tile(Kind::Pixelate, "Pixelate").0.center());
    assert!(e.selected_objects().iter().all(|a| a.width == 20));
    e.execute(Command::Undo);
    e.tool = Some(Kind::Blur);
    frame(&mut e, vec![]);
    click(&mut e, tile(Kind::Pixelate, "My effect").0.center());
    assert!(saved.matches(&e.selected_objects()));
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    e.select_only(None);
    frame(&mut e, vec![]);
    click(&mut e, tile(Kind::Pixelate, "Pixelate").0.center());
    let preferences = Store::open(&dir.path().join("preferences"))
        .unwrap()
        .read()
        .unwrap();
    let mut reopened = crate::canvas::tests::editor(&ctx);
    reopened.quick_controls.prefs = preferences;
    reopened.tool = Some(Kind::Pixelate);
    reopened.restore_drawing_defaults(reopened.tool);
    let next = reopened.new_annotation(Kind::Pixelate, [10.0, 10.0], [70.0, 70.0]);
    assert_eq!(next.width, 20);
    assert_eq!(
        reopened.quick_controls.style_choice("pixelate"),
        Some("Intensity 20")
    );
    frame(&mut reopened, vec![]);
    frame(&mut reopened, vec![]);
    assert!(tile(Kind::Pixelate, "Pixelate").1);
    assert!(!tile(Kind::Blur, "Blur").1);
    assert_eq!(e.session.document.annotations, original);
    e.select_only(Some(3));
    frame(&mut e, vec![]);
    assert!(tile(Kind::Blur, "Blur").2 && tile(Kind::Pixelate, "Pixelate").2);
    assert_eq!(e.session.document.annotations[2], original[2]);
}
