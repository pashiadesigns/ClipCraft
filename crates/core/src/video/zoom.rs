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
            if let super::links::Layer::Zoom(id) = layer
                && !self.layer_hidden(layer)
                && let Some([x, y, side]) = self
                    .zooms
                    .iter()
                    .find(|z| z.id == id)
                    .and_then(|z| z.view(time))
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
        }
    }
    pub fn active(&self, t: u64) -> bool {
        self.start <= t && t < self.end
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
        {
            return Err(Error::Invalid("Zoom needs a valid range, 1–10× magnification and transitions within its duration.".into()));
        }
        Ok(())
    }
    pub fn view(&self, time: u64) -> Option<[f32; 3]> {
        if !self.active(time) {
            return None;
        }
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
        let side = 1. + (1. / self.scale - 1.) * weight;
        let cx = 0.5 + (self.center[0] - 0.5) * weight;
        let cy = 0.5 + (self.center[1] - 0.5) * weight;
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
