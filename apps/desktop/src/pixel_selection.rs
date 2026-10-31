//! Transient masks share Crop's bounded drag/handle interaction.
//! Pixel edits use the engine; copying is read-only and Cut copies before clearing.
use crate::{Command, Editor, canvas_view::Transform, crop::Crop, egui};
use capture_core::{
    pixel_mask::{PixelMask, ShapeMask},
    pixels::PixelRect,
};
mod shapes;
use shapes::Choice;

pub(super) struct PixelSelection {
    region: Crop,
    pending: Option<PixelAction>,
    choice: Choice,
    shape: ShapeMask,
    inverted: bool,
    path: Vec<[f32; 2]>,
    drawing: bool,
}

impl PixelSelection {
    pub(super) fn awaiting_action(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn dragging(&self) -> bool {
        self.region.dragging() || (self.drawing && self.choice == Choice::Freehand)
    }
    pub(super) fn cursor(
        &self,
        pos: egui::Pos2,
        transform: Transform,
        size: [u32; 2],
    ) -> Option<egui::CursorIcon> {
        if self.pending.is_some() {
            Some(egui::CursorIcon::Default)
        } else if self.drawing || (self.path.is_empty() && self.choice.is_path()) {
            Some(egui::CursorIcon::Crosshair)
        } else {
            self.region.cursor(pos, transform, size)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PixelAction {
    Cut,
    Clear,
    Lift,
}

fn apply_action(
    session: &mut crate::Session,
    mask: impl Into<PixelMask>,
    action: PixelAction,
    copy: impl FnOnce(&image::RgbaImage) -> Result<(), String>,
) -> Result<(), String> {
    let mask = mask.into();
    if action == PixelAction::Cut {
        let pixels = session
            .document
            .copy_masked_pixels(&mask)
            .map_err(|e| e.to_string())?;
        copy(&pixels)?;
    }
    session
        .execute(if action == PixelAction::Lift {
            Command::LiftMaskedPixels { mask }
        } else {
            Command::ClearMaskedPixels { mask }
        })
        .map_err(|e| e.to_string())
}

impl Editor {
    pub(super) fn begin_pixel_selection(&mut self) {
        self.cancel_gesture();
        self.fill_tool.active = false;
        self.select_only(None);
        self.tool = None;
        self.pixel_selection = Some(PixelSelection {
            region: Crop::empty(),
            pending: None,
            choice: Choice::Rectangle,
            shape: ShapeMask::Rectangle,
            inverted: false,
            path: Vec::new(),
            drawing: false,
        });
        self.status = "Selection: drag a rectangle, move it or resize its handles. Copy preserves editable objects.".into();
    }

    pub(super) fn selected_pixels(&self) -> Option<PixelRect> {
        self.selected_mask()
            .map(|m| m.output_bounds([self.session.document.width, self.session.document.height]))
    }
    fn selected_mask(&self) -> Option<PixelMask> {
        let selection = self.pixel_selection.as_ref()?;
        if selection.dragging() || (selection.choice.is_path() && selection.path.is_empty()) {
            return None;
        }
        let [x, y, r, b] = selection.region.bounds;
        let mask = PixelMask {
            bounds: PixelRect {
                x,
                y,
                width: r.saturating_sub(x),
                height: b.saturating_sub(y),
            },
            shape: selection.shape.clone(),
            inverted: selection.inverted,
        };
        mask.validate([self.session.document.width, self.session.document.height])
            .ok()?;
        Some(mask)
    }

    pub(super) fn copy_selected_pixels(&mut self) {
        let Some(mask) = self.selected_mask() else {
            return;
        };
        let rect = mask.output_bounds([self.session.document.width, self.session.document.height]);
        self.status = match self
            .session
            .document
            .copy_masked_pixels(&mask)
            .map_err(|e| e.to_string())
            .and_then(|image| crate::clipboard::copy_image(&image))
        {
            Ok(()) => format!(
                "Copied selection: {} × {} px. Image unchanged.",
                rect.width, rect.height
            ),
            Err(error) => error,
        };
    }

    pub(super) fn request_pixel_action(&mut self, action: PixelAction) {
        if self.selected_pixels().is_none() {
            return;
        }
        if self.session.document.annotations.is_empty() {
            self.apply_pixel_action(action);
        } else if let Some(selection) = self.pixel_selection.as_mut() {
            selection.pending = Some(action);
            self.status =
                "Review flattening in Selection Properties before changing pixels.".into();
        }
    }

    fn apply_pixel_action(&mut self, action: PixelAction) {
        let Some(mask) = self.selected_mask() else {
            return;
        };
        let next_id = self.session.next_id();
        match apply_action(
            &mut self.session,
            mask,
            action,
            crate::clipboard::copy_image,
        ) {
            Ok(()) => {
                self.needs_render = true;
                self.select_only(None);
                self.status = match action {
                    PixelAction::Cut => "Cut selected pixels to clipboard. Undo restores the image and objects.",
                    PixelAction::Clear => "Selection cleared to transparency. Undo restores the image and objects.",
                    PixelAction::Lift => "Selected pixels are an editable image. Drag to move; Undo restores each operation.",
                }.into();
                if action == PixelAction::Lift {
                    if self
                        .session
                        .document
                        .annotations
                        .iter()
                        .any(|a| a.id == next_id)
                    {
                        self.pixel_selection = None;
                        self.select_only(Some(next_id));
                        self.tool = Some(crate::Kind::Image);
                    } else {
                        self.status = "The selection contains no visible pixels.".into();
                    }
                }
            }
            Err(error) => self.status = format!("{error}. Pixels have not been changed."),
        }
        if let Some(selection) = self.pixel_selection.as_mut() {
            selection.pending = None;
        }
    }

    pub(super) fn pixel_selection_properties(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Selection Properties",
            "Rectangle/Ellipse: drag an area. Polygon: click vertices, then double-click or Enter to close. Freehand: drag a loop. Move or resize the finished mask with its bounds handles. Inverse selects outside the shape. Copy preserves objects; Cut, Clear and Move pixels flatten after review. Move pixels lifts the mask as an editable image: drag/resize/rotate it normally. Each edit has one Undo. Original pixels remain in the project.",
        );
        crate::shell::badge(ui, self.shell.badges, "EDIT-043");
        self.mask_shape_controls(ui);
        let pending = self.pixel_selection.as_ref().and_then(|s| s.pending);
        if let Some(action) = pending {
            ui.separator();
            ui.label(format!(
                "This merges {} editable objects into the image.",
                self.session.document.annotations.len()
            ));
            if selection_button(
                ui,
                match action {
                    PixelAction::Cut => "Flatten and cut",
                    PixelAction::Clear => "Flatten and clear",
                    PixelAction::Lift => "Flatten and move pixels",
                },
                match action {
                    PixelAction::Cut => "Cut",
                    PixelAction::Clear => "Clear",
                    PixelAction::Lift => "Move",
                },
                "Apply the selected pixel edit",
            )
            .clicked()
            {
                self.apply_pixel_action(action);
            }
            if selection_button(
                ui,
                "Cancel pixel edit",
                "Cancel",
                "Keep the image unchanged",
            )
            .clicked()
            {
                if let Some(selection) = self.pixel_selection.as_mut() {
                    selection.pending = None;
                }
                self.status = "Pixel edit cancelled. Image unchanged.".into();
            }
            return;
        }
        let selected = self.selected_pixels();
        if let Some(rect) = selected {
            ui.label(format!(
                "{} × {} px at {}, {}",
                rect.width, rect.height, rect.x, rect.y
            ));
        } else {
            ui.label("No pixels selected");
        }
        ui.add_enabled_ui(selected.is_some(), |ui| {
            if selection_button(
                ui,
                "Copy selection",
                "Copy",
                "Copy selected pixels · Ctrl+C",
            )
            .clicked()
            {
                self.copy_selected_pixels();
            }
            if selection_button(ui, "Cut selection", "Cut", "Cut selected pixels · Ctrl+X")
                .clicked()
            {
                self.request_pixel_action(PixelAction::Cut);
            }
            if selection_button(
                ui,
                "Clear selection",
                "Clear",
                "Clear selected pixels to transparency · Delete",
            )
            .clicked()
            {
                self.request_pixel_action(PixelAction::Clear);
            }
            if selection_button(
                ui,
                "Move pixels",
                "Move",
                "Lift selected pixels into an editable image object",
            )
            .clicked()
            {
                self.request_pixel_action(PixelAction::Lift);
            }
        });
        if selection_button(
            ui,
            "Select entire image",
            "Select all",
            "Select all image pixels",
        )
        .clicked()
        {
            let size = [self.session.document.width, self.session.document.height];
            if let Some(selection) = self.pixel_selection.as_mut() {
                selection.region.bounds = [0, 0, size[0], size[1]];
                selection.choice = Choice::Rectangle;
                selection.shape = ShapeMask::Rectangle;
                selection.inverted = false;
                selection.path.clear();
                selection.drawing = false;
            }
        }
        if selection_button(
            ui,
            "Deselect pixels",
            "Deselect",
            "Remove the selection rectangle",
        )
        .clicked()
        {
            let choice = self
                .pixel_selection
                .as_ref()
                .map(|s| s.choice)
                .unwrap_or(Choice::Rectangle);
            self.begin_pixel_selection();
            self.set_mask_shape(choice);
        }
        ui.separator();
        crate::shell::badge(ui, self.shell.badges, "EDIT-015");
    }

    pub(super) fn pixel_selection_canvas(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        transform: Transform,
        interactive: bool,
    ) {
        let size = [self.session.document.width, self.session.document.height];
        let Some(selection) = self.pixel_selection.as_mut() else {
            return;
        };
        let cancel = selection.interact_shape(
            ui,
            response,
            transform,
            size,
            interactive && selection.pending.is_none(),
            self.view.panning,
        );
        selection.paint_shape(ui, transform, size);
        if cancel {
            self.begin_pixel_selection();
        }
    }

    pub(super) fn pixel_selection_shortcuts(&mut self, ctx: &egui::Context) {
        if self.selected_pixels().is_none()
            || self
                .pixel_selection
                .as_ref()
                .is_some_and(|s| s.pending.is_some())
            || self.pending.is_some()
            || self.new_image.is_some()
            || self.resize_image.is_some()
            || self.resize_canvas.is_some()
            || self.image_transform.is_some()
            || self.capture_result.is_some()
            || self.text_draft.is_some()
            || self.shell.help
            || !self.shell.canvas_enabled()
            || ctx.egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        let (copy, cut, clear) = ctx.input_mut(|input| {
            let mut copy =
                crate::shortcuts::consume_exact(input, egui::Modifiers::COMMAND, egui::Key::C);
            let mut cut =
                crate::shortcuts::consume_exact(input, egui::Modifiers::COMMAND, egui::Key::X);
            input.events.retain(|e| match e {
                egui::Event::Copy => {
                    copy = true;
                    false
                }
                egui::Event::Cut => {
                    cut = true;
                    false
                }
                _ => true,
            });
            (
                copy,
                cut,
                crate::shortcuts::consume_exact(input, egui::Modifiers::NONE, egui::Key::Delete),
            )
        });
        if cut {
            self.request_pixel_action(PixelAction::Cut);
        } else if clear {
            self.request_pixel_action(PixelAction::Clear);
        } else if copy {
            self.copy_selected_pixels();
        }
    }
}

/// Shared sizing and original vector marks keep every Selection action readable.
fn selection_button(ui: &mut egui::Ui, label: &str, icon: &str, tip: &str) -> egui::Response {
    let r = ui.add_sized([ui.available_width(), 30.0], egui::Button::new(""));
    let c = r.rect.left_center() + egui::vec2(18.0, 0.0);
    let enabled = ui.is_enabled();
    let ink = if enabled {
        egui::Color32::from_gray(235)
    } else {
        egui::Color32::from_gray(112)
    };
    let accent = if enabled {
        egui::Color32::from_rgb(103, 174, 234)
    } else {
        ink
    };
    match icon {
        "Copy" | "Cut" => crate::ribbon::draw_icon(
            ui.painter(),
            c,
            icon,
            if icon == "Cut" && enabled {
                egui::Color32::from_rgb(238, 155, 89)
            } else {
                accent
            },
        ),
        "Move" => crate::icons::draw_scaled(
            ui.painter(),
            egui::Rect::from_center_size(c, egui::vec2(22., 22.)),
            "Move",
            accent,
            0.65,
        ),
        "Clear" => crate::icons::draw_scaled(
            ui.painter(),
            egui::Rect::from_center_size(c, egui::vec2(22.0, 22.0)),
            "Eraser",
            ink,
            0.65,
        ),
        "Cancel" => {
            let stroke = egui::Stroke::new(1.6, ink);
            ui.painter()
                .line_segment([c - egui::vec2(6.0, 6.0), c + egui::vec2(6.0, 6.0)], stroke);
            ui.painter().line_segment(
                [c - egui::vec2(6.0, -6.0), c + egui::vec2(6.0, -6.0)],
                stroke,
            );
        }
        _ => {
            let rect = egui::Rect::from_center_size(c, egui::vec2(18.0, 16.0));
            for (a, b) in [
                (rect.left_top(), rect.right_top()),
                (rect.right_top(), rect.right_bottom()),
                (rect.right_bottom(), rect.left_bottom()),
                (rect.left_bottom(), rect.left_top()),
            ] {
                ui.painter().extend(egui::Shape::dashed_line(
                    &[a, b],
                    egui::Stroke::new(1.4, accent),
                    3.0,
                    2.0,
                ));
            }
            if icon == "Select all" {
                ui.painter()
                    .rect_filled(rect.shrink(4.0), 0.0, accent.gamma_multiply(0.65));
            } else {
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_top()],
                    egui::Stroke::new(1.8, ink),
                );
            }
        }
    }
    ui.painter().text(
        r.rect.left_center() + egui::vec2(38.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        ink,
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("selection-action", label)), r.rect));
    r.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> crate::Document {
        serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap()
    }

    #[test]
    fn cut_copies_exact_pixels_before_clear_and_failure_preserves_objects_history_and_redo() {
        let doc = fixture();
        let original = serde_json::to_vec(&doc).unwrap();
        let rect = PixelRect {
            x: 100,
            y: 60,
            width: 400,
            height: 200,
        };
        let expected = doc.copy_pixels(rect).unwrap();
        let mut session = crate::Session::new(doc).unwrap();
        apply_action(&mut session, rect, PixelAction::Cut, |pixels| {
            assert_eq!(pixels, &expected);
            Err("Clipboard busy".into())
        })
        .unwrap_err();
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
        assert!(!session.can_undo());
        apply_action(&mut session, rect, PixelAction::Cut, |pixels| {
            assert_eq!(pixels, &expected);
            Ok(())
        })
        .unwrap();
        assert!(session.document.annotations.is_empty());
        assert!(
            session
                .document
                .copy_pixels(rect)
                .unwrap()
                .pixels()
                .all(|p| p.0 == [0; 4])
        );
        session.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
        assert!(!session.can_undo());
        apply_action(&mut session, rect, PixelAction::Cut, |_| {
            Err("Clipboard busy".into())
        })
        .unwrap_err();
        assert!(session.can_redo());
        session.execute(Command::Redo).unwrap();
        assert!(session.document.annotations.is_empty());
    }

    #[test]
    fn reverse_selection_live_handles_review_cancel_clear_and_undo() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        editor.session = crate::Session::new(fixture()).unwrap();
        let original = serde_json::to_vec(&editor.session.document).unwrap();
        editor.begin_pixel_selection();
        let frame = |editor: &mut Editor, events| {
            let mut viewport = egui::Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    viewport = ui.available_rect_before_wrap();
                    editor.show_canvas(ui);
                },
            );
            output.textures_delta.clear();
            editor
                .view
                .transform(viewport, egui::vec2(640.0, 400.0), ctx.pixels_per_point())
        };
        let transform = frame(&mut editor, vec![]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let drag = |editor: &mut Editor, start, end| {
            let start = transform.screen_point(start);
            let end = transform.screen_point(end);
            frame(
                editor,
                vec![Event::PointerMoved(start), button(start, true)],
            );
            frame(editor, vec![Event::PointerMoved(end)]);
            assert!(editor.pixel_selection.as_ref().unwrap().region.dragging());
            assert_eq!(
                serde_json::to_vec(&editor.session.document).unwrap(),
                original
            );
            frame(editor, vec![button(end, false)]);
        };
        drag(&mut editor, [500.0, 260.0], [100.0, 60.0]);
        assert_eq!(
            editor.selected_pixels(),
            Some(PixelRect {
                x: 100,
                y: 60,
                width: 400,
                height: 200
            })
        );
        drag(&mut editor, [300.0, 160.0], [320.0, 180.0]);
        assert_eq!(
            editor.selected_pixels(),
            Some(PixelRect {
                x: 120,
                y: 80,
                width: 400,
                height: 200
            })
        );
        drag(&mut editor, [520.0, 280.0], [600.0, 350.0]);
        assert_eq!(
            editor.selected_pixels(),
            Some(PixelRect {
                x: 120,
                y: 80,
                width: 480,
                height: 270
            })
        );
        assert!(!editor.session.can_undo());
        editor.request_pixel_action(PixelAction::Clear);
        assert!(editor.pixel_selection.as_ref().unwrap().pending.is_some());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        editor.pixel_selection.as_mut().unwrap().pending = None;
        assert!(!editor.session.can_undo());
        editor.request_pixel_action(PixelAction::Clear);
        editor.apply_pixel_action(PixelAction::Clear);
        assert!(editor.session.document.annotations.is_empty());
        editor.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        assert!(editor.pixel_selection.is_none());
    }
    #[test]
    fn ellipse_polygon_freehand_inverse_and_move_pixels_use_real_pointer_workflows() {
        use egui::{Event, Modifiers, PointerButton};
        for choice in [Choice::Ellipse, Choice::Polygon, Choice::Freehand] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.session = crate::Session::new(fixture()).unwrap();
            e.begin_pixel_selection();
            e.set_mask_shape(choice);
            let original = serde_json::to_vec(&e.session.document).unwrap();
            let frame = |e: &mut Editor, events| {
                let mut viewport = egui::Rect::NOTHING;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 600.),
                        )),
                        events,
                        focused: true,
                        ..Default::default()
                    },
                    |ui| {
                        viewport = ui.available_rect_before_wrap();
                        e.show_canvas(ui);
                    },
                );
                output.textures_delta.clear();
                e.view
                    .transform(viewport, egui::vec2(640., 400.), ctx.pixels_per_point())
            };
            let t = frame(&mut e, vec![]);
            frame(&mut e, vec![]);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            let points = if choice == Choice::Ellipse {
                vec![[100., 60.], [260., 180.]]
            } else {
                vec![[100., 60.], [260., 70.], [220., 180.], [120., 160.]]
            };
            if choice == Choice::Polygon {
                for p in &points {
                    let p = t.screen_point(*p);
                    for pressed in [true, false] {
                        frame(&mut e, vec![Event::PointerMoved(p), button(p, pressed)]);
                    }
                }
                assert!(e.selected_pixels().is_none());
                frame(
                    &mut e,
                    vec![Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
            } else {
                let p = t.screen_point(points[0]);
                frame(&mut e, vec![Event::PointerMoved(p), button(p, true)]);
                for p in &points[1..] {
                    frame(&mut e, vec![Event::PointerMoved(t.screen_point(*p))]);
                    assert!(e.selected_pixels().is_none());
                }
                frame(
                    &mut e,
                    vec![button(t.screen_point(*points.last().unwrap()), false)],
                );
            }
            let mask = e
                .selected_mask()
                .unwrap_or_else(|| panic!("No mask for {choice:?}"));
            assert!(!e.session.can_undo());
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
            assert_eq!(
                mask.bounds,
                PixelRect {
                    x: 100,
                    y: 60,
                    width: 160,
                    height: 120
                }
            );
            let copied = e.session.document.copy_masked_pixels(&mask).unwrap();
            assert_eq!(
                copied.get_pixel(copied.width() - 1, copied.height() - 1)[3],
                0
            );
            if let Ok(root) = std::env::var("CLIPCRAFT_MASK_UI_DIR") {
                let v = egui::Context::default();
                crate::shell::configure(&v);
                e.shell.badges = false;
                let draw = |e: &mut Editor| {
                    crate::property_layout::tests::PaintedOutput(v.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(262., 700.),
                            )),
                            focused: true,
                            ..Default::default()
                        },
                        |ui| e.pixel_selection_properties(ui),
                    ))
                };
                let first = draw(&mut e);
                let last = draw(&mut e);
                crate::property_layout::tests::dump_meshes(
                    &v,
                    &root,
                    &format!("{choice:?}"),
                    first,
                    last,
                );
            }
            e.pixel_selection.as_mut().unwrap().inverted = true;
            let inverse = e.selected_mask().unwrap();
            assert_eq!(
                inverse.output_bounds([640, 400]),
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 640,
                    height: 400
                }
            );
            apply_action(&mut e.session, inverse, PixelAction::Cut, |_| {
                Err("Clipboard busy".into())
            })
            .unwrap_err();
            assert!(!e.session.can_undo());
            e.pixel_selection.as_mut().unwrap().inverted = false;
            e.request_pixel_action(PixelAction::Lift);
            assert!(e.pixel_selection.as_ref().unwrap().awaiting_action());
            assert!(!e.session.can_undo());
            // Click the actual reviewed action; no OS clipboard adapter is invoked by Lift.
            let pane = |e: &mut Editor, events| {
                let mut o = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(262., 700.),
                        )),
                        events,
                        focused: true,
                        ..Default::default()
                    },
                    |ui| e.pixel_selection_properties(ui),
                );
                o.textures_delta.clear();
            };
            pane(&mut e, vec![]);
            pane(&mut e, vec![]);
            let p = ctx
                .data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::unique((
                        "selection-action",
                        "Flatten and move pixels",
                    )))
                })
                .unwrap()
                .center();
            for pressed in [true, false] {
                pane(&mut e, vec![Event::PointerMoved(p), button(p, pressed)]);
            }
            assert!(e.pixel_selection.is_none());
            assert_eq!(e.selected_annotation().unwrap().kind, crate::Kind::Image);
            assert_eq!(e.session.document.annotations.len(), 1);
            let lifted = serde_json::to_vec(&e.session.document).unwrap();
            let t = frame(&mut e, vec![]);
            frame(&mut e, vec![]);
            // Drag the body clear of the center crop-placement handle.
            let from = t.screen_point([150., 105.]);
            let to = t.screen_point([185., 135.]);
            frame(&mut e, vec![Event::PointerMoved(from), button(from, true)]);
            frame(&mut e, vec![Event::PointerMoved(to)]);
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), lifted);
            assert_ne!(
                e.selected_annotation().unwrap().start,
                [100., 60.],
                "choice={choice:?} drag={} selected={:?} from={from:?} to={to:?} status={}",
                e.edit_drag.is_some(),
                e.selected,
                e.status
            );
            frame(&mut e, vec![button(to, false)]);
            assert_eq!(e.session.document.annotations[0].start, [135., 90.]);
            e.execute(Command::Undo);
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), lifted);
            e.execute(Command::Undo);
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
            assert!(!e.session.can_undo());
        }
    }
}
