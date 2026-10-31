use crate::{Annotation, Kind};
use image::RgbaImage;

pub(crate) fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

impl Annotation {
    pub fn supports_rounding(&self) -> bool {
        matches!(self.kind, Kind::Rectangle | Kind::Callout | Kind::Simplify)
            || (self.kind == Kind::Image && self.image.as_ref().is_some_and(|b| !b.is_cursor()))
    }

    /// Relative adjustment from a stationary corner grab point. Repeated gestures
    /// start at the existing radius; rotated objects use their local axes.
    pub fn rounded_by_drag(
        &self,
        handle: crate::selection::Handle,
        from: [f32; 2],
        to: [f32; 2],
    ) -> crate::Result<Self> {
        use crate::selection::Handle;
        if !handle.is_radius()
            || !self.supports_rounding()
            || !from.into_iter().chain(to).all(f32::is_finite)
        {
            return Err(crate::Error::Invalid(
                "Invalid corner rounding gesture".into(),
            ));
        }
        let from = self.rotation().inverse_point(from);
        let to = self.rotation().inverse_point(to);
        let x = if matches!(handle, Handle::RadiusTopLeft | Handle::RadiusBottomLeft) {
            1.0
        } else {
            -1.0
        };
        let y = if matches!(handle, Handle::RadiusTopLeft | Handle::RadiusTopRight) {
            1.0
        } else {
            -1.0
        };
        let mut rounded = self.clone();
        rounded.corner_radius =
            (self.corner_radius + (to[0] - from[0]) * x + (to[1] - from[1]) * y)
                .clamp(0.0, self.max_corner_radius());
        Ok(rounded)
    }

    pub fn max_corner_radius(&self) -> f32 {
        if !self.supports_rounding() {
            return 0.0;
        }
        let (min, max) = self.body_bounds();
        ((max[0] - min[0]).min(max[1] - min[1]) * 0.5).max(0.0)
    }
    pub(crate) fn rounding_inset(&self) -> f32 {
        (self.max_corner_radius() * 0.2).clamp(1.0, 10.0)
    }
    pub(crate) fn rounded_polygon(&self, polygon: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
        if self.corner_radius == 0.0 {
            return polygon;
        }
        let (min, max) = self.body_bounds();
        let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
        let mut out = Vec::new();
        for (i, &corner) in polygon.iter().enumerate() {
            if !corners.contains(&corner) {
                out.push(corner);
                continue;
            }
            let prev = polygon[(i + polygon.len() - 1) % polygon.len()];
            let next = polygon[(i + 1) % polygon.len()];
            let before = (prev[0] - corner[0]).hypot(prev[1] - corner[1]);
            let after = (next[0] - corner[0]).hypot(next[1] - corner[1]);
            let radius = self.corner_radius.min(before * 0.5).min(after * 0.5);
            if radius <= 0.0 {
                out.push(corner);
                continue;
            }
            let entry = [
                corner[0] + (prev[0] - corner[0]) * radius / before,
                corner[1] + (prev[1] - corner[1]) * radius / before,
            ];
            let exit = [
                corner[0] + (next[0] - corner[0]) * radius / after,
                corner[1] + (next[1] - corner[1]) * radius / after,
            ];
            let center = [
                entry[0] + exit[0] - corner[0],
                entry[1] + exit[1] - corner[1],
            ];
            let angle = (entry[1] - center[1]).atan2(entry[0] - center[0]);
            let steps = (radius * std::f32::consts::FRAC_PI_2)
                .ceil()
                .clamp(4.0, 64.0) as usize;
            for n in 0..=steps {
                let theta = angle + std::f32::consts::FRAC_PI_2 * n as f32 / steps as f32;
                out.push([
                    center[0] + radius * theta.cos(),
                    center[1] + radius * theta.sin(),
                ]);
            }
        }
        out
    }
}
pub(crate) fn render_rectangle(image: &mut RgbaImage, annotation: &Annotation) {
    let mut stroke = annotation.clone();
    stroke.kind = Kind::Pen;
    stroke.corner_radius = 0.0;
    let (min, max) = annotation.body_bounds();
    stroke.points = annotation.rounded_polygon(vec![min, [max[0], min[1]], max, [min[0], max[1]]]);
    stroke.points.push(stroke.points[0]);
    crate::arrow::render(image, &stroke);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session, selection::Handle};
    #[test]
    fn radius_handles_preserve_bounds_undo_and_roundtrip_for_both_shapes() {
        for kind in [Kind::Rectangle, Kind::Callout] {
            let mut a = Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,
                analysis: None,
                highlighter_rect: false,
                group: None,
                rotation_degrees: 0.0,
                id: 1,
                kind,
                start: [20.0, 20.0],
                end: [100.0, 80.0],
                width: 2,
                color: [0, 0, 0, 255],
                corner_radius: 0.0,
                fill: None,
                points: vec![],
                text: None,
                callout: None,
            };
            if kind == Kind::Callout {
                a.text = Some(crate::text::TextStyle {
                    content: String::new(),
                    ..Default::default()
                });
                a.callout = Some(crate::callout::CalloutStyle::for_box(
                    a.start,
                    a.end,
                    [140, 120],
                ));
            }
            let original = a.clone();
            for angle in [0.0, 37.0, 90.0] {
                let mut probe = original.clone();
                probe.rotation_degrees = angle;
                probe.corner_radius = 12.0;
                let handles = probe.handles();
                for (handle, from) in handles.iter().copied().filter(|(h, _)| h.is_radius()) {
                    let local = probe.rotation().inverse_point(from);
                    let x = if matches!(handle, Handle::RadiusTopLeft | Handle::RadiusBottomLeft) {
                        3.0
                    } else {
                        -3.0
                    };
                    let y = if matches!(handle, Handle::RadiusTopLeft | Handle::RadiusTopRight) {
                        4.0
                    } else {
                        -4.0
                    };
                    let to = probe.rotation().point([local[0] + x, local[1] + y]);
                    assert_eq!(probe.rounded_by_drag(handle, from, from).unwrap(), probe);
                    let rounded = probe.rounded_by_drag(handle, from, to).unwrap();
                    assert!((rounded.corner_radius - 19.0).abs() < 0.001);
                    assert_eq!(rounded.handles(), handles);
                    let more = rounded.rounded_by_drag(handle, from, to).unwrap();
                    assert!((more.corner_radius - 26.0).abs() < 0.001);
                    let less = more.rounded_by_drag(handle, to, from).unwrap();
                    assert!((less.corner_radius - 19.0).abs() < 0.001);
                    assert_eq!(less.handles(), handles);
                    assert_eq!(probe.hit_handle(from, 1.0), Some(handle));
                    assert!(probe.rounded_by_drag(handle, [f32::NAN, 0.0], to).is_err());
                }
            }
            let max = a.max_corner_radius();
            a = a
                .resized(Handle::RadiusTopLeft, [43.0, 43.0], [140, 120])
                .unwrap();
            assert_eq!((a.start, a.end), (original.start, original.end));
            assert_eq!(a.corner_radius, max);
            assert_eq!(a.callout, original.callout);
            let mut s = Session::new(Document::blank(140, 120).unwrap()).unwrap();
            s.execute(Command::Add {
                annotation: original.clone(),
            })
            .unwrap();
            s.execute(Command::Update {
                annotation: a.clone(),
            })
            .unwrap();
            let rounded = s.document.render().unwrap();
            assert_eq!(rounded.get_pixel(20, 20).0, [255, 255, 255, 255]);
            assert!(!a.hit_test([20.0, 20.0], 0.0));
            assert!(rounded.pixels().any(|p| p.0 != [255, 255, 255, 255]));
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations[0], original);
            s.execute(Command::Redo).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("rounded.json");
            s.save(&path).unwrap();
            assert_eq!(Document::load(&path).unwrap().render().unwrap(), rounded);
            let reset = a
                .resized(Handle::RadiusBottomRight, a.end, [140, 120])
                .unwrap();
            assert_eq!(reset.corner_radius, 0.0);
            let shrunk = a
                .resized(Handle::BottomRight, [35.0, 35.0], [140, 120])
                .unwrap();
            assert!(shrunk.corner_radius <= shrunk.max_corner_radius());
            for invalid in [f32::NAN, -1.0, max + 1.0] {
                let mut bad = a.clone();
                bad.corner_radius = invalid;
                assert!(s.execute(Command::Update { annotation: bad }).is_err());
                assert_eq!(s.document.annotations[0], a);
            }
        }
    }
}
