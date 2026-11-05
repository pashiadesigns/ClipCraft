//! Geometry for direct manipulation, independent of UI coordinates and zoom.
use crate::{Annotation, Document, Error, Kind, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    Start,
    End,
    Bend,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
    Pointer,
    RadiusTopLeft,
    RadiusTopRight,
    RadiusBottomLeft,
    RadiusBottomRight,
}

impl Handle {
    pub fn is_radius(self) -> bool {
        matches!(
            self,
            Self::RadiusTopLeft
                | Self::RadiusTopRight
                | Self::RadiusBottomLeft
                | Self::RadiusBottomRight
        )
    }
}

impl Annotation {
    pub fn is_freehand(&self) -> bool {
        matches!(self.kind, Kind::Pen | Kind::Highlighter | Kind::Eraser) && !self.highlighter_rect
    }
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        let (mut min, mut max) = self.body_bounds();
        if let Some(style) = &self.callout {
            for axis in 0..2 {
                min[axis] = min[axis].min(style.pointer[axis]);
                max[axis] = max[axis].max(style.pointer[axis]);
            }
        }
        self.rotation().bounds(min, max)
    }

    pub fn body_bounds(&self) -> ([f32; 2], [f32; 2]) {
        if self.line_style.is_some() && matches!(self.kind, Kind::Arrow | Kind::Line) {
            return crate::line_style::bounds(self);
        }
        if !self.points.is_empty() {
            let mut min = self.points[0];
            let mut max = min;
            for p in &self.points {
                for axis in 0..2 {
                    min[axis] = min[axis].min(p[axis]);
                    max[axis] = max[axis].max(p[axis]);
                }
            }
            return (min, max);
        }
        (
            [
                self.start[0].min(self.end[0]),
                self.start[1].min(self.end[1]),
            ],
            [
                self.start[0].max(self.end[0]),
                self.start[1].max(self.end[1]),
            ],
        )
    }

    pub fn handles(&self) -> Vec<(Handle, [f32; 2])> {
        let handles = if matches!(self.kind, Kind::Arrow | Kind::Line) {
            vec![
                (Handle::Start, self.start),
                (Handle::End, self.end),
                (Handle::Bend, crate::line_style::bend_point(self)),
            ]
        } else {
            let (min, max) = self.body_bounds();
            let mut handles = vec![
                (Handle::TopLeft, min),
                (Handle::TopRight, [max[0], min[1]]),
                (Handle::BottomLeft, [min[0], max[1]]),
                (Handle::BottomRight, max),
                (Handle::Top, [(min[0] + max[0]) * 0.5, min[1]]),
                (Handle::Bottom, [(min[0] + max[0]) * 0.5, max[1]]),
                (Handle::Left, [min[0], (min[1] + max[1]) * 0.5]),
                (Handle::Right, [max[0], (min[1] + max[1]) * 0.5]),
            ];
            if self.supports_rounding() && self.max_corner_radius() > 0.0 {
                // These are grab points, not a measurement of the current radius.
                let inset = self.rounding_inset();
                handles.extend([
                    (Handle::RadiusTopLeft, [min[0] + inset, min[1] + inset]),
                    (Handle::RadiusTopRight, [max[0] - inset, min[1] + inset]),
                    (Handle::RadiusBottomLeft, [min[0] + inset, max[1] - inset]),
                    (Handle::RadiusBottomRight, [max[0] - inset, max[1] - inset]),
                ]);
            }
            if let Some(style) = &self.callout {
                handles.insert(0, (Handle::Pointer, style.pointer));
            }
            handles
        };
        handles
            .into_iter()
            .map(|(handle, point)| (handle, self.rotation().point(point)))
            .collect()
    }

    pub fn hit_handle(&self, point: [f32; 2], tolerance: f32) -> Option<Handle> {
        self.handles()
            .into_iter()
            .filter(|(_, p)| distance(*p, point) <= tolerance)
            .min_by(|(_, a), (_, b)| distance(*a, point).total_cmp(&distance(*b, point)))
            .map(|(h, _)| h)
    }

    pub fn hit_test(&self, point: [f32; 2], tolerance: f32) -> bool {
        if !point.iter().all(|v| v.is_finite()) || !tolerance.is_finite() || tolerance < 0.0 {
            return false;
        }
        let point = self.rotation().inverse_point(point);
        let tolerance = tolerance
            + if matches!(self.kind, Kind::Blur | Kind::Pixelate) || self.highlighter_rect {
                0.0
            } else {
                self.width as f32 / 2.0
            };
        if self.kind == Kind::Polygon {
            return crate::callout::hit_polygon(&self.points, point, tolerance);
        }
        if self.kind == Kind::Ellipse {
            return crate::ellipse::hit_test(self, point, tolerance);
        }
        if self.kind == Kind::Callout
            || (matches!(self.kind, Kind::Rectangle) && self.corner_radius > 0.0)
        {
            return crate::callout::hit_test(self, point, tolerance);
        }
        if self.is_freehand() {
            return self
                .points
                .windows(2)
                .any(|pair| segment_distance(point, pair[0], pair[1]) <= tolerance);
        }
        if matches!(self.kind, Kind::Arrow | Kind::Line) {
            if self.line_style.is_some() {
                return crate::line_style::hit(self, point, tolerance);
            }
            if segment_distance(point, self.start, self.end) <= tolerance {
                return true;
            }
            self.kind == Kind::Arrow
                && self.arrow_head().is_some_and(|wings| {
                    wings
                        .into_iter()
                        .any(|p| segment_distance(point, self.end, p) <= tolerance)
                })
        } else {
            let (min, max) = self.bounds();
            point[0] >= min[0] - tolerance
                && point[0] <= max[0] + tolerance
                && point[1] >= min[1] - tolerance
                && point[1] <= max[1] + tolerance
        }
    }

    /// Clamp the entire object, preserving size and direction at canvas edges.
    pub fn translated(&self, delta: [f32; 2], canvas: [u32; 2]) -> Result<Self> {
        validate_point(delta, canvas)?;
        let (min, max) = self.bounds();
        if min[0] < 0.0 || min[1] < 0.0 || max[0] > canvas[0] as f32 || max[1] > canvas[1] as f32 {
            return Err(Error::Invalid("Object is outside the canvas".into()));
        }
        let delta = [
            delta[0].clamp(-min[0], canvas[0] as f32 - max[0]),
            delta[1].clamp(-min[1], canvas[1] as f32 - max[1]),
        ];
        let mut moved = self.clone();
        for (axis, amount) in delta.into_iter().enumerate() {
            moved.start[axis] += amount;
            moved.end[axis] += amount;
            for p in &mut moved.points {
                p[axis] += amount;
            }
            if let Some(style) = &mut moved.callout {
                style.pointer[axis] += amount;
            }
        }
        Ok(moved)
    }

    /// A corner can cross its opposite corner. Arrow endpoints retain meaning.
    pub fn resized(&self, handle: Handle, point: [f32; 2], canvas: [u32; 2]) -> Result<Self> {
        if self.rotation_degrees != 0.0 || self.mirror_x {
            return self.resize_rotated(handle, point, canvas, Default::default());
        }
        if matches!(
            handle,
            Handle::Top | Handle::Bottom | Handle::Left | Handle::Right
        ) {
            return self.resized_with_modifiers(handle, point, canvas, Default::default());
        }
        validate_point(point, canvas)?;
        let point = [
            point[0].clamp(0.0, canvas[0] as f32),
            point[1].clamp(0.0, canvas[1] as f32),
        ];
        let mut resized = self.clone();
        if matches!(
            handle,
            Handle::RadiusTopLeft
                | Handle::RadiusTopRight
                | Handle::RadiusBottomLeft
                | Handle::RadiusBottomRight
        ) {
            if !self.supports_rounding() {
                return Err(Error::Invalid(
                    "This object does not support corner-radius handles".into(),
                ));
            }
            let (min, max) = self.body_bounds();
            let x = if matches!(handle, Handle::RadiusTopLeft | Handle::RadiusBottomLeft) {
                point[0] - min[0]
            } else {
                max[0] - point[0]
            };
            let y = if matches!(handle, Handle::RadiusTopLeft | Handle::RadiusTopRight) {
                point[1] - min[1]
            } else {
                max[1] - point[1]
            };
            resized.corner_radius =
                (x + y - 2.0 * self.rounding_inset()).clamp(0.0, self.max_corner_radius());
            return Ok(resized);
        }
        if handle == Handle::Pointer {
            let style = resized
                .callout
                .as_mut()
                .ok_or_else(|| Error::Invalid("Only callouts have a pointer handle".into()))?;
            style.pointer = point;
            return Ok(resized);
        }
        if matches!(self.kind, Kind::Arrow | Kind::Line) {
            match handle {
                Handle::Bend => return Ok(crate::line_style::bend_to(self, point)),
                Handle::Start => resized.start = point,
                Handle::End => resized.end = point,
                _ => return Err(Error::Invalid("Arrows resize by their endpoints".into())),
            }
        } else {
            let (min, max) = self.body_bounds();
            let opposite = match handle {
                Handle::TopLeft => max,
                Handle::TopRight => [min[0], max[1]],
                Handle::BottomLeft => [max[0], min[1]],
                Handle::BottomRight => min,
                _ => return Err(Error::Invalid("Boxes resize by their corners".into())),
            };
            resized.start = [point[0].min(opposite[0]), point[1].min(opposite[1])];
            resized.end = [point[0].max(opposite[0]), point[1].max(opposite[1])];
            if !self.points.is_empty() {
                let anchor = match handle {
                    Handle::TopLeft => min,
                    Handle::TopRight => [max[0], min[1]],
                    Handle::BottomLeft => [min[0], max[1]],
                    _ => max,
                };
                for p in &mut resized.points {
                    for axis in 0..2 {
                        let extent = anchor[axis] - opposite[axis];
                        p[axis] = if extent.abs() > f32::EPSILON {
                            opposite[axis]
                                + (p[axis] - opposite[axis]) * (point[axis] - opposite[axis])
                                    / extent
                        } else {
                            opposite[axis]
                        };
                        p[axis] = p[axis].clamp(0.0, canvas[axis] as f32);
                    }
                }
                resized.start = resized.points[0];
                resized.end = *resized.points.last().unwrap();
            }
        }
        resized.corner_radius = resized.corner_radius.min(resized.max_corner_radius());
        if let Some(style) = &resized.callout {
            style.validate(&resized, canvas, true)?;
        }
        Ok(resized)
    }

    pub(crate) fn arrow_head(&self) -> Option<[[f32; 2]; 2]> {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        let distance = dx.hypot(dy);
        if distance <= 1.0 {
            return None;
        }
        let length = (self.width as f32 * 4.0 + 8.0).min(distance * 0.6);
        let ux = dx / distance;
        let uy = dy / distance;
        Some([-1.0, 1.0].map(|sign| {
            [
                self.end[0] - length * ux + sign * length * 0.5 * uy,
                self.end[1] - length * uy - sign * length * 0.5 * ux,
            ]
        }))
    }
}

impl Document {
    /// The last annotation is painted last and receives selection first.
    pub fn hit_test(&self, point: [f32; 2], tolerance: f32) -> Option<u64> {
        self.annotations
            .iter()
            .rev()
            .find(|a| self.object_editable(a.id) && a.hit_test(point, tolerance))
            .map(|a| a.id)
    }
}

fn validate_point(point: [f32; 2], canvas: [u32; 2]) -> Result<()> {
    if !point.iter().all(|v| v.is_finite()) || canvas.contains(&0) {
        return Err(Error::Invalid(
            "Manipulation requires finite coordinates and a valid canvas".into(),
        ));
    }
    Ok(())
}
fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
pub(crate) fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let length_squared = delta[0] * delta[0] + delta[1] * delta[1];
    if length_squared == 0.0 {
        return distance(p, a);
    }
    let t =
        (((p[0] - a[0]) * delta[0] + (p[1] - a[1]) * delta[1]) / length_squared).clamp(0.0, 1.0);
    distance(p, [a[0] + t * delta[0], a[1] + t * delta[1]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};
    fn object(kind: Kind) -> Annotation {
        Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,

            focus: None,

            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
            id: 1,
            kind,
            start: [10.0, 20.0],
            end: [70.0, 60.0],
            color: [255, 0, 0, 255],
            width: 2,
            text: None,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: None,
        }
    }

    #[test]
    fn hit_testing_uses_paint_order_and_actual_arrow_segments() {
        let mut doc = Document::blank(100, 100).unwrap();
        let arrow = object(Kind::Arrow);
        doc.annotations.push(object(Kind::Rectangle));
        doc.annotations.push(Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,

            focus: None,

            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
            id: 2,
            ..arrow.clone()
        });
        assert_eq!(doc.hit_test([40.0, 40.0], 2.0), Some(2));
        assert_eq!(doc.hit_test([12.0, 58.0], 2.0), Some(1));
        assert_eq!(doc.hit_test([90.0, 90.0], 2.0), None);
        assert!(arrow.hit_test(arrow.arrow_head().unwrap()[0], 0.0));
        assert!(!arrow.hit_test([12.0, 58.0], 2.0));
        assert!(!arrow.hit_test([f32::NAN, 1.0], 2.0));
    }

    #[test]
    fn movement_clamps_whole_object_and_preserves_reversed_endpoints() {
        let a = Annotation {
            start: [70.0, 60.0],
            end: [10.0, 20.0],
            ..object(Kind::Arrow)
        };
        let moved = a.translated([-100.0, 100.0], [100, 100]).unwrap();
        assert_eq!(moved.start, [60.0, 100.0]);
        assert_eq!(moved.end, [0.0, 60.0]);
        assert!(a.translated([f32::INFINITY, 0.0], [100, 100]).is_err());
    }

    #[test]
    fn corner_resize_can_cross_and_arrow_resize_preserves_direction() {
        let rect = object(Kind::Rectangle)
            .resized(Handle::TopLeft, [90.0, 80.0], [100, 100])
            .unwrap();
        assert_eq!((rect.start, rect.end), ([70.0, 60.0], [90.0, 80.0]));
        let arrow = object(Kind::Arrow)
            .resized(Handle::End, [-5.0, 150.0], [100, 100])
            .unwrap();
        assert_eq!((arrow.start, arrow.end), ([10.0, 20.0], [0.0, 100.0]));
        assert!(
            arrow
                .resized(Handle::TopLeft, [5.0, 5.0], [100, 100])
                .is_err()
        );
    }

    #[test]
    fn previews_are_pure_and_one_commit_is_one_undo_then_roundtrips() {
        let mut doc = Document::blank(100, 100).unwrap();
        let original = object(Kind::Redact);
        doc.annotations.push(original.clone());
        let mut session = Session::new(doc).unwrap();
        for amount in [1.0, 5.0, 10.0] {
            original.translated([amount, amount], [100, 100]).unwrap();
        }
        assert!(!session.can_undo());
        assert!(!session.is_modified());
        let moved = original.translated([15.0, 20.0], [100, 100]).unwrap();
        session
            .execute(Command::Update {
                annotation: moved.clone(),
            })
            .unwrap();
        let rendered = session.document.render().unwrap();
        assert_eq!(rendered.get_pixel(26, 41).0, [0, 0, 0, 255]);
        assert_eq!(rendered.get_pixel(11, 21).0, [255, 255, 255, 255]);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, vec![original]);
        assert!(!session.is_modified());
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.annotations, vec![moved]);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("moved.json");
        session.save(&path).unwrap();
        assert_eq!(Document::load(&path).unwrap().render().unwrap(), rendered);
    }
}
