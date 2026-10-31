//! Bounds-based smart guides. Closest-axis matching adapts DesignCraft
//! geom/src/snap.rs at ea08e4c2ba290f092063c99d70c4bdd5afc45887 (MIT).
//! Copyright and license: licenses/designcraft. ClipCraft adds annotation
//! collection, group movement, canvas limits and guide validation.
use crate::{Annotation, Error, Result, multi_selection};

pub struct SnapResult {
    pub annotations: Vec<Annotation>,
    /// Image-space X/Y coordinates; guides extend across the visible canvas.
    pub guides: [Option<f32>; 2],
}

fn anchors(objects: &[Annotation], axis: usize) -> [f32; 3] {
    let min = objects
        .iter()
        .map(|a| a.visual_body_bounds().0[axis])
        .fold(f32::INFINITY, f32::min);
    let max = objects
        .iter()
        .map(|a| a.visual_body_bounds().1[axis])
        .fold(f32::NEG_INFINITY, f32::max);
    [min, (min + max) * 0.5, max]
}

pub fn translated(
    moving: &[Annotation],
    all: &[Annotation],
    delta: [f32; 2],
    tolerance: f32,
    canvas: [u32; 2],
) -> Result<SnapResult> {
    translate_with(moving, all, delta, tolerance, |objects, delta| {
        multi_selection::translated(objects, delta, canvas)
    })
}

pub fn translated_expanding(
    moving: &[Annotation],
    all: &[Annotation],
    delta: [f32; 2],
    tolerance: f32,
) -> Result<SnapResult> {
    translate_with(
        moving,
        all,
        delta,
        tolerance,
        crate::canvas_expansion::translated,
    )
}

fn translate_with(
    moving: &[Annotation],
    all: &[Annotation],
    delta: [f32; 2],
    tolerance: f32,
    translate: impl Fn(&[Annotation], [f32; 2]) -> Result<Vec<Annotation>>,
) -> Result<SnapResult> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(Error::Invalid("Invalid snapping tolerance".into()));
    }
    let initial = translate(moving, delta)?;
    if initial.is_empty() {
        return Ok(SnapResult {
            annotations: initial,
            guides: [None; 2],
        });
    }
    let mut correction = [0.0; 2];
    let mut guides = [None; 2];
    for axis in 0..2 {
        let sources = anchors(&initial, axis);
        // Stable object/anchor order also makes equidistant ties deterministic.
        let closest = all
            .iter()
            .filter(|a| !moving.iter().any(|m| m.id == a.id))
            .flat_map(|a| anchors(std::slice::from_ref(a), axis))
            .flat_map(|target| sources.map(|source| (target, target - source)))
            .filter(|(target, d)| target.is_finite() && d.abs() <= tolerance)
            .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
        if let Some((target, d)) = closest {
            guides[axis] = Some(target);
            correction[axis] = d;
        }
    }
    let annotations = translate(&initial, correction)?;
    for (axis, guide) in guides.iter_mut().enumerate() {
        if let Some(target) = *guide
            && !anchors(&annotations, axis)
                .iter()
                .any(|v| (v - target).abs() < 0.001)
        {
            *guide = None;
        }
    }
    Ok(SnapResult {
        annotations,
        guides,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session, steps};
    fn marker(id: u64, center: [f32; 2]) -> Annotation {
        steps::marker(
            id,
            center,
            40,
            [20, 90, 150, 255],
            (id as u32, false),
            [640, 400],
        )
        .unwrap()
    }
    #[test]
    fn centers_edges_long_guides_and_thresholds_preserve_one_undo() {
        let moving = marker(1, [100.0, 80.0]);
        let target = marker(2, [300.0, 330.0]);
        let all = vec![moving.clone(), target.clone()];
        let snapped = translated(
            std::slice::from_ref(&moving),
            &all,
            [197.0, 0.0],
            6.0,
            [640, 400],
        )
        .unwrap();
        assert_eq!(snapped.annotations[0].start, [280.0, 60.0]);
        assert!(snapped.guides[0].is_some());
        assert_eq!(
            snapped.guides[1], None,
            "X snapping is not limited by Y proximity"
        );
        let edge = translated(
            std::slice::from_ref(&moving),
            &all,
            [158.0, 0.0],
            3.0,
            [640, 400],
        )
        .unwrap();
        assert_eq!(edge.annotations[0].end[0], 280.0);
        assert_eq!(edge.guides[0], Some(280.0));
        let outside = translated(
            std::slice::from_ref(&moving),
            &all,
            [190.0, 0.0],
            3.0,
            [640, 400],
        )
        .unwrap();
        assert_eq!(outside.annotations[0].start[0], 270.0);
        assert_eq!(outside.guides, [None; 2]);
        let mut document = Document::blank(640, 400).unwrap();
        document.annotations = all;
        let original = document.clone();
        let mut session = Session::new(document).unwrap();
        session
            .execute(Command::UpdateMany {
                annotations: snapped.annotations,
            })
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("snap.capture.json");
        session.save(&path).unwrap();
        assert_eq!(
            Document::load(&path).unwrap().annotations,
            session.document.annotations
        );
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, original.annotations);
        assert!(!session.can_undo());
    }
    #[test]
    fn groups_exclude_themselves_preserve_spacing_and_reject_nonfinite_inputs() {
        let group = vec![marker(1, [100.0, 80.0]), marker(2, [160.0, 130.0])];
        let free = translated(&group, &group, [5.0, 3.0], 6.0, [640, 400]).unwrap();
        assert_eq!(free.guides, [None; 2]);
        assert_eq!(free.annotations[0].start, [85.0, 63.0]);
        let mut all = group.clone();
        all.push(marker(3, [300.0, 330.0]));
        let snapped = translated(&group, &all, [167.0, 0.0], 6.0, [640, 400]).unwrap();
        assert!(snapped.guides[0].is_some());
        assert_eq!(
            snapped.annotations[1].start[0] - snapped.annotations[0].start[0],
            60.0
        );
        let edge = translated(&group, &all, [1000.0, -1000.0], 6.0, [640, 400]).unwrap();
        assert_eq!(edge.annotations[1].end[0], 640.0);
        assert_eq!(edge.annotations[0].start[1], 0.0);
        assert!(translated(&group, &all, [f32::NAN, 0.0], 6.0, [640, 400]).is_err());
        assert!(translated(&group, &all, [0.0, 0.0], f32::NAN, [640, 400]).is_err());
    }
}
