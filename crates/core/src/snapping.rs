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

/// What a dragged point or object may land on, besides other objects.
#[derive(Clone, Copy)]
pub struct Targets<'a> {
    pub all: &'a [Annotation],
    pub exclude: &'a [u64],
    pub canvas: [u32; 2],
    pub grid: Option<u32>,
}
fn nearest_grid(value: f32, grid: u32) -> f32 {
    (value / grid as f32).round() * grid as f32
}
/// Moves a resize handle or drawing point onto the closest edge, center or
/// grid line within `tolerance`. Returns the point and the guides to show.
pub fn point(p: [f32; 2], targets: Targets<'_>, tolerance: f32) -> ([f32; 2], [Option<f32>; 2]) {
    let mut out = p;
    let mut guides = [None; 2];
    for axis in 0..2 {
        let mut best: Option<f32> = None;
        let mut consider = |target: f32| {
            let closer = best.is_none_or(|b| (target - p[axis]).abs() < (b - p[axis]).abs());
            if target.is_finite() && (target - p[axis]).abs() <= tolerance && closer {
                best = Some(target);
            }
        };
        consider(0.);
        consider(targets.canvas[axis] as f32);
        if let Some(grid) = targets.grid {
            consider(nearest_grid(p[axis], grid));
        }
        for a in targets
            .all
            .iter()
            .filter(|a| !targets.exclude.contains(&a.id))
        {
            for target in anchors(std::slice::from_ref(a), axis) {
                consider(target);
            }
        }
        if let Some(target) = best {
            out[axis] = target;
            guides[axis] = Some(target);
        }
    }
    (out, guides)
}
/// After objects have had their chance, a moved selection also lands on the
/// canvas edges and the grid. Axes already snapped to an object keep that.
pub fn settled(
    mut result: SnapResult,
    canvas: [u32; 2],
    grid: Option<u32>,
    tolerance: Option<f32>,
) -> Result<SnapResult> {
    let Some(tolerance) = tolerance else {
        return Ok(result);
    };
    if result.annotations.is_empty() {
        return Ok(result);
    }
    let mut correction = [0.0; 2];
    for axis in 0..2 {
        if result.guides[axis].is_some() {
            continue;
        }
        let sources = anchors(&result.annotations, axis);
        let mut best: Option<(f32, f32)> = None;
        for source in sources {
            let mut targets = vec![0., canvas[axis] as f32];
            if let Some(grid) = grid {
                targets.push(nearest_grid(source, grid));
            }
            for target in targets {
                let d = target - source;
                if d.abs() <= tolerance && best.is_none_or(|(_, b)| d.abs() < b.abs()) {
                    best = Some((target, d));
                }
            }
        }
        if let Some((target, d)) = best {
            result.guides[axis] = Some(target);
            correction[axis] = d;
        }
    }
    if correction != [0.0; 2] {
        result.annotations = crate::canvas_expansion::translated(&result.annotations, correction)?;
    }
    Ok(result)
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
    fn points_land_on_edges_objects_and_the_grid_only_when_close() {
        let object = marker(2, [300.0, 330.0]);
        let all = vec![object.clone()];
        let targets = Targets {
            all: &all,
            exclude: &[],
            canvas: [640, 400],
            grid: None,
        };
        assert_eq!(point([3.0, 200.0], targets, 6.0).0, [0.0, 200.0]);
        let (p, guides) = point([298.0, 397.0], targets, 6.0);
        assert_eq!(p, [300.0, 400.0]);
        assert_eq!(guides, [Some(300.0), Some(400.0)]);
        assert_eq!(point([150.0, 150.0], targets, 6.0).0, [150.0, 150.0]);
        let grid = Targets {
            grid: Some(20),
            ..targets
        };
        assert_eq!(point([143.0, 163.0], grid, 6.0).0, [140.0, 160.0]);
        let ignored = Targets {
            exclude: &[2],
            ..targets
        };
        assert_eq!(point([298.0, 200.0], ignored, 6.0).0, [298.0, 200.0]);
    }
    #[test]
    fn a_moved_object_settles_on_the_grid_and_the_canvas_edge() {
        let moving = marker(1, [100.0, 80.0]);
        let base = translated(
            std::slice::from_ref(&moving),
            &[],
            [3.0, 0.0],
            6.0,
            [640, 400],
        )
        .unwrap();
        let snapped = settled(base, [640, 400], Some(20), Some(6.0)).unwrap();
        assert_eq!(snapped.annotations[0].start[0].rem_euclid(20.0), 0.0);
        assert!(snapped.guides[0].is_some());
        let base = translated(
            std::slice::from_ref(&moving),
            &[],
            [3.0, 0.0],
            6.0,
            [640, 400],
        )
        .unwrap();
        let free = settled(base, [640, 400], Some(20), None).unwrap();
        assert_eq!(free.annotations[0].start, [83.0, 60.0]);
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
