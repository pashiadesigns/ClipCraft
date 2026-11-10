//! Turns a track's frame-by-frame required cover into the box that is shown.
//! A value stays one steady size: the box grows ahead of fast movement, holds
//! through it and eases back, and it always contains what each frame requires.
use crate::video::motion_path::Node;

/// How long a burst of movement keeps the box at its larger size, each side.
const HOLD_MS: u64 = 200;
/// How fast the size eases, in pixels per millisecond at 1080p.
const EASE: f32 = 0.16;
/// Centers that stay within this many pixels count as not moving.
const STEADY_PX: f32 = 4.;
/// Sizes up to this far past the typical size belong to its steady size.
const USUAL: f32 = 1.25;

/// `page` is the top and bottom of the scrolling page; pass it only for a
/// value that lives on the page. Nodes wholly outside it become invisible.
pub fn apply(nodes: &mut [Node], value_height: f32, size: [u32; 2], page: Option<[f32; 2]>) {
    if nodes.is_empty() {
        return;
    }
    // Recognized bounds can run short at the ends of a word; a line's height
    // is reliable, and more would cover the lines above and below.
    let grow = [2. + value_height * 0.12, 2. + value_height * 0.08];
    let sides: Vec<[f32; 4]> = nodes
        .iter()
        .map(|n| {
            [
                n.rect[0] - grow[0],
                n.rect[1] - grow[1],
                n.rect[0] + n.rect[2] + grow[0],
                n.rect[1] + n.rect[3] + grow[1],
            ]
        })
        .collect();
    let times: Vec<u64> = nodes.iter().map(|n| n.at).collect();
    let shown: Vec<bool> = nodes.iter().map(|n| n.visible).collect();
    let ease = EASE * size[1] as f32 / 1080.;
    let axis = |a: usize| {
        let center = steady_centers(&sides, a);
        // A hidden node requires nothing.
        let half: Vec<f32> = sides
            .iter()
            .zip(&center)
            .zip(&shown)
            .map(|((s, c), on)| {
                if *on {
                    (c - s[a]).max(s[a + 2] - c)
                } else {
                    0.
                }
            })
            .collect();
        let half = envelope(&half, &times, ease);
        (center, half)
    };
    let (cx, hx) = axis(0);
    let (cy, hy) = axis(1);
    let (width, height) = (size[0] as f32, size[1] as f32);
    let [top, bottom] = page.unwrap_or([0., height]);
    for (i, n) in nodes.iter_mut().enumerate() {
        let x0 = (cx[i] - hx[i]).clamp(0., width - 1.);
        let x1 = (cx[i] + hx[i]).clamp(x0 + 1., width);
        let y0 = (cy[i] - hy[i]).max(top);
        let y1 = (cy[i] + hy[i]).min(bottom);
        if y1 - y0 < 1. || !shown[i] {
            n.visible = false;
            let y = y0.clamp(0., height - 1.);
            n.rect = [x0, y, x1 - x0, (y1 - y0).max(1.)];
        } else {
            n.rect = [x0, y0, x1 - x0, y1 - y0];
        }
    }
}
/// The center of each cover along one axis, steadied where it isn't moving.
fn steady_centers(sides: &[[f32; 4]], axis: usize) -> Vec<f32> {
    let raw: Vec<f32> = sides.iter().map(|s| (s[axis] + s[axis + 2]) / 2.).collect();
    let mut held = raw.first().copied().unwrap_or(0.);
    (0..raw.len())
        .map(|i| {
            let mut near = raw[i.saturating_sub(2)..(i + 3).min(raw.len())].to_vec();
            near.sort_by(f32::total_cmp);
            if near[near.len() - 1] - near[0] > STEADY_PX {
                held = raw[i];
            } else if (raw[i] - held).abs() > STEADY_PX {
                // A slow drift steps rarely instead of following every reading.
                held = near[near.len() / 2];
            }
            held
        })
        .collect()
}
/// A size that never falls below `half`, and changes only by easing.
fn envelope(half: &[f32], times: &[u64], ease: f32) -> Vec<f32> {
    let mut sorted = half.to_vec();
    sorted.sort_by(f32::total_cmp);
    let usual = sorted[sorted.len() / 2] * USUAL;
    let steady = half
        .iter()
        .copied()
        .filter(|v| *v <= usual)
        .fold(0., f32::max);
    let mut out: Vec<f32> = times
        .iter()
        .map(|t| {
            let a = times.partition_point(|v| v + HOLD_MS < *t);
            let b = times.partition_point(|v| *v <= t + HOLD_MS);
            half[a..b].iter().copied().fold(steady, f32::max)
        })
        .collect();
    for i in 1..out.len() {
        out[i] = out[i].max(out[i - 1] - ease * (times[i] - times[i - 1]) as f32);
    }
    for i in (0..out.len() - 1).rev() {
        out[i] = out[i].max(out[i + 1] - ease * (times[i + 1] - times[i]) as f32);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const SIZE: [u32; 2] = [1920, 1080];
    fn node(at: u64, rect: [f32; 4]) -> Node {
        Node {
            at,
            rect,
            visible: true,
            sprite: 0,
            confidence: 1.,
        }
    }
    fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
        outer[0] <= inner[0] + 0.01
            && outer[1] <= inner[1] + 0.01
            && outer[0] + outer[2] >= inner[0] + inner[2] - 0.01
            && outer[1] + outer[3] >= inner[1] + inner[3] - 0.01
    }
    #[test]
    fn a_still_value_keeps_one_size_through_uneven_readings() {
        let mut nodes: Vec<Node> = (0..120)
            .map(|i| {
                let wide = i % 15 != 0;
                let r = if wide {
                    [100., 200., 188., 36.]
                } else {
                    [103., 203., 182., 30.]
                };
                node(i as u64 * 33, r)
            })
            .collect();
        apply(&mut nodes, 30., SIZE, None);
        assert!(
            nodes.windows(2).all(|n| n[0].rect == n[1].rect),
            "{:?}",
            nodes.iter().map(|n| n.rect).collect::<Vec<_>>()
        );
    }
    #[test]
    fn the_shown_box_contains_every_frame_and_changes_size_gradually() {
        // Per-frame unions that pump with scroll speed.
        let speeds = [0, 0, 20, 56, 26, 70, 18, 90, 30, 60, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut y = 900.;
        let mut before = vec![];
        for (i, v) in speeds.iter().cycle().take(180).enumerate() {
            y = (y - *v as f32 * 0.2_f32).max(100.);
            before.push(node(i as u64 * 33, [460., y, 190., 36. + *v as f32]));
        }
        let mut nodes = before.clone();
        apply(&mut nodes, 30., SIZE, None);
        for (a, b) in nodes.iter().zip(&before) {
            assert!(contains(a.rect, b.rect), "{:?} {:?}", a.rect, b.rect);
        }
        let step = 33. * EASE + 0.01;
        for n in nodes.windows(2) {
            assert!((n[0].rect[3] - n[1].rect[3]).abs() <= 2. * step, "{n:?}");
            assert!((n[0].rect[2] - n[1].rect[2]).abs() <= 2. * step, "{n:?}");
        }
    }
    #[test]
    fn small_differences_in_position_do_not_move_the_box() {
        let wobble = [0., 1.5, -1., 2., 0.5, -1.5, 1., 0., -2., 1.5];
        let mut nodes: Vec<Node> = wobble
            .iter()
            .cycle()
            .take(90)
            .enumerate()
            .map(|(i, d)| node(i as u64 * 33, [300. + d, 500. - d, 180., 30.]))
            .collect();
        apply(&mut nodes, 30., SIZE, None);
        assert!(nodes.windows(2).all(|n| n[0].rect == n[1].rect));
    }
    #[test]
    fn a_page_value_is_clipped_to_the_page_and_hidden_outside_it() {
        let mut nodes = vec![
            node(0, [100., 1000., 180., 30.]),
            node(33, [100., 1060., 180., 18.]),
            node(66, [100., 40., 180., 30.]),
        ];
        apply(&mut nodes, 30., SIZE, Some([60., 990.]));
        assert!(!nodes[1].visible);
        for n in nodes.iter().filter(|n| n.visible) {
            assert!(n.rect[1] >= 60. && n.rect[1] + n.rect[3] <= 990., "{n:?}");
        }
    }
}
