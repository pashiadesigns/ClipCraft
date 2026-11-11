use super::*;
use capture_core::{
    Document, Session,
    clipboard_payload::{ClipboardPayload, ImagePayload},
};
use image::RgbaImage;

#[derive(Default)]
struct MemoryClipboard {
    bytes: Option<Vec<u8>>,
    fallback: Option<RgbaImage>,
    fail: bool,
    reads: usize,
    writes: usize,
}
impl Transfer for MemoryClipboard {
    fn read(&mut self) -> Result<ClipboardPayload, String> {
        self.reads += 1;
        if self.fail {
            return Err("clipboard busy".into());
        }
        ClipboardPayload::decode(
            self.bytes
                .as_ref()
                .ok_or("clipboard replaced by external text")?,
        )
        .map_err(|e| e.to_string())
    }
    fn write(&mut self, p: &ClipboardPayload, image: &RgbaImage) -> Result<(), String> {
        self.writes += 1;
        if self.fail {
            return Err("clipboard busy".into());
        }
        self.bytes = Some(p.encode().map_err(|e| e.to_string())?);
        self.fallback = Some(image.clone());
        Ok(())
    }
}
fn setup(ctx: &egui::Context) -> Editor {
    let mut e = crate::canvas::tests::editor(ctx);
    e.session = Session::new(Document::canvas(100, 80, [20, 30, 40, 255]).unwrap()).unwrap();
    e.select_only(None);
    e
}
fn image_clipboard() -> MemoryClipboard {
    MemoryClipboard {
        bytes: Some(
            ClipboardPayload {
                schema_version: 1,
                content: Content::Image(
                    ImagePayload::from_rgba(12, 8, vec![255; 12 * 8 * 4]).unwrap(),
                ),
            }
            .encode()
            .unwrap(),
        ),
        ..Default::default()
    }
}

#[test]
fn focused_guide_screenshot_copies_only_selected_child_and_preserves_crop() {
    use std::sync::Arc;
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    let raster = ImagePayload::from_rgba(20, 12, [27, 83, 151, 255].repeat(20 * 12))
        .unwrap()
        .image;
    let bitmap = capture_core::bitmap::Bitmap::new(raster)
        .unwrap()
        .with_focus([0.2, 0.3])
        .unwrap();
    let mut screenshot = Arc::new(bitmap).annotation(1);
    screenshot.start = [10., 10.];
    screenshot.end = [30., 22.];
    screenshot.group = Some(1);
    let mut other = screenshot.clone();
    other.id = 2;
    other.start = [50., 40.];
    other.end = [70., 52.];
    e.session
        .execute(Command::AddMany {
            annotations: vec![screenshot.clone(), other],
        })
        .unwrap();
    assert_eq!(
        e.session.document.version,
        capture_core::MAX_PROJECT_VERSION
    );
    e.enter_group_child(1);
    let mut clipboard = MemoryClipboard::default();
    e.copy_objects_with(false, &mut clipboard);
    let payload = clipboard.read().unwrap();
    let Content::Objects(payload) = payload.content else {
        panic!()
    };
    assert_eq!(payload.objects.len(), 1);
    assert_eq!(payload.objects[0].group, None);
    assert_eq!(payload.objects[0].image, screenshot.image);
    assert_eq!(clipboard.fallback.as_ref().unwrap().dimensions(), (20, 12));
    assert_eq!(
        clipboard.fallback.as_ref().unwrap().get_pixel(5, 5).0,
        [27, 83, 151, 255]
    );
    e.paste_canvas_with(&mut clipboard);
    assert_eq!(e.session.document.annotations.len(), 3);
    assert_eq!(e.selected_objects().len(), 1);
    e.execute(Command::Undo);
    e.editing_group = None;
    e.select_only(Some(1));
    e.copy_objects_with(false, &mut clipboard);
    let Content::Objects(payload) = clipboard.read().unwrap().content else {
        panic!()
    };
    assert_eq!(
        payload.objects.len(),
        2,
        "explicit group selection still copies both"
    );
}

#[test]
fn external_image_and_editable_objects_share_paste_selection_cut_and_one_undo() {
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    let mut clipboard = image_clipboard();
    let before = serde_json::to_vec(&e.session.document).unwrap();
    e.paste_canvas_with(&mut clipboard);
    assert_eq!(clipboard.reads, 1);
    assert_eq!(e.selected_objects().len(), 1);
    assert_eq!(e.selected_objects()[0].kind, Kind::Image);
    assert_eq!(e.selected_objects()[0].start, [44., 36.]);
    assert_eq!(e.tool, None);
    let original = e.session.document.annotations[0].clone();
    let first = e.session.document.render().unwrap();
    e.copy_objects_with(false, &mut clipboard);
    assert_eq!(clipboard.writes, 1);
    assert_eq!(clipboard.fallback.as_ref().unwrap().dimensions(), (12, 8));
    e.paste_canvas_with(&mut clipboard);
    assert_eq!(e.session.document.annotations.len(), 2);
    assert_ne!(e.selected, Some(original.id));
    assert_eq!(e.selected_objects()[0].image, original.image);
    assert_eq!(e.session.document.render().unwrap(), first);
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations.len(), 1);
    e.select_only(Some(original.id));
    clipboard.fail = true;
    e.copy_objects_with(true, &mut clipboard);
    assert_eq!(e.session.document.annotations.len(), 1);
    assert!(e.status.contains("busy"));
    clipboard.fail = false;
    e.copy_objects_with(true, &mut clipboard);
    assert!(e.session.document.annotations.is_empty());
    e.execute(Command::Undo);
    assert_eq!(e.session.document.annotations[0], original);
    e.execute(Command::Undo);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
}

#[test]
fn copy_selection_is_portable_and_never_reuses_objects_after_external_replacement() {
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    let mut fixture: Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    fixture.source_png = Document::canvas(fixture.width, fixture.height, [17, 90, 23, 255])
        .unwrap()
        .source_png;
    e.session = Session::new(fixture).unwrap();
    let ids: Vec<_> = e
        .session
        .document
        .annotations
        .iter()
        .filter(|a| matches!(a.kind, Kind::Callout | Kind::Rectangle | Kind::Step))
        .map(|a| a.id)
        .collect();
    e.select_only(ids.first().copied());
    e.extra_selected.extend(ids.iter().skip(1));
    let source = e.session.document.source_png.clone();
    let mut clipboard = MemoryClipboard::default();
    e.copy_objects_with(false, &mut clipboard);
    assert!(clipboard.bytes.is_some());
    assert!(!String::from_utf8_lossy(clipboard.bytes.as_ref().unwrap()).contains(&source));
    let count = e.session.document.annotations.len();
    e.paste_canvas_with(&mut clipboard);
    assert_eq!(e.selected_objects().len(), ids.len());
    assert_eq!(e.session.document.annotations.len(), count + ids.len());
    let current = serde_json::to_vec(&e.session.document).unwrap();
    clipboard.bytes = None;
    e.paste_canvas_with(&mut clipboard);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), current);
    assert!(e.status.contains("replaced"));
    clipboard.bytes = Some(b"bad payload".to_vec());
    e.paste_canvas_with(&mut clipboard);
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), current);
}

#[test]
fn clipboard_shortcuts_deduplicate_events_and_respect_text_modal_and_library_ownership() {
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    let mut action = None;
    let key = |modifiers| egui::Event::Key {
        key: egui::Key::V,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![
                key(egui::Modifiers::COMMAND),
                egui::Event::Paste(String::new()),
            ],
            ..Default::default()
        },
        |ui| {
            assert!(e.clipboard_owns_input(ui.ctx()));
            action = ui.input_mut(shortcut);
            assert_eq!(ui.input_mut(shortcut), None);
        },
    );
    output.textures_delta.clear();
    assert_eq!(action, Some(Action::Paste));
    let mut output = ctx.run_ui(
        egui::RawInput {
            events: vec![key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT)],
            ..Default::default()
        },
        |ui| assert_eq!(ui.input_mut(shortcut), None),
    );
    output.textures_delta.clear();
    e.shell.library = true;
    assert!(!e.clipboard_owns_input(&ctx));
    e.shell.library = false;
    e.pending = Some(crate::FileAction::New);
    assert!(!e.clipboard_owns_input(&ctx));
    e.pending = None;
    let fixture: Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    e.session = Session::new(fixture).unwrap();
    let id = e
        .session
        .document
        .annotations
        .iter()
        .find(|a| a.kind == Kind::Text)
        .unwrap()
        .id;
    e.select_only(Some(id));
    e.begin_text_edit();
    assert!(e.text_draft.is_some());
    assert!(!e.clipboard_owns_input(&ctx));
}

#[test]
fn pasted_image_properties_and_render_are_visible_without_unusable_drawing_presets() {
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = setup(&ctx);
    e.paste_canvas_with(&mut image_clipboard());
    let frame = |e: &mut Editor| {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(262., 640.),
                )),
                ..Default::default()
            },
            |ui| {
                e.property_tabs(ui);
                e.properties(ui);
            },
        ))
    };
    let first = frame(&mut e);
    let output = frame(&mut e);
    let text = |needle| crate::color_widgets::tests::text_rect(&output.shapes, needle);
    assert!(text("Opacity").is_some());
    assert!(text("Properties").is_some());
    assert!(text("Presets").is_none());
    if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_CLIPBOARD_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        e.session
            .document
            .save(&std::path::Path::new(&dir).join("pasted.capture.json"))
            .unwrap();
        e.session
            .document
            .export_png(&std::path::Path::new(&dir).join("pasted.png"))
            .unwrap();
        dump_meshes(&ctx, &dir, "ImageProperties", first, output);
    }
}

#[test]
fn empty_object_selection_copies_full_composite_raster_without_changing_document() {
    let ctx = egui::Context::default();
    let mut e = setup(&ctx);
    e.select_only(None);
    let before = e.session.document.clone();
    let expected = before.render().unwrap();
    let mut written = None;
    e.copy_all_with(|doc| {
        written = Some(doc.render().unwrap());
        Ok(())
    });
    assert_eq!(written, Some(expected));
    assert_eq!(
        serde_json::to_value(&e.session.document).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
}
