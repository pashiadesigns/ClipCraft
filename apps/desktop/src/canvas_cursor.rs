//! Canvas-only feedback. Tool marks are UI paint, never document annotations.
use crate::{Editor, Kind, canvas_view::Transform, egui};
use capture_core::selection::Handle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Feedback {
    pub icon: egui::CursorIcon,
    pub tool: Option<&'static str>,
}

impl Feedback {
    fn interaction(icon: egui::CursorIcon) -> Self {
        Self { icon, tool: None }
    }
    fn tool(name: &'static str) -> Self {
        Self {
            icon: if name == "Text" {
                egui::CursorIcon::Text
            } else {
                egui::CursorIcon::Crosshair
            },
            tool: Some(name),
        }
    }
}

fn handle_feedback(handle: Handle) -> Feedback {
    if handle.is_radius() {
        Feedback::tool("Round corners")
    } else {
        Feedback::interaction(handle_cursor(handle))
    }
}

fn handle_cursor(handle: Handle) -> egui::CursorIcon {
    match handle {
        Handle::TopLeft | Handle::BottomRight => egui::CursorIcon::ResizeNwSe,
        Handle::TopRight | Handle::BottomLeft => egui::CursorIcon::ResizeNeSw,
        Handle::Top | Handle::Bottom => egui::CursorIcon::ResizeVertical,
        Handle::Left | Handle::Right => egui::CursorIcon::ResizeHorizontal,
        Handle::Bend => egui::CursorIcon::Grab,
        _ => egui::CursorIcon::Crosshair,
    }
}

impl Editor {
    pub(super) fn hover_annotation(
        &self,
        position: egui::Pos2,
        transform: Transform,
    ) -> Option<&crate::Annotation> {
        if self.shell.annotations_hidden
            || self.text_draft.is_some()
            || self.step_placement.is_some()
            || self.drag_start.is_some()
            || self.edit_drag.is_some()
            || self.group_drag.is_some()
            || self.view.panning
            || self.fill_tool.active
            || self.crop.is_some()
            || self.pixel_selection.is_some()
            || self.cut_out.is_some()
            || !transform
                .image
                .intersect(transform.viewport)
                .contains(position)
        {
            return None;
        }
        let id = self.tool_hit(transform.image_point(position), 7.0 / transform.scale)?;
        if self.is_selected(id) {
            return None;
        }
        self.session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
    }

    pub(super) fn paint_hover_annotation(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: Transform,
        interactive: bool,
    ) {
        if !interactive
            || !response.hovered()
            || ui.input(|i| !i.focused || i.pointer.any_down() || i.key_down(egui::Key::Space))
        {
            return;
        }
        let Some(annotation) = ui
            .input(|i| i.pointer.hover_pos())
            .and_then(|p| self.hover_annotation(p, transform))
        else {
            return;
        };
        let painter = ui.painter().with_clip_rect(transform.viewport);
        let (min, max) = annotation.bounds();
        let color = egui::Color32::from_rgb(70, 195, 225);
        painter.rect_stroke(
            egui::Rect::from_min_max(transform.screen_point(min), transform.screen_point(max))
                .expand(2.0),
            0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Outside,
        );
        for (handle, point) in annotation.handles() {
            if handle.is_radius() {
                continue;
            }
            painter.circle_filled(transform.screen_point(point), 2.5, color);
        }
    }

    fn pointer_feedback(&self, pos: egui::Pos2, transform: Transform, space: bool) -> Feedback {
        use egui::CursorIcon as Cursor;
        if self.quick_controls.format_source.is_some() {
            return Feedback::interaction(Cursor::Copy);
        }
        if self.rotation_drag.is_some() {
            return Feedback::interaction(Cursor::Grabbing);
        }
        if !space && let Some(cursor) = self.group_transform_cursor(pos, transform) {
            return Feedback::interaction(cursor);
        }
        if self.rotation_hit(pos, transform) {
            return Feedback::interaction(Cursor::Grab);
        }
        if self.group_drag.as_ref().is_some_and(|d| d.duplicate) {
            return Feedback::interaction(Cursor::Copy);
        }
        if self.view.panning || self.group_drag.is_some() || self.step_placement.is_some() {
            return Feedback::interaction(Cursor::Grabbing);
        }
        if let Some(drag) = &self.edit_drag {
            return drag
                .handle
                .map_or(Feedback::interaction(Cursor::Grabbing), handle_feedback);
        }
        if space && self.crop.is_none() && self.drag_start.is_none() {
            return Feedback::interaction(Cursor::Grab);
        }
        if let Some(crop) = &self.crop {
            return crop
                .cursor(pos, transform, self.image_size())
                .map_or_else(|| Feedback::tool("Crop"), Feedback::interaction);
        }
        if let Some(selection) = &self.pixel_selection {
            return selection
                .cursor(pos, transform, self.image_size())
                .map_or_else(|| Feedback::tool("Selection"), Feedback::interaction);
        }
        if let Some(cut) = &self.cut_out {
            return Feedback {
                icon: cut.cursor(),
                tool: (cut.cursor() != Cursor::Default).then_some("Cut Out"),
            };
        }
        if !transform.image.contains(pos) && self.drag_start.is_none() {
            return Feedback::interaction(Cursor::Default);
        }
        if self.fill_tool.active {
            return Feedback::tool("Fill");
        }
        if self.drag_start.is_none() {
            let point = transform.image_point(pos);
            let tolerance = 7.0 / transform.scale;
            if self.selected_objects().len() < 2
                && let Some(annotation) = self.selected_annotation()
                && Self::tool_matches(self.tool, annotation.kind)
                && let Some(handle) = annotation.hit_handle(point, tolerance)
            {
                return handle_feedback(handle);
            }
            if self.tool_hit(point, tolerance).is_some() {
                return Feedback::interaction(Cursor::Grab);
            }
        }
        match self.tool {
            None => Feedback::interaction(Cursor::Default),
            Some(kind) => Feedback::tool(match kind {
                Kind::Simplify => "Simplify",
                Kind::Image => "Move",
                Kind::Arrow => "Arrow",
                Kind::Line => "Line",
                Kind::Rectangle => "Shape",
                Kind::Ellipse => "Ellipse",
                Kind::Polygon => "Polygon",
                Kind::Text => "Text",
                Kind::Callout => "Callout",
                Kind::Pen => "Pen",
                Kind::Highlighter => "Highlighter",
                Kind::Step => "Step",
                Kind::Redact => "Redact",
                Kind::Blur => "Blur",
                Kind::Pixelate => "Pixelate",
                Kind::Eraser => "Eraser",
            }),
        }
    }

    fn image_size(&self) -> [u32; 2] {
        [self.session.document.width, self.session.document.height]
    }

    pub(super) fn canvas_pointer(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: Transform,
        interactive: bool,
    ) {
        if !interactive || !ui.input(|i| i.focused) || self.text_draft.is_some() {
            return;
        }
        let dragging = self.rotation_drag.is_some()
            || self.view.panning
            || self.step_placement.is_some()
            || self.group_drag.is_some()
            || self.edit_drag.is_some()
            || self.drag_start.is_some()
            || self.crop.as_ref().is_some_and(|c| c.dragging())
            || self.pixel_selection.as_ref().is_some_and(|s| s.dragging())
            || self.cut_out.as_ref().is_some_and(|c| c.dragging());
        if !response.hovered() && !dragging {
            return;
        }
        let Some(pos) = ui.input(|i| i.pointer.hover_pos()) else {
            return;
        };
        let mut feedback =
            self.pointer_feedback(pos, transform, ui.input(|i| i.key_down(egui::Key::Space)));
        if feedback.tool.is_some() && !transform.image.contains(pos) && !dragging {
            feedback = Feedback::interaction(egui::CursorIcon::Default);
        }
        ui.ctx().set_cursor_icon(feedback.icon);
        if let Some(name) = feedback.tool.filter(|_| transform.viewport.contains(pos)) {
            // The OS cursor moves independently of app frames. Paint the hotspot
            // and badge together and suppress that separate cursor only here.
            // Their distance cannot vary with pointer acceleration or frame rate.
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            let scale = 1.0 / ui.ctx().pixels_per_point();
            let rect = tool_badge_rect(pos, scale);
            let painter = ui
                .ctx()
                .layer_painter(egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::unique("canvas_tool_pointer"),
                ))
                .with_clip_rect(transform.viewport);
            if matches!(name, "Pen" | "Highlighter" | "Eraser")
                && !(name == "Highlighter" && self.drawing_rectangle_highlighter())
            {
                let radius = self.width as f32 * transform.scale * 0.5;
                painter.circle_filled(
                    pos,
                    radius,
                    egui::Color32::from_rgba_unmultiplied(
                        self.color[0],
                        self.color[1],
                        self.color[2],
                        if name == "Highlighter" { 80 } else { 40 },
                    ),
                );
                painter.circle_stroke(
                    pos,
                    radius,
                    egui::Stroke::new(2. * scale, egui::Color32::WHITE),
                );
                painter.circle_stroke(
                    pos,
                    radius,
                    egui::Stroke::new(scale, egui::Color32::from_gray(35)),
                );
                return;
            }
            if matches!(name, "Line" | "Arrow") {
                let start = pos + egui::vec2(12., 30.) * scale;
                painter.line_segment(
                    [start, start + egui::vec2(25. * scale, 0.)],
                    egui::Stroke::new(
                        self.width as f32 * transform.scale,
                        egui::Color32::from_rgb(self.color[0], self.color[1], self.color[2]),
                    ),
                );
            }
            paint_hotspot(&painter, pos, feedback.icon, scale);
            painter.rect_filled(rect, 2.0, egui::Color32::from_white_alpha(235));
            painter.rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(scale, egui::Color32::from_gray(40)),
                egui::StrokeKind::Inside,
            );
            if name == "Round corners" {
                // A continuous rounded L marks this as radius adjustment.
                let at = |x, y| rect.min + egui::vec2(x, y) * scale;
                let mut points = vec![at(3.0, 13.0), at(3.0, 9.0)];
                for i in 0..=12 {
                    let angle =
                        std::f32::consts::PI + i as f32 / 12.0 * std::f32::consts::FRAC_PI_2;
                    points.push(at(9.0 + 6.0 * angle.cos(), 9.0 + 6.0 * angle.sin()));
                }
                points.push(at(13.0, 3.0));
                painter.add(egui::Shape::line(
                    points,
                    egui::Stroke::new(1.8 * scale, egui::Color32::from_gray(25)),
                ));
            } else if name == "Step" {
                let label = capture_core::steps::label(self.next_step_number(), self.step_letters)
                    .unwrap_or_else(|_| "×".into());
                paint_step_badge(&painter, rect, &label, scale);
            } else {
                crate::icons::draw_scaled(
                    &painter,
                    rect,
                    name,
                    egui::Color32::from_gray(25),
                    0.5 * scale,
                );
            }
        }
    }
}

fn paint_step_badge(painter: &egui::Painter, rect: egui::Rect, label: &str, scale: f32) {
    let color = egui::Color32::from_gray(25);
    let radius = 6.75 * scale;
    painter.circle_stroke(rect.center(), radius, egui::Stroke::new(scale, color));
    let font = egui::FontId::proportional(10.0 * scale);
    let mut galley = painter.layout_no_wrap(label.into(), font, color);
    let available = egui::vec2(10.0, 10.0) * scale;
    let fit = (available.x / galley.size().x)
        .min(available.y / galley.size().y)
        .min(1.0);
    if fit < 1.0 {
        galley = painter.layout_no_wrap(
            label.into(),
            egui::FontId::proportional(10.0 * scale * fit),
            color,
        );
    }
    painter.galley(rect.center() - galley.size() * 0.5, galley, color);
}

fn tool_badge_rect(pos: egui::Pos2, scale: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        pos + egui::vec2(7.0, 9.0) * scale,
        egui::vec2(16.0, 16.0) * scale,
    )
}

fn paint_hotspot(p: &egui::Painter, pos: egui::Pos2, icon: egui::CursorIcon, scale: f32) {
    let at = |x, y| pos + egui::vec2(x, y) * scale;
    let lines = match icon {
        egui::CursorIcon::Text => vec![
            [at(0.0, -6.0), at(0.0, 6.0)],
            [at(-3.0, -6.0), at(3.0, -6.0)],
            [at(-3.0, 6.0), at(3.0, 6.0)],
        ],
        egui::CursorIcon::ResizeVertical => vec![
            [at(0.0, -7.0), at(0.0, 7.0)],
            [at(-3.0, -4.0), at(0.0, -7.0)],
            [at(3.0, -4.0), at(0.0, -7.0)],
            [at(-3.0, 4.0), at(0.0, 7.0)],
            [at(3.0, 4.0), at(0.0, 7.0)],
        ],
        egui::CursorIcon::ResizeHorizontal => vec![
            [at(-7.0, 0.0), at(7.0, 0.0)],
            [at(-4.0, -3.0), at(-7.0, 0.0)],
            [at(-4.0, 3.0), at(-7.0, 0.0)],
            [at(4.0, -3.0), at(7.0, 0.0)],
            [at(4.0, 3.0), at(7.0, 0.0)],
        ],
        _ => vec![
            [at(-7.0, 0.0), at(-3.0, 0.0)],
            [at(3.0, 0.0), at(7.0, 0.0)],
            [at(0.0, -7.0), at(0.0, -3.0)],
            [at(0.0, 3.0), at(0.0, 7.0)],
        ],
    };
    for (width, color) in [(3.0, egui::Color32::BLACK), (1.0, egui::Color32::WHITE)] {
        for line in &lines {
            p.line_segment(*line, egui::Stroke::new(width * scale, color));
        }
        if icon == egui::CursorIcon::Crosshair {
            p.rect_stroke(
                egui::Rect::from_center_size(pos, egui::Vec2::splat(6.0 * scale)),
                0.0,
                egui::Stroke::new(width * scale, color),
                egui::StrokeKind::Middle,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{CursorIcon as Cursor, Event, Modifiers, PointerButton};

    fn step_cursor_label(ctx: &egui::Context, editor: &mut Editor) -> String {
        // The context applies a changed DPI on its first pass.
        frame(ctx, editor, vec![]);
        let (transform, _) = frame(ctx, editor, vec![]);
        let pos = transform.screen_point([540., 300.]);
        let output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                focused: true,
                events: vec![Event::PointerMoved(pos)],
                ..Default::default()
            },
            |ui| editor.show_canvas(ui),
        ));
        let badge = tool_badge_rect(pos, 1.0 / ctx.pixels_per_point());
        output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t)
                    if badge.contains_rect(t.galley.rect.translate(t.pos.to_vec2())) =>
                {
                    Some(t.galley.text().to_owned())
                }
                _ => None,
            })
            .expect("next Step label painted inside the cursor badge")
    }

    #[test]
    fn cursor_label_matches_placement_after_edits_deletion_undo_reopen_and_mode_switch() {
        for letters in [false, true] {
            let ctx = egui::Context::default();
            let mut e = crate::canvas::tests::editor(&ctx);
            e.tool = Some(Kind::Step);
            e.step_letters = letters;
            e.step_number = 2;
            for number in 2..=4 {
                let label = capture_core::steps::label(number, letters).unwrap();
                assert_eq!(step_cursor_label(&ctx, &mut e), label);
                let (t, _) = frame(&ctx, &mut e, vec![]);
                let pos = t.screen_point([60. + number as f32 * 75., 80.]);
                frame(
                    &ctx,
                    &mut e,
                    vec![
                        Event::PointerMoved(pos),
                        button(pos, PointerButton::Primary, true),
                    ],
                );
                assert_eq!(
                    e.step_placement
                        .as_ref()
                        .unwrap()
                        .text
                        .as_ref()
                        .unwrap()
                        .content,
                    label
                );
                frame(
                    &ctx,
                    &mut e,
                    vec![button(pos, PointerButton::Primary, false)],
                );
                assert_eq!(
                    e.session
                        .document
                        .annotations
                        .last()
                        .unwrap()
                        .text
                        .as_ref()
                        .unwrap()
                        .content,
                    label
                );
            }
            e.execute(crate::Command::Undo);
            assert_eq!(
                step_cursor_label(&ctx, &mut e),
                capture_core::steps::label(4, letters).unwrap()
            );
            e.execute(crate::Command::Redo);
            assert_eq!(
                step_cursor_label(&ctx, &mut e),
                capture_core::steps::label(5, letters).unwrap()
            );
            let mut last = e.session.document.annotations.last().unwrap().clone();
            last.text.as_mut().unwrap().content = capture_core::steps::label(26, letters).unwrap();
            e.execute(crate::Command::Update {
                annotation: last.clone(),
            });
            assert_eq!(
                step_cursor_label(&ctx, &mut e),
                capture_core::steps::label(27, letters).unwrap()
            );
            e.execute(crate::Command::Remove { id: last.id });
            assert_eq!(
                step_cursor_label(&ctx, &mut e),
                capture_core::steps::label(4, letters).unwrap()
            );
            e.execute(crate::Command::Undo);
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cursor-steps.capture.json");
            e.session.save(&path).unwrap();
            e.session =
                capture_core::Session::new(capture_core::Document::load(&path).unwrap()).unwrap();
            assert_eq!(
                step_cursor_label(&ctx, &mut e),
                capture_core::steps::label(27, letters).unwrap()
            );
            let before = e.session.document.clone();
            e.step_letters = !letters;
            let expected = e.step_marker([540., 300.]).unwrap().text.unwrap().content;
            assert_eq!(step_cursor_label(&ctx, &mut e), expected);
            assert_eq!(e.session.document.annotations, before.annotations);
            assert!(!e.session.can_undo());
        }
    }

    #[test]
    fn step_cursor_labels_fit_one_two_and_three_characters_at_each_dpi() {
        for ppp in [1.0, 1.25, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(ppp);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.tool = Some(Kind::Step);
            for (letters, number) in [
                (false, 4),
                (false, 99),
                (false, 999),
                (true, 2),
                (true, 27),
                (true, 999),
            ] {
                e.step_letters = letters;
                e.step_number = number;
                assert_eq!(
                    step_cursor_label(&ctx, &mut e),
                    capture_core::steps::label(number, letters).unwrap()
                );
            }
            e.step_number = 1000;
            assert_eq!(step_cursor_label(&ctx, &mut e), "×");
            assert!(e.step_marker([540., 300.]).is_err());
        }
        if let Ok(dir) = std::env::var("CLIPCRAFT_STEP_CURSOR_UI_DIR") {
            let ctx = egui::Context::default();
            let render = || {
                crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600., 120.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.horizontal(|ui| {
                            for label in ["B", "C", "D", "4", "5", "6", "99", "999", "AA"] {
                                ui.vertical(|ui| {
                                    ui.label(label);
                                    let (rect, _) = ui.allocate_exact_size(
                                        egui::vec2(48., 48.),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(rect, 2., egui::Color32::WHITE);
                                    paint_step_badge(ui.painter(), rect, label, 3.);
                                });
                            }
                        });
                    },
                ))
            };
            let first = render();
            let second = render();
            crate::property_layout::tests::dump_meshes(&ctx, &dir, "StepLabels-3x", first, second);
        }
    }

    #[test]
    fn accelerated_pointer_events_paint_hotspot_and_mark_from_the_same_frame_at_each_dpi() {
        for ppp in [1.0, 1.25, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(ppp);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.tool = Some(Kind::Rectangle);
            frame(&ctx, &mut e, vec![]);
            let (t, _) = frame(&ctx, &mut e, vec![]);
            let mut previous = t.screen_point([50.0, 50.0]);
            for point in [[60.0, 52.0], [400.0, 260.0], [100.0, 90.0], [550.0, 330.0]] {
                let pos = t.screen_point(point);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        focused: true,
                        events: vec![Event::PointerMoved(previous), Event::PointerMoved(pos)],
                        ..Default::default()
                    },
                    |ui| e.show_canvas(ui),
                );
                output.textures_delta.clear();
                assert_eq!(
                    output.platform_output.cursor_icon,
                    Cursor::None,
                    "dpi {ppp} point {pos:?} image {:?}",
                    t.image
                );
                let badge = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Rect(r) if r.fill == egui::Color32::from_white_alpha(235) => {
                            Some(r.rect)
                        }
                        _ => None,
                    })
                    .expect("paired tool mark");
                let hotspot = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if r.stroke.color == egui::Color32::WHITE
                                && (r.rect.width() * ppp - 6.0).abs() < 0.01 =>
                        {
                            Some(r.rect.center())
                        }
                        _ => None,
                    })
                    .expect("painted pointer hotspot");
                assert!(hotspot.distance(pos) < 0.001);
                assert!(((badge.min - hotspot) * ppp - egui::vec2(7.0, 9.0)).length() < 0.001);
                assert!((badge.width() * ppp - 16.0).abs() < 0.001);
                previous = pos;
            }
            assert_eq!(
                frame(&ctx, &mut e, vec![Event::PointerGone]).1,
                Cursor::Default
            );
            assert!(!e.session.can_undo());
        }
    }

    #[test]
    fn stationary_radius_handles_show_rounding_cursor_after_release_and_restore_on_leave() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        let mut shape = fixture
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Rectangle)
            .unwrap()
            .clone();
        shape.start = [100.0, 100.0];
        shape.end = [300.0, 240.0];
        shape.corner_radius = 60.0;
        e.session.document.annotations = vec![shape.clone()];
        e.selected = Some(shape.id);
        e.tool = Some(Kind::Rectangle);
        let (t, _) = frame(&ctx, &mut e, vec![]);
        for (h, p) in shape.handles().into_iter().filter(|(h, _)| h.is_radius()) {
            let pos = t.screen_point(p);
            assert_eq!(shape.hit_handle(p, 1.0), Some(h));
            assert_eq!(
                e.pointer_feedback(pos, t, false).tool,
                Some("Round corners")
            );
            assert_eq!(
                frame(&ctx, &mut e, vec![Event::PointerMoved(pos)]).1,
                Cursor::None
            );
        }
        let pos = t.screen_point(
            shape
                .handles()
                .into_iter()
                .find(|(h, _)| h.is_radius())
                .unwrap()
                .1,
        );
        let paint_ctx = egui::Context::default();
        e.refresh_canvas_display(&paint_ctx);
        let mut render = || {
            crate::property_layout::tests::PaintedOutput(paint_ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    focused: true,
                    events: vec![Event::PointerMoved(pos)],
                    ..Default::default()
                },
                |ui| e.show_canvas(ui),
            ))
        };
        let first = render();
        let output = render();
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_CURSOR_DIR") {
            crate::property_layout::tests::dump_meshes(
                &paint_ctx,
                &root,
                "RoundCorners",
                first,
                output,
            );
        }
        assert_eq!(
            frame(&ctx, &mut e, vec![Event::PointerGone]).1,
            Cursor::Default
        );
        assert!(!e.session.can_undo());
        assert_eq!(e.session.document.annotations, vec![shape]);
    }

    #[test]
    fn badge_has_half_size_and_constant_hotspot_offset_at_every_position() {
        for pos in [
            egui::pos2(0.0, 0.0),
            egui::pos2(50.5, 60.25),
            egui::pos2(795.0, 595.0),
            egui::pos2(-10.0, -4.0),
        ] {
            let rect = tool_badge_rect(pos, 1.0);
            assert_eq!(rect.min - pos, egui::vec2(7.0, 9.0));
            assert_eq!(rect.size(), egui::vec2(16.0, 16.0));
        }
    }

    fn frame(ctx: &egui::Context, editor: &mut Editor, events: Vec<Event>) -> (Transform, Cursor) {
        let mut viewport = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                viewport = ui.available_rect_before_wrap();
                editor.show_canvas(ui);
            },
        );
        output.textures_delta.clear();
        (
            editor
                .view
                .transform(viewport, egui::vec2(640.0, 400.0), ctx.pixels_per_point()),
            output.platform_output.cursor_icon,
        )
    }
    fn button(pos: egui::Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn compatible_hover_excludes_selection_other_tools_and_comparison_without_mutation() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.quick_controls.set_auto_switch_tool(false);
        e.session.document.annotations.push(
            capture_core::steps::marker(
                1,
                [120.0, 100.0],
                44,
                [20, 100, 170, 255],
                (1, false),
                [640, 400],
            )
            .unwrap(),
        );
        e.tool = Some(Kind::Step);
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let (t, _) = frame(&ctx, &mut e, vec![]);
        let p = t.screen_point([120.0, 100.0]);
        frame(&ctx, &mut e, vec![Event::PointerMoved(p)]);
        assert_eq!(e.hover_annotation(p, t).map(|a| a.id), Some(1));
        e.tool = Some(Kind::Rectangle);
        assert!(e.hover_annotation(p, t).is_none());
        e.tool = None;
        assert_eq!(e.hover_annotation(p, t).map(|a| a.id), Some(1));
        e.select_only(Some(1));
        assert!(e.hover_annotation(p, t).is_none());
        e.select_only(None);
        e.shell.annotations_hidden = true;
        assert!(e.hover_annotation(p, t).is_none());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert!(!e.session.can_undo());
    }

    #[test]
    fn hovering_tools_never_edits_pixels_and_stays_inside_enabled_canvas() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let bytes = serde_json::to_vec(&editor.session.document).unwrap();
        let pixels = editor.session.document.render().unwrap();
        let (transform, _) = frame(&ctx, &mut editor, vec![]);
        let pos = transform.screen_point([400.0, 200.0]);
        let mut names = std::collections::HashSet::new();
        for tool in [
            Kind::Arrow,
            Kind::Line,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Text,
            Kind::Callout,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Step,
            Kind::Redact,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Eraser,
        ] {
            editor.tool = Some(tool);
            let (_, cursor) = frame(&ctx, &mut editor, vec![Event::PointerMoved(pos)]);
            assert_eq!(cursor, Cursor::None);
            assert!(names.insert(editor.pointer_feedback(pos, transform, false).tool.unwrap()));
        }
        editor.fill_tool.active = true;
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(pos)]).1,
            Cursor::None
        );
        assert_eq!(
            editor.pointer_feedback(pos, transform, false).tool,
            Some("Fill")
        );
        editor.fill_tool.active = false;
        editor.begin_pixel_selection();
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(pos)]).1,
            Cursor::None
        );
        editor.begin_cut_out();
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(pos)]).1,
            Cursor::None
        );
        editor.cancel_gesture();
        editor.tool = Some(Kind::Text);
        assert_eq!(
            frame(
                &ctx,
                &mut editor,
                vec![Event::PointerMoved(egui::pos2(4.0, 4.0))]
            )
            .1,
            Cursor::Default
        );
        editor.shell.help = true;
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(pos)]).1,
            Cursor::Default
        );
        editor.shell.help = false;
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerGone]).1,
            Cursor::Default
        );
        assert_eq!(serde_json::to_vec(&editor.session.document).unwrap(), bytes);
        assert_eq!(editor.session.document.render().unwrap(), pixels);
        assert!(!editor.session.can_undo());
    }

    #[test]
    fn own_tool_hover_drag_resize_and_pan_follow_real_interaction_priority() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let object = crate::Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id: 1,
            kind: Kind::Rectangle,
            start: [100.0, 100.0],
            end: [300.0, 240.0],
            width: 3,
            color: [0, 0, 0, 255],
            text: None,
            callout: None,
            points: vec![],
            corner_radius: 0.0,
            fill: None,
        };
        editor.session.document.annotations.push(object);
        editor.tool = Some(Kind::Rectangle);
        let original = serde_json::to_vec(&editor.session.document).unwrap();
        let (t, _) = frame(&ctx, &mut editor, vec![]);
        let center = t.screen_point([200.0, 170.0]);
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(center)]).1,
            Cursor::Grab
        );
        frame(
            &ctx,
            &mut editor,
            vec![
                button(center, PointerButton::Primary, true),
                button(center, PointerButton::Primary, false),
            ],
        );
        assert_eq!(editor.selected, Some(1));
        assert_eq!(editor.session.document.annotations.len(), 1);
        let corner = t.screen_point([300.0, 240.0]);
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(corner)]).1,
            Cursor::ResizeNwSe
        );
        frame(
            &ctx,
            &mut editor,
            vec![button(corner, PointerButton::Primary, true)],
        );
        let dragged = t.screen_point([330.0, 270.0]);
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(dragged)]).1,
            Cursor::ResizeNwSe
        );
        assert!(editor.edit_drag.is_some());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        frame(
            &ctx,
            &mut editor,
            vec![button(dragged, PointerButton::Primary, false)],
        );
        editor.execute(crate::Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(center),
                button(center, PointerButton::Primary, true),
            ],
        );
        let moved = center + egui::vec2(25.0, 25.0);
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(moved)]).1,
            Cursor::Grabbing
        );
        frame(
            &ctx,
            &mut editor,
            vec![button(moved, PointerButton::Primary, false)],
        );
        editor.execute(crate::Command::Undo);
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(center),
                button(center, PointerButton::Middle, true),
            ],
        );
        assert_eq!(
            frame(&ctx, &mut editor, vec![Event::PointerMoved(moved)]).1,
            Cursor::Grabbing
        );
        frame(
            &ctx,
            &mut editor,
            vec![button(moved, PointerButton::Middle, false)],
        );
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        assert_eq!(editor.tool, Some(Kind::Rectangle));
    }

    #[test]
    fn rectangular_selection_cursors_distinguish_handles_body_and_new_selection() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let (t, _) = frame(&ctx, &mut editor, vec![]);
        let mut region = crate::crop::Crop::empty();
        region.bounds = [100, 100, 300, 240];
        for (point, cursor) in [
            ([100.0, 100.0], Cursor::ResizeNwSe),
            ([300.0, 100.0], Cursor::ResizeNeSw),
            ([200.0, 100.0], Cursor::ResizeVertical),
            ([300.0, 170.0], Cursor::ResizeHorizontal),
            ([200.0, 170.0], Cursor::Grab),
        ] {
            assert_eq!(
                region.cursor(t.screen_point(point), t, [640, 400]),
                Some(cursor)
            );
        }
        assert_eq!(
            region.cursor(t.screen_point([400.0, 300.0]), t, [640, 400]),
            None
        );
        region.bounds = [0, 0, 640, 400];
        assert_eq!(
            region.cursor(t.screen_point([200.0, 170.0]), t, [640, 400]),
            None
        );
    }
}

#[cfg(test)]
mod brush_preview_tests {
    use super::*;
    #[test]
    fn pen_and_highlighter_cursor_diameter_tracks_stroke_and_canvas_zoom() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.));
        for kind in [Kind::Pen, Kind::Highlighter, Kind::Eraser] {
            for zoom in [0.5, 1., 2.5] {
                e.tool = Some(kind);
                e.width = 24;
                let draw = || {
                    crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                        egui::RawInput {
                            focused: true,
                            screen_rect: Some(area),
                            events: vec![egui::Event::PointerMoved(egui::pos2(300., 250.))],
                            ..Default::default()
                        },
                        |ui| {
                            let r = ui.interact(
                                area,
                                egui::Id::unique("brush-preview-test"),
                                egui::Sense::click(),
                            );
                            e.canvas_pointer(
                                ui,
                                &r,
                                Transform {
                                    image: area,
                                    viewport: area,
                                    scale: zoom,
                                    zoom,
                                },
                                true,
                            );
                        },
                    ))
                };
                draw();
                let output = draw();
                assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Circle(c) if (c.radius-12.*zoom).abs()<0.01)),"{kind:?} {zoom}");
                assert!(e.session.document.annotations.is_empty());
            }
        }
    }
}
