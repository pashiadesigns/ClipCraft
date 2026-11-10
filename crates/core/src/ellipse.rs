//! Ellipse anchors adapted from DesignCraft's geom/src/shapes.rs, MIT licensed.
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! Upstream ea08e4c2ba290f092063c99d70c4bdd5afc45887; license: licenses/designcraft/LICENSE-MIT.
//! Changes: image-coordinate arrays, bounded cubic sampling and ClipCraft rendering/hit testing.
use crate::{Annotation, Kind};
use image::RgbaImage;

const KAPPA: f64 = 0.552_284_749_830_793_6;

/// Closed, bounded approximation shared by drawing preview and exported strokes.
pub fn outline(start: [f32; 2], end: [f32; 2]) -> Vec<[f32; 2]> {
    if start.iter().chain(&end).any(|v| !v.is_finite()) {
        return Vec::new();
    }
    let min = [start[0].min(end[0]), start[1].min(end[1])].map(f64::from);
    let max = [start[0].max(end[0]), start[1].max(end[1])].map(f64::from);
    let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    let rx = (max[0] - min[0]) * 0.5;
    let ry = (max[1] - min[1]) * 0.5;
    if rx == 0.0 || ry == 0.0 {
        return vec![min.map(|v| v as f32), max.map(|v| v as f32)];
    }
    // DesignCraft's four smooth anchors: left, top, right, bottom.
    let anchors = [
        [c[0] - rx, c[1]],
        [c[0], c[1] - ry],
        [c[0] + rx, c[1]],
        [c[0], c[1] + ry],
    ];
    let incoming = [
        [0.0, ry * KAPPA],
        [-rx * KAPPA, 0.0],
        [0.0, -ry * KAPPA],
        [rx * KAPPA, 0.0],
    ];
    let steps = (rx.max(ry) / 0.08).sqrt().ceil().clamp(8.0, 512.0) as usize;
    let mut path = Vec::with_capacity(4 * steps + 1);
    for i in 0..4 {
        let next = (i + 1) % 4;
        let a = anchors[i];
        let b = [a[0] - incoming[i][0], a[1] - incoming[i][1]];
        let d = anchors[next];
        let c = [d[0] + incoming[next][0], d[1] + incoming[next][1]];
        for n in 0..steps {
            let t = n as f64 / steps as f64;
            let u = 1.0 - t;
            path.push(std::array::from_fn(|axis| {
                (u.powi(3) * a[axis]
                    + 3.0 * u * u * t * b[axis]
                    + 3.0 * u * t * t * c[axis]
                    + t.powi(3) * d[axis]) as f32
            }));
        }
    }
    path.push(path[0]);
    path
}

pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let mut stroke = annotation.clone();
    stroke.kind = Kind::Pen;
    stroke.points = outline(annotation.start, annotation.end);
    crate::arrow::render(image, &stroke);
}

pub(crate) fn hit_test(annotation: &Annotation, p: [f32; 2], tolerance: f32) -> bool {
    let (min, max) = annotation.body_bounds();
    let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    let r = [(max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5];
    if r[0] > 0.0
        && r[1] > 0.0
        && ((p[0] - c[0]) / r[0]).powi(2) + ((p[1] - c[1]) / r[1]).powi(2) <= 1.0
    {
        return true;
    }
    outline(min, max)
        .windows(2)
        .any(|pair| crate::selection::segment_distance(p, pair[0], pair[1]) <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session, selection::Handle};
    // Adapted upstream ellipse_bounds_and_area regression, plus ClipCraft history/output checks.
    #[test]
    fn bounds_area_reverse_and_degenerate_paths() {
        let path = outline([0.0, 0.0], [100.0, 60.0]);
        assert_eq!(path, outline([100.0, 60.0], [0.0, 0.0]));
        assert_eq!(path.first(), path.last());
        for axis in 0..2 {
            assert_eq!(
                path.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min),
                0.0
            );
            assert_eq!(
                path.iter()
                    .map(|p| p[axis])
                    .fold(f32::NEG_INFINITY, f32::max),
                [100.0, 60.0][axis]
            );
        }
        let area: f64 = path
            .windows(2)
            .map(|p| {
                f64::from(p[0][0]) * f64::from(p[1][1]) - f64::from(p[1][0]) * f64::from(p[0][1])
            })
            .sum::<f64>()
            .abs()
            * 0.5;
        assert!((area - std::f64::consts::PI * 50.0 * 30.0).abs() / area < 0.001);
        assert_eq!(outline([5.0, 5.0], [5.0, 50.0]).len(), 2);
        assert!(outline([f32::NAN, 0.0], [1.0, 1.0]).is_empty());
        assert!(outline([0.0, 0.0], [32768.0, 32768.0]).len() <= 2049);
    }
    #[test]
    fn editable_ellipse_hit_resize_undo_and_output_roundtrip() {
        let mut s = Session::new(Document::blank(160, 120).unwrap()).unwrap();
        let a = Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,

            focus: None,

            highlighter_rect: false,
            hard_edge: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
            id: 1,
            kind: Kind::Ellipse,
            start: [20.0, 20.0],
            end: [120.0, 80.0],
            color: [30, 120, 220, 128],
            width: 6,
            corner_radius: 0.0,
            fill: None,
            points: vec![],
            text: None,
            callout: None,
        };
        assert!(a.hit_test([70.0, 50.0], 0.0));
        assert!(!a.hit_test([20.0, 20.0], 0.0));
        s.execute(Command::Add {
            annotation: a.clone(),
        })
        .unwrap();
        let before = s.document.render().unwrap();
        assert_eq!(before.get_pixel(70, 50).0, [255, 255, 255, 255]);
        assert_eq!(before.get_pixel(70, 20).0, [142, 187, 237, 255]);
        let resized = a
            .resized(Handle::BottomRight, [140.0, 100.0], [160, 120])
            .unwrap();
        s.execute(Command::Update {
            annotation: resized,
        })
        .unwrap();
        assert_ne!(s.document.render().unwrap(), before);
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.render().unwrap(), before);
        s.execute(Command::Redo).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ellipse.json");
        s.save(&file).unwrap();
        assert_eq!(
            Document::load(&file).unwrap().render().unwrap(),
            s.document.render().unwrap()
        );
        let mut bad = a;
        bad.corner_radius = 1.0;
        assert!(s.execute(Command::Update { annotation: bad }).is_err());
    }
}
