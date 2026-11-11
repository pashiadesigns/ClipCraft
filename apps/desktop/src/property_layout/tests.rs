use super::*;
use egui::{Event, Modifiers, PointerButton, Rect};

pub(crate) struct PaintedOutput(pub(crate) egui::FullOutput);
type LinePreviewTextures = Vec<(egui::TextureId, Vec<egui::epaint::ImageDelta>)>;

#[test]
fn slider_rails_end_together_and_numeric_fields_keep_matching_dimensions() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let render = || {
        PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(262., 400.),
                )),
                ..Default::default()
            },
            |ui| {
                for (label, mut value, suffix) in [
                    ("Opacity", 100., "%"),
                    ("Text stroke size", 2.5, ""),
                    ("Corners", 4096., ""),
                    ("Spacing", -2.5, " px"),
                    ("Angle", 225., "°"),
                ] {
                    slider(
                        ui,
                        label,
                        egui::Slider::new(&mut value, -10.0..=4096.0).suffix(suffix),
                    );
                }
            },
        ))
    };
    render();
    let output = render();
    let rects = |color| {
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill == color => Some(r.rect),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let fields = rects(Color32::from_rgb(65, 71, 74));
    let rails = rects(Color32::from_rgb(70, 77, 80));
    assert_eq!(fields.len(), 5);
    assert_eq!(rails.len(), 5);
    for (field, rail) in fields.iter().zip(&rails) {
        assert!((field.right() - fields[0].right()).abs() < 0.1);
        assert!((field.width() - fields[0].width()).abs() < 0.1);
        assert!((field.height() - fields[0].height()).abs() < 0.1);
        assert!((rail.right() - rails[0].right()).abs() < 0.1);
        assert!(rail.right() < field.left());
    }
    assert!(
        rails[1].width() < rails[0].width(),
        "long labels may shorten only the rail's left end"
    );
}

#[test]
fn endpoint_menu_and_size_slider_apply_live_to_line_batch_and_inherit() {
    use capture_core::line_style::Endpoint;
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.tool = Some(Kind::Line);
    let a = e.new_annotation(Kind::Line, [100., 100.], [340., 100.]);
    let mut b = a.clone();
    b.id = a.id + 1;
    b.start[1] += 80.;
    b.end[1] += 80.;
    e.session.document.annotations = vec![a.clone(), b.clone()];
    e.select_only(Some(a.id));
    e.click_selection(Some(b.id), true);
    let original = e.session.document.annotations.clone();
    let (_, mut first) = frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let menu = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique(("endpoint-menu", "End")))
            .unwrap()
    });
    click(&ctx, &mut e, menu);
    frame(&ctx, &mut e, vec![]);
    let choice = ctx.data(|d| {
        d.get_temp::<Rect>(egui::Id::unique(("endpoint-choice", "End", "Triangle")))
            .unwrap()
    });
    click(&ctx, &mut e, choice);
    frame(&ctx, &mut e, vec![]);
    e.settle_property_preview(&ctx);
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.line_style.is_some_and(|s| s.end == Endpoint::Triangle))
    );
    e.execute(capture_core::Command::Undo);
    assert_eq!(e.session.document.annotations, original);
    assert!(!e.session.can_undo());
    click(&ctx, &mut e, menu);
    frame(&ctx, &mut e, vec![]);
    click(&ctx, &mut e, choice);
    frame(&ctx, &mut e, vec![]);
    e.settle_property_preview(&ctx);
    let selected = e.session.document.annotations.clone();
    let rail = ctx.data(|d| {
        d.get_temp::<(Rect, Rect)>(egui::Id::unique(("property-slider", "End size")))
            .unwrap()
            .1
    });
    let from = rail.center();
    let to = from + egui::vec2(15., 0.);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        pressed,
        button: PointerButton::Primary,
        modifiers: Modifiers::NONE,
    };
    frame(
        &ctx,
        &mut e,
        vec![Event::PointerMoved(from), button(from, true)],
    );
    frame(&ctx, &mut e, vec![Event::PointerMoved(to)]);
    assert_eq!(e.session.document.annotations, selected);
    assert!(
        e.preview_document()
            .annotations
            .iter()
            .all(|a| a.line_style.unwrap().end_size > 3.)
    );
    frame(&ctx, &mut e, vec![button(to, false)]);
    e.settle_property_preview(&ctx);
    let (_, output) = frame(&ctx, &mut e, vec![]);
    if let Ok(root) = std::env::var("CLIPDESIGNER_LINE_UI_DIR") {
        let updates: LinePreviewTextures = ctx.data(|d| {
            d.get_temp(egui::Id::unique("line-preview-textures"))
                .unwrap()
        });
        first.0.textures_delta.set.clear();
        for (id, updates) in updates {
            first
                .0
                .textures_delta
                .set
                .entry(id)
                .or_default()
                .extend(updates);
        }
        dump_meshes(&ctx, &root, "line-properties", first, output);
    }
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.line_style.unwrap().end_size > 3.)
    );
    e.execute(capture_core::Command::Undo);
    assert_eq!(e.session.document.annotations, selected);
    assert_eq!(
        e.new_annotation(Kind::Line, [10., 10.], [70., 70.])
            .line_style
            .unwrap()
            .end,
        Endpoint::Triangle
    );
}
#[test]
fn step_outline_toggle_and_width_drag_apply_live_to_batch_with_one_undo() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let mut doc = capture_core::Document::blank(640, 400).unwrap();
    for (id, x, color) in [
        (1, 120.0, [190, 30, 40, 255]),
        (2, 300.0, [30, 60, 180, 255]),
    ] {
        doc.annotations.push(
            capture_core::steps::marker(id, [x, 100.0], 64, color, (id as u32, false), [640, 400])
                .unwrap(),
        );
    }
    e.session = capture_core::Session::new(doc.clone()).unwrap();
    e.tool = Some(Kind::Step);
    e.select_only(Some(1));
    e.click_selection(Some(2), true);
    frame(&ctx, &mut e, vec![]);
    let (_, out) = frame(&ctx, &mut e, vec![]);
    let checkbox = crate::color_widgets::tests::text_rect(&out.shapes, "Show outline").unwrap();
    click(&ctx, &mut e, checkbox);
    e.settle_property_preview(&ctx);
    assert!(
        e.session
            .document
            .annotations
            .iter()
            .all(|a| a.step_outline.is_some())
    );
    assert_eq!(
        e.session.document.version,
        capture_core::MAX_PROJECT_VERSION
    );
    e.execute(capture_core::Command::Undo);
    assert_eq!(e.session.document.annotations, doc.annotations);
    assert!(!e.session.can_undo());
    e.execute(capture_core::Command::Redo);
    let before = e.session.document.clone();
    frame(&ctx, &mut e, vec![]);
    frame(&ctx, &mut e, vec![]);
    let (_, control) = ctx.data(|d| {
        d.get_temp::<(Rect, Rect)>(egui::Id::unique(("property-slider", "Outline size")))
            .unwrap()
    });
    let start = egui::pos2(control.left() + 40.0, control.center().y);
    let end = start + egui::vec2(35.0, 0.0);
    let events = |p, pressed| {
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            },
        ]
    };
    frame(&ctx, &mut e, events(start, true));
    frame(&ctx, &mut e, vec![Event::PointerMoved(end)]);
    assert_eq!(e.session.document.annotations, before.annotations);
    let preview = e.preview_document();
    assert_eq!(preview.version, capture_core::MAX_PROJECT_VERSION);
    for (a, b) in preview.annotations.iter().zip(&before.annotations) {
        assert!(a.step_outline.as_ref().unwrap().width > b.step_outline.as_ref().unwrap().width);
        assert_eq!(a.color, b.color);
        assert_eq!(a.text, b.text);
        assert_eq!(a.start, b.start);
    }
    frame(&ctx, &mut e, events(end, false));
    e.settle_property_preview(&ctx);
    assert_eq!(e.session.document.annotations, preview.annotations);
    let expected = preview.annotations[1].step_outline.clone();
    e.select_only(None);
    e.add_step([450.0, 250.0]);
    assert_eq!(
        e.session.document.annotations.last().unwrap().step_outline,
        expected
    );
    e.execute(capture_core::Command::Undo);
    e.select_only(Some(2));
    if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_UI_DIR") {
        let paint_ctx = egui::Context::default();
        crate::shell::configure(&paint_ctx);
        let mut painted = crate::canvas::tests::editor(&paint_ctx);
        painted.session = capture_core::Session::new(preview.clone()).unwrap();
        painted.tool = Some(Kind::Step);
        painted.select_only(Some(2));
        let (_, first) = frame(&paint_ctx, &mut painted, vec![]);
        let (_, output) = frame(&paint_ctx, &mut painted, vec![]);
        dump_meshes(&paint_ctx, &root, "StepOutline", first, output);
        std::fs::write(
            std::path::Path::new(&root).join("step-outline.capture.json"),
            serde_json::to_vec_pretty(&preview).unwrap(),
        )
        .unwrap();
    }
    e.execute(capture_core::Command::Undo);
    assert_eq!(e.session.document.annotations, before.annotations);
}
impl std::ops::Deref for PaintedOutput {
    type Target = egui::FullOutput;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Drop for PaintedOutput {
    fn drop(&mut self) {
        self.0.textures_delta.clear();
    }
}
fn frame(ctx: &egui::Context, e: &mut Editor, events: Vec<Event>) -> ([Rect; 2], PaintedOutput) {
    let mut tabs = [Rect::NOTHING; 2];
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(262.0, 980.0),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            tabs = e.property_tabs(ui).map(|r| r.rect);
            egui::ScrollArea::vertical().show(ui, |ui| e.properties(ui));
        },
    );
    if std::env::var_os("CLIPDESIGNER_LINE_UI_DIR").is_some() {
        ctx.data_mut(|d| {
            let key = egui::Id::unique("line-preview-textures");
            let mut textures = d.get_temp::<LinePreviewTextures>(key).unwrap_or_default();
            textures.extend(
                output
                    .textures_delta
                    .set
                    .iter()
                    .map(|(id, updates)| (*id, updates.to_vec())),
            );
            d.insert_temp(key, textures);
        });
    }
    (tabs, PaintedOutput(output))
}
#[test]
fn corner_slider_numbers_cannot_widen_the_properties_content() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut widths = vec![];
    for mut radius in [0.0, 9.99, 10.0, 99.999, 100.0, 999.999, 4096.0] {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(262.0, 300.0),
                )),
                ..Default::default()
            },
            |ui| {
                let available = ui.available_width();
                slider(ui, "Corners", egui::Slider::new(&mut radius, 0.0..=4096.0));
                let (row, control) = ctx.data(|d| {
                    d.get_temp::<(Rect, Rect)>(egui::Id::unique(("property-slider", "Corners")))
                        .unwrap()
                });
                assert!(
                    row.width() <= available + 0.1,
                    "{radius}: {} > {available}",
                    row.width()
                );
                widths.push(control.width());
            },
        );
        output.textures_delta.clear();
    }
    assert!(
        widths.iter().all(|w| (w - widths[0]).abs() < 0.1),
        "{widths:?}"
    );
}
fn click(ctx: &egui::Context, e: &mut Editor, rect: Rect) {
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
        );
    }
}
fn texts(output: &egui::FullOutput) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(shapes) => {
                for s in shapes {
                    collect(s, out);
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    for shape in &output.shapes {
        collect(&shape.shape, &mut out);
    }
    out
}

#[test]
fn tabs_separate_object_list_from_tool_settings_without_document_changes() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.tool = Some(Kind::Step);
    e.add_step([90.0, 90.0]);
    let before = serde_json::to_vec(&e.session.document).unwrap();
    frame(&ctx, &mut e, vec![]);
    let (tabs, output) = frame(&ctx, &mut e, vec![]);
    assert!((tabs[0].width() - tabs[1].width()).abs() < 0.1);
    assert!((tabs[0].right() - tabs[1].left()).abs() < 0.1);
    assert!(texts(&output).iter().any(|s| s == "Presets"));
    assert!(!texts(&output).iter().any(|s| s == "Step 1"));
    click(&ctx, &mut e, tabs[1]);
    assert_eq!(e.shell.property_tab, PropertyTab::Objects);
    let (tabs, output) = frame(&ctx, &mut e, vec![]);
    assert!(texts(&output).iter().any(|s| s == "Step 1"));
    assert!(!texts(&output).iter().any(|s| s == "Presets"));
    click(&ctx, &mut e, tabs[0]);
    assert_eq!(e.shell.property_tab, PropertyTab::Tool);
    assert_eq!(e.selected, Some(1));
    assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    e.shell.property_tab = PropertyTab::Objects;
    e.choose_tool(2);
    assert_eq!(e.shell.property_tab, PropertyTab::Tool);
}

#[test]
fn property_groups_remain_passive_and_fit_the_reference_width() {
    if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_HELP_DIR") {
        dump_help_panels(&root);
    }
    let fixture: capture_core::Document = serde_json::from_str(include_str!(
        "../../../../crates/core/tests/fixtures/properties.capture.json"
    ))
    .unwrap();
    for kind in [
        Kind::Arrow,
        Kind::Text,
        Kind::Callout,
        Kind::Rectangle,
        Kind::Step,
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = capture_core::Session::new(fixture.clone()).unwrap();
        e.tool = Some(kind);
        e.selected = Some(
            fixture
                .annotations
                .iter()
                .find(|a| a.kind == kind)
                .unwrap()
                .id,
        );
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let (_, first) = frame(&ctx, &mut e, vec![]);
        let (_, output) = frame(&ctx, &mut e, vec![]);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert!(e.property_preview.is_none());
        assert!(!e.session.can_undo());
        let labels = texts(&output);
        assert!(labels.iter().any(|s| s == "Properties"));
        assert!(labels.iter().any(|s| s == "Presets"));
        // Compare painted glyph placement, not just allocated row rectangles:
        // egui's sized labels otherwise center each title independently.
        let mut left = None;
        for label in [
            "Thickness",
            "Width",
            "Opacity",
            "Fill opacity",
            "Corners",
            "Shadow",
            "Rotation",
            "Curve",
            "Text stroke",
            "Show outline",
            "Marker size",
        ] {
            if let Some(rect) = crate::color_widgets::tests::text_rect(&output.shapes, label) {
                let margin = *left.get_or_insert(rect.left());
                assert!(
                    (rect.left() - margin).abs() < 0.1,
                    "{kind:?} {label}: {} != {margin}",
                    rect.left()
                );
            }
        }
        assert!(!labels.iter().any(|s| matches!(
            s.as_str(),
            "Edit text" | "Add text" | "Pointer position" | "Position" | "Font size" | "Font"
        )));
        // Optional headless paint data; never opens an application or captures a screen.
        if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_UI_DIR") {
            dump_meshes(&ctx, &root, &format!("{kind:?}"), first, output);
        }
    }
}

// Optional production-pane snapshots for visual review; no desktop or clipboard.
fn dump_help_panels(root: &str) {
    for (index, tool) in crate::shell::TOOLS.iter().enumerate() {
        if tool.name == "Crop" {
            continue; // Crop uses its canvas overlay, not a property pane.
        }
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.select_only(None);
        e.choose_tool(index);
        let (_, first) = frame(&ctx, &mut e, vec![]);
        let (_, output) = frame(&ctx, &mut e, vec![]);
        dump_meshes(&ctx, root, tool.name, first, output);
    }
    // Hover a real header help target, with a wider screen for its popup.
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    ctx.style_mut_of(egui::Theme::Dark, |s| s.interaction.tooltip_delay = 0.0);
    let mut e = crate::canvas::tests::editor(&ctx);
    e.tool = Some(crate::Kind::Rectangle);
    let mut first = None;
    let mut pos = None;
    for n in 0..4 {
        let mut output = PaintedOutput(
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 980.0),
                    )),
                    events: pos
                        .map(|p| vec![Event::PointerMoved(p)])
                        .unwrap_or_default(),
                    time: Some(n as f64),
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(246.0);
                    e.property_tabs(ui);
                    e.properties(ui);
                },
            ),
        );
        if n == 0 {
            pos = ctx
                .data(|d| d.get_temp::<Rect>(egui::Id::unique(("property-help", "Properties"))))
                .map(|r| r.center());
            first = Some(output);
        } else if n == 3 {
            assert!(
                texts(&output)
                    .iter()
                    .any(|t| t.starts_with("Drag to select pixels."))
            );
            dump_meshes(&ctx, root, "SelectionHelp", first.take().unwrap(), output);
        } else {
            first
                .as_mut()
                .unwrap()
                .0
                .textures_delta
                .append(std::mem::take(&mut output.0.textures_delta));
        }
    }
}

#[test]
fn selected_step_size_slider_previews_batch_while_held_and_undoes_once() {
    let ctx = egui::Context::default();
    crate::shell::configure(&ctx);
    let mut e = crate::canvas::tests::editor(&ctx);
    let mut doc = capture_core::Document::blank(640, 400).unwrap();
    doc.annotations = vec![
        capture_core::steps::marker(
            1,
            [150.0, 120.0],
            48,
            [180, 20, 40, 255],
            (8, false),
            [640, 400],
        )
        .unwrap(),
        capture_core::steps::marker(
            2,
            [310.0, 120.0],
            64,
            [30, 90, 180, 255],
            (9, false),
            [640, 400],
        )
        .unwrap(),
    ];
    e.session = capture_core::Session::new(doc.clone()).unwrap();
    e.tool = Some(Kind::Step);
    e.select_only(Some(1));
    e.click_selection(Some(2), true);
    frame(&ctx, &mut e, vec![]);
    let (_, out) = frame(&ctx, &mut e, vec![]);
    fn label(s: &egui::Shape) -> Option<Rect> {
        match s {
            egui::Shape::Text(t) if t.galley.text() == "Marker size" => {
                Some(t.galley.rect.translate(t.pos.to_vec2()))
            }
            egui::Shape::Vec(v) => v.iter().find_map(label),
            _ => None,
        }
    }
    let label = out.shapes.iter().find_map(|s| label(&s.shape)).unwrap();
    let pos = egui::pos2(label.left() + 120.0, label.center().y);
    let events = |pos, pressed| {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
        ]
    };
    frame(&ctx, &mut e, events(pos, true));
    let drag = pos + egui::vec2(24.0, 0.0);
    frame(&ctx, &mut e, vec![Event::PointerMoved(drag)]);
    assert_eq!(
        serde_json::to_value(&e.session.document).unwrap(),
        serde_json::to_value(&doc).unwrap(),
        "held slider must not commit"
    );
    assert!(!e.session.can_undo());
    let preview = e.preview_document();
    assert_ne!(
        preview.annotations, doc.annotations,
        "actual slider must resize visible objects"
    );
    let sizes = preview
        .annotations
        .iter()
        .map(|a| a.end[0] - a.start[0])
        .collect::<Vec<_>>();
    assert!((sizes[0] - sizes[1]).abs() < 0.01);
    for (a, b) in preview.annotations.iter().zip(&doc.annotations) {
        // The label keeps its text and grows with the marker (BJ10).
        let (ta, tb) = (a.text.as_ref().unwrap(), b.text.as_ref().unwrap());
        assert_eq!(ta.content, tb.content);
        assert!(ta.size_points > tb.size_points);
        assert_eq!(a.color, b.color);
    }
    frame(&ctx, &mut e, events(drag, false));
    e.settle_property_preview(&ctx);
    assert_eq!(e.session.document.annotations, preview.annotations);
    let reopened: capture_core::Document =
        serde_json::from_slice(&serde_json::to_vec(&e.session.document).unwrap()).unwrap();
    assert_eq!(reopened.render().unwrap(), preview.render().unwrap());
    e.execute(capture_core::Command::Undo);
    assert_eq!(
        serde_json::to_value(&e.session.document).unwrap(),
        serde_json::to_value(&doc).unwrap()
    );
    assert!(!e.session.can_undo());
}
pub(crate) fn dump_meshes(
    ctx: &egui::Context,
    root: &str,
    name: &str,
    first: PaintedOutput,
    output: PaintedOutput,
) {
    let root = std::path::Path::new(root).join(name);
    std::fs::create_dir_all(&root).unwrap();
    let mut textures = std::collections::HashMap::<egui::TextureId, image::RgbaImage>::new();
    for (id, deltas) in first
        .textures_delta
        .set
        .iter()
        .chain(output.textures_delta.set.iter())
    {
        for delta in deltas {
            let egui::ImageData::Color(image) = &delta.image;
            let bytes = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let patch =
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, bytes)
                    .unwrap();
            if let Some([x, y]) = delta.pos {
                image::imageops::replace(textures.get_mut(id).unwrap(), &patch, x as i64, y as i64);
            } else {
                textures.insert(*id, patch);
            }
        }
    }
    for (id, texture) in textures {
        texture.save(root.join(format!("{id:?}.png"))).unwrap();
    }
    let meshes: Vec<_> = ctx.tessellate(output.shapes.clone(), output.pixels_per_point).into_iter().filter_map(|p| {
        let egui::epaint::Primitive::Mesh(m) = p.primitive else { return None; };
        Some(serde_json::json!({"clip": [p.clip_rect.min.x,p.clip_rect.min.y,p.clip_rect.max.x,p.clip_rect.max.y], "texture":format!("{:?}.png", m.texture_id), "indices":m.indices,"vertices":m.vertices.iter().map(|v| serde_json::json!([v.pos.x,v.pos.y,v.uv.x,v.uv.y,v.color.to_array()])).collect::<Vec<_>>() }))
    }).collect();
    std::fs::write(
        root.join("meshes.json"),
        serde_json::to_vec(&meshes).unwrap(),
    )
    .unwrap();
}
