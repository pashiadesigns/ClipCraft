use crate::{Annotation, Command, Editor, egui};
#[derive(Clone)]
struct Drag {
    original: Annotation,
    from: [f32; 2],
    resize: bool,
}
impl Editor {
    pub(super) fn paint_magnified_handles(&self, ui: &egui::Ui, t: crate::canvas_view::Transform) {
        let Some(a) = self.selected_annotation() else {
            return;
        };
        let Some(b) = capture_core::magnified::lens(&a) else {
            return;
        };
        let (lo, hi) = b.body_bounds();
        let center = t.screen_point(
            b.rotation()
                .point([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]),
        );
        let corner = t.screen_point(b.rotation().point(hi));
        let p = ui.painter().with_clip_rect(t.viewport);
        p.circle_filled(center, 6., crate::shell::ACCENT);
        p.circle_stroke(center, 7., egui::Stroke::new(1., egui::Color32::WHITE));
        p.rect_filled(
            egui::Rect::from_center_size(corner, egui::vec2(10., 10.)),
            2.,
            crate::shell::ACCENT,
        );
    }
    pub(super) fn magnified_input(
        &mut self,
        ui: &egui::Ui,
        t: crate::canvas_view::Transform,
        enabled: bool,
    ) -> bool {
        let key = egui::Id::unique("magnified-callout-drag");
        let mut drag = ui.ctx().data(|d| d.get_temp::<Drag>(key));
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.ctx().data_mut(|d| d.remove::<Drag>(key));
            return drag.is_some();
        }
        let Some(a) = self
            .selected_annotation()
            .filter(|a| a.focus.and_then(|f| f.magnify).is_some())
        else {
            ui.ctx().data_mut(|d| d.remove::<Drag>(key));
            return false;
        };
        let b = capture_core::magnified::lens(&a).unwrap();
        let (lo, hi) = b.body_bounds();
        let center = t.screen_point(
            b.rotation()
                .point([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]),
        );
        let corner = t.screen_point(b.rotation().point(hi));
        let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
            return drag.is_some();
        };
        let resize = pos.distance(corner) < 12.;
        let hovered = enabled && t.viewport.contains(pos) && (resize || pos.distance(center) < 12.);
        if hovered || drag.is_some() {
            ui.ctx()
                .set_cursor_icon(if drag.as_ref().map_or(resize, |d| d.resize) {
                    egui::CursorIcon::ResizeNwSe
                } else {
                    egui::CursorIcon::Move
                });
        }
        if hovered && ui.input(|i| i.pointer.primary_pressed()) {
            drag = Some(Drag {
                original: a,
                from: t.image_point(pos),
                resize,
            });
            self.video.pause();
        }
        if let Some(d) = drag {
            let point = t.image_point(pos);
            let mut updated = d.original.clone();
            let (lo, hi) = updated.body_bounds();
            let from = updated.rotation().inverse_point(d.from);
            let to = updated.rotation().inverse_point(point);
            let m = updated.focus.as_mut().unwrap().magnify.as_mut().unwrap();
            if d.resize {
                m.scale =
                    (m.scale + 2. * (to[0] - from[0]) / (hi[0] - lo[0]).max(1.)).clamp(1., 8.);
            } else {
                for i in 0..2 {
                    m.offset[i] = (m.offset[i] + (to[i] - from[i]) / (hi[i] - lo[i]).max(1.))
                        .clamp(-100., 100.);
                }
            }
            if self
                .session
                .document
                .annotations
                .iter()
                .any(|a| a.id == updated.id && *a != updated)
            {
                self.execute(Command::Update {
                    annotation: updated,
                });
            }
            if ui.input(|i| i.pointer.primary_released() || !i.focused) {
                ui.ctx().data_mut(|data| data.remove::<Drag>(key));
            } else {
                ui.ctx().data_mut(|data| data.insert_temp(key, d));
            }
            return true;
        }
        false
    }
}
