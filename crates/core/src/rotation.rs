//! Object rotation geometry, independent of canvas zoom and UI state.
//! Gesture delta/Shift snapping and corner zones adapt DesignCraft select.rs,
//! MIT, revision ea08e4c2ba290f092063c99d70c4bdd5afc45887.
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! ClipCraft adds validated coordinates, inverse mapping, continuous angle
//! unwrapping and a fixed-screen-distance dedicated handle.
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub center: [f32; 2],
    pub mirror_x: bool,
    /// Clockwise degrees in image coordinates, where Y points downward.
    pub degrees: f32,
}

fn finite(point: [f32; 2]) -> bool {
    point
        .iter()
        .all(|n| n.is_finite() && n.abs() <= 1_000_000.0)
}

fn normalized(degrees: f32) -> f32 {
    let angle = (degrees + 180.0).rem_euclid(360.0) - 180.0;
    if angle == 0.0 { 0.0 } else { angle }
}

impl Rotation {
    pub fn new(center: [f32; 2], degrees: f32) -> Result<Self> {
        if !finite(center) || !degrees.is_finite() || degrees.abs() > 1_000_000.0 {
            return Err(Error::Invalid("Invalid object rotation".into()));
        }
        Ok(Self {
            center,
            mirror_x: false,
            degrees: normalized(degrees),
        })
    }

    pub fn point(self, point: [f32; 2]) -> [f32; 2] {
        let point = if self.mirror_x {
            [2. * self.center[0] - point[0], point[1]]
        } else {
            point
        };
        // Exact identity avoids moving coordinates merely by storing a zero angle.
        if self.degrees == 0.0 {
            return point;
        }
        let (sin, cos) = self.degrees.to_radians().sin_cos();
        let [x, y] = [point[0] - self.center[0], point[1] - self.center[1]];
        [
            self.center[0] + cos * x - sin * y,
            self.center[1] + sin * x + cos * y,
        ]
    }

    pub fn inverse_point(self, point: [f32; 2]) -> [f32; 2] {
        let point = Self {
            degrees: -self.degrees,
            mirror_x: false,
            ..self
        }
        .point(point);
        if self.mirror_x {
            [2. * self.center[0] - point[0], point[1]]
        } else {
            point
        }
    }

    pub fn bounds(self, min: [f32; 2], max: [f32; 2]) -> ([f32; 2], [f32; 2]) {
        let points = [min, [max[0], min[1]], max, [min[0], max[1]]].map(|p| self.point(p));
        let low = [0, 1].map(|axis| points.iter().map(|p| p[axis]).fold(f32::INFINITY, f32::min));
        let high = [0, 1].map(|axis| {
            points
                .iter()
                .map(|p| p[axis])
                .fold(f32::NEG_INFINITY, f32::max)
        });
        (low, high)
    }

    /// Position of a node above the local top midpoint, at a constant UI distance.
    pub fn handle(self, min: [f32; 2], max: [f32; 2], scale: f32) -> Result<[f32; 2]> {
        validate_box(min, max, scale)?;
        Ok(self.point([(min[0] + max[0]) * 0.5, min[1] - 24.0 / scale]))
    }

    /// Optional outside-corner affordance from the audited DesignCraft tool.
    pub fn in_corner_zone(self, min: [f32; 2], max: [f32; 2], point: [f32; 2], scale: f32) -> bool {
        if validate_box(min, max, scale).is_err() || !finite(point) {
            return false;
        }
        let p = self.inverse_point(point);
        let outside = p[0] < min[0] || p[0] > max[0] || p[1] < min[1] || p[1] > max[1];
        // At low zoom several corner zones may overlap. The closest handle
        // wins, so a pointer inside its resize hit area cannot become rotation.
        let distance = [min, [max[0], min[1]], max, [min[0], max[1]]]
            .iter()
            .map(|corner| (p[0] - corner[0]).hypot(p[1] - corner[1]) * scale)
            .fold(f32::INFINITY, f32::min);
        outside && distance > 5.0 && distance <= 18.0
    }
}

fn validate_box(min: [f32; 2], max: [f32; 2], scale: f32) -> Result<()> {
    if !finite(min)
        || !finite(max)
        || min[0] > max[0]
        || min[1] > max[1]
        || !scale.is_finite()
        || !(0.001..=256.0).contains(&scale)
    {
        return Err(Error::Invalid("Invalid rotation bounds or scale".into()));
    }
    Ok(())
}

/// Preview-only state. The caller persists one final angle on release, so a
/// full turn or repeated previews never transform source geometry repeatedly.
pub struct RotationGesture {
    original: Rotation,
    last_angle: f32,
    delta: f32,
    preview: Rotation,
}

impl RotationGesture {
    pub fn begin(original: Rotation, pointer: [f32; 2]) -> Result<Self> {
        let original = Rotation {
            mirror_x: original.mirror_x,
            ..Rotation::new(original.center, original.degrees)?
        };
        let angle = pointer_angle(original.center, pointer)?
            .ok_or_else(|| Error::Invalid("Start rotation away from the object center".into()))?;
        Ok(Self {
            original,
            last_angle: angle,
            delta: 0.0,
            preview: original,
        })
    }
    pub fn update(&mut self, pointer: [f32; 2], constrain: bool) -> Result<Rotation> {
        if let Some(angle) = pointer_angle(self.original.center, pointer)? {
            self.delta += normalized(angle - self.last_angle);
            self.last_angle = angle;
        }
        let delta = if constrain {
            (self.delta / 45.0).round() * 45.0
        } else {
            self.delta
        };
        self.preview = Rotation {
            mirror_x: self.original.mirror_x,
            ..Rotation::new(self.original.center, self.original.degrees + delta)?
        };
        Ok(self.preview)
    }
    pub fn preview(&self) -> Rotation {
        self.preview
    }
    pub fn original(&self) -> Rotation {
        self.original
    }
}

fn pointer_angle(center: [f32; 2], p: [f32; 2]) -> Result<Option<f32>> {
    if !finite(p) {
        return Err(Error::Invalid("Invalid rotation pointer".into()));
    }
    let [x, y] = [p[0] - center[0], p[1] - center[1]];
    Ok((x.hypot(y) >= 0.0001).then(|| y.atan2(x).to_degrees()))
}

impl crate::Annotation {
    pub fn can_rotate(&self) -> bool {
        !matches!(
            self.kind,
            crate::Kind::Blur | crate::Kind::Pixelate | crate::Kind::Eraser
        )
    }
    pub fn rotation(&self) -> Rotation {
        if matches!(self.kind, crate::Kind::Line | crate::Kind::Arrow) {
            return Rotation {
                mirror_x: self.mirror_x,
                center: [0, 1].map(|i| (self.start[i] + self.end[i]) * 0.5),
                degrees: self.rotation_degrees,
            };
        }
        let (min, max) = self.body_bounds();
        Rotation {
            mirror_x: self.mirror_x,
            center: [0, 1].map(|i| (min[i] + max[i]) * 0.5),
            degrees: self.rotation_degrees,
        }
    }
    pub fn with_rotation(&self, degrees: f32) -> Result<Self> {
        if !self.can_rotate() {
            return Err(Error::Invalid(
                "Rotation of raster-effect regions is not implemented yet".into(),
            ));
        }
        let rotation = Rotation::new(self.rotation().center, degrees)?;
        let mut result = self.clone();
        result.rotation_degrees = rotation.degrees;
        Ok(result)
    }
    pub fn visual_body_bounds(&self) -> ([f32; 2], [f32; 2]) {
        let (min, max) = self.body_bounds();
        self.rotation().bounds(min, max)
    }

    pub(crate) fn resize_rotated(
        &self,
        handle: crate::selection::Handle,
        point: [f32; 2],
        canvas: [u32; 2],
        mods: crate::resize_modifiers::ResizeModifiers,
    ) -> Result<Self> {
        let rotation = self.rotation();
        let mut local = self.clone();
        local.rotation_degrees = 0.0;
        local.mirror_x = false;
        let resized =
            local.resized_with_modifiers(handle, rotation.inverse_point(point), canvas, mods)?;
        let new_center = resized.rotation().center;
        let placed_center = rotation.point(new_center);
        let delta = [0, 1].map(|i| placed_center[i] - new_center[i]);
        let mut values = crate::canvas_expansion::translated(&[resized], delta)?;
        let mut resized = values.remove(0);
        resized.rotation_degrees = self.rotation_degrees;
        resized.mirror_x = self.mirror_x;
        Ok(resized)
    }
}

impl crate::Document {
    pub fn rotate_annotation(&mut self, id: u64, degrees: f32) -> Result<bool> {
        let mut candidate = self.clone();
        let target = candidate
            .annotations
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| Error::Invalid("Annotation does not exist".into()))?;
        let rotated = target.with_rotation(degrees)?;
        if *target == rotated {
            return Ok(false);
        }
        *target = rotated;
        candidate.version = crate::MAX_PROJECT_VERSION;
        candidate.validate()?;
        *self = candidate;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: [f32; 2], b: [f32; 2]) {
        assert!((a[0] - b[0]).hypot(a[1] - b[1]) < 0.001, "{a:?} != {b:?}");
    }

    #[test]
    fn every_fixture_object_has_invertible_rotation_and_enclosing_bounds() {
        let doc: crate::Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        for object in &doc.annotations {
            let (min, max) = object.bounds();
            let center = [0, 1].map(|i| (min[i] + max[i]) * 0.5);
            for angle in [0.0, 90.0, 180.0, -45.0, 37.0, 360.0] {
                let rotation = Rotation::new(center, angle).unwrap();
                let (low, high) = rotation.bounds(min, max);
                for p in object
                    .handles()
                    .iter()
                    .map(|(_, p)| *p)
                    .chain(object.points.iter().copied())
                {
                    let q = rotation.point(p);
                    close(rotation.inverse_point(q), p);
                    assert!((0..2).all(|i| q[i] >= low[i] - 0.001 && q[i] <= high[i] + 0.001));
                }
            }
        }
        close(
            Rotation::new([100.0, 100.0], 90.0)
                .unwrap()
                .point([150.0, 100.0]),
            [100.0, 150.0],
        );
    }

    #[test]
    fn node_and_corner_zones_keep_screen_distances_across_zoom_and_orientation() {
        let min = [100.0, 80.0];
        let max = [300.0, 180.0];
        for scale in [0.1, 0.5, 1.0, 1.5, 4.0, 16.0] {
            for angle in [0.0, 90.0, 137.0] {
                let r = Rotation::new([200.0, 130.0], angle).unwrap();
                let node = r.handle(min, max, scale).unwrap();
                let top = r.point([200.0, 80.0]);
                assert!(((node[0] - top[0]).hypot(node[1] - top[1]) * scale - 24.0).abs() < 0.001);
                for (distance, expected) in [(3.0, false), (10.0, true), (25.0, false)] {
                    let p = r.point([min[0] - distance / scale, min[1]]);
                    assert_eq!(r.in_corner_zone(min, max, p, scale), expected);
                }
                assert!(!r.in_corner_zone(min, max, r.point([101.0, 81.0]), scale));
            }
        }
    }

    #[test]
    fn drag_wraps_continuously_and_shift_does_not_lose_original_geometry() {
        let pointer = |degrees: f32| {
            let r = degrees.to_radians();
            [100.0 + 50.0 * r.cos(), 100.0 + 50.0 * r.sin()]
        };
        let original = Rotation::new([100.0, 100.0], 12.0).unwrap();
        let mut drag = RotationGesture::begin(original, pointer(170.0)).unwrap();
        assert!((drag.update(pointer(-170.0), false).unwrap().degrees - 32.0).abs() < 0.001);
        assert!((drag.update(pointer(-145.0), true).unwrap().degrees - 57.0).abs() < 0.001);
        assert!((drag.update(pointer(-120.0), false).unwrap().degrees - 82.0).abs() < 0.001);
        let last = drag.preview();
        assert_eq!(drag.update(original.center, false).unwrap(), last);
        assert_eq!(drag.original(), original);
        assert!(drag.update([f32::NAN, 0.0], false).is_err());
        assert_eq!(drag.preview(), last);
    }

    #[test]
    fn invalid_rotation_and_zero_length_gestures_are_rejected() {
        for angle in [f32::NAN, f32::INFINITY, 2_000_000.0] {
            assert!(Rotation::new([0.0; 2], angle).is_err());
        }
        assert!(Rotation::new([f32::INFINITY, 0.0], 0.0).is_err());
        let r = Rotation::new([0.0; 2], 0.0).unwrap();
        assert!(RotationGesture::begin(r, [0.0; 2]).is_err());
        assert!(r.handle([1.0; 2], [0.0; 2], 1.0).is_err());
        assert!(r.handle([0.0; 2], [10.0; 2], 0.0).is_err());
    }

    #[test]
    fn persisted_rotation_preserves_geometry_source_and_export_matches_viewport() {
        let fixture: crate::Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        let dir = tempfile::tempdir().unwrap();
        for object in fixture.annotations.iter().filter(|a| a.can_rotate()) {
            let mut doc = crate::Document::blank(fixture.width, fixture.height).unwrap();
            doc.annotations.push(object.clone());
            let original = serde_json::to_vec(&doc).unwrap();
            assert!(
                !String::from_utf8(original.clone())
                    .unwrap()
                    .contains("rotation_degrees")
            );
            let mut session = crate::Session::new(doc).unwrap();
            session
                .execute(crate::Command::RotateAnnotation {
                    id: object.id,
                    degrees: 37.0,
                })
                .unwrap();
            let rotated = &session.document.annotations[0];
            assert_eq!(rotated.start, object.start);
            assert_eq!(rotated.end, object.end);
            assert_eq!(rotated.points, object.points);
            assert_eq!(rotated.text, object.text);
            assert_eq!(rotated.callout, object.callout);
            assert_eq!(session.document.source_png, "");
            assert_eq!(session.document.version, crate::MAX_PROJECT_VERSION);
            let output = session.document.render().unwrap();
            let display = session
                .document
                .display_list()
                .unwrap()
                .render_viewport(crate::display::Viewport {
                    size: [fixture.width, fixture.height],
                    origin: [0.0; 2],
                    scale: 1.0,
                })
                .unwrap();
            assert_eq!(output, display, "{:?} export transform", object.kind);
            let path = dir.path().join("rotated.capture.json");
            session.document.save(&path).unwrap();
            assert_eq!(
                crate::Document::load(&path).unwrap().render().unwrap(),
                output
            );
            session.execute(crate::Command::Undo).unwrap();
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
            assert!(!session.can_undo());
            session.execute(crate::Command::Redo).unwrap();
            assert_eq!(session.document.render().unwrap(), output);
            session.execute(crate::Command::RevertOriginal).unwrap();
            assert!(session.document.annotations.is_empty());
            assert_eq!(session.document.version, crate::MAX_PROJECT_VERSION);
        }
    }

    #[test]
    fn rotated_hit_handles_and_resize_keep_opposite_world_anchor() {
        let doc: crate::Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        let mut a = doc
            .annotations
            .into_iter()
            .find(|a| a.kind == crate::Kind::Rectangle)
            .unwrap();
        a.start = [150.0, 150.0];
        a.end = [300.0, 250.0];
        a.rotation_degrees = 37.0;
        let (min, max) = a.body_bounds();
        let rotation = a.rotation();
        assert!(a.hit_test(rotation.point([200.0, 180.0]), 1.0));
        assert!(!a.hit_test(rotation.point([100.0, 100.0]), 1.0));
        let from = rotation.point(max);
        assert_eq!(
            a.hit_handle(from, 1.0),
            Some(crate::selection::Handle::BottomRight)
        );
        let opposite = rotation.point(min);
        let resized = a
            .resized_with_modifiers(
                crate::selection::Handle::BottomRight,
                rotation.point([340.0, 280.0]),
                [640, 400],
                Default::default(),
            )
            .unwrap();
        assert_eq!(resized.rotation_degrees, 37.0);
        close(resized.rotation().point(resized.body_bounds().0), opposite);
        assert_eq!(resized.text, a.text);
        let mut check = crate::Document::blank(640, 400).unwrap();
        check.version = crate::MAX_PROJECT_VERSION;
        check.annotations.push(resized);
        check.validate().unwrap();
    }

    #[test]
    fn invalid_unsupported_noop_and_legacy_rotation_do_not_destroy_redo() {
        let mut doc = crate::Document::blank(320, 200).unwrap();
        let a = crate::steps::marker(
            1,
            [100.0, 100.0],
            40,
            [0, 0, 0, 255],
            (1, false),
            [320, 200],
        )
        .unwrap();
        doc.annotations.push(a);
        let mut session = crate::Session::new(doc).unwrap();
        session
            .execute(crate::Command::RotateAnnotation {
                id: 1,
                degrees: 90.0,
            })
            .unwrap();
        session.execute(crate::Command::Undo).unwrap();
        let original = serde_json::to_vec(&session.document).unwrap();
        for (id, degrees) in [(1, f32::NAN), (2, 20.0)] {
            assert!(
                session
                    .execute(crate::Command::RotateAnnotation { id, degrees })
                    .is_err()
            );
        }
        session
            .execute(crate::Command::RotateAnnotation {
                id: 1,
                degrees: 360.0,
            })
            .unwrap();
        assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
        assert!(session.can_redo());
        assert!(!session.can_undo());
        let mut invalid = session.document.clone();
        invalid.version = 2;
        invalid.annotations[0].rotation_degrees = 45.0;
        assert!(invalid.validate().is_err());
        invalid.version = crate::MAX_PROJECT_VERSION;
        invalid.annotations[0].text = None;
        for kind in [
            crate::Kind::Blur,
            crate::Kind::Pixelate,
            crate::Kind::Eraser,
        ] {
            invalid.annotations[0].kind = kind;
            assert!(invalid.validate().is_err());
            assert!(invalid.annotations[0].with_rotation(90.0).is_err());
        }
    }
}
