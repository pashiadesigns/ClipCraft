//! Modifier geometry adapted from DesignCraft tools/src/select.rs
//! (finish_resize/aspect_scale) and geom/src/lib.rs (constrain_angle), MIT.
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! Revision ea08e4c2ba290f092063c99d70c4bdd5afc45887.
//! ClipCraft adds canvas bounds, annotation paths and atomic command integration.
use crate::{Annotation, Error, Kind, Result, selection::Handle};

#[derive(Clone, Copy, Debug, Default)]
pub struct ResizeModifiers {
    pub constrain: bool,
    pub from_center: bool,
}

impl Annotation {
    pub fn resized_with_modifiers(
        &self,
        handle: Handle,
        point: [f32; 2],
        canvas: [u32; 2],
        mods: ResizeModifiers,
    ) -> Result<Self> {
        if self.rotation_degrees != 0.0 || self.mirror_x {
            return self.resize_rotated(handle, point, canvas, mods);
        }
        let edge = matches!(
            handle,
            Handle::Top | Handle::Bottom | Handle::Left | Handle::Right
        );
        if (!mods.constrain && !mods.from_center && !edge)
            || matches!(
                handle,
                Handle::Bend
                    | Handle::Pointer
                    | Handle::RadiusTopLeft
                    | Handle::RadiusTopRight
                    | Handle::RadiusBottomLeft
                    | Handle::RadiusBottomRight
            )
        {
            return self.resized(handle, point, canvas);
        }
        if !point.iter().all(|n| n.is_finite()) || canvas.contains(&0) {
            return Err(Error::Invalid("Invalid resize point or canvas".into()));
        }
        if matches!(self.kind, Kind::Arrow | Kind::Line) {
            return resize_endpoint(self, handle, point, canvas, mods);
        }
        let (min, max) = self.body_bounds();
        let corner = match handle {
            Handle::TopLeft => min,
            Handle::TopRight => [max[0], min[1]],
            Handle::BottomLeft => [min[0], max[1]],
            Handle::BottomRight => max,
            Handle::Top => [(min[0] + max[0]) * 0.5, min[1]],
            Handle::Bottom => [(min[0] + max[0]) * 0.5, max[1]],
            Handle::Left => [min[0], (min[1] + max[1]) * 0.5],
            Handle::Right => [max[0], (min[1] + max[1]) * 0.5],
            _ => return Err(Error::Invalid("Boxes resize by their corners".into())),
        };
        let anchor = [0, 1].map(|axis| {
            if mods.from_center {
                (min[axis] + max[axis]) * 0.5
            } else {
                min[axis] + max[axis] - corner[axis]
            }
        });
        let extent = [0, 1].map(|axis| corner[axis] - anchor[axis]);
        let mut scales = [0, 1].map(|axis| {
            if extent[axis].abs() > f32::EPSILON {
                (point[axis] - anchor[axis]) / extent[axis]
            } else {
                1.0
            }
        });
        let limits = [0, 1].map(|axis| {
            let mut limits = [f32::NEG_INFINITY, f32::INFINITY];
            for bound in [min[axis], max[axis]] {
                let offset = bound - anchor[axis];
                if offset.abs() > f32::EPSILON {
                    let low = -anchor[axis] / offset;
                    let high = (canvas[axis] as f32 - anchor[axis]) / offset;
                    limits[0] = limits[0].max(low.min(high));
                    limits[1] = limits[1].min(low.max(high));
                }
            }
            limits
        });
        if mods.constrain && (0..2).all(|i| max[i] - min[i] > f32::EPSILON) {
            let driver = if matches!(handle, Handle::Top | Handle::Bottom) {
                scales[1]
            } else if matches!(handle, Handle::Left | Handle::Right)
                || scales[0].abs() > scales[1].abs()
            {
                scales[0]
            } else {
                scales[1]
            };
            let scale = driver.clamp(
                limits[0][0].max(limits[1][0]),
                limits[0][1].min(limits[1][1]),
            );
            scales = [scale; 2];
        } else {
            for axis in 0..2 {
                scales[axis] = scales[axis].clamp(limits[axis][0], limits[axis][1]);
            }
        }
        let map = |p: [f32; 2]| {
            [0, 1].map(|axis| {
                (anchor[axis] + (p[axis] - anchor[axis]) * scales[axis])
                    .clamp(0.0, canvas[axis] as f32)
            })
        };
        let mut result = self.clone();
        let a = map(min);
        let b = map(max);
        result.start = [a[0].min(b[0]), a[1].min(b[1])];
        result.end = [a[0].max(b[0]), a[1].max(b[1])];
        if !result.points.is_empty() {
            result.points = self.points.iter().copied().map(map).collect();
            result.start = result.points[0];
            result.end = *result.points.last().unwrap();
        }
        result.corner_radius = result.corner_radius.min(result.max_corner_radius());
        if let Some(callout) = &result.callout {
            callout.validate(&result, canvas, true)?;
        }
        Ok(result)
    }
}

fn resize_endpoint(
    a: &Annotation,
    handle: Handle,
    point: [f32; 2],
    canvas: [u32; 2],
    mods: ResizeModifiers,
) -> Result<Annotation> {
    let other = match handle {
        Handle::Start => a.end,
        Handle::End => a.start,
        _ => return Err(Error::Invalid("Lines resize by their endpoints".into())),
    };
    let anchor = if mods.from_center {
        [(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5]
    } else {
        other
    };
    let mut vector = [point[0] - anchor[0], point[1] - anchor[1]];
    if mods.constrain {
        let length = vector[0].hypot(vector[1]);
        let step = std::f32::consts::FRAC_PI_4;
        let angle = (vector[1].atan2(vector[0]) / step).round() * step;
        vector = [angle.cos() * length, angle.sin() * length];
    }
    let mut limit = 1.0_f32;
    for axis in 0..2 {
        if vector[axis].abs() > f32::EPSILON {
            let available = if mods.from_center {
                anchor[axis].min(canvas[axis] as f32 - anchor[axis])
            } else if vector[axis] > 0.0 {
                canvas[axis] as f32 - anchor[axis]
            } else {
                anchor[axis]
            };
            limit = limit.min(available / vector[axis].abs());
        }
    }
    let to = [0, 1].map(|i| (anchor[i] + vector[i] * limit).clamp(0.0, canvas[i] as f32));
    let other = if mods.from_center {
        [0, 1].map(|i| (anchor[i] - vector[i] * limit).clamp(0.0, canvas[i] as f32))
    } else {
        other
    };
    let mut result = a.clone();
    if handle == Handle::Start {
        result.start = to;
        result.end = other;
    } else {
        result.start = other;
        result.end = to;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session};
    fn shape() -> Annotation {
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
            kind: Kind::Rectangle,
            start: [100.0, 100.0],
            end: [180.0, 140.0],
            color: [30, 60, 90, 255],
            width: 3,
            corner_radius: 8.0,
            fill: Some([80, 90, 100, 255]),
            points: vec![],
            text: Some(crate::text::TextStyle {
                content: "Keep label".into(),
                ..Default::default()
            }),
            callout: None,
        }
    }
    #[test]
    fn midpoint_handles_preserve_the_other_dimension_and_modifier_combinations() {
        let a = shape();
        assert_eq!(a.handles().len(), 12);
        for (handle, point, start, end) in [
            (Handle::Top, [800.0, 70.0], [100.0, 70.0], [180.0, 140.0]),
            (
                Handle::Bottom,
                [-800.0, 180.0],
                [100.0, 100.0],
                [180.0, 180.0],
            ),
            (Handle::Left, [60.0, -800.0], [60.0, 100.0], [180.0, 140.0]),
            (
                Handle::Right,
                [220.0, 800.0],
                [100.0, 100.0],
                [220.0, 140.0],
            ),
        ] {
            let b = a.resized(handle, point, [640, 400]).unwrap();
            assert_eq!((b.start, b.end), (start, end));
            assert_eq!(a.text, b.text);
        }
        let shift = ResizeModifiers {
            constrain: true,
            from_center: false,
        };
        let alt = ResizeModifiers {
            constrain: false,
            from_center: true,
        };
        let both = ResizeModifiers {
            constrain: true,
            from_center: true,
        };
        let b = a
            .resized_with_modifiers(Handle::BottomRight, [260.0, 170.0], [640, 400], shift)
            .unwrap();
        assert_eq!((b.start, b.end), ([100.0, 100.0], [260.0, 180.0]));
        let b = a
            .resized_with_modifiers(Handle::Top, [140.0, 80.0], [640, 400], alt)
            .unwrap();
        assert_eq!((b.start, b.end), ([100.0, 80.0], [180.0, 160.0]));
        let b = a
            .resized_with_modifiers(Handle::Top, [140.0, 80.0], [640, 400], shift)
            .unwrap();
        assert_eq!((b.start, b.end), ([80.0, 80.0], [200.0, 140.0]));
        let b = a
            .resized_with_modifiers(Handle::TopLeft, [80.0, 70.0], [640, 400], both)
            .unwrap();
        assert_eq!((b.start, b.end), ([40.0, 70.0], [240.0, 170.0]));
        for handle in [
            Handle::TopLeft,
            Handle::TopRight,
            Handle::BottomLeft,
            Handle::BottomRight,
            Handle::Top,
            Handle::Bottom,
            Handle::Left,
            Handle::Right,
        ] {
            for point in [[-900.0, -900.0], [900.0, 900.0], [-900.0, 900.0]] {
                let b = a
                    .resized_with_modifiers(handle, point, [640, 400], both)
                    .unwrap();
                let (min, max) = b.body_bounds();
                assert!((max[0] - min[0] - 2.0 * (max[1] - min[1])).abs() < 0.001);
                assert!((min[0] + max[0] - 280.0).abs() < 0.001);
                assert!((min[1] + max[1] - 240.0).abs() < 0.001);
                Document {
                    annotations: vec![b],
                    ..Document::blank(640, 400).unwrap()
                }
                .validate()
                .unwrap();
            }
        }
        assert!(
            a.resized_with_modifiers(Handle::Top, [f32::NAN, 1.0], [640, 400], both)
                .is_err()
        );
    }
    #[test]
    fn paths_callout_labels_and_line_constraints_survive_save_and_undo() {
        for kind in [
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Eraser,
            Kind::Callout,
            Kind::Text,
            Kind::Step,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Redact,
        ] {
            let mut a = shape();
            a.kind = kind;
            a.corner_radius = 0.0;
            a.fill = None;
            if !matches!(
                kind,
                Kind::Rectangle
                    | Kind::Ellipse
                    | Kind::Polygon
                    | Kind::Callout
                    | Kind::Text
                    | Kind::Step
            ) {
                a.text = None;
            }
            if kind == Kind::Callout {
                a.callout = Some(crate::callout::CalloutStyle::for_box(
                    a.start,
                    a.end,
                    [640, 400],
                ));
            }
            a.points = match kind {
                Kind::Polygon => crate::polygon::outline(a.start, a.end, 5),
                Kind::Pen | Kind::Highlighter | Kind::Eraser => {
                    vec![a.start, [120.0, 140.0], a.end]
                }
                _ => vec![],
            };
            let b = a
                .resized_with_modifiers(
                    Handle::Right,
                    [230.0, 120.0],
                    [640, 400],
                    ResizeModifiers {
                        constrain: true,
                        from_center: true,
                    },
                )
                .unwrap();
            assert_eq!(a.text, b.text);
            assert_eq!(a.callout, b.callout);
            let mut s = Session::new(Document {
                annotations: vec![a.clone()],
                ..Document::blank(640, 400).unwrap()
            })
            .unwrap();
            s.execute(Command::Update {
                annotation: b.clone(),
            })
            .unwrap();
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("resize.capture.json");
            s.save(&path).unwrap();
            let reopened = Document::load(&path).unwrap();
            assert_eq!(reopened.annotations, vec![b]);
            assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, vec![a]);
            assert!(!s.can_undo());
        }
        for kind in [Kind::Arrow, Kind::Line] {
            let mut a = shape();
            a.kind = kind;
            a.corner_radius = 0.0;
            a.text = None;
            a.fill = None;
            for handle in [Handle::Start, Handle::End] {
                let b = a
                    .resized_with_modifiers(
                        handle,
                        [800.0, 450.0],
                        [640, 400],
                        ResizeModifiers {
                            constrain: true,
                            from_center: true,
                        },
                    )
                    .unwrap();
                assert!((b.start[0] + b.end[0] - 280.0).abs() < 0.001);
                assert!((b.start[1] + b.end[1] - 240.0).abs() < 0.001);
                let dx = (b.end[0] - b.start[0]).abs();
                let dy = (b.end[1] - b.start[1]).abs();
                assert!((dx - dy).abs() < 0.001 || dx < 0.001 || dy < 0.001);
            }
        }
    }
}
