//! Reusable annotation compositions, independent of their source project.
use crate::{
    Document, Error, Kind, Result,
    clipboard_payload::ObjectPayload,
    video::editor::{Project, Timing},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    pub objects: ObjectPayload,
    pub timing: BTreeMap<u64, Timing>,
}
impl Composition {
    pub fn capture(doc: &Document, ids: &[u64]) -> Result<Self> {
        let mut objects = ObjectPayload::capture(doc, ids)?;
        if objects.objects.len() < 2 || objects.objects.len() > 128 {
            return Err(Error::Invalid(
                "Select 2 to 128 objects, then Save as block.".into(),
            ));
        }
        let mut timing = BTreeMap::new();
        let origin = doc
            .video
            .as_ref()
            .and_then(|p| {
                objects
                    .objects
                    .iter()
                    .filter_map(|a| p.objects.get(&a.id))
                    .map(|t| t.start)
                    .min()
            })
            .unwrap_or(0);
        let mut ranks = BTreeMap::new();
        if let Some(p) = &doc.video {
            for (rank, layer) in p.layers().into_iter().enumerate() {
                if let crate::video::links::Layer::Annotation(id) = layer {
                    ranks.insert(id, rank as u64 + 1);
                }
            }
        }
        for (i, a) in objects.objects.iter_mut().enumerate() {
            if let Some(mut t) = doc
                .video
                .as_ref()
                .and_then(|p| p.objects.get(&a.id))
                .cloned()
            {
                t.start -= origin;
                t.end -= origin;
                t.track = ranks[&t.track];
                t.motion_path = None;
                timing.insert(i as u64 + 1, t);
            }
            a.id = i as u64 + 1;
            a.group = Some(1);
        }
        let out = Self { objects, timing };
        out.validate()?;
        Ok(out)
    }
    pub fn validate(&self) -> Result<()> {
        if !(2..=128).contains(&self.objects.objects.len()) || self.timing.len() > 128 {
            return Err(Error::Invalid("A block contains 2–128 objects.".into()));
        }
        self.objects.validate()?;
        if !self.timing.is_empty() {
            let mut d =
                Document::blank(self.objects.source_canvas[0], self.objects.source_canvas[1])?;
            d.annotations = self.objects.objects.clone();
            let mut p = Project::new(
                "preset-background.png".into(),
                self.objects.source_canvas,
                1,
                30,
            );
            p.objects = self.timing.clone();
            p.tracks = self.timing.values().map(|t| t.track).collect();
            p.tracks.sort();
            p.tracks.dedup();
            if p.objects.values().any(|t| t.motion_path.is_some()) {
                return Err(Error::Invalid(
                    "Recorded paths do not belong in reusable blocks.".into(),
                ));
            }
            d.video = Some(p);
            d.promote_features();
            d.validate()?;
        }
        Ok(())
    }
    /// Place fresh objects, use destination numbering, and preserve relative times.
    pub fn place(
        &self,
        target: &Document,
        center: [f32; 2],
    ) -> Result<(Vec<crate::Annotation>, Option<Project>)> {
        self.validate()?;
        let mut objects = self.objects.place(target, center)?;
        let mut p = target.video.clone();
        let mut numeric = crate::steps::next_number(target, false);
        let mut letters = crate::steps::next_number(target, true);
        let count = objects.len() as u64;
        let mut tracks = BTreeMap::new();
        if let Some(p) = &mut p {
            let mut source_tracks: Vec<_> = self.timing.values().map(|t| t.track).collect();
            if source_tracks.is_empty() {
                source_tracks = (1..=objects.len() as u64).collect();
            }
            source_tracks.sort();
            source_tracks.dedup();
            for rank in source_tracks.into_iter().rev() {
                let track = p.add_track();
                p.track_settings_mut(crate::video::links::Layer::Annotation(track))
                    .name = "Block".into();
                // Created for this import only: removed again once its objects are deleted.
                p.automatic_tracks.push(track);
                tracks.insert(rank, track);
            }
        }
        for (i, a) in objects.iter_mut().enumerate() {
            if a.kind == Kind::Step
                && let Some(text) = a.text.as_mut()
            {
                let letter = !text.content.is_empty()
                    && text.content.chars().all(|c| c.is_ascii_alphabetic());
                let n = if letter { &mut letters } else { &mut numeric };
                let replacement = crate::steps::label(*n, letter)?;
                crate::text::editing::replace(text, 0..text.content.chars().count(), &replacement)?;
                *n += 1;
            }
            if let Some(p) = &mut p {
                let mut t = self
                    .timing
                    .get(&self.objects.objects[i].id)
                    .cloned()
                    .unwrap_or_else(|| Timing::new(count - i as u64, 0, 5000));
                let base = &self.objects.objects[i];
                let (lo, hi) = base.body_bounds();
                let (next, end) = a.body_bounds();
                let scale = [0, 1].map(|i| {
                    if (hi[i] - lo[i]).abs() > 0.001 {
                        (end[i] - next[i]) / (hi[i] - lo[i])
                    } else {
                        1.
                    }
                });
                t.keyframes.transform(
                    scale,
                    [next[0] - lo[0] * scale[0], next[1] - lo[1] * scale[1]],
                );
                if a.kind == Kind::Step {
                    t.keyframes
                        .channels
                        .remove(&crate::video::keyframes::Property::Text);
                }
                t.track = tracks[&t.track];
                t.start += p.cursor;
                t.end += p.cursor;
                p.place(a.id, t);
            }
        }
        Ok((objects, p))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_copies_are_independent_and_renumbered() {
        let mut source = Document::blank(640, 360).unwrap();
        for id in 1..=2 {
            source.annotations.push(
                crate::steps::marker(
                    id,
                    [id as f32 * 80., 80.],
                    44,
                    [20, 150, 190, 255],
                    (id as u32, false),
                    [640, 360],
                )
                .unwrap(),
            );
        }
        let preset = Composition::capture(&source, &[1, 2]).unwrap();
        let (a, _) = preset.place(&source, [300., 200.]).unwrap();
        assert_eq!(a[0].text.as_ref().unwrap().content, "3");
        assert_eq!(a[1].text.as_ref().unwrap().content, "4");
        assert_eq!(
            preset.objects.objects[0].text.as_ref().unwrap().content,
            "1"
        );
        assert_eq!(a[0].group, a[1].group);
        assert_ne!(a[0].id, source.annotations[0].id);
    }
    #[test]
    fn preset_tracks_disappear_once_their_objects_are_deleted() {
        let mut doc = Document::blank(640, 360).unwrap();
        let mut p = Project::new("clip.mp4".into(), [640, 360], 10_000, 30);
        for id in 1..=2 {
            doc.annotations.push(
                crate::steps::marker(
                    id,
                    [id as f32 * 80., 80.],
                    44,
                    [20, 150, 190, 255],
                    (id as u32, false),
                    [640, 360],
                )
                .unwrap(),
            );
            let track = p.add_track();
            p.place(id, Timing::new(track, 0, 3000));
        }
        doc.video = Some(p);
        let preset = Composition::capture(&doc, &[1, 2]).unwrap();
        let before = doc.video.as_ref().unwrap().tracks.clone();
        let (objects, project) = preset.place(&doc, [300., 200.]).unwrap();
        let mut project = project.unwrap();
        let added: Vec<u64> = project
            .tracks
            .iter()
            .copied()
            .filter(|t| !before.contains(t))
            .collect();
        assert_eq!(added.len(), 2);
        doc.annotations.extend(objects.iter().cloned());
        project.sync(&doc.annotations);
        assert!(
            added.iter().all(|t| project.tracks.contains(t)),
            "kept while used"
        );
        // Delete one copy: its track goes; delete the other: the last preset track goes too.
        let ids: Vec<u64> = objects.iter().map(|a| a.id).collect();
        doc.annotations.retain(|a| a.id != ids[0]);
        project.sync(&doc.annotations);
        assert_eq!(
            project.tracks.iter().filter(|t| added.contains(t)).count(),
            1
        );
        doc.annotations.retain(|a| a.id != ids[1]);
        project.sync(&doc.annotations);
        assert!(project.tracks.iter().all(|t| !added.contains(t)));
        // Tracks the person made by hand are never removed automatically.
        let manual: Vec<u64> = before
            .iter()
            .copied()
            .filter(|t| !doc.video.as_ref().unwrap().automatic_tracks.contains(t))
            .collect();
        assert_eq!(manual.len(), 2);
        assert!(manual.iter().all(|t| project.tracks.contains(t)));
    }
}
