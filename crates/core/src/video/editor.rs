//! Editable video timeline. Time is integer milliseconds; intervals are half-open.
use crate::{Annotation, Document, Error, Kind, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Animation {
    #[default]
    None,
    Fade,
    Float,
    Path,
    Typewriter,
    Scale,
}
impl Animation {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Fade,
        Self::Float,
        Self::Path,
        Self::Typewriter,
        Self::Scale,
    ];
    pub fn label(self, exit: bool) -> &'static str {
        match (self, exit) {
            (Self::None, _) => "None",
            (Self::Fade, false) => "Fade in",
            (Self::Fade, true) => "Fade out",
            (Self::Float, false) => "Float in",
            (Self::Float, true) => "Float out",
            (Self::Path, false) => "Draw along path",
            (Self::Path, true) => "Retract along path",
            (Self::Typewriter, false) => "Typewriter",
            (Self::Typewriter, true) => "Erase",
            (Self::Scale, false) => "Expand from center",
            (Self::Scale, true) => "Shrink to center",
        }
    }
    pub fn supports(self, a: &Annotation) -> bool {
        if a.focus.is_some() && matches!(self, Self::Path | Self::Typewriter) {
            return false;
        }
        !matches!(a.kind, Kind::Redact | Kind::Blur | Kind::Pixelate)
            && match self {
                Self::Path => {
                    a.is_shape()
                        || matches!(
                            a.kind,
                            Kind::Line
                                | Kind::Arrow
                                | Kind::Callout
                                | Kind::Pen
                                | Kind::Highlighter
                        )
                }
                Self::Typewriter => a.text.is_some(),
                Self::Scale => a.is_shape(),
                _ => true,
            }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    #[serde(default, skip_serializing_if = "super::keyframes::Set::is_empty")]
    pub keyframes: super::keyframes::Set,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_path: Option<std::sync::Arc<super::motion_path::Path>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub track: u64,
    pub start: u64,
    pub end: u64,
    pub entrance: Animation,
    pub exit: Animation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entrance_stack: Vec<Animation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exit_stack: Vec<Animation>,
    pub entrance_ms: u64,
    pub exit_ms: u64,
    pub paired: bool,
    pub smooth: bool,
    pub direction: u8,
    #[serde(default)]
    pub path_reverse: bool,
    #[serde(default)]
    pub retract_from_start: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entrance_durations: BTreeMap<Animation, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub exit_durations: BTreeMap<Animation, u64>,
}
impl Timing {
    pub fn new(track: u64, start: u64, end: u64) -> Self {
        Self {
            keyframes: Default::default(),
            motion_path: None,
            label: String::new(),
            track,
            start,
            end,
            entrance: Animation::None,
            exit: Animation::None,
            entrance_stack: vec![],
            exit_stack: vec![],
            entrance_ms: 400,
            exit_ms: 400,
            paired: true,
            smooth: true,
            direction: 0,
            path_reverse: false,
            retract_from_start: false,
            entrance_durations: BTreeMap::new(),
            exit_durations: BTreeMap::new(),
        }
    }
    pub fn active(&self, t: u64) -> bool {
        self.start <= t && t < self.end
    }
    /// The time to draw at. A redaction stamped slightly after a frame still
    /// covers that frame, so its first frame is never left bare.
    fn drawn_at(&self, time: u64) -> u64 {
        let redaction = self
            .motion_path
            .as_ref()
            .is_some_and(|p| p.purpose == super::motion_path::Purpose::Redaction);
        if redaction
            && time < self.start
            && self.start - time <= super::motion_path::NEAREST_FRAME_MS
        {
            self.start
        } else {
            time
        }
    }
    pub fn stack_compatible(primary: Animation, stack: &[Animation], candidate: Animation) -> bool {
        candidate != Animation::None
            && candidate != primary
            && !stack.contains(&candidate)
            && !([primary].into_iter().chain(stack.iter().copied()).any(|a| {
                matches!(
                    (a, candidate),
                    (Animation::Fade, Animation::Float) | (Animation::Float, Animation::Fade)
                )
            }))
    }
    /// Gives this object, `a`, what the edit from `before` to `after` changed on
    /// another selected object. Start and End move by the same amount, so the
    /// objects keep their arrangement. Animations `a` cannot use are left alone.
    pub fn copy_change(&mut self, before: &Timing, after: &Timing, a: &Annotation) {
        let shift = after.start as i64 - before.start as i64;
        let stretch = after.end as i64 - before.end as i64;
        if shift != 0 || stretch != 0 {
            let start = self.start.saturating_add_signed(shift);
            self.end = self.end.saturating_add_signed(stretch).max(start + 1);
            self.start = start;
            if shift != 0 {
                if let Some(path) = self.motion_path.as_mut() {
                    let path = std::sync::Arc::make_mut(path);
                    path.offset_ms = path.offset_ms.saturating_add_signed(shift);
                }
                self.keyframes.trim_start(shift);
            }
        }
        macro_rules! copy {
            ($($field:ident),+) => {$(
                if after.$field != before.$field {
                    self.$field = after.$field.clone();
                }
            )+};
        }
        copy!(
            entrance_ms,
            exit_ms,
            paired,
            smooth,
            direction,
            path_reverse,
            retract_from_start
        );
        if after.entrance != before.entrance && after.entrance.supports(a) {
            self.entrance = after.entrance;
        }
        if after.exit != before.exit && after.exit.supports(a) {
            self.exit = after.exit;
        }
        if after.entrance_stack != before.entrance_stack {
            self.entrance_stack = after.entrance_stack.clone();
        }
        if after.exit_stack != before.exit_stack {
            self.exit_stack = after.exit_stack.clone();
        }
        copy!(entrance_durations, exit_durations);
        self.normalize_stacks(a);
    }
    pub fn normalize_stacks(&mut self, a: &Annotation) {
        for (primary, stack) in [
            (&mut self.entrance, &mut self.entrance_stack),
            (&mut self.exit, &mut self.exit_stack),
        ] {
            let old = std::mem::take(stack);
            for candidate in old {
                if candidate.supports(a) && Self::stack_compatible(*primary, stack, candidate) {
                    if *primary == Animation::None {
                        *primary = candidate;
                    } else {
                        stack.push(candidate);
                    }
                }
            }
        }
    }
    pub fn effect_ms(&self, effect: Animation, exit: bool) -> u64 {
        let (primary, durations, fallback) = if exit {
            (self.exit, &self.exit_durations, self.exit_ms)
        } else {
            (self.entrance, &self.entrance_durations, self.entrance_ms)
        };
        if effect == primary {
            fallback
        } else {
            durations.get(&effect).copied().unwrap_or(fallback)
        }
    }
    pub fn animation_span(&self, exit: bool) -> u64 {
        let (primary, stack) = if exit {
            (self.exit, &self.exit_stack)
        } else {
            (self.entrance, &self.entrance_stack)
        };
        std::iter::once(primary)
            .chain(stack.iter().copied())
            .filter(|a| *a != Animation::None)
            .map(|a| self.effect_ms(a, exit))
            .max()
            .unwrap_or(0)
    }
    pub fn clamp_animations(&mut self) {
        let n = self.end.saturating_sub(self.start);
        self.entrance_ms = self.entrance_ms.min(n);
        self.exit_ms = self.exit_ms.min(n - self.entrance_ms);
        self.entrance_durations
            .retain(|a, _| self.entrance_stack.contains(a));
        self.exit_durations
            .retain(|a, _| self.exit_stack.contains(a));
        for value in self.entrance_durations.values_mut() {
            *value = (*value).min(n.saturating_sub(self.exit_ms));
        }
        let room = n.saturating_sub(self.animation_span(false));
        self.exit_ms = self.exit_ms.min(room);
        for value in self.exit_durations.values_mut() {
            *value = (*value).min(room);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub path: String,
    pub size: [u32; 2],
    pub duration: u64,
    pub kind: MediaKind,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    #[serde(default, skip_serializing_if = "super::keyframes::Set::is_empty")]
    pub keyframes: super::keyframes::Set,
    #[serde(default)]
    pub mono: bool,
    pub id: u64,
    pub at: u64,
    pub track: u64,
    /// None refers to the project's original recording; imported media owns its descriptor.
    pub source: Option<Source>,
    pub label: String,
    pub start: u64,
    pub end: u64,
    pub speed: f64,
    pub fade_in: u64,
    pub fade_out: u64,
    pub audio: Vec<Audio>,
    pub rect: [f32; 4],
    pub link: Option<u64>,
    pub hidden: bool,
    pub frame: Option<super::frame::Frame>,
}
impl Segment {
    pub fn duration(&self) -> u64 {
        ((self.end.saturating_sub(self.start)) as f64 / self.speed).round() as u64
    }
    pub fn until(&self) -> u64 {
        self.at.saturating_add(self.duration())
    }
    pub fn active(&self, t: u64) -> bool {
        self.at <= t && t < self.until()
    }
    pub fn kind(&self) -> MediaKind {
        self.source.as_ref().map_or(MediaKind::Video, |s| s.kind)
    }
    pub fn source_time(&self, t: u64) -> u64 {
        self.start
            + ((t.saturating_sub(self.at) as f64 * self.speed).round() as u64)
                .min(self.end.saturating_sub(self.start + 1))
    }
    /// Gives this clip what the edit from `before` to `after` changed on another
    /// selected clip. Moving keeps the clips' spacing; position on the canvas and
    /// the source range stay each clip's own.
    pub fn copy_change(&mut self, before: &Segment, after: &Segment) {
        self.at = self
            .at
            .saturating_add_signed(after.at as i64 - before.at as i64);
        macro_rules! copy {
            ($($field:ident),+) => {$(
                if after.$field != before.$field {
                    self.$field = after.$field.clone();
                }
            )+};
        }
        copy!(mono, fade_in, fade_out, hidden);
        if let (Some(mine), Some(old), Some(new)) = (
            self.frame.as_mut(),
            before.frame.as_ref(),
            after.frame.as_ref(),
        ) {
            mine.copy_change(old, new);
        } else if before.frame.is_none() && after.frame.is_some() {
            self.frame = after.frame.clone();
        }
        for (mine, (old, new)) in self
            .audio
            .iter_mut()
            .zip(before.audio.iter().zip(&after.audio))
        {
            if old.volume != new.volume {
                mine.volume = new.volume;
            }
            if old.muted != new.muted {
                mine.muted = new.muted;
            }
            if old.fade_in != new.fade_in {
                mine.fade_in = new.fade_in;
            }
            if old.fade_out != new.fade_out {
                mine.fade_out = new.fade_out;
            }
        }
        self.clamp_fades();
    }
    pub fn clamp_fades(&mut self) {
        self.fade_in = self.fade_in.min(self.duration());
        self.fade_out = self.fade_out.min(self.duration() - self.fade_in);
    }
    pub fn new(id: u64, at: u64, track: u64, source: Option<Source>, duration: u64) -> Self {
        let label = source
            .as_ref()
            .and_then(|s| std::path::Path::new(&s.path).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Recording".into());
        Self {
            keyframes: Default::default(),
            mono: false,
            id,
            at,
            track,
            source,
            label,
            start: 0,
            end: duration,
            speed: 1.,
            fade_in: 0,
            fade_out: 0,
            audio: vec![],
            rect: [0., 0., 1., 1.],
            link: None,
            hidden: false,
            frame: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub path: String,
    pub label: String,
    pub volume: f32,
    pub muted: bool,
    pub fade_in: u64,
    pub fade_out: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Audio,
    Voiceover,
    Video,
}
impl TrackKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Audio => "Audio",
            Self::Voiceover => "Voiceover",
            Self::Video => "Video",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub gif_ranges: Vec<crate::gif_studio::Range>,
    #[serde(default)]
    pub track_settings: Vec<super::editing::TrackSettings>,
    #[serde(default)]
    pub zooms: Vec<super::zoom::Zoom>,
    /// The quality this project was recorded at. None for imported video.
    #[serde(default)]
    pub recorded_quality: Option<super::Quality>,
    pub source: String,
    pub source_duration: u64,
    pub source_size: [u32; 2],
    pub fps: u32,
    pub segments: Vec<Segment>,
    pub tracks: Vec<u64>,
    #[serde(default)]
    pub automatic_tracks: Vec<u64>,
    #[serde(skip)]
    pub annotation_track: Option<u64>,
    pub objects: BTreeMap<u64, Timing>,
    pub media_tracks: Vec<u64>,
    pub layer_order: Vec<super::links::Layer>,
    pub media_levels: BTreeMap<u64, f32>,
    pub media_kinds: BTreeMap<u64, TrackKind>,
    #[serde(default)]
    pub track_effects: BTreeMap<u64, super::effects::Chain>,
    pub clip_effects: BTreeMap<u64, super::effects::Chain>,
    pub crop: [u32; 4],
    pub fill: bool,
    pub background: [u8; 4],
    /// Editor cursor is not an edit; ignore it in saved-state comparisons.
    #[serde(default)]
    pub cursor: u64,
}
impl Project {
    pub fn same_clock(&self, other: &Self) -> bool {
        self.source == other.source
            && self.duration() == other.duration()
            && self.segments.len() == other.segments.len()
            && self.segments.iter().zip(&other.segments).all(|(a, b)| {
                a.id == b.id
                    && a.at == b.at
                    && a.start == b.start
                    && a.end == b.end
                    && a.speed == b.speed
                    && a.source == b.source
                    && a.audio
                        .iter()
                        .map(|a| &a.path)
                        .eq(b.audio.iter().map(|a| &a.path))
            })
    }
    pub fn new(source: String, size: [u32; 2], duration: u64, fps: u32) -> Self {
        Self {
            gif_ranges: vec![],
            track_settings: vec![],
            zooms: vec![],
            recorded_quality: None,
            source,
            source_duration: duration,
            source_size: size,
            fps,
            segments: vec![Segment::new(1, 0, 1, None, duration)],
            tracks: vec![1],
            automatic_tracks: vec![1],
            annotation_track: None,
            objects: BTreeMap::new(),
            media_tracks: vec![1],
            layer_order: vec![],
            media_levels: BTreeMap::new(),
            media_kinds: BTreeMap::new(),
            track_effects: BTreeMap::new(),
            clip_effects: BTreeMap::new(),
            crop: [0, 0, size[0], size[1]],
            fill: false,
            background: [19, 22, 22, 255],
            cursor: 0,
        }
    }
    pub fn duration(&self) -> u64 {
        self.segments
            .iter()
            .map(Segment::until)
            .chain(self.objects.values().map(|t| t.end))
            .chain(self.zooms.iter().map(|z| z.end))
            .max()
            .unwrap_or(1)
    }
    /// Audio edits do not invalidate decoded video or its retained raster textures.
    pub fn same_visual(&self, other: &Self) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        for p in [&mut a, &mut b] {
            p.gif_ranges.clear();
            p.clip_effects.clear();
            p.track_effects.clear();
            p.media_levels.clear();
            for state in &mut p.track_settings {
                state.name.clear();
                state.muted = false;
            }
            p.track_settings.retain(|s| s.hidden);
            for s in &mut p.segments {
                s.mono = false;
            }
            p.cursor = 0;
            p.annotation_track = None;
            for s in &mut p.segments {
                s.audio.clear();
            }
        }
        a == b
    }
    pub fn same_edit(&self, other: &Self) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.cursor = 0;
        b.cursor = 0;
        a.annotation_track = None;
        b.annotation_track = None;
        a == b
    }
    pub fn source_time(&self, time: u64) -> u64 {
        self.segments
            .iter()
            .find(|s| s.active(time))
            .map_or(0, |s| s.source_time(time))
    }
    pub fn next_media_id(&self) -> u64 {
        self.segments.iter().map(|s| s.id).max().unwrap_or(0) + 1
    }
    pub fn add_media_track(&mut self) -> u64 {
        let id = self.media_tracks.iter().max().copied().unwrap_or(0) + 1;
        self.media_tracks.push(id);
        id
    }
    pub fn track_kind(&self, id: u64) -> TrackKind {
        self.media_kinds.get(&id).copied().unwrap_or_else(|| {
            if self
                .segments
                .iter()
                .any(|c| c.track == id && c.label == "Voiceover")
            {
                TrackKind::Voiceover
            } else if self
                .segments
                .iter()
                .any(|c| c.track == id && c.kind() != MediaKind::Audio)
            {
                TrackKind::Video
            } else {
                TrackKind::Audio
            }
        })
    }
    pub fn track_label(&self, id: u64) -> String {
        if let Some(name) = self.custom_track_name(super::links::Layer::Media(id)) {
            return name.into();
        }
        format!(
            "{} {}",
            self.track_kind(id).label(),
            self.media_tracks.iter().position(|v| *v == id).unwrap_or(0) + 1
        )
    }
    pub fn add_typed_track(&mut self, kind: TrackKind) -> u64 {
        let id = self.add_media_track();
        self.media_kinds.insert(id, kind);
        id
    }
    pub fn has_audio(&self) -> bool {
        self.segments.iter().any(|s| !s.audio.is_empty())
    }
    pub fn media_paths(&self) -> impl Iterator<Item = &String> {
        std::iter::once(&self.source).chain(self.segments.iter().flat_map(|s| {
            s.source
                .iter()
                .map(|a| &a.path)
                .chain(s.audio.iter().map(|a| &a.path))
        }))
    }
    pub fn media_paths_mut(&mut self) -> impl Iterator<Item = &mut String> {
        std::iter::once(&mut self.source).chain(self.segments.iter_mut().flat_map(|s| {
            s.source
                .iter_mut()
                .map(|a| &mut a.path)
                .chain(s.audio.iter_mut().map(|a| &mut a.path))
        }))
    }
    /// Same-track overlaps crossfade. Extra concurrent clips are rejected instead of silently hidden.
    pub fn opacity(&self, clip: &Segment, time: u64) -> f64 {
        if !clip.active(time) {
            return 0.;
        }
        let mut gain = 1f64;
        if clip.fade_in > 0 {
            gain *= ((time - clip.at) as f64 / clip.fade_in as f64).min(1.);
        }
        if clip.fade_out > 0 {
            gain *= ((clip.until() - time) as f64 / clip.fade_out as f64).min(1.);
        }
        for other in &self.segments {
            if other.id == clip.id
                || other.track != clip.track
                || (other.kind() == MediaKind::Audio) != (clip.kind() == MediaKind::Audio)
                || !other.active(time)
            {
                continue;
            }
            let lo = clip.at.max(other.at);
            let hi = clip.until().min(other.until());
            let p = (time - lo) as f64 / (hi - lo).max(1) as f64;
            gain *= if (clip.at, clip.id) > (other.at, other.id) {
                p
            } else {
                1. - p
            };
        }
        gain
    }
    pub fn free(&self, track: u64, id: u64, start: u64, end: u64) -> bool {
        !self
            .objects
            .iter()
            .any(|(i, t)| *i != id && t.track == track && t.start < end && start < t.end)
    }
    pub fn add_track(&mut self) -> u64 {
        let n = self.tracks.iter().copied().max().unwrap_or(0) + 1;
        let mut order = self.layers();
        self.tracks.push(n);
        let insert = order.iter().take_while(|l| matches!(l, super::links::Layer::Zoom(t) if self.zooms.iter().any(|z| z.track() == *t && z.pinned))).count();
        order.insert(insert, super::links::Layer::Annotation(n));
        self.layer_order = order;
        n
    }
    pub fn place(&mut self, id: u64, mut t: Timing) {
        t.clamp_animations();
        if let Some(track) = self.annotation_track
            && t.track == 0
            && !self.objects.contains_key(&id)
            && self.tracks.contains(&track)
            && self.free(track, id, t.start, t.end)
        {
            t.track = track;
        }
        if !self.tracks.contains(&t.track) || !self.free(t.track, id, t.start, t.end) {
            t.track = self
                .tracks
                .iter()
                .copied()
                .find(|n| self.free(*n, id, t.start, t.end))
                .unwrap_or_else(|| {
                    let id = self.add_track();
                    self.automatic_tracks.push(id);
                    id
                });
        }
        self.objects.insert(id, t);
    }
    /// Puts each listed zoom on the first zoom track it fits without overlapping
    /// another zoom, starting a new zoom track only when every existing one
    /// overlaps (BB6). Zooms are packed in start order.
    pub fn pack_zooms(&mut self, ids: &[u64]) {
        let mut order: Vec<_> = ids.to_vec();
        order.sort_by_key(|id| {
            self.zooms
                .iter()
                .find(|z| z.id == *id)
                .map_or(0, |z| z.start)
        });
        for id in order {
            let Some(index) = self.zooms.iter().position(|z| z.id == id) else {
                continue;
            };
            let (start, end) = (self.zooms[index].start, self.zooms[index].end);
            let mut tracks: Vec<u64> = self
                .zooms
                .iter()
                .filter(|z| z.id != id)
                .map(|z| z.track())
                .collect();
            tracks.dedup();
            let free = tracks.into_iter().find(|t| {
                self.zooms
                    .iter()
                    .filter(|z| z.id != id && z.track() == *t)
                    .all(|z| z.end <= start || end <= z.start)
            });
            let track = free.unwrap_or_else(|| {
                self.zooms
                    .iter()
                    .flat_map(|z| [z.id, z.lane])
                    .max()
                    .unwrap_or(0)
                    + 1
            });
            self.zooms[index].lane = track;
        }
    }
    /// Rebuilds following zooms from the project's recorded cursor (BA4).
    pub fn refollow_zooms(&mut self, size: [u32; 2]) {
        if self.zooms.iter().all(|z| z.follow.is_none()) {
            return;
        }
        let Some((start, path)) = self.objects.values().find_map(|t| {
            t.motion_path
                .as_ref()
                .filter(|p| p.purpose == super::motion_path::Purpose::Cursor)
                .map(|p| (t.start, p.clone()))
        }) else {
            return;
        };
        super::instruction::refollow(&mut self.zooms, &path, start, size);
    }
    pub fn prune_automatic_tracks(&mut self) {
        let empty: Vec<_> = self
            .automatic_tracks
            .iter()
            .copied()
            .filter(|id| !self.objects.values().any(|t| t.track == *id))
            .collect();
        self.tracks.retain(|id| !empty.contains(id));
        self.automatic_tracks.retain(|id| self.tracks.contains(id));
        self.layer_order
            .retain(|l| !matches!(l, super::links::Layer::Annotation(id) if empty.contains(id)));
        self.track_settings.retain(
            |s| !matches!(s.layer, super::links::Layer::Annotation(id) if empty.contains(&id)),
        );
        if self.annotation_track.is_some_and(|id| empty.contains(&id)) {
            self.annotation_track = None;
        }
    }
    pub fn sync(&mut self, objects: &[Annotation]) {
        self.track_effects
            .retain(|id, _| self.media_tracks.contains(id));
        self.clip_effects
            .retain(|id, _| self.segments.iter().any(|s| s.id == *id));
        self.objects
            .retain(|id, _| objects.iter().any(|a| a.id == *id));
        for a in objects {
            if !self.objects.contains_key(&a.id) {
                let start = self.cursor;
                self.place(
                    a.id,
                    Timing::new(
                        0,
                        start,
                        if start < self.duration() {
                            (start + 4000).min(self.duration())
                        } else {
                            start.saturating_add(4000)
                        },
                    ),
                );
            }
            let t = self.objects.get_mut(&a.id).unwrap();
            if !t.entrance.supports(a) {
                t.entrance = Animation::None;
            }
            if !t.exit.supports(a) {
                t.exit = Animation::None;
            }
            t.normalize_stacks(a);
        }
        self.prune_automatic_tracks();
    }
    pub fn validate(&self, doc: &Document) -> Result<()> {
        // The failing rule is named so a report can be traced (BH5).
        let invalid = |why: &str| {
            Error::Invalid(format!(
                "Invalid video timeline ({why}). Use a local video up to one hour, valid object ranges, and even dimensions up to 1920 × 1080 (landscape or portrait)."
            ))
        };
        if self.gif_ranges.len() > 64 {
            return Err(invalid("too many GIF ranges"));
        }
        let mut gif_ids = std::collections::BTreeSet::new();
        for range in &self.gif_ranges {
            range.validate()?;
            if !gif_ids.insert(range.id) {
                return Err(invalid("duplicate GIF range"));
            }
        }
        let mut zoom_ids = std::collections::BTreeSet::new();
        if self.zooms.len() > 64 {
            return Err(invalid("too many zooms"));
        }
        for z in &self.zooms {
            z.validate()?;
            if self
                .zooms
                .iter()
                .filter(|other| other.active(z.start))
                .map(|z| z.scale)
                .product::<f32>()
                > 32.
            {
                return Err(Error::Invalid("Overlapping Zoom effects cannot exceed 32× combined magnification. Lower a Zoom value or separate their timing.".into()));
            }
            if !zoom_ids.insert(z.id) {
                return Err(invalid("duplicate zoom"));
            }
        }
        if self.source.is_empty()
            || self.source.len() > 32768
            || self.source.contains('\0')
            || self.source.contains("://")
            || !local_media_path(&self.source)
            || self.source_duration == 0
            || self.source_duration > 3_600_000
            || self.source_size.iter().any(|v| *v < 1 || *v > 8192)
            || ![15, 30, 60].contains(&self.fps)
            || doc.width < 64
            || doc.height < 64
            || doc.width.max(doc.height) > 1920
            || doc.width.min(doc.height) > 1080
            || !doc.width.is_multiple_of(2)
            || !doc.height.is_multiple_of(2)
            || doc.guide.is_some()
            || self.segments.len() > 1000
            || self.tracks.len() > 1000
            || self
                .automatic_tracks
                .iter()
                .any(|id| !self.tracks.contains(id))
            || self
                .automatic_tracks
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.automatic_tracks.len()
            || self.media_tracks.is_empty()
            || self.media_tracks.len() > 64
        {
            return Err(invalid("source video, size or frame rate"));
        }
        if self
            .media_kinds
            .keys()
            .any(|id| !self.media_tracks.contains(id))
        {
            return Err(invalid("track type"));
        }
        if self.media_levels.iter().any(|(id, gain)| {
            !self.media_tracks.contains(id) || !gain.is_finite() || !(0. ..=2.).contains(gain)
        }) {
            return Err(invalid("track volume"));
        }
        let layers = self.layers();
        let mut settings = std::collections::BTreeSet::new();
        if self
            .track_settings
            .iter()
            .any(|s| !layers.contains(&s.layer) || !settings.insert(s.layer) || s.name.len() > 320)
        {
            return Err(invalid("track settings"));
        }
        let mut order = std::collections::BTreeSet::new();
        if self.layer_order.iter().any(|l| {
            !order.insert(*l)
                || match l {
                    super::links::Layer::Annotation(id) => !self.tracks.contains(id),
                    super::links::Layer::Media(id) => !self.media_tracks.contains(id),
                    super::links::Layer::Zoom(t) => !self.zooms.iter().any(|z| z.track() == *t),
                }
        }) {
            return Err(invalid("layer order"));
        }
        let mut ids = std::collections::BTreeSet::new();
        for (track, chain) in &self.track_effects {
            if !self.media_tracks.contains(track) {
                return Err(invalid("track effects"));
            }
            chain.validate()?;
        }
        for (track, chain) in &self.clip_effects {
            if !self.segments.iter().any(|c| c.id == *track) {
                return Err(invalid("clip effects"));
            }
            chain.validate()?;
        }
        for s in &self.segments {
            s.keyframes.validate_media(s)?;
            if let Some(frame) = &s.frame {
                frame.validate()?;
            }
            if s.link == Some(0) {
                return Err(invalid("clip link"));
            }
            let concurrent: Vec<_> = self.segments.iter().filter(|c| c.active(s.at)).collect();
            if concurrent
                .iter()
                .filter(|c| c.kind() != MediaKind::Audio)
                .count()
                > 8
                || concurrent.iter().map(|c| c.audio.len()).sum::<usize>() > 16
            {
                return Err(Error::Invalid(
                    "Keep at most 8 picture clips and 16 audio sources playing at the same time."
                        .into(),
                ));
            }
            let limit = s
                .source
                .as_ref()
                .map_or(self.source_duration, |a| a.duration);
            if !ids.insert(s.id)
                || !self.media_tracks.contains(&s.track)
                || s.label.len() > 240
                || s.start >= s.end
                || s.end > limit
                || !s.speed.is_finite()
                || !(0.25..=4.).contains(&s.speed)
                || s.duration() == 0
                || s.until() > 14_400_000
                || s.fade_in.saturating_add(s.fade_out) > s.duration()
                || s.audio.len() > 8
                || s.rect.iter().any(|x| !x.is_finite())
                || s.rect[0] < 0.
                || s.rect[1] < 0.
                || s.rect[2] <= 0.
                || s.rect[3] <= 0.
                || s.rect[0] + s.rect[2] > 1.001
                || s.rect[1] + s.rect[3] > 1.001
            {
                return Err(invalid("clip range or placement"));
            }
            if let Some(a) = &s.source
                && (a.duration == 0
                    || a.duration
                        > if a.kind == MediaKind::Image {
                            14_400_000
                        } else {
                            3_600_000
                        }
                    || a.size.contains(&0)
                    || a.size.iter().any(|n| *n > 8192))
            {
                return Err(invalid("clip media"));
            }
            if self.segments.iter().any(|o| {
                o.id != s.id
                    && o.track == s.track
                    && o.at < s.until()
                    && s.at < o.until()
                    && (o.kind() == MediaKind::Audio) != (s.kind() == MediaKind::Audio)
            }) {
                return Err(Error::Invalid(
                    "Move audio and picture clips onto separate media tracks.".into(),
                ));
            }
            if self
                .segments
                .iter()
                .filter(|o| o.track == s.track && o.active(s.at))
                .count()
                > 2
            {
                return Err(Error::Invalid("Only two clips may overlap on a media track. Move the third clip to another track.".into()));
            }
            for a in &s.audio {
                if a.label.len() > 120
                    || !a.volume.is_finite()
                    || !(0. ..=2.).contains(&a.volume)
                    || a.fade_in > s.duration()
                    || a.fade_out > s.duration()
                {
                    return Err(invalid("clip audio"));
                }
            }
        }
        if self
            .media_tracks
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != self.media_tracks.len()
        {
            return Err(invalid("duplicate media track"));
        }
        for path in self.media_paths() {
            if path.is_empty()
                || path.len() > 32768
                || path.contains('\0')
                || path.contains("://")
                || !local_media_path(path)
            {
                return Err(invalid("media path"));
            }
        }
        let duration = self.duration();
        if duration > 14_400_000 {
            return Err(invalid("project longer than four hours"));
        }
        let [x, y, w, h] = self.crop;
        if w == 0
            || h == 0
            || x as u64 + w as u64 > self.source_size[0] as u64
            || y as u64 + h as u64 > self.source_size[1] as u64
        {
            return Err(invalid("crop"));
        }
        let unique: std::collections::BTreeSet<_> = self.tracks.iter().collect();
        if unique.len() != self.tracks.len() {
            return Err(invalid("duplicate track"));
        }
        if self.objects.len() != doc.annotations.len() {
            return Err(invalid("object count"));
        }
        for a in &doc.annotations {
            let t = self
                .objects
                .get(&a.id)
                .ok_or_else(|| invalid("object timing missing"))?;
            t.keyframes.validate()?;
            if let Some(path) = &t.motion_path {
                path.validate()?;
            }
            if t.start >= t.end
                || t.end > duration
                || !self.tracks.contains(&t.track)
                || !self.free(t.track, a.id, t.start, t.end)
                || t.direction > 3
                || t.animation_span(false)
                    .saturating_add(t.animation_span(true))
                    > t.end - t.start
                || (t.entrance != Animation::None && !t.entrance.supports(a))
                || (t.exit != Animation::None && !t.exit.supports(a))
                || [(t.entrance, &t.entrance_stack), (t.exit, &t.exit_stack)]
                    .into_iter()
                    .any(|(primary, stack)| {
                        stack.len() > 4
                            || stack.iter().enumerate().any(|(i, effect)| {
                                !effect.supports(a)
                                    || !Timing::stack_compatible(primary, &stack[..i], *effect)
                            })
                    })
            {
                return Err(invalid("object timing"));
            }
        }
        Ok(())
    }
    pub fn split(&mut self, time: u64) -> Result<()> {
        let i = self
            .segments
            .iter()
            .position(|s| s.at < time && time < s.until())
            .ok_or_else(|| {
                Error::Invalid("Move the playhead inside a clip before splitting.".into())
            })?;
        self.split_clip(i, time)
    }
    pub fn speed(&mut self, index: usize, speed: f64) -> Result<()> {
        if !speed.is_finite() || !(0.25..=4.).contains(&speed) {
            return Err(Error::Invalid("Choose a speed from 0.25× to 4×.".into()));
        }
        let s = self
            .segments
            .get(index)
            .ok_or_else(|| Error::Invalid("Select a clip.".into()))?
            .clone();
        let old = s.duration();
        let mut edited = s.clone();
        edited.speed = speed;
        self.edit_linked(index, edited, false)?;
        let new = self.segments[index].duration();
        let map = |t: u64| {
            if t <= s.at {
                t
            } else if t >= s.at + old {
                t - old + new
            } else {
                s.at + ((t - s.at) as f64 * new as f64 / old as f64).round() as u64
            }
        };
        for other in &mut self.segments {
            if other.id != s.id && other.at >= s.until() {
                other.at = map(other.at);
            }
        }
        for t in self.objects.values_mut() {
            t.keyframes.remap_time(t.start, map, &[s.at, s.at + old]);
            if let Some(path) = &mut t.motion_path {
                std::sync::Arc::make_mut(path).remap_time(t.start, map, &[s.at, s.at + old]);
            }
            t.start = map(t.start);
            t.end = map(t.end).max(t.start + 1);
            t.clamp_animations();
        }
        for z in &mut self.zooms {
            z.start = map(z.start);
            z.end = map(z.end).max(z.start + 1);
            z.entrance_ms = z.entrance_ms.min(z.end - z.start);
            z.exit_ms = z.exit_ms.min(z.end - z.start - z.entrance_ms);
        }
        self.cursor = map(self.cursor).min(self.duration() - 1);
        Ok(())
    }
    /// Ripple removal trims/splits media and keeps every surviving source interval and linked audio.
    pub fn remove_range(&mut self, start: u64, end: u64) -> Result<Vec<u64>> {
        if start >= end || end > self.duration() || end - start == self.duration() {
            return Err(Error::Invalid(
                "Keep at least one frame of the project.".into(),
            ));
        }
        let map = |t: u64| {
            if t <= start {
                t
            } else if t >= end {
                t - (end - start)
            } else {
                start
            }
        };
        let mut clips = vec![];
        let mut copied_effects = vec![];
        let mut right_links = BTreeMap::new();
        let mut next_link = self.next_link();
        let mut id = self.next_media_id();
        for s in &self.segments {
            s.keyframes.validate_media(s)?;
            for (part, (lo, hi)) in [(s.at, start.min(s.until())), (end.max(s.at), s.until())]
                .into_iter()
                .enumerate()
            {
                if hi <= lo {
                    continue;
                }
                let mut c = s.clone();
                c.at = map(lo);
                c.keyframes.trim_start((lo - s.at) as i64);
                c.start = s.start + ((lo - s.at) as f64 * s.speed).round() as u64;
                c.end = (s.start + ((hi - s.at) as f64 * s.speed).round() as u64).min(s.end);
                if part == 1 && start > s.at {
                    c.link = s.link.map(|link| {
                        *right_links.entry(link).or_insert_with(|| {
                            let n = next_link;
                            next_link += 1;
                            n
                        })
                    });
                    c.id = id;
                    copied_effects.push((s.id, c.id));
                    id += 1;
                }
                if c.start < c.end {
                    c.clamp_fades();
                    let duration = c.duration();
                    for a in &mut c.audio {
                        a.fade_in = a.fade_in.min(duration);
                        a.fade_out = a.fade_out.min(duration);
                    }
                    clips.push(c);
                }
            }
        }
        self.segments = clips;
        for (from, to) in copied_effects {
            self.copy_clip_effects(from, to);
        }
        self.clip_effects
            .retain(|id, _| self.segments.iter().any(|c| c.id == *id));
        let mut removed = vec![];
        for (id, t) in &mut self.objects {
            t.keyframes.remap_time(t.start, map, &[start, end]);
            if let Some(path) = &mut t.motion_path {
                std::sync::Arc::make_mut(path).remap_time(
                    t.start,
                    map,
                    &[start.saturating_sub(1), end],
                );
            }
            t.start = map(t.start);
            t.end = map(t.end);
            t.clamp_animations();
            if t.start == t.end {
                removed.push(*id);
            }
        }
        for id in &removed {
            self.objects.remove(id);
        }
        for z in &mut self.zooms {
            z.start = map(z.start);
            z.end = map(z.end);
            z.entrance_ms = z.entrance_ms.min(z.end - z.start);
            z.exit_ms = z.exit_ms.min(z.end - z.start - z.entrance_ms);
        }
        self.zooms.retain(|z| z.end > z.start);
        self.layer_order.retain(|layer| {
            !matches!(layer, super::links::Layer::Zoom(t) if !self.zooms.iter().any(|z| z.track() == *t))
        });
        self.cursor = map(self.cursor).min(self.duration().saturating_sub(1));
        Ok(removed)
    }
}

pub fn animated(a: &Annotation, t: &Timing, time: u64) -> Option<Annotation> {
    let time = t.drawn_at(time);
    if !t.active(time) {
        return None;
    }
    let keyed = (!t.keyframes.is_empty()).then(|| t.keyframes.annotation(a, time - t.start));
    let a = keyed.as_ref().unwrap_or(a);
    let sampled = t.motion_path.as_ref().map(|p| p.apply(a, time - t.start));
    let a = match &sampled {
        Some(Some(a)) => a,
        Some(None) => return None,
        None => a,
    };
    let age = time - t.start;
    let left = t.end - time;
    let (anim, progress, exit) = if t.entrance_ms > 0 && age < t.entrance_ms {
        (t.entrance, age as f32 / t.entrance_ms as f32, false)
    } else if t.exit_ms > 0 && left < t.exit_ms {
        (t.exit, left as f32 / t.exit_ms as f32, true)
    } else {
        return Some(a.clone());
    };
    animated_effect(a, t, anim, progress, exit)
}
fn animated_effect(
    a: &Annotation,
    t: &Timing,
    anim: Animation,
    mut p: f32,
    exit: bool,
) -> Option<Annotation> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut a = a.clone();
    if matches!(a.kind, Kind::Redact | Kind::Blur | Kind::Pixelate) {
        return Some(a);
    }
    if t.smooth && anim != Animation::Typewriter {
        p = p * p * (3. - 2. * p);
    }
    match anim {
        Animation::Fade | Animation::Float => {
            apply_opacity(&mut a, p);
            if anim == Animation::Float {
                let d = 40. * (1. - p);
                let offset = match t.direction {
                    1 => [d, 0.],
                    2 => [0., -d],
                    3 => [0., d],
                    _ => [-d, 0.],
                };
                a = crate::canvas_expansion::translated(&[a], offset)
                    .ok()?
                    .remove(0);
            }
        }
        Animation::Typewriter => {
            if let Some(text) = a.text.as_mut() {
                let n = text.content.graphemes(true).count();
                text.content = text
                    .content
                    .graphemes(true)
                    .take((n as f32 * p).ceil() as usize)
                    .collect();
                if let Some(r) = text.rich.as_mut() {
                    let n = text.content.chars().count();
                    r.runs.retain_mut(|r| {
                        r.end = r.end.min(n);
                        r.start < r.end
                    });
                    r.paragraphs.retain(|p| p.start <= n);
                }
            }
        }
        Animation::Scale => {
            let center = [(a.start[0] + a.end[0]) / 2., (a.start[1] + a.end[1]) / 2.];
            let p = p.max(0.001);
            for point in [&mut a.start, &mut a.end]
                .into_iter()
                .chain(a.points.iter_mut())
            {
                for axis in 0..2 {
                    point[axis] = center[axis] + (point[axis] - center[axis]) * p
                }
            }
            a.corner_radius *= p;
        }
        Animation::Path => {
            if !matches!(a.kind, Kind::Line | Kind::Arrow) {
                let closed = a.is_shape() || a.kind == Kind::Callout || a.highlighter_rect;
                let mut points = if a.kind == Kind::Callout {
                    a.callout_polygon()
                } else if closed {
                    a.frame_boundary()
                } else {
                    a.points.clone()
                };
                if closed && !points.is_empty() {
                    if t.path_reverse {
                        points[1..].reverse();
                    }
                    points.push(points[0]);
                }
                if exit && t.retract_from_start {
                    points.reverse();
                }
                let rotation = a.rotation();
                for point in &mut points {
                    *point = rotation.point(*point);
                }
                let length: f32 = points
                    .windows(2)
                    .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
                    .sum();
                let mut left = length * p;
                let mut partial = points.first().copied().into_iter().collect::<Vec<_>>();
                for edge in points.windows(2) {
                    let n = (edge[1][0] - edge[0][0]).hypot(edge[1][1] - edge[0][1]);
                    if n > left {
                        partial.push([
                            edge[0][0] + (edge[1][0] - edge[0][0]) * left / n,
                            edge[0][1] + (edge[1][1] - edge[0][1]) * left / n,
                        ]);
                        break;
                    }
                    partial.push(edge[1]);
                    left -= n;
                }
                if let Some(c) = a.callout.as_ref() {
                    a.color = c.outline;
                }
                a.kind = Kind::Pen;
                a.points = partial;
                a.fill = None;
                a.text = None;
                a.image = None;
                a.callout = None;
                a.highlighter_rect = false;
                a.rotation_degrees = 0.;
                a.mirror_x = false;
                return Some(a);
            }
            let mut style = a
                .line_style
                .unwrap_or_else(|| crate::line_style::Style::for_kind(a.kind));
            style.reveal = Some(p as f64);
            style.reveal_from_end = exit && t.retract_from_start;
            a.line_style = Some(style);
        }
        Animation::None => (),
    }
    Some(a)
}
pub fn objects_at(doc: &Document, time: u64) -> Vec<Annotation> {
    let Some(v) = &doc.video else {
        return doc.annotations.clone();
    };
    let mut out: Vec<_> = doc
        .annotations
        .iter()
        .filter(|a| doc.object_visible(a.id))
        .flat_map(|a| {
            v.objects
                .get(&a.id)
                .map(|t| animated_parts(a, t, time))
                .unwrap_or_default()
                .into_iter()
                .map(|a| {
                    (
                        v.tracks
                            .iter()
                            .position(|id| Some(id) == v.objects.get(&a.id).map(|t| &t.track))
                            .unwrap_or(0),
                        a,
                    )
                })
        })
        .collect();
    out.sort_by_key(|(track, _)| std::cmp::Reverse(*track));
    out.into_iter().map(|(_, a)| a).collect()
}

/// Compose independent body, outline and text animation channels. The original
/// annotation stays editable; these transient parts are used only for rendering.
pub fn animated_parts(a: &Annotation, t: &Timing, time: u64) -> Vec<Annotation> {
    let time = t.drawn_at(time);
    if !t.active(time) {
        return vec![];
    }
    let keyed = (!t.keyframes.is_empty()).then(|| t.keyframes.annotation(a, time - t.start));
    let a = keyed.as_ref().unwrap_or(a);
    let sampled = t.motion_path.as_ref().map(|p| p.apply(a, time - t.start));
    let a = match &sampled {
        Some(Some(a)) => a,
        Some(None) => return vec![],
        None => a,
    };
    let exit = if time - t.start < t.animation_span(false) {
        false
    } else if t.end - time < t.animation_span(true) {
        true
    } else {
        return vec![a.clone()];
    };
    let (primary, stack) = if exit {
        (t.exit, &t.exit_stack)
    } else {
        (t.entrance, &t.entrance_stack)
    };
    let effects: Vec<_> = [primary].into_iter().chain(stack.iter().copied()).collect();
    let mut part = a.clone();
    let progress = |effect| {
        let ms = t.effect_ms(effect, exit);
        if ms == 0 {
            1.
        } else {
            (if exit { t.end - time } else { time - t.start }) as f32 / ms as f32
        }
        .clamp(0., 1.)
    };
    // Spatial transforms and complete fades affect every component together.
    for effect in [
        Animation::Scale,
        Animation::Float,
        Animation::Fade,
        Animation::Typewriter,
    ] {
        if effects.contains(&effect)
            && let Some(next) = animated_effect(&part, t, effect, progress(effect), exit)
        {
            part = next;
        }
    }
    if effects.contains(&Animation::Path) {
        let outline = animated_effect(&part, t, Animation::Path, progress(Animation::Path), exit);
        if matches!(
            a.kind,
            Kind::Line | Kind::Arrow | Kind::Pen | Kind::Highlighter
        ) {
            return outline.into_iter().collect();
        }
        part.width = 0;
        part.step_outline = None;
        if let Some(callout) = part.callout.as_mut() {
            callout.outline[3] = 0;
        }
        let mut parts = vec![part];
        if let Some(mut outline) = outline {
            outline.shadow = None;
            parts.push(outline);
        }
        parts
    } else {
        vec![part]
    }
}
pub fn composite(doc: &Document, frame: &image::RgbaImage, time: u64) -> Result<image::RgbaImage> {
    if frame.dimensions() != (doc.width, doc.height) {
        return Err(Error::Invalid("Video composite dimensions changed.".into()));
    }
    let mut output = frame.clone();
    let objects: Vec<_> = objects_at(doc, time).into_iter().collect();
    let backdrop = crate::focus::Backdrop::new(&objects);
    for a in &objects {
        if !backdrop.render(&mut output, a) {
            crate::render_annotation(&mut output, a);
        }
    }
    Ok(output)
}

/// Accept ordinary local paths and Windows extended drive paths, not network/device namespaces.
pub fn local_media_path(path: &str) -> bool {
    if let Some(drive) = path.strip_prefix(r"\\?\") {
        let b = drive.as_bytes();
        return b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
    }
    !path.starts_with(r"\\")
}

pub(crate) fn apply_opacity(a: &mut Annotation, p: f32) {
    if matches!(a.kind, Kind::Redact | Kind::Blur | Kind::Pixelate) {
        return;
    }
    let alpha = |c: &mut [u8; 4]| c[3] = (c[3] as f32 * p.clamp(0., 1.)).round() as u8;
    if let Some(focus) = a.focus.as_mut() {
        focus.opacity *= p;
    }
    alpha(&mut a.color);
    if let Some(c) = a.fill.as_mut() {
        alpha(c)
    }
    if let Some(s) = a.text.as_mut()
        && let Some(c) = s.color.as_mut()
    {
        alpha(c)
    }
    if let Some(s) = a.shadow.as_mut() {
        alpha(&mut s.color)
    }
    if let Some(c) = a.callout.as_mut() {
        alpha(&mut c.fill);
        alpha(&mut c.outline);
    }
    if let Some(text) = a.text.as_mut()
        && let Some(outline) = text.outline.as_mut()
    {
        alpha(&mut outline.color);
    }
    if let Some(text) = a.text.as_mut()
        && let Some(typography) = text.typography.as_mut()
        && let Some(highlight) = typography.highlight.as_mut()
    {
        alpha(highlight);
    }
    if let Some(outline) = a.step_outline.as_mut() {
        alpha(&mut outline.color);
    }
    if let Some(text) = a.text.as_mut()
        && let Some(rich) = text.rich.as_mut()
    {
        for run in &mut rich.runs {
            if let Some(color) = run.format.color.as_mut() {
                alpha(color)
            }
            if let Some(outline) = run.format.outline.as_mut() {
                alpha(&mut outline.color);
            }
            if let Some(highlight) = run.format.typography.highlight.as_mut() {
                alpha(highlight);
            }
        }
    }
    if let Some(bitmap) = a.image.as_mut() {
        *bitmap = bitmap
            .with_frame_opacity((bitmap.frame_opacity() as f32 * p).round() as u8)
            .into();
    }
}
