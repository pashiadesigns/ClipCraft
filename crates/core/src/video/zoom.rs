//! A view transform affecting only layers beneath its timeline position.
use crate::{Annotation, Error, Result};
use serde::{Deserialize, Serialize};
/// Pixel-space affine view for a layer. The inverse is used for editing the
/// original objects while their zoomed result is visible.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub offset: [f32; 2],
    pub scale: f32,
}
impl View {
    pub fn point(self, p: [f32; 2]) -> [f32; 2] {
        [
            p[0] * self.scale + self.offset[0],
            p[1] * self.scale + self.offset[1],
        ]
    }
    pub fn inverse(self, p: [f32; 2]) -> [f32; 2] {
        [
            (p[0] - self.offset[0]) / self.scale,
            (p[1] - self.offset[1]) / self.scale,
        ]
    }
}
impl super::editor::Project {
    pub fn layer_view(&self, layer: super::links::Layer, size: [u32; 2], time: u64) -> View {
        let mut view = View {
            offset: [0.; 2],
            scale: 1.,
        };
        for layer in self
            .layers()
            .into_iter()
            .take(self.layer_index(layer))
            .rev()
        {
            if let super::links::Layer::Zoom(track) = layer
                && !self.layer_hidden(layer)
                && let Some([x, y, side]) = self
                    .zooms
                    .iter()
                    .filter(|z| z.track() == track)
                    .find_map(|z| z.view(time))
            {
                view.offset = [
                    (view.offset[0] - x * size[0] as f32) / side,
                    (view.offset[1] - y * size[1] as f32) / side,
                ];
                view.scale /= side;
            }
        }
        view
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motion {
    Snap,
    Linear,
    Smooth,
    Accelerate,
    Decelerate,
}
impl Motion {
    pub const ALL: [Self; 5] = [
        Self::Snap,
        Self::Linear,
        Self::Smooth,
        Self::Accelerate,
        Self::Decelerate,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Snap => "Instant",
            Self::Linear => "Constant speed",
            Self::Smooth => "Smooth ease",
            Self::Accelerate => "Accelerate",
            Self::Decelerate => "Soft landing",
        }
    }
    fn value(self, t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        match self {
            Self::Snap => 1.,
            Self::Linear => t,
            Self::Smooth => t * t * (3. - 2. * t),
            Self::Accelerate => t * t * t,
            Self::Decelerate => 1. - (1. - t).powi(3),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zoom {
    pub id: u64,
    pub start: u64,
    pub end: u64,
    /// Normalized center and view width; height shares the same fraction, preserving project aspect.
    pub center: [f32; 2],
    pub scale: f32,
    pub entrance: Motion,
    pub exit: Motion,
    pub entrance_ms: u64,
    pub exit_ms: u64,
    pub return_to_full: bool,
    pub pinned: bool,
    /// When set, the entrance pans from this center at full magnification
    /// instead of zooming in from the whole frame (continuing a previous zoom).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan_from: Option<[f32; 2]>,
    /// Optional followed centers (ms after `start`, normalized point), sampled
    /// from the recorded cursor so the view tracks it between clicks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub track: Vec<(u64, [f32; 2])>,
    /// How `track` follows the recorded cursor (BA4); `None` for zooms that never followed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<Follow>,
    /// Shared zoom track (BB6). Zooms on one track never overlap in time. 0 means
    /// the zoom has a track of its own, keyed by its id.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub lane: u64,
}
fn is_zero(v: &u64) -> bool {
    *v == 0
}
/// Which cursor movement a zoom follows between nearby clicks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FollowMode {
    Off,
    /// The cursor as it appears in the video, including path reduction.
    #[default]
    Shown,
    /// The cursor exactly as it moved while recording.
    Recorded,
}
impl FollowMode {
    pub const ALL: [Self; 3] = [Self::Off, Self::Shown, Self::Recorded];
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Shown => "As shown",
            Self::Recorded => "As recorded",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            Self::Off => "The zoom stays where you placed it.",
            Self::Shown => {
                "Follows the cursor as it appears in the video, including path reduction."
            }
            Self::Recorded => "Follows the cursor exactly as you moved it while recording.",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Follow {
    pub mode: FollowMode,
    /// Pointer tip relative to the cursor image's top-left, in source pixels.
    pub tip: [f32; 2],
}
impl Zoom {
    pub fn new(id: u64, start: u64, end: u64) -> Self {
        Self {
            id,
            start,
            end,
            center: [0.5, 0.5],
            scale: 2.,
            entrance: Motion::Smooth,
            exit: Motion::Smooth,
            entrance_ms: 500,
            exit_ms: 500,
            return_to_full: true,
            pinned: true,
            pan_from: None,
            track: vec![],
            follow: None,
            lane: 0,
        }
    }
    /// The zoom track this zoom sits on (see [`Zoom::lane`]).
    pub fn track(&self) -> u64 {
        if self.lane == 0 { self.id } else { self.lane }
    }
    pub fn active(&self, t: u64) -> bool {
        self.start <= t && t < self.end
    }
    /// Gives this zoom what an edit changed on another selected zoom. Where each
    /// zoom looks stays its own; moving keeps their spacing.
    pub fn copy_change(&mut self, before: &Zoom, after: &Zoom) {
        let start = self
            .start
            .saturating_add_signed(after.start as i64 - before.start as i64);
        let end = self
            .end
            .saturating_add_signed(after.end as i64 - before.end as i64);
        self.start = start;
        self.end = end.max(start + 1);
        macro_rules! copy {
            ($($field:ident),+) => {$(
                if after.$field != before.$field {
                    self.$field = after.$field;
                }
            )+};
        }
        copy!(
            scale,
            entrance,
            exit,
            entrance_ms,
            exit_ms,
            return_to_full,
            pinned
        );
        if after.follow != before.follow {
            self.follow = after.follow;
        }
        let span = self.end - self.start;
        self.entrance_ms = self.entrance_ms.min(span);
        self.exit_ms = self.exit_ms.min(span - self.entrance_ms);
    }
    pub fn validate(&self) -> Result<()> {
        if self.id == 0
            || self.start >= self.end
            || self.end > 14_400_000
            || !self.scale.is_finite()
            || !(1. ..=10.).contains(&self.scale)
            || self
                .center
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || self.entrance_ms.saturating_add(self.exit_ms) > self.end - self.start
            || self
                .pan_from
                .is_some_and(|c| c.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v)))
            || self.track.len() > 4000
            || self.track.windows(2).any(|w| w[0].0 >= w[1].0)
            || self.track.iter().any(|(at, c)| {
                *at > self.end - self.start
                    || c.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            })
        {
            return Err(Error::Invalid("Zoom needs a valid range, 1–10× magnification and transitions within its duration.".into()));
        }
        Ok(())
    }
    /// The followed center at `time`, or the fixed center when there is no track.
    pub fn target(&self, time: u64) -> [f32; 2] {
        let age = time.saturating_sub(self.start);
        let Some(first) = self.track.first() else {
            return self.center;
        };
        if age <= first.0 {
            return first.1;
        }
        let i = self.track.partition_point(|(at, _)| *at <= age);
        match (self.track.get(i - 1), self.track.get(i)) {
            (Some(a), Some(b)) => {
                let t = (age - a.0) as f32 / (b.0 - a.0) as f32;
                [
                    a.1[0] + (b.1[0] - a.1[0]) * t,
                    a.1[1] + (b.1[1] - a.1[1]) * t,
                ]
            }
            (Some(a), None) => a.1,
            _ => self.center,
        }
    }
    pub fn view(&self, time: u64) -> Option<[f32; 3]> {
        if !self.active(time) {
            return None;
        }
        let center = self.target(time);
        let mut weight = if self.entrance_ms == 0 {
            1.
        } else {
            self.entrance
                .value((time - self.start) as f32 / self.entrance_ms as f32)
        };
        if self.return_to_full && self.exit_ms > 0 && time > self.end - self.exit_ms {
            weight = weight.min(
                1. - self
                    .exit
                    .value((time - (self.end - self.exit_ms)) as f32 / self.exit_ms as f32),
            );
        }
        let (side, cx, cy) = if let Some(from) = self.pan_from {
            // Hold magnification and slide from the previous target; the exit
            // (if any) still returns to the whole frame.
            let pan = if self.entrance_ms == 0 {
                1.
            } else {
                self.entrance
                    .value((time - self.start) as f32 / self.entrance_ms as f32)
            };
            let out = if self.return_to_full && self.exit_ms > 0 && time > self.end - self.exit_ms {
                self.exit
                    .value((time - (self.end - self.exit_ms)) as f32 / self.exit_ms as f32)
            } else {
                0.
            };
            let keep = 1. - out;
            let x = from[0] + (center[0] - from[0]) * pan;
            let y = from[1] + (center[1] - from[1]) * pan;
            (
                1. + (1. / self.scale - 1.) * keep,
                0.5 + (x - 0.5) * keep,
                0.5 + (y - 0.5) * keep,
            )
        } else {
            (
                1. + (1. / self.scale - 1.) * weight,
                0.5 + (center[0] - 0.5) * weight,
                0.5 + (center[1] - 0.5) * weight,
            )
        };
        Some([
            (cx - side / 2.).clamp(0., 1. - side),
            (cy - side / 2.).clamp(0., 1. - side),
            side,
        ])
    }
    pub fn transform(&self, objects: &mut [Annotation], size: [u32; 2], time: u64) {
        let Some([x, y, side]) = self.view(time) else {
            return;
        };
        let scale = 1. / side;
        let point = |p: &mut [f32; 2]| {
            p[0] = (p[0] - x * size[0] as f32) * scale;
            p[1] = (p[1] - y * size[1] as f32) * scale;
        };
        for a in objects {
            point(&mut a.start);
            point(&mut a.end);
            for p in &mut a.points {
                point(p);
            }
            a.width = (a.width as f32 * scale).round().max(1.) as u32;
            a.corner_radius *= scale;
            if let Some(t) = &mut a.text {
                t.size_points *= scale;
                t.padding *= scale;
                if let Some(ty) = &mut t.typography {
                    ty.spacing *= scale;
                }
                crate::text::rich::scale_runs(t, scale);
                if let Some(o) = &mut t.outline {
                    o.width *= scale;
                }
            }
            if let Some(c) = &mut a.callout {
                point(&mut c.pointer);
                c.padding *= scale;
            }
            if let Some(o) = &mut a.step_outline {
                o.width *= scale;
            }
            if let Some(s) = &mut a.shadow {
                s.distance *= scale;
                s.blur *= scale;
            }
        }
    }
}
