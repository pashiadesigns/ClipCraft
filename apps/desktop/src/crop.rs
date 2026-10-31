use crate::{Command, Editor, canvas_view::Transform, egui};

pub(super) struct Crop {
    // Exclusive right/bottom edges in image pixels.
    pub(super) bounds: [u32; 4],
    drag: Option<Drag>,
}

struct Drag {
    original: [u32; 4],
    origin: [i32; 2],
    mode: Mode,
}
#[derive(Clone, Copy)]
enum Mode {
    New,
    Move,
    Resize(usize),
}

impl Crop {
    fn new(size: [u32; 2]) -> Self {
        Self {
            bounds: [0, 0, size[0], size[1]],
            drag: None,
        }
    }
    pub(super) fn empty() -> Self {
        Self {
            bounds: [0; 4],
            drag: None,
        }
    }
    pub(super) fn dragging(&self) -> bool {
        self.drag.is_some()
    }
    fn mode_at(&self, pos: egui::Pos2, transform: Transform, size: [u32; 2]) -> Mode {
        let nonempty = self.bounds[2] > self.bounds[0] && self.bounds[3] > self.bounds[1];
        self.points()
            .iter()
            .position(|p| nonempty && transform.screen_point(*p).distance(pos) <= 8.0)
            .map(Mode::Resize)
            .unwrap_or_else(|| {
                let point = transform.image_point(pos);
                if nonempty
                    && self.bounds != [0, 0, size[0], size[1]]
                    && point[0] >= self.bounds[0] as f32
                    && point[0] <= self.bounds[2] as f32
                    && point[1] >= self.bounds[1] as f32
                    && point[1] <= self.bounds[3] as f32
                {
                    Mode::Move
                } else {
                    Mode::New
                }
            })
    }
    pub(super) fn cursor(
        &self,
        pos: egui::Pos2,
        transform: Transform,
        size: [u32; 2],
    ) -> Option<egui::CursorIcon> {
        use egui::CursorIcon as Cursor;
        let mode = self
            .drag
            .as_ref()
            .map_or_else(|| self.mode_at(pos, transform, size), |d| d.mode);
        Some(match mode {
            Mode::New => return None,
            Mode::Move => {
                if self.dragging() {
                    Cursor::Grabbing
                } else {
                    Cursor::Grab
                }
            }
            Mode::Resize(0 | 4) => Cursor::ResizeNwSe,
            Mode::Resize(2 | 6) => Cursor::ResizeNeSw,
            Mode::Resize(1 | 5) => Cursor::ResizeVertical,
            Mode::Resize(_) => Cursor::ResizeHorizontal,
        })
    }
    pub(super) fn interact(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        transform: Transform,
        size: [u32; 2],
        interactive: bool,
        panning: bool,
    ) -> bool {
        let visible = transform.image.intersect(transform.viewport);
        let point = |p: egui::Pos2| transform.image_point(p).map(|v| v.round() as i32);
        let mut cancel = false;
        if interactive {
            cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if !ui.input(|i| i.focused) {
                if let Some(drag) = self.drag.take() {
                    self.bounds = drag.original;
                }
            } else if !panning && !ui.input(|i| i.key_down(egui::Key::Space)) {
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(origin) = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|p| visible.expand(6.0).contains(*p))
                {
                    let mode = self.mode_at(origin, transform, size);
                    let p = point(origin);
                    self.drag = Some(Drag {
                        original: self.bounds,
                        origin: [p[0].clamp(0, size[0] as i32), p[1].clamp(0, size[1] as i32)],
                        mode,
                    });
                }
                if self.drag.is_some()
                    && let Some(p) = response.interact_pointer_pos()
                {
                    self.update_drag(point(p), size);
                    ui.ctx().request_repaint();
                }
                if response.drag_stopped_by(egui::PointerButton::Primary) {
                    self.drag = None;
                }
            }
        }
        cancel
    }
    pub(super) fn paint(
        &self,
        ui: &mut egui::Ui,
        transform: Transform,
        thirds: bool,
    ) -> egui::Rect {
        let visible = transform.image.intersect(transform.viewport);
        let rect = |bounds: [u32; 4]| {
            egui::Rect::from_min_max(
                transform.screen_point([bounds[0] as f32, bounds[1] as f32]),
                transform.screen_point([bounds[2] as f32, bounds[3] as f32]),
            )
        };
        if self.bounds[2] <= self.bounds[0] || self.bounds[3] <= self.bounds[1] {
            return rect(self.bounds);
        }
        let selected = rect(self.bounds);
        let p = ui.painter().with_clip_rect(visible);
        for shade in [
            egui::Rect::from_min_max(visible.min, egui::pos2(visible.right(), selected.top())),
            egui::Rect::from_min_max(egui::pos2(visible.left(), selected.bottom()), visible.max),
            egui::Rect::from_min_max(
                egui::pos2(visible.left(), selected.top()),
                egui::pos2(selected.left(), selected.bottom()),
            ),
            egui::Rect::from_min_max(
                egui::pos2(selected.right(), selected.top()),
                egui::pos2(visible.right(), selected.bottom()),
            ),
        ] {
            if shade.is_positive() {
                p.rect_filled(shade, 0.0, egui::Color32::from_black_alpha(150));
            }
        }
        let stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        p.rect_stroke(selected, 0.0, stroke, egui::StrokeKind::Inside);
        for fraction in if thirds {
            &[1.0 / 3.0, 2.0 / 3.0][..]
        } else {
            &[][..]
        } {
            let x = selected.left() + selected.width() * fraction;
            let y = selected.top() + selected.height() * fraction;
            p.line_segment(
                [
                    egui::pos2(x, selected.top()),
                    egui::pos2(x, selected.bottom()),
                ],
                stroke,
            );
            p.line_segment(
                [
                    egui::pos2(selected.left(), y),
                    egui::pos2(selected.right(), y),
                ],
                stroke,
            );
        }
        for point in self.points() {
            let handle =
                egui::Rect::from_center_size(transform.screen_point(point), egui::vec2(7.0, 7.0));
            ui.painter().rect_filled(handle, 0.0, egui::Color32::WHITE);
            ui.painter().rect_stroke(
                handle,
                0.0,
                egui::Stroke::new(1.0, egui::Color32::BLACK),
                egui::StrokeKind::Inside,
            );
        }
        selected
    }
    fn points(&self) -> [[f32; 2]; 8] {
        let [l, t, r, b] = self.bounds.map(|v| v as f32);
        [
            [l, t],
            [(l + r) / 2.0, t],
            [r, t],
            [r, (t + b) / 2.0],
            [r, b],
            [(l + r) / 2.0, b],
            [l, b],
            [l, (t + b) / 2.0],
        ]
    }
    fn update_drag(&mut self, p: [i32; 2], size: [u32; 2]) {
        let Some(d) = &self.drag else { return };
        let [l, t, r, b] = d.original.map(|v| v as i32);
        let [w, h] = size.map(|v| v as i32);
        let q = [p[0].clamp(0, w), p[1].clamp(0, h)];
        self.bounds = match d.mode {
            Mode::New => [
                d.origin[0].min(q[0]),
                d.origin[1].min(q[1]),
                d.origin[0].max(q[0]),
                d.origin[1].max(q[1]),
            ],
            Mode::Move => {
                let dx = (q[0] - d.origin[0]).clamp(-l, w - r);
                let dy = (q[1] - d.origin[1]).clamp(-t, h - b);
                [l + dx, t + dy, r + dx, b + dy]
            }
            Mode::Resize(i) => [
                if [0, 6, 7].contains(&i) {
                    q[0].min(r - 1)
                } else {
                    l
                },
                if [0, 1, 2].contains(&i) {
                    q[1].min(b - 1)
                } else {
                    t
                },
                if [2, 3, 4].contains(&i) {
                    q[0].max(l + 1)
                } else {
                    r
                },
                if [4, 5, 6].contains(&i) {
                    q[1].max(t + 1)
                } else {
                    b
                },
            ],
        }
        .map(|v| v as u32);
    }
}

impl Editor {
    pub(super) fn begin_crop(&mut self) {
        self.cancel_gesture();
        self.select_only(None);
        self.tool = None;
        self.crop = Some(Crop::new([
            self.session.document.width,
            self.session.document.height,
        ]));
        self.status="Crop: drag a new rectangle or adjust its handles. Apply to crop; Cancel preserves the image.".into();
    }

    fn apply_crop(&mut self) {
        let Some(crop) = self.crop.as_ref() else {
            return;
        };
        let [x, y, right, bottom] = crop.bounds;
        match self.session.execute(Command::CropImage {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }) {
            Ok(()) => {
                self.crop = None;
                self.select_only(None);
                self.needs_render = true;
                self.view = crate::canvas_view::CanvasView::default();
                self.status =
                    "Cropped image. Undo restores the previous image and editable objects.".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    pub(super) fn crop_canvas(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        transform: Transform,
        interactive: bool,
    ) {
        let size = [self.session.document.width, self.session.document.height];
        let Some(crop) = self.crop.as_mut() else {
            return;
        };
        let mut cancel = crop.interact(
            ui,
            response,
            transform,
            size,
            interactive,
            self.view.panning,
        );
        let selected = crop.paint(ui, transform, true);
        let flatten = !self.session.document.annotations.is_empty();
        let toolbar_width = if flatten { 390.0 } else { 260.0 };
        let pos = egui::pos2(
            (selected.center().x - toolbar_width * 0.5).clamp(
                transform.viewport.left(),
                (transform.viewport.right() - toolbar_width).max(transform.viewport.left()),
            ),
            (selected.bottom() + 12.0)
                .min(transform.viewport.bottom() - 60.0)
                .max(transform.viewport.top()),
        );
        let mut apply = false;
        let bounds = crop.bounds;
        let valid = bounds[2] > bounds[0] && bounds[3] > bounds[1];
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                pos,
                egui::vec2(toolbar_width, 60.0),
            )),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_gray(40))
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(interactive, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "{} × {} px",
                                    bounds[2] - bounds[0],
                                    bounds[3] - bounds[1]
                                ));
                                apply = ui
                                    .add_enabled(
                                        valid,
                                        egui::Button::new(if flatten {
                                            "Flatten and crop"
                                        } else {
                                            "Crop image"
                                        }),
                                    )
                                    .clicked();
                                cancel |= ui.button("Cancel crop").clicked();
                            });
                            if flatten {
                                ui.small("Objects will flatten. Undo restores their editability.");
                            }
                        });
                    });
            },
        );
        if cancel {
            self.crop = None;
            self.status = "Crop canceled. Image unchanged.".into();
        } else if apply {
            self.apply_crop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_navigation_preserves_selection_and_footer_zoom_then_applies_once() {
        use crate::canvas_view::{ZoomAction, ZoomMode};
        use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let original = e.session.document.clone();
        e.view.apply(ZoomAction::Set(ZoomMode::Manual(2.0)), None);
        e.begin_crop();
        let bounds = [100, 60, 500, 260];
        e.crop.as_mut().unwrap().bounds = bounds;
        let frame = |e: &mut Editor, events| {
            let mut buttons = vec![];
            let mut viewport = Rect::NOTHING;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.horizontal(|ui| buttons = e.bottom_zoom_controls(ui));
                    viewport = ui.available_rect_before_wrap();
                    e.show_canvas(ui);
                },
            );
            out.textures_delta.clear();
            (
                e.view
                    .transform(viewport, egui::vec2(640.0, 400.0), ctx.pixels_per_point()),
                buttons,
            )
        };
        let button = |pos, button, pressed| Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        let key = |pressed| Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let (mut before, _) = frame(&mut e, vec![]);
        for pointer_button in [PointerButton::Middle, PointerButton::Primary] {
            let start = egui::pos2(340.0, 210.0);
            let end = start + egui::vec2(24.0, 17.0);
            let mut events = vec![
                Event::PointerMoved(start),
                button(start, pointer_button, true),
            ];
            if pointer_button == PointerButton::Primary {
                events.insert(0, key(true));
            }
            frame(&mut e, events);
            let (after, _) = frame(&mut e, vec![Event::PointerMoved(end)]);
            assert_eq!(after.image.min - before.image.min, egui::vec2(24.0, 17.0));
            frame(&mut e, vec![button(end, pointer_button, false), key(false)]);
            assert_eq!(e.crop.as_ref().unwrap().bounds, bounds);
            assert!(!e.crop.as_ref().unwrap().dragging());
            before = after;
        }
        let anchor = egui::pos2(370.0, 230.0);
        let point = before.image_point(anchor);
        let (zoomed, _) = frame(
            &mut e,
            vec![
                Event::PointerMoved(anchor),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0.0, 100.0),
                    modifiers: Modifiers::CTRL | Modifiers::COMMAND,
                },
            ],
        );
        assert!(zoomed.zoom > before.zoom);
        assert!((zoomed.screen_point(point) - anchor).length() < 0.001);
        assert_eq!(e.crop.as_ref().unwrap().bounds, bounds);
        for name in ["Fit to Screen", "Zoom out", "Zoom in", "Actual size · 100%"] {
            let (_, buttons) = frame(&mut e, vec![]);
            let rect = buttons
                .iter()
                .find(|(label, _)| *label == name)
                .unwrap()
                .1
                .rect;
            for pressed in [true, false] {
                frame(
                    &mut e,
                    vec![
                        Event::PointerMoved(rect.center()),
                        button(rect.center(), PointerButton::Primary, pressed),
                    ],
                );
            }
            assert_eq!(
                e.crop.as_ref().unwrap().bounds,
                bounds,
                "{name} must retain crop"
            );
            assert!(!e.session.can_undo());
        }
        let (view, _) = frame(&mut e, vec![]);
        assert_eq!(view.zoom, 1.0);
        // Existing crop handles still work in the new transform.
        let start = view.screen_point([100.0, 60.0]);
        let end = view.screen_point([80.0, 40.0]);
        frame(
            &mut e,
            vec![
                Event::PointerMoved(start),
                button(start, PointerButton::Primary, true),
            ],
        );
        frame(&mut e, vec![Event::PointerMoved(end)]);
        frame(&mut e, vec![button(end, PointerButton::Primary, false)]);
        assert_eq!(e.crop.as_ref().unwrap().bounds, [80, 40, 500, 260]);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        e.apply_crop();
        assert_eq!(
            [e.session.document.width, e.session.document.height],
            [420, 220]
        );
        e.execute(Command::Undo);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert!(!e.session.can_undo());
    }

    #[test]
    fn crop_handles_and_movement_preserve_positive_bounded_selection() {
        let initial = [20, 30, 140, 100];
        let expected = [
            [10, 15, 140, 100],
            [20, 15, 140, 100],
            [20, 15, 150, 100],
            [20, 30, 150, 100],
            [20, 30, 150, 110],
            [20, 30, 140, 110],
            [10, 30, 140, 110],
            [10, 30, 140, 100],
        ];
        for (i, bounds) in expected.into_iter().enumerate() {
            let mut crop = Crop {
                bounds: initial,
                drag: Some(Drag {
                    original: initial,
                    origin: [0, 0],
                    mode: Mode::Resize(i),
                }),
            };
            let p = [
                if [0, 6, 7].contains(&i) { 10 } else { 150 },
                if [0, 1, 2].contains(&i) { 15 } else { 110 },
            ];
            crop.update_drag(p, [200, 160]);
            assert_eq!(crop.bounds, bounds);
            crop.update_drag([-1000, -1000], [200, 160]);
            assert!(crop.bounds[2] > crop.bounds[0] && crop.bounds[3] > crop.bounds[1]);
            crop.update_drag([1000, 1000], [200, 160]);
            assert!(crop.bounds[2] <= 200 && crop.bounds[3] <= 160);
        }
        let mut crop = Crop {
            bounds: initial,
            drag: Some(Drag {
                original: initial,
                origin: [80, 65],
                mode: Mode::Move,
            }),
        };
        crop.update_drag([1000, 1000], [200, 160]);
        assert_eq!(crop.bounds, [80, 90, 200, 160]);
        crop.update_drag([-1000, -1000], [200, 160]);
        assert_eq!(crop.bounds, [0, 0, 120, 70]);
    }

    #[test]
    fn native_input_crop_preview_reverse_drag_escape_and_one_undo() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let doc: crate::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        editor.session = crate::Session::new(doc.clone()).unwrap();
        let original = serde_json::to_vec(&doc).unwrap();
        editor.begin_crop();
        let frame = |editor: &mut Editor, events, focused| {
            let mut viewport = egui::Rect::NOTHING;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    focused,
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
        let transform = frame(&mut editor, vec![], true);
        let start = transform.screen_point([500.0, 260.0]);
        let end = transform.screen_point([100.0, 60.0]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![Event::PointerMoved(start), button(start, true)],
            true,
        );
        frame(&mut editor, vec![Event::PointerMoved(end)], true);
        assert_eq!(editor.crop.as_ref().unwrap().bounds, [100, 60, 500, 260]);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        frame(&mut editor, vec![button(end, false)], true);
        editor.apply_crop();
        assert_eq!(
            [
                editor.session.document.width,
                editor.session.document.height
            ],
            [400, 200]
        );
        assert!(editor.session.document.annotations.is_empty());
        editor.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        editor.begin_crop();
        frame(
            &mut editor,
            vec![Event::PointerMoved(start), button(start, true)],
            true,
        );
        frame(&mut editor, vec![Event::PointerMoved(end)], true);
        frame(&mut editor, vec![], false);
        assert_eq!(editor.crop.as_ref().unwrap().bounds, [0, 0, 640, 400]);
        assert!(editor.crop.as_ref().unwrap().drag.is_none());
        frame(&mut editor, vec![button(end, false)], true);
        frame(
            &mut editor,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            true,
        );
        assert!(editor.crop.is_none());
        assert!(editor.session.can_redo());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
    }
}
