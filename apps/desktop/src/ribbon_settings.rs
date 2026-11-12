//! Original icon-based command groups. Existing document commands remain the
//! single implementation; unavailable workflows stay visibly disabled.
use crate::tips::Tip as _;
use crate::{Command, Editor, egui};
use egui::{Color32, Response, Stroke};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Forward,
    Front,
    Backward,
    Back,
    Group,
    Ungroup,
    FlattenAll,
    ResizeImage,
    ResizeCanvas,
    Clockwise,
    CounterClockwise,
    HalfTurn,
    FlipHorizontal,
    FlipVertical,
    RotateAngle,
    ResetRotation,
    Snapping,
    PixelGrid,
    AutoExpand,
}
impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Forward => "Bring forward",
            Self::Front => "Bring to front",
            Self::Backward => "Send backward",
            Self::Back => "Send to back",
            Self::Group => "Group objects",
            Self::Ungroup => "Ungroup objects",
            Self::FlattenAll => "Flatten page · Ctrl+Shift+T",
            Self::ResizeImage => "Resize image",
            Self::ResizeCanvas => "Resize canvas",
            Self::Clockwise => "Rotate clockwise",
            Self::CounterClockwise => "Rotate counterclockwise",
            Self::HalfTurn => "Rotate 180 degrees",
            Self::FlipHorizontal => "Flip horizontally",
            Self::FlipVertical => "Flip vertically",
            Self::RotateAngle => "Rotate by angle",
            Self::ResetRotation => "Reset rotation",
            Self::Snapping => "Canvas snapping",
            Self::PixelGrid => "Pixel grid · Ctrl+Alt+G · visible at 600% and above",
            Self::AutoExpand => "Auto-expand canvas",
        }
    }
    fn order(self) -> Option<capture_core::StackOrder> {
        use capture_core::StackOrder as S;
        match self {
            Self::Forward => Some(S::BringForward),
            Self::Front => Some(S::BringToFront),
            Self::Backward => Some(S::SendBackward),
            Self::Back => Some(S::SendToBack),
            _ => None,
        }
    }
    fn transform(self) -> Option<capture_core::ImageTransform> {
        use capture_core::ImageTransform as T;
        match self {
            Self::Clockwise => Some(T::Clockwise),
            Self::CounterClockwise => Some(T::CounterClockwise),
            Self::HalfTurn => Some(T::HalfTurn),
            Self::FlipHorizontal => Some(T::FlipHorizontal),
            Self::FlipVertical => Some(T::FlipVertical),
            _ => None,
        }
    }
    #[cfg(test)]
    pub(crate) fn enabled_for_test(self, e: &Editor) -> bool {
        self.enabled(e)
    }
    fn enabled(self, e: &Editor) -> bool {
        if e.is_video() {
            if self.order().is_some() {
                return e.selected.is_some();
            }
            if matches!(
                self,
                Self::FlattenAll | Self::ResizeCanvas | Self::ResizeImage | Self::AutoExpand
            ) {
                return false;
            }
        }
        if e.shell.library {
            return false;
        }
        if let Some(order) = self.order() {
            let ids: Vec<_> = e.selected_objects().iter().map(|a| a.id).collect();
            return if ids.len() > 1 {
                e.session
                    .document
                    .arranged_groups(&ids, order)
                    .is_ok_and(|order| {
                        !order.iter().copied().eq(e
                            .session
                            .document
                            .annotations
                            .iter()
                            .map(|a| a.id))
                    })
            } else {
                e.selected
                    .is_some_and(|id| e.session.document.can_arrange(id, order))
            };
        }
        match self {
            Self::Group => {
                let objects = e.selected_objects();
                objects.len() > 1
                    && (objects[0].group.is_none()
                        || objects.iter().any(|a| a.group != objects[0].group))
            }
            Self::Ungroup => e.selected_objects().iter().any(|a| a.group.is_some()),
            Self::FlattenAll => !e.session.document.annotations.is_empty(),
            Self::ResetRotation => e.session.document.can_reset_rotation(),
            _ => true,
        }
    }
}

impl Editor {
    /// Formatting, with what Advanced held (approved mockup
    /// `artifacts/br-proposal/formatting-merge.html`): the selection first,
    /// then the picture, guides and tools. Video keeps the first four groups.
    pub(super) fn formatting_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        use crate::ribbon::{divider, group};
        let video = self.is_video();
        group(ui, "Alignment", 70.0, height, |ui| {
            self.show_layout_grid(ui, false);
        });
        divider(ui, height);
        group(ui, "Distribution", 60.0, height, |ui| {
            self.show_layout_grid(ui, true);
        });
        divider(ui, height);
        group(ui, "Arrange", 66.0, height, |ui| {
            self.settings_grid(
                ui,
                &[
                    Action::Forward,
                    Action::Front,
                    Action::Backward,
                    Action::Back,
                ],
                2,
            );
        });
        divider(ui, height);
        group(ui, "Grouping", 66.0, height, |ui| {
            let actions: &[Action] = if video {
                &[Action::Group, Action::Ungroup]
            } else {
                &[Action::Group, Action::Ungroup, Action::FlattenAll]
            };
            self.settings_grid(ui, actions, 2);
        });
        divider(ui, height);
        if video {
            return;
        }
        group(ui, "Rotate & flip", 132.0, height, |ui| {
            self.settings_grid(
                ui,
                &[
                    Action::Clockwise,
                    Action::CounterClockwise,
                    Action::HalfTurn,
                    Action::FlipHorizontal,
                    Action::FlipVertical,
                    Action::RotateAngle,
                    Action::ResetRotation,
                ],
                4,
            );
        });
        divider(ui, height);
        group(ui, "Size", 66.0, height, |ui| {
            self.settings_grid(
                ui,
                &[
                    Action::ResizeImage,
                    Action::ResizeCanvas,
                    Action::AutoExpand,
                ],
                2,
            );
        });
        divider(ui, height);
        group(ui, "Guides", 36.0, height, |ui| {
            self.settings_grid(ui, &[Action::Snapping, Action::PixelGrid], 1);
        });
        divider(ui, height);
        group(ui, "Tools", 150.0, height, |ui| {
            let r = crate::ribbon::command_tile(
                ui,
                "Grab Text",
                "Grab Text",
                "Grab Text from the visible canvas or selected area",
                !self.ocr.busy(),
                false,
                72.,
                height,
            );
            if r.clicked() {
                self.grab_canvas_text(ui.ctx());
            }
            self.restore_ribbon(ui, height);
        });
    }
    pub(super) fn settings_grid(
        &mut self,
        ui: &mut egui::Ui,
        actions: &[Action],
        columns: usize,
    ) -> Vec<(Action, Response)> {
        let mut responses = vec![];
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            for row in actions.chunks(columns) {
                ui.horizontal(|ui| {
                    for &action in row {
                        let selected = match action {
                            Action::Snapping => self.quick_controls.snapping(),
                            Action::PixelGrid => self.shell.pixel_grid,
                            Action::AutoExpand => {
                                self.session.document.canvas_behavior.dynamic_expansion
                            }
                            _ => false,
                        };
                        let response =
                            command_icon(ui, action.label(), action.enabled(self), selected);
                        if action == Action::Snapping {
                            egui::Popup::menu(&response)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show(|ui| {
                                    ui.set_width(260.);
                                    self.quick_controls.snapping_popup(ui);
                                });
                        } else if response.clicked() {
                            self.run_settings_action(action);
                        }
                        responses.push((action, response));
                    }
                });
            }
        });
        responses
    }
    pub(super) fn run_settings_action(&mut self, action: Action) {
        if !action.enabled(self) {
            return;
        }
        match action {
            Action::Snapping => {
                self.quick_controls
                    .set_snapping(!self.quick_controls.snapping());
                self.snapping_guides = [None; 2];
                return;
            }
            Action::PixelGrid => {
                self.toggle_pixel_grid();
                return;
            }
            Action::AutoExpand => {
                let enabled = !self.session.document.canvas_behavior.dynamic_expansion;
                self.execute(Command::SetCanvasExpansion { enabled });
                return;
            }
            _ => {}
        }
        if !self.commit_text() {
            return;
        }
        if let Some(order) = action.order() {
            if let Some(mut v) = self.session.document.video.clone() {
                if let Some(track) = self
                    .selected
                    .and_then(|id| v.objects.get(&id).map(|t| t.track))
                    && let Some(index) = v.tracks.iter().position(|id| *id == track)
                {
                    use capture_core::StackOrder as S;
                    let next = match order {
                        S::BringForward => index.saturating_sub(1),
                        S::BringToFront => 0,
                        S::SendBackward => (index + 1).min(v.tracks.len() - 1),
                        S::SendToBack => v.tracks.len() - 1,
                    };
                    v.tracks.remove(index);
                    v.tracks.insert(next, track);
                    self.commit_video(v);
                }
                return;
            }
            let ids: Vec<_> = self.selected_objects().iter().map(|a| a.id).collect();
            if ids.len() > 1 {
                self.execute(Command::ArrangeMany { ids, order });
            } else if let Some(id) = self.selected {
                self.execute(Command::Arrange { id, order });
            }
        } else if let Some(transform) = action.transform() {
            self.begin_image_transform(transform);
        } else {
            match action {
                Action::Group | Action::Ungroup => {
                    let ids = self.selected_objects().iter().map(|a| a.id).collect();
                    self.execute(if action == Action::Group {
                        Command::GroupObjects { ids }
                    } else {
                        Command::UngroupObjects { ids }
                    });
                }
                Action::ResizeImage => self.begin_resize(),
                Action::RotateAngle => self.begin_rotate_image(),
                Action::ResetRotation => self.reset_rotation(),
                Action::ResizeCanvas => self.begin_resize_canvas(),
                Action::FlattenAll => self.flatten_all(),
                _ => {}
            }
        }
    }
    pub(super) fn bottom_zoom_controls(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Vec<(&'static str, Response)> {
        use crate::canvas_view::{ZoomAction as A, ZoomMode as M};
        let mut responses = vec![];
        let mut action = None;
        ui.add_enabled_ui(
            self.shell.canvas_enabled() && self.text_draft.is_none(),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (name, choice) in [
                    ("Zoom out", A::Out),
                    ("Zoom in", A::In),
                    ("Fit to Screen", A::Set(M::Fit)),
                    ("Actual size · 100%", A::Set(M::Manual(1.0))),
                ] {
                    let selected = matches!(choice,A::Set(mode) if self.view.mode==mode);
                    let r = command_icon(ui, name, true, selected);
                    if r.clicked() {
                        action = Some(choice);
                    }
                    responses.push((name, r));
                }
                let raster = command_icon(ui, "Raster Preview", true, self.shell.raster_preview)
                    .tip("Preview the PNG at its actual pixel resolution. Zoom in to inspect pixels; toggle off to edit.");
                if raster.clicked() { self.toggle_raster_preview(); }
                responses.push(("Raster Preview", raster));
                ui.menu_button(self.view.label(), |ui| {
                    for percent in [10, 25, 50, 75, 100, 125, 150, 200, 300, 400, 800, 1600] {
                        let mode = M::Manual(percent as f32 / 100.0);
                        if ui
                            .selectable_label(self.view.mode == mode, format!("{percent}%"))
                            .clicked()
                        {
                            action = Some(A::Set(mode));
                            ui.close();
                        }
                    }
                })
                .response
                .tip("Canvas zoom · Configured zoom modifier + wheel to zoom · middle-drag to pan");
            },
        );
        if let Some(action) = action {
            // Changing the view must not discard the user's crop selection.
            let crop = self.crop.take();
            self.cancel_gesture();
            self.crop = crop;
            self.view.apply(action, None);
            ui.ctx().request_repaint();
        }
        responses
    }
}

pub(super) fn command_icon(
    ui: &mut egui::Ui,
    name: &str,
    enabled: bool,
    selected: bool,
) -> Response {
    let response = ui
        .push_id(name, |ui| {
            ui.add_enabled(
                enabled,
                egui::Button::new("")
                    .min_size(egui::vec2(30.0, 26.0))
                    .selected(selected),
            )
        })
        .inner;
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, name));
    let color = if enabled {
        Color32::from_rgb(163, 206, 233)
    } else {
        Color32::from_gray(108)
    };
    draw(ui.painter(), response.rect.center(), name, color);
    response.tip_title(name)
}
pub(crate) fn draw(p: &egui::Painter, c: egui::Pos2, name: &str, color: Color32) {
    let at = |x, y| c + egui::vec2(x, y);
    let stroke = Stroke::new(1.4, color);
    let box_at = |x, y, w, h| egui::Rect::from_center_size(at(x, y), egui::vec2(w, h));
    let outline = |r| p.rect_stroke(r, 1.0, stroke, egui::StrokeKind::Inside);
    if name.starts_with("Zoom") {
        p.circle_stroke(at(-1.0, -1.0), 6.0, stroke);
        p.line_segment([at(3.0, 3.0), at(8.0, 8.0)], stroke);
        p.line_segment([at(-4.0, -1.0), at(2.0, -1.0)], stroke);
        if name == "Zoom in" {
            p.line_segment([at(-1.0, -4.0), at(-1.0, 2.0)], stroke);
        }
    } else if name == "Raster Preview" {
        for row in 0..3 {
            for col in 0..3 {
                let r = box_at(-6.0 + col as f32 * 6.0, -6.0 + row as f32 * 6.0, 5.0, 5.0);
                if (row + col) % 2 == 0 {
                    p.rect_filled(r, 0.0, color);
                } else {
                    outline(r);
                }
            }
        }
    } else if name.starts_with("Actual") {
        outline(box_at(0.0, 0.0, 20.0, 16.0));
        p.text(
            c,
            egui::Align2::CENTER_CENTER,
            "1:1",
            egui::FontId::proportional(9.0),
            color,
        );
    } else if name == "Fit to Screen" {
        outline(box_at(0.0, 0.0, 20.0, 16.0));
        for (x, y) in [(-6.0, -4.0), (6.0, 4.0)] {
            p.arrow(at(x, y), egui::vec2(-x * 0.6, -y * 0.6), stroke);
        }
    } else if name.starts_with("Bring") || name.starts_with("Send") {
        outline(box_at(-3.0, -3.0, 12.0, 12.0));
        p.rect_filled(box_at(3.0, 3.0, 12.0, 12.0), 1.0, Color32::from_gray(48));
        outline(box_at(3.0, 3.0, 12.0, 12.0));
        let d = if name.starts_with("Bring") { -1.0 } else { 1.0 };
        p.arrow(at(0.0, -d * 5.0), egui::vec2(0.0, d * 10.0), stroke);
        if name.ends_with("front") || name.ends_with("back") {
            p.line_segment([at(-4.0, d * 8.0), at(4.0, d * 8.0)], stroke);
        }
    } else if name.starts_with("Group") || name.starts_with("Ungroup") {
        outline(box_at(-3.0, -3.0, 9.0, 9.0));
        outline(box_at(4.0, 4.0, 9.0, 9.0));
        if name.starts_with("Group") {
            outline(box_at(0.0, 0.0, 23.0, 23.0));
        } else {
            for x in [-9.0, 9.0] {
                p.arrow(at(x * 0.5, 0.0), egui::vec2(x * 0.4, 0.0), stroke);
            }
        }
    } else if name.starts_with("Flatten") {
        for y in [-5.0, 0.0, 5.0] {
            p.add(egui::Shape::closed_line(
                vec![at(-9.0, y), at(0.0, y - 4.0), at(9.0, y), at(0.0, y + 4.0)],
                stroke,
            ));
        }
        if name.starts_with("Flatten page") {
            p.line_segment([at(-9.0, 10.0), at(9.0, 10.0)], stroke);
        }
    } else if name.starts_with("Resize") {
        outline(box_at(0.0, 0.0, 20.0, 17.0));
        if name.ends_with("canvas") {
            outline(box_at(-3.0, -2.0, 10.0, 8.0));
        }
        p.arrow(at(-3.0, 3.0), egui::vec2(10.0, -9.0), stroke);
    } else if name == "Reset rotation" {
        // An upright square with an arrow turning back.
        outline(box_at(0.0, 2.0, 12.0, 12.0));
        p.add(egui::Shape::line(
            vec![
                at(7.0, -9.0),
                at(0.0, -10.0),
                at(-6.0, -8.0),
                at(-9.0, -4.0),
            ],
            stroke,
        ));
        p.arrow(at(-9.0, -4.0), egui::vec2(0.0, 0.1), stroke);
        p.line_segment([at(-9.0, -4.0), at(-9.0, -8.0)], stroke);
        p.line_segment([at(-9.0, -4.0), at(-5.0, -4.0)], stroke);
    } else if name == "Rotate by angle" {
        // A square tilted by an angle, with the angle's arc.
        let r = 6.0_f32;
        let turn = |x: f32, y: f32| {
            let (s, c) = 0.5_f32.sin_cos();
            at(x * c - y * s, x * s + y * c)
        };
        p.add(egui::Shape::closed_line(
            vec![turn(-r, -r), turn(r, -r), turn(r, r), turn(-r, r)],
            stroke,
        ));
        p.line_segment([at(-9.0, 9.0), at(9.0, 9.0)], stroke);
    } else if name.starts_with("Rotate") {
        let d = if name.contains("counter") { -1.0 } else { 1.0 };
        p.add(egui::Shape::line(
            vec![
                at(-d * 7.0, 4.0),
                at(-d * 7.0, -4.0),
                at(0.0, -8.0),
                at(d * 7.0, -4.0),
            ],
            stroke,
        ));
        p.arrow(at(d * 7.0, -4.0), egui::vec2(0.0, 7.0), stroke);
        if name.contains("180") {
            p.text(
                at(0.0, 3.0),
                egui::Align2::CENTER_CENTER,
                "180",
                egui::FontId::proportional(8.0),
                color,
            );
        }
    } else if name.starts_with("Flip") {
        let vertical = name.contains("vertically");
        let pos = |x, y| if vertical { at(y, x) } else { at(x, y) };
        p.line_segment([pos(0.0, -10.0), pos(0.0, 10.0)], stroke);
        for d in [-1.0, 1.0] {
            p.add(egui::Shape::closed_line(
                vec![pos(d * 3.0, -7.0), pos(d * 9.0, 7.0), pos(d * 3.0, 7.0)],
                stroke,
            ));
        }
    } else if name == "Canvas snapping" {
        p.add(egui::Shape::line(
            vec![
                at(-7.0, -7.0),
                at(-7.0, 3.0),
                at(-4.0, 7.0),
                at(4.0, 7.0),
                at(7.0, 3.0),
                at(7.0, -7.0),
            ],
            Stroke::new(3.0, color),
        ));
        for x in [-7.0, 7.0] {
            p.line_segment(
                [at(x - 2.0, -3.0), at(x + 2.0, -3.0)],
                Stroke::new(2.0, Color32::from_rgb(238, 145, 95)),
            );
        }
    } else if name.starts_with("Pixel grid") {
        for n in [-8.0, 0.0, 8.0] {
            p.line_segment([at(n, -8.0), at(n, 8.0)], stroke);
            p.line_segment([at(-8.0, n), at(8.0, n)], stroke);
        }
    } else if name.starts_with("Canvas color") {
        p.circle_stroke(c, 9.0, stroke);
        for (x, y) in [(-4.0, -3.0), (3.0, -4.0), (5.0, 3.0)] {
            p.circle_filled(at(x, y), 2.0, color);
        }
    } else if name.starts_with("Trim") {
        p.add(egui::Shape::line(
            vec![at(-6.0, -10.0), at(-6.0, 6.0), at(10.0, 6.0)],
            stroke,
        ));
        p.add(egui::Shape::line(
            vec![at(-10.0, -6.0), at(6.0, -6.0), at(6.0, 10.0)],
            stroke,
        ));
    } else {
        outline(box_at(0.0, 0.0, 18.0, 20.0));
        p.line_segment([at(-5.0, -3.0), at(5.0, -3.0)], stroke);
        p.line_segment([at(-5.0, 3.0), at(3.0, 3.0)], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton};

    fn frame(
        ctx: &egui::Context,
        e: &mut Editor,
        events: Vec<Event>,
    ) -> Vec<(&'static str, Response)> {
        let mut responses = vec![];
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                responses.extend(
                    e.settings_grid(
                        ui,
                        &[
                            Action::Forward,
                            Action::Back,
                            Action::ResizeImage,
                            Action::ResizeCanvas,
                            Action::Clockwise,
                            Action::FlattenAll,
                            Action::Group,
                            Action::Snapping,
                            Action::PixelGrid,
                        ],
                        5,
                    )
                    .into_iter()
                    .map(|(a, r)| (a.label(), r)),
                );
                ui.horizontal(|ui| responses.extend(e.bottom_zoom_controls(ui)));
            },
        );
        output.textures_delta.clear();
        responses
    }
    fn click(ctx: &egui::Context, e: &mut Editor, name: &str) {
        frame(ctx, e, vec![]);
        let buttons = frame(ctx, e, vec![]);
        let r = &buttons.iter().find(|(n, _)| *n == name).unwrap().1;
        let pos = r.rect.center();
        for pressed in [true, false] {
            frame(
                ctx,
                e,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        pressed,
                        button: PointerButton::Primary,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn snapping_popup_stays_open_for_both_switches_and_closes_outside() {
        use crate::property_layout::tests::PaintedOutput;
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        crate::shell::configure(&ctx);
        let render = |e: &mut Editor, events: Vec<Event>| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    e.settings_grid(ui, &[Action::Snapping], 1);
                },
            ))
        };
        let click_at = |e: &mut Editor, p| {
            for pressed in [true, false] {
                render(
                    e,
                    vec![
                        Event::PointerMoved(p),
                        Event::PointerButton {
                            pos: p,
                            pressed,
                            button: PointerButton::Primary,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
        };
        render(&mut e, vec![]);
        click_at(&mut e, egui::pos2(20., 20.));
        let before = e.quick_controls.snapping();
        let output = render(&mut e, vec![]);
        let p = crate::color_widgets::tests::text_rect(&output.shapes, "Enable snapping")
            .unwrap()
            .center();
        click_at(&mut e, p);
        assert_ne!(e.quick_controls.snapping(), before);
        assert!(egui::Popup::is_any_open(&ctx));
        let output = render(&mut e, vec![]);
        let p = crate::color_widgets::tests::text_rect(&output.shapes, "Infinite candidates")
            .unwrap()
            .center();
        click_at(&mut e, p);
        assert!(egui::Popup::is_any_open(&ctx));
        click_at(&mut e, egui::pos2(560., 360.));
        assert!(!egui::Popup::is_any_open(&ctx));
    }
    #[test]
    fn relocated_icon_actions_use_document_commands_and_keep_view_changes_out_of_undo() {
        use crate::canvas_view::ZoomMode;
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for id in [1, 2] {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    [100.0 * id as f32, 100.0],
                    40,
                    [40, 80, 120, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        e.select_only(Some(1));
        let original = e.session.document.annotations.clone();
        click(&ctx, &mut e, Action::Forward.label());
        assert_eq!(e.session.document.annotations[1].id, 1);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        click(&ctx, &mut e, Action::Group.label());
        assert!(!e.session.can_undo());
        let snap = e.quick_controls.snapping();
        click(&ctx, &mut e, Action::Snapping.label());
        assert_eq!(e.quick_controls.snapping(), snap);
        assert!(egui::Popup::is_any_open(&ctx));
        egui::Popup::close_all(&ctx);
        click(&ctx, &mut e, Action::PixelGrid.label());
        assert!(e.shell.pixel_grid);
        click(&ctx, &mut e, "Actual size · 100%");
        assert_eq!(e.view.mode, ZoomMode::Manual(1.0));
        click(&ctx, &mut e, "Zoom in");
        assert!(matches!(e.view.mode, ZoomMode::Manual(x) if x > 1.0));
        click(&ctx, &mut e, "Fit to Screen");
        assert_eq!(e.view.mode, ZoomMode::Fit);
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        for (dimensions, enlarge) in [
            (egui::vec2(320.0, 200.0), true),
            (egui::vec2(2400.0, 1600.0), false),
        ] {
            let t = e.view.transform(viewport, dimensions, 1.0);
            assert_eq!(t.zoom > 1.0, enlarge);
            assert!(t.image.width() <= 800.01 && t.image.height() <= 600.01);
            assert!(
                (t.image.width() - 800.0).abs() < 0.01 || (t.image.height() - 600.0).abs() < 0.01
            );
            assert!(t.image.center().distance(viewport.center()) < 0.01);
        }
        let buttons = frame(&ctx, &mut e, vec![]);
        let actual = &buttons
            .iter()
            .find(|(n, _)| *n == "Actual size · 100%")
            .unwrap()
            .1;
        let raster = &buttons
            .iter()
            .find(|(n, _)| *n == "Raster Preview")
            .unwrap()
            .1;
        assert!((raster.rect.left() - actual.rect.right() - 2.0).abs() < 0.01);
        click(&ctx, &mut e, "Raster Preview");
        assert!(e.shell.raster_preview);
        assert_eq!(e.selected, Some(1));
        click(&ctx, &mut e, "Raster Preview");
        assert!(!e.shell.raster_preview);
        assert_eq!(e.session.document.annotations, original);
        assert!(!e.session.can_undo());
        click(&ctx, &mut e, Action::ResizeImage.label());
        assert!(e.resize_image.take().is_some());
        click(&ctx, &mut e, Action::ResizeCanvas.label());
        assert!(e.resize_canvas.take().is_some());
        click(&ctx, &mut e, Action::Clockwise.label());
        assert!(e.image_transform.is_none());
        assert_eq!(e.session.document.annotations.len(), original.len());
        assert_eq!(e.session.document.annotations[0].rotation_degrees, 90.);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        e.select_only(None);
        click(&ctx, &mut e, Action::Clockwise.label());
        assert_eq!(
            e.image_transform.take(),
            Some(capture_core::ImageTransform::Clockwise)
        );
        assert_eq!(e.session.document.annotations, original);
        let pixels = e.session.document.render().unwrap();
        click(&ctx, &mut e, Action::FlattenAll.label());
        assert!(e.session.document.annotations.is_empty());
        assert_eq!(e.session.document.render().unwrap(), pixels);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        e.shell.library = true;
        assert!(!Action::ResizeImage.enabled(&e));
        e.run_settings_action(Action::FlattenAll);
        assert_eq!(e.session.document.annotations, original);
    }
}
