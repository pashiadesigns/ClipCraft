use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};
use capture_core::{Document, Session};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
struct Dropped(PathBuf);
impl egui::DroppedFile for Dropped {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}
fn editor(ctx: &egui::Context) -> Editor {
    crate::shell::configure(ctx);
    let mut e = crate::canvas::tests::editor(ctx);
    let mut d = Document::canvas(500, 400, [255; 4]).unwrap();
    let mut a = capture_core::templates::placeholder()
        .unwrap()
        .annotation(1);
    a.kind = Kind::Ellipse;
    a.start = [100., 80.];
    a.end = [350., 320.];
    a.color = [20, 90, 200, 255];
    d.annotations.push(a);
    d.promote_features();
    e.session = Session::new(d).unwrap();
    e.tool = None;
    e.needs_render = true;
    e
}
fn evidence(ctx: &egui::Context, mut output: PaintedOutput) -> PaintedOutput {
    let key = egui::Id::unique("frame-page-test-textures");
    type Updates = Vec<(egui::TextureId, egui::epaint::ImageDelta)>;
    let mut all = ctx.data(|d| d.get_temp::<Updates>(key)).unwrap_or_default();
    for (id, deltas) in &output.textures_delta.set {
        for delta in deltas {
            all.push((*id, delta.clone()));
        }
    }
    ctx.data_mut(|d| d.insert_temp(key, all.clone()));
    output.0.textures_delta.clear();
    for (id, delta) in all {
        output.0.textures_delta.push(id, delta);
    }
    output
}
fn frame(ctx: &egui::Context, e: &mut Editor, input: egui::RawInput) -> PaintedOutput {
    if e.needs_render {
        e.refresh_canvas_display(ctx);
    }
    evidence(
        ctx,
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(650., 500.),
                )),
                focused: true,
                ..input
            },
            |ui| e.show_canvas(ui),
        )),
    )
}
fn pointer(p: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: p,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn external_image_hover_drop_retains_frame_then_undo_and_invalid_drop_are_atomic() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    frame(&ctx, &mut e, Default::default());
    frame(&ctx, &mut e, Default::default());
    let p = e
        .view
        .current_transform()
        .unwrap()
        .screen_point([200., 180.]);
    let before = e.session.document.annotations[0].clone();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("drop.png");
    image::RgbaImage::from_pixel(80, 120, image::Rgba([200, 80, 40, 255]))
        .save(&path)
        .unwrap();
    let hover = |path: PathBuf| egui::RawInput {
        events: vec![egui::Event::PointerMoved(p)],
        hovered_files: vec![egui::HoveredFile {
            path: Some(path),
            ..Default::default()
        }],
        ..Default::default()
    };
    let first = frame(&ctx, &mut e, hover(path.clone()));
    let output = frame(&ctx, &mut e, hover(path.clone()));
    assert!(
        crate::color_widgets::tests::text_rect(&output.shapes, "Drop image to fill frame")
            .is_some()
    );
    assert_eq!(e.session.document.annotations[0], before);
    if let Ok(dir) = std::env::var("CLIPCRAFT_FRAME_PAGES_UI") {
        dump_meshes(&ctx, &dir, "frame-drop-target", first, output);
    }
    frame(
        &ctx,
        &mut e,
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(p)],
            dropped_files: vec![Arc::new(Dropped(path))],
            ..Default::default()
        },
    );
    let a = &e.session.document.annotations[0];
    assert!(!a.image.as_ref().unwrap().is_placeholder());
    assert_eq!(a.kind, Kind::Ellipse);
    assert_eq!(a.start, before.start);
    assert_eq!(a.end, before.end);
    assert_eq!(a.color, before.color);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations[0], before);
    frame(
        &ctx,
        &mut e,
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(p)],
            dropped_files: vec![Arc::new(Dropped(temp.path().join("missing.png")))],
            ..Default::default()
        },
    );
    assert_eq!(e.session.document.annotations[0], before);
    let mut state = e.session.document.object_state(1);
    state.locked = true;
    e.session.document.object_states.insert(1, state);
    let locked = frame(&ctx, &mut e, hover(temp.path().join("drop.png")));
    assert!(
        crate::color_widgets::tests::text_rect(&locked.shapes, "Drop image to fill frame")
            .is_none()
    );
}

#[test]
fn single_click_shape_double_click_image_properties_and_frame_styles() {
    let ctx = egui::Context::default();
    let mut e = editor(&ctx);
    frame(&ctx, &mut e, Default::default());
    frame(&ctx, &mut e, Default::default());
    // Keep away from the central Choose Image action.
    let p = e
        .view
        .current_transform()
        .unwrap()
        .screen_point([200., 120.]);
    for (time, pressed) in [(1., true), (1.03, false)] {
        frame(
            &ctx,
            &mut e,
            egui::RawInput {
                time: Some(time),
                events: vec![egui::Event::PointerMoved(p), pointer(p, pressed)],
                ..Default::default()
            },
        );
    }
    assert_eq!(e.selected, Some(1));
    assert!(!content_mode(&ctx, 1));
    for (time, pressed) in [(1.10, true), (1.13, false)] {
        frame(
            &ctx,
            &mut e,
            egui::RawInput {
                time: Some(time),
                events: vec![egui::Event::PointerMoved(p), pointer(p, pressed)],
                ..Default::default()
            },
        );
    }
    assert!(content_mode(&ctx, 1));
    let show = |e: &mut Editor| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(262., 800.),
                )),
                ..Default::default()
            },
            |ui| e.properties(ui),
        ))
    };
    let first = evidence(&ctx, show(&mut e));
    let output = evidence(&ctx, show(&mut e));
    for label in [
        "Image frame styles",
        "Square",
        "Rounded",
        "Circle",
        "Replace image…",
    ] {
        assert!(
            crate::color_widgets::tests::text_rect(&output.shapes, label).is_some(),
            "{label}"
        );
    }
    if let Ok(dir) = std::env::var("CLIPCRAFT_FRAME_PAGES_UI") {
        dump_meshes(&ctx, &dir, "shape-frame-properties", first, output);
    }
}
