//! Regular polygon vertices adapted from DesignCraft geom/src/shapes.rs (MIT).
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! Revision ea08e4c2ba290f092063c99d70c4bdd5afc45887; licenses/designcraft/LICENSE-MIT.
//! Changes: bounded side count, image-coordinate arrays and fit to drag bounds.
use crate::{Annotation, Kind};
use image::RgbaImage;

pub fn outline(start: [f32; 2], end: [f32; 2], sides: u32) -> Vec<[f32; 2]> {
    if !(3..=64).contains(&sides) || start.iter().chain(&end).any(|n| !n.is_finite()) {
        return Vec::new();
    }
    let min = [start[0].min(end[0]), start[1].min(end[1])];
    let max = [start[0].max(end[0]), start[1].max(end[1])];
    let vertices: Vec<_> = (0..sides)
        .map(|i| {
            let angle =
                -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * i as f64 / sides as f64;
            [angle.cos(), angle.sin()]
        })
        .collect();
    let bounds = [0, 1].map(|axis| {
        [
            vertices
                .iter()
                .map(|p| p[axis])
                .fold(f64::INFINITY, f64::min),
            vertices
                .iter()
                .map(|p| p[axis])
                .fold(f64::NEG_INFINITY, f64::max),
        ]
    });
    vertices
        .into_iter()
        .map(|p| {
            [0, 1].map(|axis| {
                min[axis]
                    + ((p[axis] - bounds[axis][0]) / (bounds[axis][1] - bounds[axis][0])) as f32
                        * (max[axis] - min[axis])
            })
        })
        .collect()
}

pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let mut stroke = annotation.clone();
    stroke.kind = Kind::Pen;
    if let Some(first) = stroke.points.first().copied() {
        stroke.points.push(first);
    }
    crate::arrow::render(image, &stroke);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session, selection::Handle};

    #[test]
    fn bounded_vertices_fit_reverse_drag_and_preserve_triangle_area() {
        for sides in 3..=64 {
            let points = outline([20.0, 30.0], [120.0, 110.0], sides);
            assert_eq!(points.len(), sides as usize);
            assert_eq!(points, outline([120.0, 110.0], [20.0, 30.0], sides));
            for (axis, min, max) in [(0, 20.0, 120.0), (1, 30.0, 110.0)] {
                assert_eq!(
                    points.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min),
                    min
                );
                assert_eq!(
                    points
                        .iter()
                        .map(|p| p[axis])
                        .fold(f32::NEG_INFINITY, f32::max),
                    max
                );
            }
            if sides == 3 {
                let area = points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                    .sum::<f32>()
                    .abs()
                    * 0.5;
                assert!((area - 4000.0).abs() < 0.01);
            }
        }
        assert!(outline([0.0; 2], [10.0; 2], 65).is_empty());
        assert!(outline([f32::NAN, 0.0], [10.0; 2], 3).is_empty());
    }

    #[test]
    fn filled_polygon_hit_resize_sides_undo_and_saved_pixels() {
        let a = Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id: 1,
            kind: Kind::Polygon,
            start: [20.0, 20.0],
            end: [120.0, 100.0],
            color: [20, 50, 80, 255],
            width: 3,
            corner_radius: 0.0,
            fill: Some([30, 120, 220, 128]),
            points: outline([20.0, 20.0], [120.0, 100.0], 3),
            text: None,
            callout: None,
        };
        assert!(a.hit_test([70.0, 70.0], 0.0));
        assert!(!a.hit_test([21.0, 21.0], 0.0));
        let mut doc = Document::blank(160, 140).unwrap();
        doc.annotations.push(a.clone());
        let mut session = Session::new(doc).unwrap();
        let before = session.document.render().unwrap();
        assert_eq!(before.get_pixel(70, 70).0, [142, 187, 237, 255]);
        let resized = a
            .resized(Handle::BottomRight, [140.0, 120.0], [160, 140])
            .unwrap();
        assert_eq!(resized.body_bounds(), ([20.0, 20.0], [140.0, 120.0]));
        let mut six = resized.clone();
        six.points = outline(six.body_bounds().0, six.body_bounds().1, 6);
        session
            .execute(Command::Update { annotation: six })
            .unwrap();
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), before);
        assert!(!session.can_undo());
        session.execute(Command::Redo).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("polygon.json");
        session.save(&path).unwrap();
        assert_eq!(
            Document::load(&path).unwrap().render().unwrap(),
            session.document.render().unwrap()
        );
        let mut invalid = a;
        invalid.points.truncate(2);
        assert!(
            session
                .execute(Command::Update {
                    annotation: invalid
                })
                .is_err()
        );
    }
}
