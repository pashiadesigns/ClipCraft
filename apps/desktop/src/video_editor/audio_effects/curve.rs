use egui::Pos2;
/// Display-only monotone interpolation. Samples and measured peaks remain unchanged.
pub(super) fn smooth(points: &[Pos2]) -> Vec<Pos2> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let slopes: Vec<_> = points
        .windows(2)
        .map(|p| (p[1].y - p[0].y) / (p[1].x - p[0].x).max(0.001))
        .collect();
    let tangent = |i: usize| {
        if i == 0 {
            return slopes[0];
        }
        if i == points.len() - 1 {
            return slopes[i - 1];
        }
        let a = slopes[i - 1];
        let b = slopes[i];
        if a * b <= 0. {
            0.
        } else {
            2. * a * b / (a + b)
        }
    };
    let mut out = vec![];
    for (i, p) in points.windows(2).enumerate() {
        let dx = (p[1].x - p[0].x).max(0.001);
        let steps = (dx / 1.5).ceil().clamp(2., 16.) as usize;
        for j in 0..steps {
            let t = j as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let y = (2. * t3 - 3. * t2 + 1.) * p[0].y
                + (t3 - 2. * t2 + t) * dx * tangent(i)
                + (-2. * t3 + 3. * t2) * p[1].y
                + (t3 - t2) * dx * tangent(i + 1);
            out.push(egui::pos2(
                p[0].x + dx * t,
                y.clamp(p[0].y.min(p[1].y), p[0].y.max(p[1].y)),
            ));
        }
    }
    out.push(*points.last().unwrap());
    out
}

#[test]
fn smoothed_measurements_keep_samples_and_never_invent_peaks() {
    let p = [
        egui::pos2(0., 2.),
        egui::pos2(10., 20.),
        egui::pos2(30., -3.),
        egui::pos2(50., 10.),
    ];
    let out = smooth(&p);
    for point in p {
        assert!(out.contains(&point));
    }
    for point in out {
        let ends = p
            .windows(2)
            .find(|v| v[0].x <= point.x && point.x <= v[1].x)
            .unwrap();
        assert!(point.y >= ends[0].y.min(ends[1].y) && point.y <= ends[0].y.max(ends[1].y));
    }
}
