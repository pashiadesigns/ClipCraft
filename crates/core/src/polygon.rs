//! Regular polygon vertices adapted from DesignCraft geom/src/shapes.rs (MIT).
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! Revision ea08e4c2ba290f092063c99d70c4bdd5afc45887; licenses/designcraft/LICENSE-MIT.
//! Changes: bounded side count, image-coordinate arrays and fit to drag bounds.
use crate::{Annotation, Kind};
use image::RgbaImage;

/// Ready-made shapes, drawn from corners placed in a unit box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ready {
    Triangle,
    RightTriangle,
    Diamond,
    Star,
    Cross,
    Chevron,
    ArrowRight,
    ArrowLeft,
    ArrowUp,
    ArrowDown,
}
impl Ready {
    pub const ALL: [Self; 10] = [
        Self::Triangle,
        Self::RightTriangle,
        Self::Diamond,
        Self::Star,
        Self::Cross,
        Self::Chevron,
        Self::ArrowRight,
        Self::ArrowLeft,
        Self::ArrowUp,
        Self::ArrowDown,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Triangle => "Triangle",
            Self::RightTriangle => "Right triangle",
            Self::Diamond => "Diamond",
            Self::Star => "Star",
            Self::Cross => "Cross",
            Self::Chevron => "Chevron",
            Self::ArrowRight => "Arrow right",
            Self::ArrowLeft => "Arrow left",
            Self::ArrowUp => "Arrow up",
            Self::ArrowDown => "Arrow down",
        }
    }
    fn unit(self) -> Vec<[f32; 2]> {
        const ARROW: [[f32; 2]; 7] = [
            [0., 0.28],
            [0.6, 0.28],
            [0.6, 0.],
            [1., 0.5],
            [0.6, 1.],
            [0.6, 0.72],
            [0., 0.72],
        ];
        match self {
            Self::Triangle => vec![[0.5, 0.], [1., 1.], [0., 1.]],
            Self::RightTriangle => vec![[0., 0.], [1., 1.], [0., 1.]],
            Self::Diamond => vec![[0.5, 0.], [1., 0.5], [0.5, 1.], [0., 0.5]],
            Self::Star => {
                let vertices: Vec<[f64; 2]> = (0..10)
                    .map(|i| {
                        let angle = -std::f64::consts::FRAC_PI_2
                            + std::f64::consts::TAU * f64::from(i) / 10.;
                        let radius = if i % 2 == 0 { 1. } else { 0.42 };
                        [angle.cos() * radius, angle.sin() * radius]
                    })
                    .collect();
                let low = [0, 1].map(|a| vertices.iter().map(|p| p[a]).fold(f64::MAX, f64::min));
                let high = [0, 1].map(|a| vertices.iter().map(|p| p[a]).fold(f64::MIN, f64::max));
                vertices
                    .iter()
                    .map(|p| [0, 1].map(|a| ((p[a] - low[a]) / (high[a] - low[a])) as f32))
                    .collect()
            }
            Self::Cross => vec![
                [0.35, 0.],
                [0.65, 0.],
                [0.65, 0.35],
                [1., 0.35],
                [1., 0.65],
                [0.65, 0.65],
                [0.65, 1.],
                [0.35, 1.],
                [0.35, 0.65],
                [0., 0.65],
                [0., 0.35],
                [0.35, 0.35],
            ],
            Self::Chevron => vec![
                [0., 0.],
                [0.7, 0.],
                [1., 0.5],
                [0.7, 1.],
                [0., 1.],
                [0.3, 0.5],
            ],
            Self::ArrowRight => ARROW.to_vec(),
            Self::ArrowLeft => ARROW.iter().map(|p| [1. - p[0], p[1]]).collect(),
            Self::ArrowDown => ARROW.iter().map(|p| [p[1], p[0]]).collect(),
            Self::ArrowUp => ARROW.iter().map(|p| [p[1], 1. - p[0]]).collect(),
        }
    }
    /// This shape's corners, filling the box dragged out from `start` to `end`.
    pub fn outline(self, start: [f32; 2], end: [f32; 2]) -> Vec<[f32; 2]> {
        let min = [start[0].min(end[0]), start[1].min(end[1])];
        let max = [start[0].max(end[0]), start[1].max(end[1])];
        self.unit()
            .into_iter()
            .map(|p| [0, 1].map(|a| min[a] + p[a] * (max[a] - min[a])))
            .collect()
    }
    /// Which ready-made shape these corners are, whatever size it was drawn.
    pub fn of(points: &[[f32; 2]]) -> Option<Self> {
        let first = *points.first()?;
        let low = [0, 1].map(|a| points.iter().map(|p| p[a]).fold(first[a], f32::min));
        let high = [0, 1].map(|a| points.iter().map(|p| p[a]).fold(first[a], f32::max));
        if (0..2).any(|a| high[a] <= low[a]) {
            return None;
        }
        Self::ALL.into_iter().find(|shape| {
            let unit = shape.unit();
            unit.len() == points.len()
                && unit.iter().zip(points).all(|(u, p)| {
                    (0..2).all(|a| ((p[a] - low[a]) / (high[a] - low[a]) - u[a]).abs() < 0.02)
                })
        })
    }
}

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
    fn ready_shapes_fill_their_box_and_are_recognized_at_any_size() {
        for shape in Ready::ALL {
            let points = shape.outline([120., 110.], [20., 30.]);
            assert_eq!(Ready::of(&points), Some(shape), "{shape:?}");
            for (axis, low, high) in [(0, 20., 120.), (1, 30., 110.)] {
                let values = || points.iter().map(|p| p[axis]);
                assert_eq!(values().fold(f32::MAX, f32::min), low, "{shape:?}");
                assert_eq!(values().fold(f32::MIN, f32::max), high, "{shape:?}");
            }
            assert!((3..=64).contains(&points.len()));
        }
        assert_eq!(Ready::of(&outline([0.; 2], [10.; 2], 5)), None);
    }

    #[test]
    fn a_star_fills_its_middle_and_leaves_its_notches_empty() {
        let mut a = Annotation {
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
            kind: Kind::Polygon,
            start: [10., 10.],
            end: [110., 110.],
            color: [20, 50, 80, 255],
            width: 1,
            corner_radius: 0.0,
            fill: Some([200, 40, 40, 255]),
            points: vec![],
            text: None,
            callout: None,
        };
        a.points = Ready::Star.outline(a.start, a.end);
        let mut doc = Document::blank(120, 120).unwrap();
        doc.annotations.push(a);
        let image = doc.render().unwrap();
        assert_eq!(image.get_pixel(60, 62).0, [200, 40, 40, 255]);
        assert_eq!(image.get_pixel(14, 100).0, [255, 255, 255, 255]);
    }

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

            focus: None,

            highlighter_rect: false,
            hard_edge: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
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
