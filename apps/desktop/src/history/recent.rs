//! Pin and close on the Recent strip (LIB-007, approved mockup
//! `artifacts/br-proposal/roadmap-ui.html`). Pinned items stay first. Close
//! takes an item out of Recent and keeps it in the Library.
use super::*;
use crate::tips::Tip as _;

/// Pinned items first, without closed ones, otherwise in Library order.
pub(super) fn recent_order(entries: Vec<Entry>) -> Vec<Entry> {
    let (pinned, rest): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .filter(|e| !e.organize.closed)
        .partition(|e| e.organize.pinned);
    pinned.into_iter().chain(rest).collect()
}
fn star_points(center: egui::Pos2, radius: f32) -> Vec<egui::Pos2> {
    (0..10)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / 10.;
            let r = if i % 2 == 0 { radius } else { radius * 0.42 };
            center + egui::vec2(angle.cos(), angle.sin()) * r
        })
        .collect()
}
/// The pin mark in a card's top-left corner.
pub(super) fn paint_star(ui: &egui::Ui, card: egui::Rect, pinned: bool) {
    let center = card.left_top() + egui::vec2(13., 13.);
    let painter = ui.painter();
    painter.circle_filled(
        center,
        10.,
        if pinned {
            crate::shell::ACCENT
        } else {
            egui::Color32::from_black_alpha(150)
        },
    );
    painter.add(egui::Shape::closed_line(
        star_points(center, 7.),
        egui::Stroke::new(1.3, egui::Color32::WHITE),
    ));
}
impl Editor {
    /// Pin and close marks over a Recent card.
    pub(super) fn recent_marks(&mut self, ui: &mut egui::Ui, card: &egui::Response, entry: &Entry) {
        let id = ui.make_persistent_id(("recent-marks", &entry.id));
        let pin_rect = egui::Rect::from_center_size(
            card.rect.left_top() + egui::vec2(13., 13.),
            egui::vec2(22., 22.),
        );
        let close_rect = egui::Rect::from_center_size(
            card.rect.right_top() + egui::vec2(-13., 13.),
            egui::vec2(22., 22.),
        );
        let pin = ui
            .interact(pin_rect, id.with("pin"), egui::Sense::click())
            .tip_title(if entry.organize.pinned {
                "Unpin"
            } else {
                "Pin"
            })
            .tip("Keep this item first in Recent.");
        let close = ui
            .interact(close_rect, id.with("close"), egui::Sense::click())
            .tip_title("Close")
            .tip("Take this item out of Recent. It stays in the Library.");
        let hot = card.contains_pointer() || pin.contains_pointer() || close.contains_pointer();
        if entry.organize.pinned || hot {
            paint_star(ui, card.rect, entry.organize.pinned);
        }
        if hot {
            let c = close_rect.center();
            let painter = ui.painter();
            painter.circle_filled(
                c,
                10.,
                if close.contains_pointer() {
                    egui::Color32::from_rgb(190, 60, 60)
                } else {
                    egui::Color32::from_black_alpha(150)
                },
            );
            let stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
            for sign in [-1., 1.] {
                painter.line_segment(
                    [
                        c + egui::vec2(-4., sign * 4.),
                        c + egui::vec2(4., -sign * 4.),
                    ],
                    stroke,
                );
            }
        }
        if pin.clicked()
            && let Some(store) = self.history.store.as_mut()
        {
            match store.set_pinned(&entry.id, !entry.organize.pinned) {
                Ok(()) => self.history.refresh(),
                Err(e) => self.history.error = crate::guided::friendly(&e.to_string()),
            }
        }
        if close.clicked()
            && let Some(store) = self.history.store.as_mut()
        {
            match store.set_closed(&entry.id, true) {
                Ok(()) => self.history.refresh(),
                Err(e) => self.history.error = crate::guided::friendly(&e.to_string()),
            }
        }
    }
    pub(super) fn reopen_in_recent(&mut self, id: &str) {
        let closed = self
            .history
            .entries()
            .iter()
            .any(|e| e.id == id && e.organize.closed);
        if closed
            && let Some(store) = self.history.store.as_mut()
            && store.set_closed(id, false).is_ok()
        {
            self.history.refresh();
        }
    }
}
