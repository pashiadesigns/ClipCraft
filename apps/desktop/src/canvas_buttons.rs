//! Screen-space collision policy shared by canvas action buttons. Do not leave
//! hidden controls hit-testable, and do not latch visibility to a zoom preset.
use crate::egui::Rect;

pub(super) fn visible(rects: &[Rect]) -> Vec<bool> {
    let mut visible = vec![true; rects.len()];
    let mut order: Vec<_> = (0..rects.len()).collect();
    order.sort_unstable_by(|&a, &b| rects[a].top().total_cmp(&rects[b].top()));
    let mut active: Vec<usize> = Vec::new();
    for index in order {
        let rect = rects[index].expand(2.);
        active.retain(|&other| rects[other].bottom() + 2. > rect.top());
        for &other in &active {
            if rect.intersect(rects[other].expand(2.)).is_positive() {
                visible[index] = false;
                visible[other] = false;
            }
        }
        active.push(index);
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egui::{pos2, vec2};
    #[test]
    fn colliding_buttons_hide_symmetrically_and_return_at_first_clear_spacing() {
        let button = |x| Rect::from_min_size(pos2(x, 0.), vec2(24., 24.));
        for spacing in [10., 20., 27., 28., 28.1, 29., 40.] {
            let shown = visible(&[button(0.), button(spacing), button(100.)]);
            assert_eq!(shown, vec![spacing >= 28., spacing >= 28., true]);
        }
    }
}
