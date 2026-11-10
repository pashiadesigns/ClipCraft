//! Separate narration sources share temporal edits, never visual or audio styling.
use super::editor::*;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Layer {
    Annotation(u64),
    Media(u64),
    Zoom(u64),
}

impl Project {
    pub fn separate_audio(&mut self, index: usize) -> Result<()> {
        let source = self
            .segments
            .get(index)
            .ok_or_else(|| Error::Invalid("Select a video clip with audio.".into()))?
            .clone();
        if source.kind() == MediaKind::Audio || source.audio.is_empty() {
            return Err(Error::Invalid("This clip has no attached audio.".into()));
        }
        let link = source.link.unwrap_or_else(|| self.next_link());
        self.segments[index].link = Some(link);
        self.segments[index].audio.clear();
        for audio in &source.audio {
            let mut clip = source.clone();
            clip.id = self.next_media_id();
            clip.track = self.add_typed_track(TrackKind::Audio);
            clip.label = audio.label.clone();
            clip.link = Some(link);
            clip.frame = None;
            clip.hidden = false;
            clip.source = Some(Source {
                path: audio.path.clone(),
                size: [1, 1],
                duration: source
                    .source
                    .as_ref()
                    .map_or(self.source_duration, |s| s.duration),
                kind: MediaKind::Audio,
            });
            clip.audio = vec![audio.clone()];
            if let Some(chain) = self.clip_effects.get(&source.id).cloned() {
                self.clip_effects.insert(clip.id, chain);
            }
            if let Some(gain) = self.media_levels.get(&source.track).copied() {
                self.media_levels.insert(clip.track, gain);
            }
            if let Some(chain) = self.track_effects.get(&source.track).cloned() {
                self.track_effects.insert(clip.track, chain);
            }
            if self.track_muted(source.track) {
                self.track_settings_mut(Layer::Media(clip.track)).muted = true;
            }
            self.segments.push(clip);
        }
        Ok(())
    }
    pub fn layers(&self) -> Vec<Layer> {
        let valid: Vec<_> = self
            .tracks
            .iter()
            .copied()
            .map(Layer::Annotation)
            .chain(self.media_tracks.iter().copied().map(Layer::Media))
            .chain(self.zooms.iter().map(|z| Layer::Zoom(z.track())))
            .collect();
        let mut out: Vec<_> = self
            .layer_order
            .iter()
            .copied()
            .filter(|l| valid.contains(l))
            .collect();
        for l in valid {
            if !out.contains(&l) {
                out.push(l);
            }
        }
        out.sort_by_key(
            |l| !matches!(l,Layer::Zoom(t) if self.zooms.iter().any(|z|z.track()==*t&&z.pinned)),
        );
        out
    }
    pub fn layer_index(&self, layer: Layer) -> usize {
        self.layers().iter().position(|l| *l == layer).unwrap_or(0)
    }
    pub fn move_layer(&mut self, layer: Layer, delta: i32) {
        let order = self.layers();
        if let Some(i) = order.iter().position(|l| *l == layer) {
            let to = (i as i32 + delta).clamp(0, order.len().saturating_sub(1) as i32) as usize;
            self.reorder_layer(layer, to);
        }
    }
    pub fn linked_indices(&self, index: usize) -> Vec<usize> {
        let Some(c) = self.segments.get(index) else {
            return vec![];
        };
        self.segments
            .iter()
            .enumerate()
            .filter(|(i, s)| *i == index || c.link.is_some() && c.link == s.link)
            .map(|(i, _)| i)
            .collect()
    }
    pub fn next_link(&self) -> u64 {
        self.segments
            .iter()
            .filter_map(|s| s.link)
            .max()
            .unwrap_or(0)
            + 1
    }
    pub fn unlink(&mut self, index: usize) {
        for i in self.linked_indices(index) {
            self.segments[i].link = None;
        }
    }
    pub fn delete_linked(&mut self, index: usize) {
        let ids: Vec<_> = self
            .linked_indices(index)
            .into_iter()
            .map(|i| self.segments[i].id)
            .collect();
        self.segments.retain(|s| !ids.contains(&s.id));
        self.clip_effects.retain(|id, _| !ids.contains(id));
    }
    /// Commit a proposed clip edit as one atomic linked operation. Track and styling
    /// changes belong only to the selected clip. Source offsets retain synchronization.
    pub fn edit_linked(&mut self, index: usize, edited: Segment, duplicate: bool) -> Result<()> {
        let original = self
            .segments
            .get(index)
            .ok_or_else(|| Error::Invalid("Select a clip.".into()))?
            .clone();
        let indices = self.linked_indices(index);
        let delta_at = edited.at as i64 - original.at as i64;
        let delta_start = edited.start as i64 - original.start as i64;
        let delta_end = edited.end as i64 - original.end as i64;
        let mut next = self.clone();
        let link = duplicate.then(|| next.next_link());
        let mut next_id = next.next_media_id();
        for i in indices {
            let old = &self.segments[i];
            let mut c = if i == index {
                edited.clone()
            } else {
                old.clone()
            };
            if i != index {
                let at = old.at as i64 + delta_at;
                let start = old.start as i64 + delta_start;
                let end = old.end as i64 + delta_end;
                if at < 0 || start < 0 || end <= start {
                    return Err(Error::Invalid("Keep both linked clips inside their recorded range. Unlink to edit separately.".into()));
                }
                c.at = at as u64;
                c.start = start as u64;
                c.end = end as u64;
                c.speed = edited.speed;
            }
            let limit = c
                .source
                .as_ref()
                .map_or(self.source_duration, |s| s.duration);
            if c.start >= c.end || c.end > limit {
                return Err(Error::Invalid(
                    "The linked recording ends here. Unlink to trim this clip separately.".into(),
                ));
            }
            if !c.keyframes.is_empty() {
                c.keyframes.offset_ms = old.keyframes.offset_ms;
                c.keyframes
                    .trim_start(((c.start as f64 - old.start as f64) / old.speed).round() as i64);
            }
            if c.speed != old.speed {
                let ratio = old.speed / c.speed;
                c.keyframes
                    .remap_time(0, |ms| (ms as f64 * ratio).round() as u64, &[]);
            }
            c.clamp_fades();
            let duration = c.duration();
            for a in &mut c.audio {
                a.fade_in = a.fade_in.min(duration);
                a.fade_out = a.fade_out.min(duration);
            }
            if duplicate {
                c.id = next_id;
                next_id += 1;
                c.link = original.link.and(link);
                next.copy_clip_effects(old.id, c.id);
                next.segments.push(c);
            } else {
                next.segments[i] = c;
            }
        }
        *self = next;
        Ok(())
    }
    pub fn split_clip(&mut self, index: usize, time: u64) -> Result<()> {
        let indices = self.linked_indices(index);
        if indices.is_empty()
            || indices
                .iter()
                .any(|i| time <= self.segments[*i].at || time >= self.segments[*i].until())
        {
            return Err(Error::Invalid(
                "Move the playhead inside both linked clips before splitting.".into(),
            ));
        }
        let mut next = self.clone();
        let new_link = next.next_link();
        for (id, i) in (next.next_media_id()..).zip(indices) {
            let s = &mut next.segments[i];
            let source = s.source_time(time);
            let mut right = s.clone();
            right.id = id;
            right.at = time;
            right.start = source;
            right.keyframes.trim_start((time - s.at) as i64);
            right.fade_in = 0;
            right.link = s.link.map(|_| new_link);
            s.end = source;
            s.fade_out = 0;
            s.clamp_fades();
            right.clamp_fades();
            next.copy_clip_effects(self.segments[i].id, right.id);
            next.segments.push(right);
        }
        // Keep chronological neighbors for existing selection and split shortcuts.
        next.segments.sort_by_key(|s| (s.track, s.at, s.id));
        *self = next;
        Ok(())
    }
}

impl Project {
    pub fn reorder_layer(&mut self, layer: Layer, target: usize) {
        let mut order = self.layers();
        if let Some(index) = order.iter().position(|l| *l == layer) {
            order.remove(index);
            order.insert(target.min(order.len()), layer);
        }
        if let Layer::Zoom(track) = layer {
            for z in self.zooms.iter_mut().filter(|z| z.track() == track) {
                z.pinned = target == 0;
            }
        }
        self.layer_order = order;
    }
    pub fn remove_layer(&mut self, layer: Layer) -> Vec<u64> {
        let mut removed = vec![];
        match layer {
            Layer::Zoom(track) => self.zooms.retain(|z| z.track() != track),
            Layer::Annotation(id) => {
                removed = self
                    .objects
                    .iter()
                    .filter(|(_, t)| t.track == id)
                    .map(|(id, _)| *id)
                    .collect();
                self.objects.retain(|_, t| t.track != id);
                self.tracks.retain(|t| *t != id);
                self.automatic_tracks.retain(|t| *t != id);
                if self.annotation_track == Some(id) {
                    self.annotation_track = None;
                }
            }
            Layer::Media(id) => {
                self.segments.retain(|s| s.track != id);
                self.media_tracks.retain(|t| *t != id);
                self.media_kinds.remove(&id);
                self.media_levels.remove(&id);
                self.track_effects.remove(&id);
                self.clip_effects
                    .retain(|id, _| self.segments.iter().any(|c| c.id == *id));
                if self.media_tracks.is_empty() {
                    self.add_media_track();
                }
            }
        }
        self.track_settings.retain(|s| s.layer != layer);
        self.layer_order.retain(|l| *l != layer);
        self.cursor = self.cursor.min(self.duration().saturating_sub(1));
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> Project {
        let mut p = Project::new("screen.mp4".into(), [320, 180], 10000, 30);
        let mut c = Segment::new(
            2,
            1000,
            p.add_typed_track(TrackKind::Video),
            Some(Source {
                path: "camera.mp4".into(),
                size: [320, 180],
                duration: 6000,
                kind: MediaKind::Video,
            }),
            4000,
        );
        c.start = 1000;
        c.end = 5000;
        c.link = Some(1);
        c.frame = Some(Default::default());
        let mut a = c.clone();
        a.id = 3;
        a.track = p.add_typed_track(TrackKind::Voiceover);
        a.source = Some(Source {
            path: "voice.wav".into(),
            size: [1, 1],
            duration: 6000,
            kind: MediaKind::Audio,
        });
        a.frame = None;
        p.segments.extend([c, a]);
        p
    }
    #[test]
    fn linked_move_trim_copy_and_delete_are_atomic_but_style_and_tracks_are_independent() {
        let mut p = pair();
        let voice = p.segments[2].clone();
        let mut c = p.segments[1].clone();
        c.at = 2000;
        c.start = 1500;
        c.end = 4500;
        c.rect = [0.6, 0.5, 0.3, 0.3];
        c.hidden = true;
        c.frame.as_mut().unwrap().radius = 50.;
        p.edit_linked(1, c.clone(), false).unwrap();
        assert_eq!(
            (p.segments[2].at, p.segments[2].start, p.segments[2].end),
            (2000, 1500, 4500)
        );
        assert_eq!(p.segments[2].rect, voice.rect);
        assert!(!p.segments[2].hidden);
        assert_eq!(p.segments[2].frame, voice.frame);
        c.at = 6000;
        p.edit_linked(1, c, true).unwrap();
        assert_eq!(p.segments.len(), 5);
        assert_ne!(p.segments[3].link, p.segments[1].link);
        assert_eq!(p.segments[3].link, p.segments[4].link);
        p.delete_linked(3);
        assert_eq!(p.segments.len(), 3);
        let before = p.clone();
        let mut c = p.segments[1].clone();
        c.end = 9000;
        assert!(p.edit_linked(1, c, false).is_err());
        assert_eq!(p, before);
        p.unlink(1);
        assert!(p.segments[1].link.is_none() && p.segments[2].link.is_none());
    }
    #[test]
    fn split_and_ripple_produce_distinct_synchronized_pairs() {
        let mut p = pair();
        p.split_clip(1, 3000).unwrap();
        let camera: Vec<_> = p
            .segments
            .iter()
            .filter(|s| {
                s.source
                    .as_ref()
                    .is_some_and(|s| s.kind == MediaKind::Video)
            })
            .collect();
        assert_eq!(camera.len(), 2);
        assert_ne!(camera[0].link, camera[1].link);
        for c in camera {
            let a = p
                .segments
                .iter()
                .find(|a| a.id != c.id && a.link == c.link)
                .unwrap();
            assert_eq!((c.at, c.start, c.end), (a.at, a.start, a.end));
        }
        let mut p = pair();
        p.remove_range(2000, 3000).unwrap();
        for c in p.segments.iter().filter(|s| s.link.is_some()) {
            assert_eq!(p.segments.iter().filter(|s| s.link == c.link).count(), 2);
        }
    }
    #[test]
    fn linked_speed_and_layer_order_survive_current_format_roundtrip() {
        let mut p = pair();
        p.speed(1, 2.).unwrap();
        assert_eq!(p.segments[1].speed, p.segments[2].speed);
        p.move_layer(Layer::Media(2), -2);
        assert_eq!(p.layers()[0], Layer::Media(2));
        let json = serde_json::to_string(&p).unwrap();
        let copy: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(copy, p);
    }
}
