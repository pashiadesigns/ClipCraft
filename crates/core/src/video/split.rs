//! Splitting timeline objects and zooms at a time, keeping motion continuous.
use super::{editor::Animation, zoom::Zoom};
use crate::{Document, Error, Result};
use std::sync::Arc;

fn outside() -> Error {
    Error::Invalid("Move the playhead inside the selected object before splitting.".into())
}

/// Splits annotation `id` at project time `time` into two editable objects.
/// The left part keeps its entrance, the right part keeps its exit, and
/// keyframes and recorded paths continue across the cut. Returns the new id.
pub fn split_object(doc: &mut Document, id: u64, time: u64) -> Result<u64> {
    let mut next = doc.clone();
    if next.object_state(id).locked {
        return Err(Error::Invalid(
            "This object is locked. Unlock it to split it.".into(),
        ));
    }
    let project = next
        .video
        .as_mut()
        .ok_or_else(|| Error::Invalid("Open a video first.".into()))?;
    let left = project.objects.get(&id).cloned().ok_or_else(outside)?;
    if time <= left.start || time >= left.end {
        return Err(outside());
    }
    let index = next
        .annotations
        .iter()
        .position(|a| a.id == id)
        .ok_or_else(outside)?;
    let new_id = next
        .annotations
        .iter()
        .map(|a| a.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    let cut = time - left.start;
    let mut right = left.clone();
    right.start = time;
    right.entrance = Animation::None;
    right.entrance_stack.clear();
    right.entrance_durations.clear();
    right.keyframes.trim_start(cut as i64);
    if let Some(path) = right.motion_path.as_mut() {
        let path = Arc::make_mut(path);
        path.offset_ms = path.offset_ms.saturating_add(cut);
    }
    if !left.label.is_empty() {
        right.label = format!("{} (2)", left.label);
    }
    let mut left = left;
    left.end = time;
    left.exit = Animation::None;
    left.exit_stack.clear();
    left.exit_durations.clear();
    left.clamp_animations();
    right.clamp_animations();
    let track = left.track;
    project.objects.insert(id, left);
    // Same track, directly after the left part: the cut leaves no overlap.
    project.objects.insert(new_id, right);
    if let Some(t) = project.objects.get_mut(&new_id) {
        t.track = track;
    }
    let mut copy = next.annotations[index].clone();
    copy.id = new_id;
    next.annotations.insert(index + 1, copy);
    let state = next.object_state(id);
    if state != Default::default() {
        next.object_states.insert(new_id, state);
    }
    next.validate()?;
    *doc = next;
    Ok(new_id)
}

/// Splits zoom `id` at `time`. The left part holds its magnification to the cut
/// and the right part continues from the same view, so playback looks unchanged.
pub fn split_zoom(project: &mut super::editor::Project, id: u64, time: u64) -> Result<u64> {
    let i = project
        .zooms
        .iter()
        .position(|z| z.id == id)
        .ok_or_else(outside)?;
    let z = project.zooms[i].clone();
    if time <= z.start + z.entrance_ms || time + z.exit_ms >= z.end {
        return Err(Error::Invalid(
            "Split a zoom while it is fully zoomed in, after its entrance and before its exit."
                .into(),
        ));
    }
    let new_id = project.zooms.iter().map(|z| z.id).max().unwrap_or(0) + 1;
    let mut right = Zoom::new(new_id, time, z.end);
    right.center = z.center;
    right.scale = z.scale;
    right.exit = z.exit;
    right.exit_ms = z.exit_ms;
    right.return_to_full = z.return_to_full;
    right.pinned = z.pinned;
    right.entrance_ms = 0;
    // The right part continues from exactly where the view is at the cut.
    right.pan_from = Some(z.target(time));
    right.track = z
        .track
        .iter()
        .filter(|(at, _)| z.start + at >= time)
        .map(|(at, c)| (z.start + at - time, *c))
        .collect();
    let left = &mut project.zooms[i];
    left.end = time;
    left.exit_ms = 0;
    left.return_to_full = false;
    left.track.retain(|(at, _)| z.start + at <= time);
    left.validate()?;
    right.validate()?;
    // Both parts stay on the same zoom track (BB6).
    right.lane = z.track();
    project.zooms.push(right);
    Ok(new_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::{
        editor::{Project, Timing},
        keyframes::{Property, Value},
    };
    fn doc() -> Document {
        let mut doc = Document::blank(640, 360).unwrap();
        let mut p = Project::new("clip.mp4".into(), [640, 360], 10_000, 30);
        let track = p.add_track();
        let mut a =
            crate::steps::marker(1, [100., 100.], 40, [255; 4], (1, false), [640, 360]).unwrap();
        a.text = None;
        a.kind = crate::Kind::Rectangle;
        let mut t = Timing::new(track, 1000, 5000);
        t.label = "Box".into();
        t.entrance = Animation::Fade;
        t.exit = Animation::Fade;
        t.keyframes.insert(
            Property::Position,
            3000,
            Value::Numbers(vec![80., 80.]),
            Value::Numbers(vec![380., 180.]),
        );
        p.place(1, t);
        doc.annotations.push(a);
        doc.video = Some(p);
        doc.validate().unwrap();
        doc
    }
    #[test]
    fn split_object_keeps_motion_continuous_and_rejects_outside_times() {
        let mut d = doc();
        let base = d.annotations[0].clone();
        let before = d.video.as_ref().unwrap().objects[&1].clone();
        let sample = |t: &Timing, time: u64| t.keyframes.annotation(&base, time - t.start).start;
        let expected = [
            sample(&before, 2000),
            sample(&before, 3500),
            sample(&before, 4500),
        ];
        let new = split_object(&mut d, 1, 3000).unwrap();
        let p = d.video.as_ref().unwrap();
        let (l, r) = (&p.objects[&1], &p.objects[&new]);
        assert_eq!((l.start, l.end, r.start, r.end), (1000, 3000, 3000, 5000));
        assert_eq!(l.track, r.track);
        assert_eq!((l.entrance, l.exit), (Animation::Fade, Animation::None));
        assert_eq!((r.entrance, r.exit), (Animation::None, Animation::Fade));
        assert_eq!(sample(l, 2000), expected[0]);
        assert_eq!(sample(r, 3500), expected[1]);
        assert_eq!(sample(r, 4500), expected[2]);
        assert_eq!(r.label, "Box (2)");
        assert_eq!(d.annotations[1].id, new);
        assert!(split_object(&mut d, 1, 3000).is_err());
        assert!(split_object(&mut d, 1, 999).is_err());
    }
    #[test]
    fn split_zoom_holds_the_same_view_across_the_cut() {
        let mut p = Project::new("clip.mp4".into(), [640, 360], 10_000, 30);
        let mut z = Zoom::new(1, 1000, 5000);
        z.center = [0.3, 0.6];
        p.zooms.push(z.clone());
        let views: Vec<_> = (1000..5000).step_by(100).map(|t| z.view(t)).collect();
        let new = split_zoom(&mut p, 1, 3000).unwrap();
        for (i, t) in (1000..5000).step_by(100).enumerate() {
            let view = p.zooms.iter().find(|z| z.active(t)).and_then(|z| z.view(t));
            let (a, b) = (view.unwrap(), views[i].unwrap());
            assert!(
                a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5),
                "{t}: {a:?} {b:?}"
            );
        }
        assert!(p.zooms.iter().any(|z| z.id == new));
        assert!(split_zoom(&mut p, 1, 1200).is_err());
    }
}
