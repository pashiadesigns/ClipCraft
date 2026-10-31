//! Workspace overlay only: no document command, source pixels or saved state.
use crate::{Editor, canvas_view::Transform, egui};

pub const MIN_ZOOM: f32 = 6.0;

pub fn segments(transform: Transform, dimensions: [u32; 2]) -> Vec<[egui::Pos2; 2]> {
    let visible = transform.image.intersect(transform.viewport);
    if transform.zoom < MIN_ZOOM || !visible.is_positive() {
        return vec![];
    }
    let first = transform.image_point(visible.min);
    let last = transform.image_point(visible.max);
    let range = |axis: usize| {
        first[axis].ceil().max(0.0) as u32..=last[axis].floor().min(dimensions[axis] as f32) as u32
    };
    let mut lines = Vec::new();
    for x in range(0) {
        let x = transform.screen_point([x as f32, 0.0]).x;
        lines.push([
            egui::pos2(x, visible.top()),
            egui::pos2(x, visible.bottom()),
        ]);
    }
    for y in range(1) {
        let y = transform.screen_point([0.0, y as f32]).y;
        lines.push([
            egui::pos2(visible.left(), y),
            egui::pos2(visible.right(), y),
        ]);
    }
    lines
}

pub fn paint(ui: &egui::Ui, transform: Transform, dimensions: [u32; 2]) {
    let painter = ui
        .painter()
        .with_clip_rect(transform.image.intersect(transform.viewport));
    let physical_pixel = 1.0 / ui.ctx().pixels_per_point();
    for line in segments(transform, dimensions) {
        // Contrasting thin strokes stay visible over both dark and light pixels.
        painter.line_segment(
            line,
            egui::Stroke::new(2.0 * physical_pixel, egui::Color32::from_black_alpha(100)),
        );
        painter.line_segment(
            line,
            egui::Stroke::new(physical_pixel, egui::Color32::from_white_alpha(130)),
        );
    }
}

impl Editor {
    pub fn toggle_pixel_grid(&mut self) {
        self.shell.pixel_grid = !self.shell.pixel_grid;
        self.status = if self.shell.pixel_grid {
            "Pixel grid enabled. Visible at 600% zoom and above; exported pixels are unchanged."
        } else {
            "Pixel grid hidden."
        }
        .into();
    }

    pub fn pixel_grid_shortcut(&mut self, ctx: &egui::Context) {
        let owns_input = self.pending.is_none()
            && self.new_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.text_draft.is_none()
            && self.group_drag.is_none()
            && self.edit_drag.is_none()
            && self.property_preview.is_none()
            && self.drag_start.is_none()
            && !self.capture_requested
            && self.capture_result.is_none()
            && !self.view.panning
            && !self.shell.help
            && self.shell.canvas_enabled()
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx);
        if owns_input
            && ctx.input_mut(|input| {
                crate::shortcuts::consume_exact(
                    input,
                    egui::Modifiers::COMMAND | egui::Modifiers::ALT,
                    egui::Key::G,
                )
            })
        {
            self.toggle_pixel_grid();
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_view::{CanvasView, ZoomAction, ZoomMode};

    #[test]
    fn grid_tracks_visible_pixel_boundaries_at_high_zoom_and_mixed_dpi() {
        let viewport = egui::Rect::from_min_size(egui::pos2(30.5, 70.25), egui::vec2(601.0, 401.0));
        for ppp in [1.0, 1.25, 2.0] {
            for zoom in [5.99, 6.0, 8.0, 16.0] {
                for pan in [
                    egui::Vec2::ZERO,
                    egui::vec2(123.3, -68.1),
                    egui::vec2(1e6, -1e6),
                ] {
                    let mut view = CanvasView::default();
                    view.apply(ZoomAction::Set(ZoomMode::Manual(zoom)), None);
                    view.pan_by(pan);
                    let transform = view.transform(viewport, egui::vec2(32768.0, 700.0), ppp);
                    let lines = segments(transform, [32768, 700]);
                    if zoom < MIN_ZOOM {
                        assert!(lines.is_empty());
                        continue;
                    }
                    assert!(!lines.is_empty());
                    assert!(
                        lines.len()
                            <= ((viewport.width() + viewport.height()) / transform.scale).ceil()
                                as usize
                                + 2
                    );
                    for [a, b] in lines {
                        assert!(viewport.expand(0.01).contains(a));
                        assert!(viewport.expand(0.01).contains(b));
                        let image = transform.image_point(a);
                        let coordinate = if a.x == b.x { image[0] } else { image[1] };
                        assert!((coordinate - coordinate.round()).abs() < 0.01);
                    }
                }
            }
        }
    }
}
