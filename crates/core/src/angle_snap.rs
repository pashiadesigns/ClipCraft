//! Shift-constrained drawing. Angle/radius approach adapted from DesignCraft
//! geom::constrain_angle (ea08e4c2ba290f092063c99d70c4bdd5afc45887), MIT.
//! See licenses/designcraft. ClipDesigner adds exact cardinal axes and ray clipping.
pub fn endpoint(start: [f32; 2], pointer: [f32; 2], canvas: [u32; 2], shift: bool) -> [f32; 2] {
    if !shift {
        return [
            pointer[0].clamp(0.0, canvas[0] as f32),
            pointer[1].clamp(0.0, canvas[1] as f32),
        ];
    }
    let delta = [pointer[0] - start[0], pointer[1] - start[1]];
    let diagonal = std::f32::consts::FRAC_1_SQRT_2;
    let octant = (delta[1].atan2(delta[0]) / std::f32::consts::FRAC_PI_4).round() as i32;
    let direction = [
        [1.0, 0.0],
        [diagonal, diagonal],
        [0.0, 1.0],
        [-diagonal, diagonal],
        [-1.0, 0.0],
        [-diagonal, -diagonal],
        [0.0, -1.0],
        [diagonal, -diagonal],
    ][octant.rem_euclid(8) as usize];
    let mut length = delta[0].hypot(delta[1]);
    for axis in 0..2 {
        if direction[axis] > 0.0 {
            length = length.min((canvas[axis] as f32 - start[axis]) / direction[axis]);
        } else if direction[axis] < 0.0 {
            length = length.min(-start[axis] / direction[axis]);
        }
    }
    [
        start[0] + length * direction[0],
        start[1] + length * direction[1],
    ]
}

/// Equal-sided creation box, using the dominant drag axis and a shared
/// boundary limit so clipping cannot turn a square back into a rectangle.
pub fn square_endpoint(start: [f32; 2], pointer: [f32; 2], canvas: [u32; 2]) -> [f32; 2] {
    let delta = [pointer[0] - start[0], pointer[1] - start[1]];
    let direction = delta.map(|d| if d < 0.0 { -1.0 } else { 1.0 });
    let mut size = delta[0].abs().max(delta[1].abs());
    for axis in 0..2 {
        size = size.min(if direction[axis] > 0.0 {
            canvas[axis] as f32 - start[axis]
        } else {
            start[axis]
        });
    }
    [
        start[0] + direction[0] * size,
        start[1] + direction[1] * size,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn square_creation_preserves_quadrant_and_equal_sides_at_every_edge() {
        for start in [[100.0, 80.0], [0.0, 0.0], [200.0, 160.0]] {
            for delta in [
                [140.0, 20.0],
                [-140.0, 20.0],
                [10.0, -250.0],
                [-10.0, -250.0],
                [0.0, 0.0],
            ] {
                let end = square_endpoint(
                    start,
                    [start[0] + delta[0], start[1] + delta[1]],
                    [200, 160],
                );
                assert!((end[0] - start[0]).abs() == (end[1] - start[1]).abs());
                assert!((0.0..=200.0).contains(&end[0]) && (0.0..=160.0).contains(&end[1]));
                for axis in 0..2 {
                    assert!((end[axis] - start[axis]) * delta[axis] >= 0.0);
                }
            }
        }
    }
    #[test]
    fn all_octants_preserve_length_and_boundaries_keep_the_angle() {
        for degrees in (0..360).step_by(7) {
            let a = (degrees as f32).to_radians();
            let end = endpoint(
                [200.0, 200.0],
                [200.0 + a.cos() * 90.0, 200.0 + a.sin() * 90.0],
                [400, 400],
                true,
            );
            let d = [end[0] - 200.0, end[1] - 200.0];
            assert!((d[0].hypot(d[1]) - 90.0).abs() < 0.001);
            assert!(d[0] == 0.0 || d[1] == 0.0 || (d[0].abs() - d[1].abs()).abs() < 0.001);
        }
        for p in [
            [1000.0, 800.0],
            [-500.0, 800.0],
            [700.0, -300.0],
            [-700.0, -800.0],
        ] {
            let end = endpoint([60.0, 40.0], p, [100, 80], true);
            assert!(end[0] >= -0.001 && end[0] <= 100.001 && end[1] >= -0.001 && end[1] <= 80.001);
            let d = [end[0] - 60.0, end[1] - 40.0];
            assert!(d[0] == 0.0 || d[1] == 0.0 || (d[0].abs() - d[1].abs()).abs() < 0.001);
        }
        assert_eq!(
            endpoint([10.0, 20.0], [72.0, 37.0], [100, 80], false),
            [72.0, 37.0]
        );
        assert_eq!(
            endpoint([10.0, 20.0], [10.0, 20.0], [100, 80], true),
            [10.0, 20.0]
        );
    }
}
