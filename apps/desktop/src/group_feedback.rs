use crate::{Editor, canvas_view::Transform, egui};
pub(super) struct Feedback {
    pub ids: Vec<u64>,
    pub started: std::time::Instant,
}
fn appearance(elapsed: f32, motion: bool) -> Option<(f32, f32)> {
    const DRAW: f32 = 0.550;
    const HOLD: f32 = 0.600;
    const FADE: f32 = 1.;
    if elapsed >= if motion { DRAW + HOLD + FADE } else { 1. } {
        return None;
    }
    Some(if motion {
        (
            (2_f32.powf(6. * (elapsed / DRAW).clamp(0., 1.)) - 1.) / 63.,
            (1. - (elapsed - DRAW - HOLD).max(0.) / FADE).clamp(0., 1.),
        )
    } else {
        (1., 1.)
    })
}
fn dot_diameter(zoom: f32) -> f32 {
    (2.5 * zoom.max(0.).sqrt()).clamp(2., 4.)
}
impl Editor {
    pub(super) fn paint_group_feedback(&mut self, ui: &egui::Ui, t: Transform) {
        let Some(f) = &self.group_feedback else {
            return;
        };
        let elapsed = f.started.elapsed().as_secs_f32();
        let motion = crate::motion::enabled();
        let Some((progress, alpha)) = appearance(elapsed, motion) else {
            self.group_feedback = None;
            return;
        };
        let mut bounds = egui::Rect::NOTHING;
        for a in self
            .session
            .document
            .annotations
            .iter()
            .filter(|a| f.ids.contains(&a.id))
        {
            let (lo, hi) = a.bounds();
            bounds = bounds.union(egui::Rect::from_two_pos(
                t.screen_point(lo),
                t.screen_point(hi),
            ));
        }
        if !bounds.is_finite() {
            return;
        }
        // Eight screen pixels outside the existing selection boundary, at every zoom.
        let r = bounds.expand(5. + 8.);
        let diameter = dot_diameter(t.zoom);
        let points = [
            r.center_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
            r.left_top(),
            r.center_top(),
        ];
        let length = 2. * (r.width() + r.height());
        let visible = progress * length;
        let color = crate::shell::ACCENT.linear_multiply(alpha);
        let painter = ui.painter().with_clip_rect(t.viewport);
        let mut offset = 0.;
        let mut dot = 0.;
        for p in points.windows(2) {
            let len = p[0].distance(p[1]);
            while dot <= offset + len && dot <= visible {
                painter.circle_filled(
                    p[0] + (p[1] - p[0]) * ((dot - offset) / len.max(0.001)),
                    diameter / 2.,
                    color,
                );
                dot += diameter + 8.;
            }
            offset += len;
        }
        if (0.550..1.150).contains(&elapsed) || !motion {
            let end = if motion { 1.150 } else { 1. };
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f32(
                    (end - elapsed).max(0.001),
                ));
        } else {
            ui.ctx().request_repaint();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approved_timing_and_successful_group_trigger() {
        assert!((appearance(0.275, true).unwrap().0 - 1. / 9.).abs() < 0.001);
        assert_eq!(appearance(0.550, true), Some((1., 1.)));
        assert_eq!(appearance(1.150, true), Some((1., 1.)));
        assert!((appearance(1.650, true).unwrap().1 - 0.5).abs() < 0.001);
        assert_eq!(appearance(2.150, true), None);
        assert_eq!(appearance(0.1, false), Some((1., 1.)));
        assert_eq!(appearance(1., false), None);
        for zoom in [0.1, 0.5, 1., 2., 16.] {
            assert!((2. ..=4.).contains(&dot_diameter(zoom)));
        }
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.execute(crate::Command::GroupObjects { ids: vec![] });
        assert!(e.group_feedback.is_none());
        let dims = [e.session.document.width, e.session.document.height];
        for id in [1, 2] {
            e.execute(crate::Command::Add {
                annotation: capture_core::steps::marker(
                    id,
                    [50. * id as f32, 50.],
                    30,
                    [0, 0, 0, 255],
                    (id as u32, false),
                    dims,
                )
                .unwrap(),
            });
        }
        e.execute(crate::Command::GroupObjects { ids: vec![1, 2] });
        assert!(e.group_feedback.is_some());
        assert_eq!(e.status, "Objects grouped.");
    }
}
