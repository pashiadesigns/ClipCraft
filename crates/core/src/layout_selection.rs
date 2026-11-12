//! Bounds alignment/distribution adapted from DesignCraft engine/src/cmd/object.rs.
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors. MIT.
//! Upstream ea08e4c2ba290f092063c99d70c4bdd5afc45887; licenses/designcraft/LICENSE-MIT.
//! Changes: ClipDesigner annotations and atomic commands; explicit enum; stable
//! center ordering; combined centering; transparent canvas growth for first-object
//! alignment; no page/spread model.
use crate::{Annotation, Document, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutAction {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
    CenterBoth,
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    HorizontalCenters,
    VerticalCenters,
    HorizontalGaps,
    VerticalGaps,
}

impl LayoutAction {
    /// Nine-position alignment within the original selection's union bounds.
    pub fn alignment_anchor(self) -> Option<[f32; 2]> {
        match self {
            Self::TopLeft => Some([0.0, 0.0]),
            Self::TopCenter => Some([0.5, 0.0]),
            Self::TopRight => Some([1.0, 0.0]),
            Self::MiddleLeft => Some([0.0, 0.5]),
            Self::CenterBoth => Some([0.5, 0.5]),
            Self::MiddleRight => Some([1.0, 0.5]),
            Self::BottomLeft => Some([0.0, 1.0]),
            Self::BottomCenter => Some([0.5, 1.0]),
            Self::BottomRight => Some([1.0, 1.0]),
            _ => None,
        }
    }
    pub fn minimum_objects(self) -> usize {
        if self.distribution().is_some() { 3 } else { 2 }
    }
    fn distribution(self) -> Option<(usize, bool)> {
        match self {
            Self::HorizontalCenters => Some((0, false)),
            Self::VerticalCenters => Some((1, false)),
            Self::HorizontalGaps => Some((0, true)),
            Self::VerticalGaps => Some((1, true)),
            _ => None,
        }
    }
}

impl Document {
    pub(crate) fn layout_selection(
        &self,
        ids: &[u64],
        action: LayoutAction,
    ) -> Result<Vec<Annotation>> {
        self.layout_with_reference(ids, action, false)
    }
    pub(crate) fn align_to_first(
        &self,
        ids: &[u64],
        action: LayoutAction,
    ) -> Result<Vec<Annotation>> {
        if !matches!(
            action,
            LayoutAction::Left
                | LayoutAction::Right
                | LayoutAction::Top
                | LayoutAction::Bottom
                | LayoutAction::CenterBoth
        ) {
            return Err(Error::Invalid(
                "Choose left, right, top, bottom or center alignment".into(),
            ));
        }
        self.layout_with_reference(ids, action, true)
    }
    fn layout_with_reference(
        &self,
        ids: &[u64],
        action: LayoutAction,
        first: bool,
    ) -> Result<Vec<Annotation>> {
        if ids.len() < action.minimum_objects() {
            return Err(Error::Invalid(format!(
                "Select at least {} objects for this action",
                action.minimum_objects()
            )));
        }
        let unique: BTreeSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() {
            return Err(Error::Invalid("Duplicate object in selection".into()));
        }
        let objects: Vec<_> = ids
            .iter()
            .map(|id| {
                self.annotations
                    .iter()
                    .find(|a| a.id == *id)
                    .cloned()
                    .ok_or_else(|| Error::Invalid("Annotation does not exist".into()))
            })
            .collect::<Result<_>>()?;
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for object in &objects {
            let (lo, hi) = object.bounds();
            for axis in 0..2 {
                min[axis] = min[axis].min(lo[axis]);
                max[axis] = max[axis].max(hi[axis]);
            }
        }
        if first {
            (min, max) = objects[0].bounds();
        }
        let mut deltas = vec![[0.0; 2]; objects.len()];
        if let Some((axis, gaps)) = action.distribution() {
            let mut ordered: Vec<_> = objects
                .iter()
                .enumerate()
                .map(|(index, a)| {
                    let (lo, hi) = a.bounds();
                    (index, lo[axis], hi[axis])
                })
                .collect();
            // Centers use center order, gaps use leading-edge order. Preserve
            // document identity/stack and make equal-key order deterministic.
            let key = |item: &(usize, f32, f32)| {
                if gaps {
                    item.1
                } else {
                    (item.1 + item.2) * 0.5
                }
            };
            ordered.sort_by(|a, b| {
                key(a)
                    .total_cmp(&key(b))
                    .then_with(|| objects[a.0].id.cmp(&objects[b.0].id))
            });
            let first = ordered[0];
            let last = ordered[ordered.len() - 1];
            let intervals = (ordered.len() - 1) as f64;
            if gaps {
                let total: f64 = ordered.iter().map(|(_, lo, hi)| f64::from(hi - lo)).sum();
                let gap = (f64::from(last.2 - first.1) - total) / intervals;
                let mut position = f64::from(first.1);
                for (index, lo, hi) in ordered {
                    deltas[index][axis] = (position - f64::from(lo)) as f32;
                    position += f64::from(hi - lo) + gap;
                }
            } else {
                let start = f64::from(first.1 + first.2) * 0.5;
                let end = f64::from(last.1 + last.2) * 0.5;
                for (rank, (index, lo, hi)) in ordered.into_iter().enumerate() {
                    deltas[index][axis] = (start + (end - start) * rank as f64 / intervals
                        - f64::from(lo + hi) * 0.5)
                        as f32;
                }
            }
        } else {
            for (object, delta) in objects.iter().zip(&mut deltas) {
                let (lo, hi) = object.bounds();
                if let Some(anchor) = action.alignment_anchor() {
                    *delta = [0, 1].map(|axis| {
                        min[axis] + (max[axis] - min[axis]) * anchor[axis]
                            - lo[axis]
                            - (hi[axis] - lo[axis]) * anchor[axis]
                    });
                    continue;
                }
                *delta = match action {
                    LayoutAction::Left => [min[0] - lo[0], 0.0],
                    LayoutAction::Right => [max[0] - hi[0], 0.0],
                    LayoutAction::Top => [0.0, min[1] - lo[1]],
                    LayoutAction::Bottom => [0.0, max[1] - hi[1]],
                    LayoutAction::HorizontalCenter => {
                        [(min[0] + max[0] - lo[0] - hi[0]) * 0.5, 0.0]
                    }
                    LayoutAction::VerticalCenter => [0.0, (min[1] + max[1] - lo[1] - hi[1]) * 0.5],
                    LayoutAction::CenterBoth => [
                        (min[0] + max[0] - lo[0] - hi[0]) * 0.5,
                        (min[1] + max[1] - lo[1] - hi[1]) * 0.5,
                    ],
                    _ => unreachable!("distribution handled above"),
                };
            }
        }
        objects
            .into_iter()
            .zip(deltas)
            .map(|(object, mut delta)| {
                // Coordinates are f32. Reapplying a displayed action must not
                // turn roundoff in a fractional center/gap into another edit.
                let roundoff = 4.0 * f32::EPSILON * max[0].max(max[1]).max(1.0);
                for value in &mut delta {
                    if value.abs() <= roundoff {
                        *value = 0.0;
                    }
                }
                if first {
                    // The command validates one expanding plan for the complete
                    // selection; individually clamping here would break alignment.
                    return Ok(crate::canvas_expansion::translated(&[object], delta)?.remove(0));
                }
                let moved = object.translated(delta, [self.width, self.height])?;
                if delta
                    .iter()
                    .enumerate()
                    .any(|(axis, d)| (moved.start[axis] - object.start[axis] - d).abs() > 0.001)
                {
                    return Err(Error::Invalid(
                        "This would push objects off the image. Align to the selection instead, or move the objects in first.".into(),
                    ));
                }
                Ok(moved)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Kind, Session};
    #[test]
    fn first_object_alignment_keeps_anchor_matches_edges_and_expands_canvas_atomically() {
        for action in [
            LayoutAction::Left,
            LayoutAction::Right,
            LayoutAction::Top,
            LayoutAction::Bottom,
            LayoutAction::CenterBoth,
        ] {
            let original = fixture();
            let mut s = Session::new(original.clone()).unwrap();
            let command = Command::AlignToFirst {
                ids: vec![2, 3, 1],
                action,
            };
            let command = serde_json::from_slice(&serde_json::to_vec(&command).unwrap()).unwrap();
            s.execute(command).unwrap();
            let anchor = &s.document.annotations[1];
            assert_eq!(anchor, &original.annotations[1]);
            let (lo, hi) = anchor.bounds();
            for object in &s.document.annotations {
                let (a, b) = object.bounds();
                let near = |a: f32, b: f32| assert!((a - b).abs() < 0.001);
                match action {
                    LayoutAction::Left => near(a[0], lo[0]),
                    LayoutAction::Right => near(b[0], hi[0]),
                    LayoutAction::Top => near(a[1], lo[1]),
                    LayoutAction::Bottom => near(b[1], hi[1]),
                    LayoutAction::CenterBoth => {
                        near(a[0] + b[0], lo[0] + hi[0]);
                        near(a[1] + b[1], lo[1] + hi[1]);
                    }
                    _ => unreachable!(),
                }
            }
            let copy: Document =
                serde_json::from_slice(&serde_json::to_vec(&s.document).unwrap()).unwrap();
            assert_eq!(copy.render().unwrap(), s.document.render().unwrap());
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, original.annotations);
            assert!(!s.can_undo());
        }
        let mut s = Session::new(fixture()).unwrap();
        let original = s.document.clone();
        // A larger target aligned to a small edge anchor adds transparent margin.
        s.execute(Command::AlignToFirst {
            ids: vec![1, 3],
            action: LayoutAction::Right,
        })
        .unwrap();
        assert!(s.document.width > original.width);
        let offset = s.document.auto_canvas_base.unwrap()[0];
        assert!(offset > 0);
        assert_eq!(
            s.document.annotations[0].end[0],
            s.document.annotations[2].end[0]
        );
        assert_eq!(
            s.document.annotations[0].start[0],
            original.annotations[0].start[0] + offset as f32
        );
        assert_eq!(s.document.base_image().unwrap().get_pixel(0, 0).0, [0; 4]);
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.annotations, original.annotations);
        assert_eq!(
            (s.document.width, s.document.height),
            (original.width, original.height)
        );
        assert!(!s.can_undo());
    }
    fn fixture() -> Document {
        let mut d = Document::blank(640, 400).unwrap();
        for (id, start, end) in [
            (1, [20.0, 20.0], [80.0, 60.0]),
            (2, [150.0, 100.0], [190.0, 160.0]),
            (3, [300.0, 240.0], [400.0, 320.0]),
        ] {
            d.annotations.push(Annotation {
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
                id,
                kind: Kind::Rectangle,
                start,
                end,
                color: [20, 90, 150, 180],
                width: 3,
                corner_radius: 0.0,
                fill: None,
                points: vec![],
                text: None,
                callout: None,
            });
        }
        d
    }
    #[test]
    fn expanding_alignment_preserves_source_pixels_origin_reopen_and_undo_redo() {
        let pixels = image::RgbaImage::from_fn(640, 400, |x, y| {
            image::Rgba([x as u8, y as u8, (x + y) as u8, 255])
        });
        for (action, anchor) in [
            (LayoutAction::Right, [5.0, 5.0]),
            (LayoutAction::Bottom, [5.0, 5.0]),
            (LayoutAction::CenterBoth, [5.0, 5.0]),
            (LayoutAction::Left, [610.0, 365.0]),
            (LayoutAction::Top, [610.0, 365.0]),
            (LayoutAction::CenterBoth, [610.0, 365.0]),
        ] {
            let mut doc = Document::from_image(pixels.clone()).unwrap();
            doc.annotations = fixture().annotations;
            doc.annotations[0].start = anchor;
            doc.annotations[0].end = [anchor[0] + 15.0, anchor[1] + 15.0];
            let mut s = Session::new(doc.clone()).unwrap();
            let align = || Command::AlignToFirst {
                ids: vec![1, 3],
                action,
            };
            s.execute(align()).unwrap();
            let expanded = s.document.clone();
            let base = expanded.auto_canvas_base.unwrap();
            assert_eq!([base[2], base[3]], [doc.width, doc.height]);
            let offset = [base[0] as f32, base[1] as f32];
            for id in [0, 1] {
                let shifted =
                    crate::canvas_expansion::translated(&[doc.annotations[id].clone()], offset)
                        .unwrap()
                        .remove(0);
                assert_eq!(
                    expanded.annotations[id], shifted,
                    "anchor and unselected object must stay with source"
                );
            }
            let image = expanded.base_image().unwrap();
            for (x, y, p) in image.enumerate_pixels() {
                if x >= base[0] && x < base[0] + 640 && y >= base[1] && y < base[1] + 400 {
                    assert_eq!(p, pixels.get_pixel(x - base[0], y - base[1]));
                } else {
                    assert_eq!(p.0, [0; 4]);
                }
            }
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("aligned.capture.json");
            s.save(&path).unwrap();
            let loaded = Document::load(&path).unwrap();
            assert_eq!(loaded.auto_canvas_base, expanded.auto_canvas_base);
            assert_eq!(loaded.render().unwrap(), expanded.render().unwrap());
            s.execute(align()).unwrap(); // Repeating an already applied alignment is a no-op.
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, doc.annotations);
            assert_eq!(s.document.base_image().unwrap(), pixels);
            assert_eq!(s.document.auto_canvas_base, None);
            assert!(!s.can_undo());
            s.execute(Command::Redo).unwrap();
            assert_eq!(s.document.annotations, expanded.annotations);
            assert_eq!(s.document.render().unwrap(), expanded.render().unwrap());
        }
        // Reject impossible growth atomically before allocating or changing history.
        let mut doc = Document::blank(32768, 1).unwrap();
        doc.annotations = fixture().annotations[..2].to_vec();
        doc.annotations[0].start = [32760.0, 0.0];
        doc.annotations[0].end = [32767.0, 1.0];
        doc.annotations[1].start = [1.0, 0.0];
        doc.annotations[1].end = [32760.0, 1.0];
        let mut s = Session::new(doc.clone()).unwrap();
        assert!(
            s.execute(Command::AlignToFirst {
                ids: vec![1, 2],
                action: LayoutAction::Left
            })
            .is_err()
        );
        assert_eq!(s.document.annotations, doc.annotations);
        assert_eq!((s.document.width, s.document.height), (32768, 1));
        assert!(!s.can_undo());
    }
    #[test]
    fn all_alignment_actions_preserve_size_stack_and_style_with_one_undo() {
        for action in [
            LayoutAction::Left,
            LayoutAction::HorizontalCenter,
            LayoutAction::Right,
            LayoutAction::Top,
            LayoutAction::VerticalCenter,
            LayoutAction::Bottom,
            LayoutAction::CenterBoth,
            LayoutAction::TopLeft,
            LayoutAction::TopCenter,
            LayoutAction::TopRight,
            LayoutAction::MiddleLeft,
            LayoutAction::MiddleRight,
            LayoutAction::BottomLeft,
            LayoutAction::BottomCenter,
            LayoutAction::BottomRight,
        ] {
            let original = fixture();
            let mut s = Session::new(original.clone()).unwrap();
            s.execute(Command::LayoutSelection {
                ids: vec![3, 1, 2],
                action,
            })
            .unwrap();
            for (before, after) in original.annotations.iter().zip(&s.document.annotations) {
                assert_eq!(before.id, after.id);
                assert_eq!(before.color, after.color);
                assert_eq!(before.width, after.width);
                for axis in 0..2 {
                    assert_eq!(
                        before.end[axis] - before.start[axis],
                        after.end[axis] - after.start[axis]
                    );
                }
                let (lo, hi) = after.bounds();
                if let Some(anchor) = action.alignment_anchor() {
                    for axis in 0..2 {
                        let actual = lo[axis] + (hi[axis] - lo[axis]) * anchor[axis];
                        let expected = 20.0 + ([400.0, 320.0][axis] - 20.0) * anchor[axis];
                        assert!(
                            (actual - expected).abs() < 0.001,
                            "{action:?}: {actual} vs {expected}"
                        );
                    }
                    continue;
                }
                match action {
                    LayoutAction::Left => assert_eq!(lo[0], 20.0),
                    LayoutAction::Right => assert_eq!(hi[0], 400.0),
                    LayoutAction::Top => assert_eq!(lo[1], 20.0),
                    LayoutAction::Bottom => assert_eq!(hi[1], 320.0),
                    LayoutAction::HorizontalCenter => assert_eq!((lo[0] + hi[0]) * 0.5, 210.0),
                    LayoutAction::VerticalCenter => assert_eq!((lo[1] + hi[1]) * 0.5, 170.0),
                    LayoutAction::CenterBoth => assert_eq!(
                        [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5],
                        [210.0, 170.0]
                    ),
                    _ => unreachable!(),
                }
            }
            let expected = s.document.annotations.clone();
            let reopened: Document =
                serde_json::from_slice(&serde_json::to_vec(&s.document).unwrap()).unwrap();
            assert_eq!(reopened.annotations, expected);
            s.execute(Command::LayoutSelection {
                ids: vec![1, 2, 3],
                action,
            })
            .unwrap();
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, original.annotations);
            assert!(!s.can_undo());
            s.execute(Command::Redo).unwrap();
            assert_eq!(s.document.annotations, expected);
        }
    }
    #[test]
    fn centers_and_gaps_distribute_horizontally_and_vertically_without_resizing() {
        for (action, axis, gaps) in [
            (LayoutAction::HorizontalCenters, 0, false),
            (LayoutAction::VerticalCenters, 1, false),
            (LayoutAction::HorizontalGaps, 0, true),
            (LayoutAction::VerticalGaps, 1, true),
        ] {
            let original = fixture();
            let mut s = Session::new(original.clone()).unwrap();
            s.execute(Command::LayoutSelection {
                ids: vec![2, 3, 1],
                action,
            })
            .unwrap();
            let intervals: Vec<_> = s
                .document
                .annotations
                .windows(2)
                .map(|pair| {
                    let (a, b) = pair[0].bounds();
                    let (c, d) = pair[1].bounds();
                    if gaps {
                        c[axis] - b[axis]
                    } else {
                        (c[axis] + d[axis] - a[axis] - b[axis]) * 0.5
                    }
                })
                .collect();
            assert!((intervals[0] - intervals[1]).abs() < 0.001);
            assert_eq!(s.document.annotations[0], original.annotations[0]);
            assert_eq!(s.document.annotations[2], original.annotations[2]);
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("layout.capture.json");
            s.save(&path).unwrap();
            let reopened = Document::load(&path).unwrap();
            assert_eq!(reopened.annotations, s.document.annotations);
            assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
        }
    }
    #[test]
    fn repeated_fractional_arrangements_do_not_drift_or_add_undo_steps() {
        let mut original = fixture();
        for (i, a) in original.annotations.iter_mut().enumerate() {
            a.start[0] += 0.13 * (i + 1) as f32;
            a.start[1] += 0.17 * (i + 1) as f32;
            a.end[0] += 0.37;
            a.end[1] += 0.29;
        }
        let mut extra = original.annotations[0].clone();
        extra.id = 4;
        extra.start = [450.37, 335.13];
        extra.end = [580.17, 390.39];
        original.annotations.push(extra);
        for action in [
            LayoutAction::HorizontalCenter,
            LayoutAction::VerticalCenter,
            LayoutAction::CenterBoth,
            LayoutAction::HorizontalCenters,
            LayoutAction::VerticalCenters,
            LayoutAction::HorizontalGaps,
            LayoutAction::VerticalGaps,
        ] {
            let mut s = Session::new(original.clone()).unwrap();
            let command = || Command::LayoutSelection {
                ids: vec![4, 2, 1, 3],
                action,
            };
            s.execute(command()).unwrap();
            let arranged = s.document.annotations.clone();
            for _ in 0..10 {
                s.execute(command()).unwrap();
                assert_eq!(s.document.annotations, arranged, "{action:?}");
            }
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, original.annotations, "{action:?}");
            assert!(!s.can_undo());
        }
    }
    #[test]
    fn invalid_layout_targets_preserve_document_and_redo() {
        let original = fixture();
        let mut s = Session::new(original.clone()).unwrap();
        s.execute(Command::LayoutSelection {
            ids: vec![1, 2, 3],
            action: LayoutAction::Left,
        })
        .unwrap();
        s.execute(Command::Undo).unwrap();
        for (ids, action) in [
            (vec![], LayoutAction::Left),
            (vec![1], LayoutAction::Left),
            (vec![1, 1, 2], LayoutAction::Left),
            (vec![1, 99], LayoutAction::Left),
            (vec![1, 2], LayoutAction::HorizontalGaps),
        ] {
            assert!(s.execute(Command::LayoutSelection { ids, action }).is_err());
            assert_eq!(s.document.annotations, original.annotations);
            assert!(s.can_redo());
            assert!(!s.can_undo());
        }
    }
}
