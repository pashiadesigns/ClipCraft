use crate::tips::Tip as _;
use crate::{Command, Editor, canvas_view::Transform, egui};
pub(super) struct Crop {
    // Exclusive right/bottom edges in image pixels.
    pub(super) bounds: [u32; 4],
    drag: Option<Drag>,
    pub(super) page: usize,
    content: capture_core::crop::CropMode,
    /// A locked width-to-height shape; None drags freely.
    ratio: Option<[u32; 2]>,
}
/// Ratios offered in Crop; None is free.
const RATIOS: [(Option<[u32; 2]>, &str); 6] = [
    (None, "Free"),
    (Some([1, 1]), "1:1 Square"),
    (Some([4, 3]), "4:3"),
    (Some([3, 2]), "3:2"),
    (Some([16, 9]), "16:9"),
    (Some([9, 16]), "9:16"),
];
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
    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            bounds: [0; 4],
            page: 0,
            content: Default::default(),
            ratio: None,
            drag: None,
        }
    }
    fn new(size: [u32; 2]) -> Self {
        Self {
            bounds: [0, 0, size[0], size[1]],
            page: 0,
            content: Default::default(),
            ratio: None,
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
        if !matches!(d.mode, Mode::Move) {
            let keep = match d.mode {
                Mode::New => {
                    let b = self.bounds;
                    [
                        i32::from(d.origin[0] as u32 != b[0]) * 2 - 1,
                        i32::from(d.origin[1] as u32 != b[1]) * 2 - 1,
                    ]
                }
                Mode::Resize(i) => [
                    if [2, 3, 4].contains(&i) {
                        -1
                    } else if [0, 6, 7].contains(&i) {
                        1
                    } else {
                        0
                    },
                    if [4, 5, 6].contains(&i) {
                        -1
                    } else if [0, 1, 2].contains(&i) {
                        1
                    } else {
                        0
                    },
                ],
                Mode::Move => [0, 0],
            };
            self.hold_ratio(keep, size);
        }
    }
    /// Reshapes the crop to the locked ratio. `keep` says which side stays put
    /// on each axis: -1 left or top, 1 right or bottom, 0 the middle.
    fn hold_ratio(&mut self, keep: [i32; 2], size: [u32; 2]) {
        let Some([rw, rh]) = self.ratio else { return };
        let ratio = rw as f32 / rh as f32;
        let [l, t, r, b] = self.bounds.map(|v| v as f32);
        let (w, h) = (r - l, b - t);
        let (mut new_w, mut new_h) = if keep[0] == 0 {
            (h * ratio, h)
        } else if keep[1] == 0 || w / ratio >= h {
            (w, w / ratio)
        } else {
            (h * ratio, h)
        };
        let room = |lo: f32, hi: f32, limit: f32, side: i32| match side {
            -1 => limit - lo,
            1 => hi,
            _ => 2. * ((lo + hi) * 0.5).min(limit - (lo + hi) * 0.5),
        };
        let fit = (room(l, r, size[0] as f32, keep[0]) / new_w)
            .min(room(t, b, size[1] as f32, keep[1]) / new_h)
            .min(1.);
        new_w = (new_w * fit).max(1.);
        new_h = (new_h * fit).max(1.);
        let place = |lo: f32, hi: f32, length: f32, side: i32| match side {
            -1 => lo,
            1 => hi - length,
            _ => (lo + hi - length) * 0.5,
        };
        let x = place(l, r, new_w, keep[0]).max(0.);
        let y = place(t, b, new_h, keep[1]).max(0.);
        self.bounds = [
            x.round() as u32,
            y.round() as u32,
            ((x + new_w).round() as u32).min(size[0]),
            ((y + new_h).round() as u32).min(size[1]),
        ];
    }
    /// Locks a ratio and reshapes the crop around its center.
    fn set_ratio(&mut self, ratio: Option<[u32; 2]>, size: [u32; 2]) {
        self.ratio = ratio;
        let Some([rw, rh]) = ratio else { return };
        let [l, t, r, b] = self.bounds.map(|v| v as f32);
        let aspect = rw as f32 / rh as f32;
        let (w, h) = if (r - l) / (b - t) > aspect {
            ((b - t) * aspect, b - t)
        } else {
            (r - l, (r - l) / aspect)
        };
        let x = (l + r - w) * 0.5;
        let y = (t + b - h) * 0.5;
        self.bounds = [
            x.round() as u32,
            y.round() as u32,
            ((x + w).round() as u32).min(size[0]),
            ((y + h).round() as u32).min(size[1]),
        ];
    }
    /// Types an exact size, keeping the top-left corner in place.
    fn set_size(&mut self, width: u32, height: u32, size: [u32; 2]) {
        let [l, t, ..] = self.bounds;
        self.bounds[2] = (l + width.max(1)).min(size[0]);
        self.bounds[3] = (t + height.max(1)).min(size[1]);
    }
}
impl Editor {
    pub(super) fn begin_crop(&mut self) {
        let doc = &self.session.document;
        let point = self
            .selected_annotation()
            .map(|a| a.start)
            .or_else(|| {
                self.view
                    .current_transform()
                    .map(|t| t.image_point(t.viewport.center()))
            })
            .unwrap_or([0.; 2]);
        let page = doc.guide.as_ref().map_or(0, |g| g.page_at(point));
        let r = doc.page_rect(page);
        self.cancel_gesture();
        self.select_only(None);
        self.tool = None;
        let mut crop = Crop::new([r[2] as u32, r[3] as u32]);
        crop.page = page;
        self.crop = Some(crop);
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
            mode: crop.content,
            page: crop.page,
        }) {
            Ok(()) => {
                self.crop = None;
                self.select_only(None);
                self.needs_render = true;
                self.view = crate::canvas_view::CanvasView::default();
                self.status =
                    "Cropped page. Objects remain editable; red crosshatching marks content excluded from export. Undo restores the crop.".into();
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
        let document = &self.session.document;
        let Some(crop) = self.crop.as_mut() else {
            return;
        };
        if interactive
            && response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = ui.input(|i| i.pointer.press_origin())
        {
            let point = transform.image_point(pos);
            let page = document.guide.as_ref().map_or(0, |g| g.page_at(point));
            if page != crop.page {
                let r = document.page_rect(page);
                let content = crop.content;
                *crop = Crop::new([r[2] as u32, r[3] as u32]);
                crop.page = page;
                crop.content = content;
            }
        }
        let r = document.page_rect(crop.page);
        let size = [r[2] as u32, r[3] as u32];
        let transform = Transform {
            image: egui::Rect::from_min_size(
                transform.screen_point([r[0], r[1]]),
                egui::vec2(r[2], r[3]) * transform.scale,
            ),
            ..transform
        };
        let mut cancel = crop.interact(
            ui,
            response,
            transform,
            size,
            interactive,
            self.view.panning,
        );
        crop.paint(ui, transform, true);
        let toolbar_width = 350.0f32.min(transform.viewport.width());
        let pos = egui::pos2(
            (transform.viewport.center().x - toolbar_width * 0.5).clamp(
                transform.viewport.left(),
                (transform.viewport.right() - toolbar_width).max(transform.viewport.left()),
            ),
            transform.viewport.bottom() + 8.0,
        );
        let mut apply = false;
        let bounds = crop.bounds;
        let valid = bounds[2] > bounds[0] && bounds[3] > bounds[1];
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                pos,
                egui::vec2(toolbar_width, 200.0),
            )),
            |ui| {
                egui::Frame::new()
                    .fill(egui::Color32::from_gray(40))
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(interactive, |ui| {
                            ui.label(format!("Page {}",crop.page+1));
                            let mut ratio=crop.ratio;
                            if crate::property_layout::choice(ui,"Ratio",&mut ratio,&RATIOS,"Lock the crop to a shape, or keep it Free."){crop.set_ratio(ratio,size);}
                            let mut w=bounds[2]-bounds[0];let mut h=bounds[3]-bounds[1];
                            let edited=ui.horizontal(|ui|{
                                let mut changed=false;
                                ui.label("Width");
                                changed|=ui.add(egui::DragValue::new(&mut w).range(1..=size[0]).suffix(" px")).tip_title("Width").tip("The width of the crop in pixels.").changed();
                                ui.label("Height");
                                let h_field=ui.add_enabled(crop.ratio.is_none(),egui::DragValue::new(&mut h).range(1..=size[1]).suffix(" px")).tip_title("Height").tip("The height of the crop in pixels.").tip_disabled("The ratio sets the height. Choose Free to type one.");
                                changed|h_field.changed()
                            }).inner;
                            if edited{
                                if let Some([rw,rh])=crop.ratio{h=((w as f32*rh as f32/rw as f32).round() as u32).max(1);}
                                crop.set_size(w,h,size);
                            }
                            ui.radio_value(&mut crop.content,capture_core::crop::CropMode::Preserve,"Preserve content outside the crop");
                            ui.radio_value(&mut crop.content,capture_core::crop::CropMode::Delete,"Delete raster content outside the crop");
                            ui.small("Shapes and text stay editable. Outside portions are excluded from export.");
                            let (yes,no)=crate::controls::dialog_actions(ui,"Apply crop","Cancel",valid,false);
                            apply=yes;cancel|=no;
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
            (e.view.current_transform().unwrap(), buttons)
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
                page: 0,
                content: Default::default(),
                ratio: None,
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
        let mut locked = Crop {
            bounds: [20, 20, 120, 120],
            page: 0,
            content: Default::default(),
            ratio: None,
            drag: None,
        };
        // Choosing a ratio reshapes around the center; dragging keeps it.
        locked.set_ratio(Some([16, 9]), [200, 160]);
        assert_eq!(locked.bounds, [20, 42, 120, 98]);
        locked.drag = Some(Drag {
            original: locked.bounds,
            origin: [120, 98],
            mode: Mode::Resize(4),
        });
        locked.update_drag([180, 120], [200, 160]);
        let [l, t, r, b] = locked.bounds;
        assert_eq!([l, t], [20, 42]);
        assert!((((r - l) as f32 / (b - t) as f32) - 16. / 9.).abs() < 0.03);
        locked.update_drag([5000, 5000], [200, 160]);
        assert!(locked.bounds[2] <= 200 && locked.bounds[3] <= 160);
        let [l, t, r, b] = locked.bounds;
        assert!((((r - l) as f32 / (b - t) as f32) - 16. / 9.).abs() < 0.03);
        let mut crop = Crop {
            bounds: initial,
            page: 0,
            content: Default::default(),
            ratio: None,
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
            editor.view.current_transform().unwrap()
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
        assert_eq!(
            editor.session.document.annotations.len(),
            doc.annotations.len()
        );
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
/// Nonoverlapping pieces of a rectangle outside the output rectangle.
pub(super) fn outside(rect: egui::Rect, keep: egui::Rect) -> Vec<egui::Rect> {
    let c = rect.intersect(keep);
    if !rect.is_positive() {
        return vec![];
    }
    if !c.is_positive() {
        return vec![rect];
    }
    [
        egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), c.top())),
        egui::Rect::from_min_max(egui::pos2(rect.left(), c.bottom()), rect.max),
        egui::Rect::from_min_max(
            egui::pos2(rect.left(), c.top()),
            egui::pos2(c.left(), c.bottom()),
        ),
        egui::Rect::from_min_max(
            egui::pos2(c.right(), c.top()),
            egui::pos2(rect.right(), c.bottom()),
        ),
    ]
    .into_iter()
    .filter(|r| r.is_positive())
    .collect()
}
impl Editor {
    pub(super) fn paint_crop_overflow(&self, ui: &egui::Ui, t: Transform) {
        if !self.session.document.canvas_behavior.allow_overflow
            || self.shell.annotations_hidden
            || self.shell.raster_preview
        {
            return;
        }
        let d = self.canvas_document();
        for a in d.annotations.iter().filter(|a| d.object_visible(a.id)) {
            let (lo, hi) = a.bounds();
            let rect = egui::Rect::from_min_max(t.screen_point(lo), t.screen_point(hi))
                .intersect(t.viewport);
            let r = d.page_rect(d.object_page(a));
            let keep = egui::Rect::from_min_size(
                t.screen_point([r[0], r[1]]),
                egui::vec2(r[2], r[3]) * t.scale,
            );
            for piece in outside(rect, keep) {
                let p = ui.painter().with_clip_rect(piece);
                p.rect_filled(
                    piece,
                    0,
                    egui::Color32::from_rgba_unmultiplied(220, 55, 65, 28),
                );
                let stroke =
                    egui::Stroke::new(1., egui::Color32::from_rgba_unmultiplied(240, 85, 95, 90));
                let spacing = 18.;
                let begin = (piece.left() / spacing).floor() * spacing - piece.height();
                let end = piece.right() + piece.height();
                let mut x = begin;
                while x < end {
                    p.line_segment(
                        [
                            egui::pos2(x, piece.top()),
                            egui::pos2(x + piece.height(), piece.bottom()),
                        ],
                        stroke,
                    );
                    p.line_segment(
                        [
                            egui::pos2(x, piece.top()),
                            egui::pos2(x - piece.height(), piece.bottom()),
                        ],
                        stroke,
                    );
                    x += spacing;
                }
            }
        }
    }
}

#[cfg(test)]
mod pasteboard_tests {
    use super::*;
    #[test]
    fn outside_object_click_and_drag_preserve_crop_and_display_cache() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut d = capture_core::Document::blank(320, 240).unwrap();
        d.annotations.push(
            capture_core::steps::marker(
                1,
                [65., 90.],
                64,
                [0, 140, 230, 255],
                (1, false),
                [320, 240],
            )
            .unwrap(),
        );
        e.session = capture_core::Session::new(d).unwrap();
        e.execute(Command::CropImage {
            x: 60,
            y: 35,
            width: 170,
            height: 140,
            mode: capture_core::crop::CropMode::Preserve,
            page: 0,
        });
        e.tool = None;
        e.view.apply(
            crate::canvas_view::ZoomAction::Set(crate::canvas_view::ZoomMode::Manual(1.)),
            None,
        );
        let frame = |e: &mut Editor, events| {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    if e.needs_render {
                        e.refresh_canvas_display(&ctx);
                    }
                    e.show_canvas(ui);
                },
            );
            out.textures_delta.clear();
        };
        frame(&mut e, vec![]);
        frame(&mut e, vec![]);
        let t = e.view.current_transform().unwrap();
        let p = t.screen_point([-12., 55.]);
        let button = |p, pressed| Event::PointerButton {
            pos: p,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&mut e, vec![Event::PointerMoved(p), button(p, true)]);
        frame(&mut e, vec![button(p, false)]);
        assert_eq!(e.selected, Some(1));
        frame(&mut e, vec![button(p, true)]);
        frame(&mut e, vec![Event::PointerMoved(p + egui::vec2(-40., 0.))]);
        frame(&mut e, vec![button(p + egui::vec2(-40., 0.), false)]);
        assert_eq!(
            [e.session.document.width, e.session.document.height],
            [170, 140]
        );
        assert!(e.session.document.annotations[0].start[0] < -40.);
        assert!(e.session.document.canvas_behavior.held.contains(&1));
        assert!(e.display.uses_retained_scene());
    }
}
