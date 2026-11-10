//! Stable clip selection and explicit track state, independent of the active track.
use super::{editor::*, links::Layer};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Item {
    Media(u64),
    Annotation(u64),
    Zoom(u64),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackSettings {
    pub layer: Layer,
    pub name: String,
    pub muted: bool,
    pub hidden: bool,
}
impl Project {
    pub fn remove_items(&mut self, selected: &BTreeSet<Item>) -> BTreeSet<u64> {
        let items = self.linked_items(selected);
        let ids: BTreeSet<_> = items
            .iter()
            .filter_map(|i| {
                if let Item::Annotation(id) = i {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        self.segments
            .retain(|s| !items.contains(&Item::Media(s.id)));
        self.clip_effects
            .retain(|id, _| !items.contains(&Item::Media(*id)));
        self.objects.retain(|id, _| !ids.contains(id));
        self.zooms.retain(|z| !items.contains(&Item::Zoom(z.id)));
        self.layer_order.retain(
            |l| !matches!(l, Layer::Zoom(t) if !self.zooms.iter().any(|z| z.track() == *t)),
        );
        self.cursor = self.cursor.min(self.duration().saturating_sub(1));
        ids
    }
    pub fn track_settings_mut(&mut self, layer: Layer) -> &mut TrackSettings {
        if let Some(i) = self.track_settings.iter().position(|s| s.layer == layer) {
            return &mut self.track_settings[i];
        }
        self.track_settings.push(TrackSettings {
            layer,
            name: String::new(),
            muted: false,
            hidden: false,
        });
        self.track_settings.last_mut().unwrap()
    }
    pub fn layer_hidden(&self, layer: Layer) -> bool {
        self.track_settings
            .iter()
            .any(|s| s.layer == layer && s.hidden)
    }
    pub fn track_muted(&self, id: u64) -> bool {
        self.track_settings
            .iter()
            .any(|s| s.layer == Layer::Media(id) && s.muted)
    }
    pub fn custom_track_name(&self, layer: Layer) -> Option<&str> {
        self.track_settings
            .iter()
            .find(|s| s.layer == layer && !s.name.trim().is_empty())
            .map(|s| s.name.as_str())
    }
    pub fn item_range(&self, item: Item) -> Option<(u64, u64, Layer)> {
        match item {
            Item::Media(id) => self
                .segments
                .iter()
                .find(|s| s.id == id)
                .map(|s| (s.at, s.until(), Layer::Media(s.track))),
            Item::Annotation(id) => self
                .objects
                .get(&id)
                .map(|t| (t.start, t.end, Layer::Annotation(t.track))),
            Item::Zoom(id) => self
                .zooms
                .iter()
                .find(|z| z.id == id)
                .map(|z| (z.start, z.end, Layer::Zoom(z.track()))),
        }
    }
    pub fn items(&self) -> Vec<Item> {
        self.segments
            .iter()
            .map(|c| Item::Media(c.id))
            .chain(self.objects.keys().map(|id| Item::Annotation(*id)))
            .chain(self.zooms.iter().map(|z| Item::Zoom(z.id)))
            .collect()
    }
    pub fn linked_items(&self, selected: &BTreeSet<Item>) -> BTreeSet<Item> {
        let links: BTreeSet<_> = self
            .segments
            .iter()
            .filter(|c| selected.contains(&Item::Media(c.id)))
            .filter_map(|c| c.link)
            .collect();
        selected
            .iter()
            .copied()
            .chain(
                self.segments
                    .iter()
                    .filter(|c| c.link.is_some_and(|l| links.contains(&l)))
                    .map(|c| Item::Media(c.id)),
            )
            .collect()
    }
    pub fn link_items(&mut self, selected: &BTreeSet<Item>) -> Result<()> {
        let all = self.linked_items(selected);
        let count = self
            .segments
            .iter()
            .filter(|c| all.contains(&Item::Media(c.id)))
            .count();
        if count < 2 {
            return Err(Error::Invalid(
                "Select at least two audio or video clips to link.".into(),
            ));
        }
        let link = self.next_link();
        for c in &mut self.segments {
            if all.contains(&Item::Media(c.id)) {
                c.link = Some(link);
            }
        }
        Ok(())
    }
    pub fn unlink_items(&mut self, selected: &BTreeSet<Item>) {
        let all = self.linked_items(selected);
        for c in &mut self.segments {
            if all.contains(&Item::Media(c.id)) {
                c.link = None;
            }
        }
    }
    /// Every member moves by the same time and layer delta; collisions are rejected,
    /// never silently rerouted to another track.
    pub fn move_items(&self, selected: &BTreeSet<Item>, delta: i64, rows: i32) -> Result<Self> {
        let items = self.linked_items(selected);
        let layers = self.layers();
        let first = items
            .iter()
            .filter_map(|i| self.item_range(*i))
            .map(|r| r.0)
            .min()
            .unwrap_or(0);
        let delta = delta.max(-(first as i64));
        let mut p = self.clone();
        for item in &items {
            let Some((_, _, layer)) = self.item_range(*item) else {
                continue;
            };
            let index = self.layer_index(layer) as i32 + rows;
            let target = if rows == 0 {
                layer
            } else {
                *layers
                    .get(usize::try_from(index).unwrap_or(usize::MAX))
                    .ok_or_else(|| {
                        Error::Invalid("Keep the selected clips within compatible tracks.".into())
                    })?
            };
            match (*item, target) {
                (Item::Media(id), Layer::Media(track)) => {
                    let c = p.segments.iter_mut().find(|c| c.id == id).unwrap();
                    c.at = c.at.saturating_add_signed(delta);
                    c.track = track;
                }
                (Item::Annotation(id), Layer::Annotation(track)) => {
                    let t = p.objects.get_mut(&id).unwrap();
                    t.start = t.start.saturating_add_signed(delta);
                    t.end = t.end.saturating_add_signed(delta);
                    t.track = track;
                }
                (Item::Zoom(id), Layer::Zoom(other))
                    if self.zooms.iter().any(|z| z.id == id && z.track() == other) =>
                {
                    let z = p.zooms.iter_mut().find(|z| z.id == id).unwrap();
                    z.start = z.start.saturating_add_signed(delta);
                    z.end = z.end.saturating_add_signed(delta);
                }
                _ => {
                    return Err(Error::Invalid(
                        "Use an annotation track for objects and a media track for audio/video."
                            .into(),
                    ));
                }
            }
        }
        for item in items {
            if let Item::Annotation(id) = item {
                let t = &p.objects[&id];
                if !p.free(t.track, id, t.start, t.end) {
                    return Err(Error::Invalid(
                        "This range already contains an annotation. Use an empty track or range."
                            .into(),
                    ));
                }
            }
        }
        Ok(p)
    }
    /// Snap to the playhead, origin, or an unselected clip edge in screen-space tolerance.
    pub fn snap_time(&self, time: u64, excluded: &BTreeSet<Item>, tolerance: u64) -> u64 {
        [0, self.cursor]
            .into_iter()
            .chain(
                self.items()
                    .into_iter()
                    .filter(|i| !excluded.contains(i))
                    .filter_map(|i| self.item_range(i))
                    .flat_map(|(a, b, _)| [a, b]),
            )
            .filter(|n| n.abs_diff(time) <= tolerance)
            .min_by_key(|n| (n.abs_diff(time), u8::from(*n != self.cursor)))
            .unwrap_or(time)
    }
    pub fn snap_move(&self, selected: &BTreeSet<Item>, delta: i64, tolerance: u64) -> i64 {
        let items = self.linked_items(selected);
        items
            .iter()
            .filter_map(|i| self.item_range(*i))
            .flat_map(|(a, b, _)| [a, b])
            .map(|edge| {
                let proposed = edge.saturating_add_signed(delta);
                delta + self.snap_time(proposed, &items, tolerance) as i64 - proposed as i64
            })
            .filter(|candidate| *candidate != delta)
            .min_by_key(|candidate| candidate.abs_diff(delta))
            .unwrap_or(delta)
    }
    /// Hold at a neighboring annotation boundary instead of reverting to the drag origin.
    pub fn move_items_near(
        &self,
        selected: &BTreeSet<Item>,
        delta: i64,
        rows: i32,
    ) -> Result<Self> {
        if let Ok(p) = self.move_items(selected, delta, rows) {
            return Ok(p);
        }
        let items = self.linked_items(selected);
        let layers = self.layers();
        let mut candidates = Vec::new();
        for item in &items {
            let Item::Annotation(id) = item else {
                continue;
            };
            let Some((start, end, layer)) = self.item_range(*item) else {
                continue;
            };
            let index = self.layer_index(layer) as i32 + rows;
            let Some(Layer::Annotation(track)) =
                layers.get(usize::try_from(index).unwrap_or(usize::MAX))
            else {
                continue;
            };
            for (other, t) in &self.objects {
                if *other != *id && !items.contains(&Item::Annotation(*other)) && t.track == *track
                {
                    candidates.extend([t.start as i64 - end as i64, t.end as i64 - start as i64]);
                }
            }
        }
        candidates.sort_by_key(|d| d.abs_diff(delta));
        for d in candidates {
            if let Ok(p) = self.move_items(selected, d, rows) {
                return Ok(p);
            }
        }
        self.move_items(selected, delta, rows)
    }
    /// Insert a held image without replacing or flattening the source video.
    pub fn insert_still(
        &self,
        id: u64,
        at: u64,
        duration: u64,
        path: String,
        all_tracks: bool,
    ) -> Result<Self> {
        let original = self
            .segments
            .iter()
            .find(|c| c.id == id && c.kind() == MediaKind::Video)
            .ok_or_else(|| Error::Invalid("Select a video clip.".into()))?;
        if at <= original.at || at >= original.until() {
            return Err(Error::Invalid(
                "Move the playhead inside the video clip before generating a still frame.".into(),
            ));
        }
        if duration == 0 || self.duration().saturating_add(duration) > 14_400_000 || path.is_empty()
        {
            return Err(Error::Invalid(
                "Choose a positive hold duration that keeps the project under four hours.".into(),
            ));
        }
        let mut tracks = BTreeSet::from([original.track]);
        if let Some(link) = original.link {
            tracks.extend(
                self.segments
                    .iter()
                    .filter(|c| c.link == Some(link))
                    .map(|c| c.track),
            );
        }
        let mut p = self.clone();
        p.segments.clear();
        let mut next_id = self.next_media_id();
        let mut next_link = self.next_link();
        let mut right_links = std::collections::BTreeMap::new();
        for c in &self.segments {
            if !all_tracks && !tracks.contains(&c.track) {
                p.segments.push(c.clone());
                continue;
            }
            if c.at >= at {
                let mut c = c.clone();
                c.at += duration;
                p.segments.push(c);
            } else if c.until() > at {
                if c.end.saturating_sub(c.start) < 2 {
                    return Err(Error::Invalid(
                        "Move the playhead at least one source frame away from the clip edge."
                            .into(),
                    ));
                }
                let source_at = c.source_time(at).clamp(c.start + 1, c.end - 1);
                let mut left = c.clone();
                left.end = source_at;
                left.fade_out = 0;
                left.clamp_fades();
                let left_duration = left.duration();
                for audio in &mut left.audio {
                    audio.fade_in = audio.fade_in.min(left_duration);
                    audio.fade_out = 0;
                }
                let mut right = c.clone();
                right.id = next_id;
                next_id += 1;
                right.at = at + duration;
                right.start = source_at;
                right.keyframes.trim_start((at - c.at) as i64);
                right.fade_in = 0;
                right.clamp_fades();
                let right_duration = right.duration();
                for audio in &mut right.audio {
                    audio.fade_in = 0;
                    audio.fade_out = audio.fade_out.min(right_duration);
                }
                if let Some(link) = c.link {
                    right.link = Some(*right_links.entry(link).or_insert_with(|| {
                        let n = next_link;
                        next_link += 1;
                        n
                    }));
                }
                p.copy_clip_effects(c.id, right.id);
                p.segments.extend([left, right]);
            } else {
                p.segments.push(c.clone());
            }
        }
        let size = original
            .source
            .as_ref()
            .map_or(self.source_size, |s| s.size);
        let mut still = Segment::new(
            next_id,
            at,
            original.track,
            Some(Source {
                path,
                size,
                duration: 14_400_000,
                kind: MediaKind::Image,
            }),
            duration,
        );
        still.label = "Still frame".into();
        let keyed = original.keyed(at);
        still.rect = keyed.rect;
        still.frame = keyed.frame.clone();
        still.hidden = original.hidden;
        p.segments.push(still);
        if all_tracks {
            for t in p.objects.values_mut() {
                if t.start < at && t.end > at {
                    t.keyframes.remap_time(
                        t.start,
                        |x| if x >= at { x + duration } else { x },
                        &[at.saturating_sub(1), at],
                    );
                }
                if t.start < at
                    && t.end > at
                    && let Some(path) = &mut t.motion_path
                {
                    std::sync::Arc::make_mut(path).remap_time(
                        t.start,
                        |x| if x >= at { x + duration } else { x },
                        &[at.saturating_sub(1), at],
                    );
                }
                if t.start >= at {
                    t.start += duration;
                }
                if t.end > at {
                    t.end += duration;
                }
                t.clamp_animations();
            }
            for z in &mut p.zooms {
                if z.start >= at {
                    z.start += duration;
                }
                if z.end > at {
                    z.end += duration;
                }
            }
        }
        p.cursor = at;
        Ok(p)
    }
    pub fn copy_clip_effects(&mut self, from: u64, to: u64) {
        if let Some(chain) = self.clip_effects.get(&from).cloned() {
            self.clip_effects.insert(to, chain);
        }
    }
}

#[cfg(test)]
mod flow_tests {
    use super::*;
    fn timeline() -> Project {
        let mut p = Project::new("source.mp4".into(), [320, 180], 10000, 30);
        p.cursor = 5500;
        p.place(1, Timing::new(1, 1000, 2000));
        p.place(2, Timing::new(1, 3000, 4000));
        p
    }
    #[test]
    fn snapping_uses_both_edges_playhead_and_excludes_selected_group() {
        let p = timeline();
        let selected = BTreeSet::from([Item::Annotation(1)]);
        assert_eq!(p.snap_move(&selected, 970, 80), 1000);
        assert_eq!(p.snap_move(&selected, 3480, 80), 3500);
        assert_eq!(p.snap_move(&selected, 700, 80), 700);
        let both = BTreeSet::from([Item::Annotation(1), Item::Annotation(2)]);
        assert_eq!(p.snap_move(&both, 30, 80), 30);
    }
    #[test]
    fn collision_holds_at_touching_boundary_and_preserves_group_offsets() {
        let p = timeline();
        let selected = BTreeSet::from([Item::Annotation(1)]);
        let next = p.move_items_near(&selected, 1100, 0).unwrap();
        assert_eq!(next.objects[&1].end, 3000);
        let back = p.move_items_near(&selected, 500, 0).unwrap();
        assert_eq!(back.objects[&1].start, 1500);
        let both = BTreeSet::from([Item::Annotation(1), Item::Annotation(2)]);
        let next = p.move_items_near(&both, 1500, 0).unwrap();
        assert_eq!(next.objects[&2].start - next.objects[&1].start, 2000);
    }
    #[test]
    fn still_insertion_splits_linked_sources_and_preserves_other_tracks() {
        let mut p = timeline();
        p.segments[0].link = Some(1);
        p.segments[0].audio.push(Audio {
            path: "source.wav".into(),
            label: "Audio".into(),
            volume: 1.,
            muted: false,
            fade_in: 0,
            fade_out: 0,
        });
        let i = 0;
        p.separate_audio(i).unwrap();
        let source_audio = p
            .segments
            .iter_mut()
            .find(|c| c.kind() == MediaKind::Audio)
            .unwrap();
        source_audio.audio[0].fade_in = 5000;
        source_audio.audio[0].fade_out = 1000;
        let other = p.add_media_track();
        let mut unrelated = p.segments[0].clone();
        unrelated.id = p.next_media_id();
        unrelated.track = other;
        unrelated.link = None;
        let unrelated_id = unrelated.id;
        p.segments.push(unrelated.clone());
        p.clip_effects
            .insert(1, super::super::effects::Chain::narration());
        let next = p
            .insert_still(1, 2000, 3500, "still.png".into(), false)
            .unwrap();
        assert_eq!(
            next.segments.iter().find(|c| c.id == unrelated_id),
            Some(&unrelated)
        );
        let still = next.segments.last().unwrap();
        assert_eq!(still.kind(), MediaKind::Image);
        assert!(still.audio.is_empty());
        assert_eq!((still.at, still.until()), (2000, 5500));
        let resumed = next
            .segments
            .iter()
            .find(|c| c.track == 1 && c.at == 5500)
            .unwrap();
        assert_eq!(resumed.start, 2000);
        assert_eq!(next.clip_effects[&resumed.id], p.clip_effects[&1]);
        let audio = next
            .segments
            .iter()
            .find(|c| c.kind() == MediaKind::Audio && c.at == 5500)
            .unwrap();
        assert_eq!(audio.link, resumed.link);
        assert_eq!((audio.audio[0].fade_in, audio.audio[0].fade_out), (0, 1000));
        let left_audio = next
            .segments
            .iter()
            .find(|c| c.kind() == MediaKind::Audio && c.at == 0)
            .unwrap();
        assert_eq!(
            (left_audio.audio[0].fade_in, left_audio.audio[0].fade_out),
            (2000, 0)
        );
        assert_eq!(next.objects, p.objects);
        let all = p
            .insert_still(1, 2000, 1000, "still.png".into(), true)
            .unwrap();
        assert_eq!(all.objects[&2].start, 4000);
        assert!(
            p.insert_still(1, 0, 1000, "still.png".into(), false)
                .is_err()
        );
        assert_eq!(p.segments[0].end, 10000);
    }
}
