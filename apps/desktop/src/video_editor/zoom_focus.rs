use super::*;
use egui::{Color32, Rect, Stroke};
pub(super) struct FocusDrag {
    pub original: Project,
    pub from: [f32; 2],
    pub corner: Option<usize>,
}
impl Editor {
    pub(crate) fn paint_zoom_focus(&self, ui: &egui::Ui, t: crate::canvas_view::Transform) {
        if !self.video.zoom_edit {
            return;
        }
        let Some(z) = self
            .video
            .timing_preview
            .as_ref()
            .or(self.session.document.video.as_ref())
            .and_then(|p| {
                p.zooms
                    .iter()
                    .find(|z| Some(z.id) == self.video.zoom_selected)
            })
        else {
            return;
        };
        let size = [
            self.session.document.width as f32,
            self.session.document.height as f32,
        ];
        let half = 0.5 / z.scale;
        let rect = Rect::from_min_max(
            t.screen_point([
                (z.center[0] - half) * size[0],
                (z.center[1] - half) * size[1],
            ]),
            t.screen_point([
                (z.center[0] + half) * size[0],
                (z.center[1] + half) * size[1],
            ]),
        );
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        let painter = ui.painter().with_clip_rect(t.viewport.intersect(t.image));
        painter.rect(
            rect,
            0.,
            Color32::from_rgba_unmultiplied(25, 158, 200, 24),
            Stroke::new(2., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
        for c in corners {
            painter.rect(
                Rect::from_center_size(c, egui::vec2(9., 9.)),
                1.,
                Color32::WHITE,
                Stroke::new(1., crate::shell::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        painter.circle_stroke(rect.center(), 7., Stroke::new(2., crate::shell::ACCENT));
        painter.text(
            rect.left_top() + egui::vec2(12., 12.),
            egui::Align2::LEFT_TOP,
            format!("Zoom focus · {:.1}×", z.scale),
            egui::FontId::proportional(13.),
            crate::shell::ACCENT,
        );
    }
    pub(super) fn zoom_focus_input(
        &mut self,
        ui: &egui::Ui,
        t: crate::canvas_view::Transform,
        enabled: bool,
    ) -> bool {
        if !self.video.zoom_edit {
            return false;
        }
        let Some(id) = self.video.zoom_selected else {
            self.video.zoom_edit = false;
            return false;
        };
        let p = self
            .video
            .timing_preview
            .as_ref()
            .or(self.session.document.video.as_ref())
            .unwrap();
        let Some(z) = p.zooms.iter().find(|z| z.id == id).cloned() else {
            self.video.zoom_edit = false;
            return false;
        };
        let size = [
            self.session.document.width as f32,
            self.session.document.height as f32,
        ];
        let half = 0.5 / z.scale;
        let rect = Rect::from_min_max(
            t.screen_point([
                (z.center[0] - half) * size[0],
                (z.center[1] - half) * size[1],
            ]),
            t.screen_point([
                (z.center[0] + half) * size[0],
                (z.center[1] + half) * size[1],
            ]),
        );
        let corners = [
            rect.left_top(),
            rect.right_top(),
            rect.right_bottom(),
            rect.left_bottom(),
        ];
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.zoom_focus_drag = None;
            self.video.timing_preview = None;
            self.video.zoom_edit = false;
            return true;
        }
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let image = t.image_point(pos);
            let point = [image[0] / size[0], image[1] / size[1]];
            let corner = corners.iter().position(|c| c.distance(pos) < 12.);
            if enabled && t.viewport.contains(pos) && (rect.contains(pos) || corner.is_some()) {
                ui.ctx().set_cursor_icon(match corner {
                    Some(0 | 2) => egui::CursorIcon::ResizeNwSe,
                    Some(_) => egui::CursorIcon::ResizeNeSw,
                    None => egui::CursorIcon::Move,
                });
                if ui.input(|i| i.pointer.primary_pressed()) {
                    self.video.zoom_focus_drag = Some(FocusDrag {
                        original: self.session.document.video.clone().unwrap(),
                        from: point,
                        corner,
                    });
                    self.video.pause();
                }
            }
            if let Some(d) = &self.video.zoom_focus_drag {
                let mut next = d.original.clone();
                let target = next.zooms.iter_mut().find(|z| z.id == id).unwrap();
                if let Some(corner) = d.corner {
                    let h = 0.5 / target.scale;
                    let signs = match corner {
                        0 => [-1., -1.],
                        1 => [1., -1.],
                        2 => [1., 1.],
                        _ => [-1., 1.],
                    };
                    let fixed = [
                        target.center[0] - signs[0] * h,
                        target.center[1] - signs[1] * h,
                    ];
                    let max_side = (if signs[0] > 0. {
                        1. - fixed[0]
                    } else {
                        fixed[0]
                    })
                    .min(if signs[1] > 0. {
                        1. - fixed[1]
                    } else {
                        fixed[1]
                    })
                    .max(0.1);
                    let side = ((point[0] - fixed[0]) * signs[0])
                        .max((point[1] - fixed[1]) * signs[1])
                        .clamp(0.1, max_side);
                    target.scale = 1. / side;
                    target.center = [
                        fixed[0] + signs[0] * side / 2.,
                        fixed[1] + signs[1] * side / 2.,
                    ];
                } else {
                    let h = 0.5 / target.scale;
                    for (i, value) in point.iter().enumerate() {
                        target.center[i] = (target.center[i] + value - d.from[i]).clamp(h, 1. - h);
                    }
                }
                ui.ctx().set_cursor_icon(match d.corner {
                    Some(0 | 2) => egui::CursorIcon::ResizeNwSe,
                    Some(_) => egui::CursorIcon::ResizeNeSw,
                    None => egui::CursorIcon::Move,
                });
                self.video.timing_preview = Some(next);
                self.needs_render = true;
                if ui.input(|i| i.pointer.primary_released()) {
                    self.video.zoom_focus_drag = None;
                    let p = self.video.timing_preview.take().unwrap();
                    self.commit_video(p);
                }
            }
        }
        true
    }
}
