//! Nondestructive normalized source window; original screenshot bytes stay shared.
use crate::{Annotation, Error, Kind, Result};
use serde::{Deserialize, Serialize};

impl crate::Document {
    /// Older guides have no stored focus; recover it from the editable click
    /// highlight in the same section without modifying the saved document.
    pub fn image_focus(&self, a: &Annotation) -> Option<[f32; 2]> {
        let bitmap = a.image.as_ref()?;
        if let Some(focus) = bitmap.focus() {
            return Some(focus);
        }
        let section = self
            .guide
            .as_ref()?
            .sections
            .iter()
            .find(|s| s.objects.contains(&a.id))?;
        let circle = section
            .objects
            .iter()
            .filter_map(|id| self.annotations.iter().find(|a| a.id == *id))
            .find(|a| a.kind == Kind::Ellipse && a.fill == Some([255, 235, 0, 65]))?;
        let (lo, hi) = circle.bounds();
        let center = a
            .rotation()
            .inverse_point([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]);
        let (min, max) = a.body_bounds();
        let v = bitmap.view().rect;
        Some(
            [0, 1].map(|i| {
                (v[i] + (center[i] - min[i]) / (max[i] - min[i]) * v[i + 2]).clamp(0., 1.)
            }),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageView {
    pub rect: [f32; 4],
}
impl Default for ImageView {
    fn default() -> Self {
        Self {
            rect: [0., 0., 1., 1.],
        }
    }
}
impl ImageView {
    pub fn validate(self) -> Result<()> {
        let [x, y, w, h] = self.rect;
        if !self.rect.into_iter().all(f32::is_finite)
            || x < 0.
            || y < 0.
            || w < 1. / 1024.
            || h < 1. / 1024.
            || w > 1.
            || h > 1.
            || x + w > 1.000001
            || y + h > 1.000001
        {
            return Err(Error::Invalid(
                "Image crop must stay inside the original screenshot".into(),
            ));
        }
        Ok(())
    }
    pub fn zoomed(self, factor: f32) -> Result<Self> {
        self.zoomed_about(factor, None)
    }
    pub fn zoomed_about(self, factor: f32, focus: Option<[f32; 2]>) -> Result<Self> {
        self.validate()?;
        if !factor.is_finite() || factor <= 0. {
            return Err(Error::Invalid("Invalid image zoom".into()));
        }
        let [x, y, w, h] = self.rect;
        // Preserve the crop aspect ratio even when either source edge is reached.
        let factor = factor.clamp(w.max(h), (w.min(h) * 1024.).max(w.max(h)));
        let nw = (w / factor).min(1.);
        let nh = (h / factor).min(1.);
        let focus = focus.unwrap_or([x + w * 0.5, y + h * 0.5]);
        if !focus
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            return Err(Error::Invalid("Invalid zoom focus".into()));
        }
        let relative = [
            ((focus[0] - x) / w).clamp(0., 1.),
            ((focus[1] - y) / h).clamp(0., 1.),
        ];
        Ok(Self {
            rect: [
                (focus[0] - nw * relative[0]).clamp(0., 1. - nw),
                (focus[1] - nh * relative[1]).clamp(0., 1. - nh),
                nw,
                nh,
            ],
        })
    }
    pub fn panned(self, delta: [f32; 2]) -> Result<Self> {
        self.validate()?;
        if !delta.into_iter().all(f32::is_finite) {
            return Err(Error::Invalid("Invalid image pan".into()));
        }
        let [x, y, w, h] = self.rect;
        Ok(Self {
            rect: [
                (x + delta[0]).clamp(0., 1. - w),
                (y + delta[1]).clamp(0., 1. - h),
                w,
                h,
            ],
        })
    }
}
impl Annotation {
    /// Ctrl-drag a frame handle to crop/reveal source pixels without rescaling.
    pub fn crop_image_handle(
        &self,
        handle: crate::selection::Handle,
        point: [f32; 2],
    ) -> Result<Self> {
        use crate::selection::Handle as H;
        if !point.into_iter().all(f32::is_finite)
            || !matches!(
                handle,
                H::Left
                    | H::Right
                    | H::Top
                    | H::Bottom
                    | H::TopLeft
                    | H::TopRight
                    | H::BottomLeft
                    | H::BottomRight
            )
        {
            return Err(Error::Invalid(
                "Invalid image crop handle or position".into(),
            ));
        }
        let bitmap = self
            .image
            .as_ref()
            .filter(|b| !b.is_cursor())
            .ok_or_else(|| Error::Invalid("Select a screenshot to crop".into()))?;
        let v = bitmap.view().rect;
        let (min, max) = self.body_bounds();
        let extent = [max[0] - min[0], max[1] - min[1]];
        let p = self.rotation().inverse_point(point);
        let mut lo = min;
        let mut hi = max;
        let minimum = [
            extent[0] / v[2] / bitmap.size()[0].min(1024) as f32,
            extent[1] / v[3] / bitmap.size()[1].min(1024) as f32,
        ];
        let src_lo = [
            min[0] - v[0] * extent[0] / v[2],
            min[1] - v[1] * extent[1] / v[3],
        ];
        let src_hi = [src_lo[0] + extent[0] / v[2], src_lo[1] + extent[1] / v[3]];
        if matches!(handle, H::Left | H::TopLeft | H::BottomLeft) {
            lo[0] = p[0].clamp(src_lo[0], hi[0] - minimum[0]);
        }
        if matches!(handle, H::Right | H::TopRight | H::BottomRight) {
            hi[0] = p[0].clamp(lo[0] + minimum[0], src_hi[0]);
        }
        if matches!(handle, H::Top | H::TopLeft | H::TopRight) {
            lo[1] = p[1].clamp(src_lo[1], hi[1] - minimum[1]);
        }
        if matches!(handle, H::Bottom | H::BottomLeft | H::BottomRight) {
            hi[1] = p[1].clamp(lo[1] + minimum[1], src_hi[1]);
        }
        let view = ImageView {
            rect: [
                (lo[0] - src_lo[0]) / (src_hi[0] - src_lo[0]),
                (lo[1] - src_lo[1]) / (src_hi[1] - src_lo[1]),
                (hi[0] - lo[0]) / (src_hi[0] - src_lo[0]),
                (hi[1] - lo[1]) / (src_hi[1] - src_lo[1]),
            ],
        };
        let mut result = self.with_image_view(Some(view))?;
        let center = self
            .rotation()
            .point([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5]);
        result.start = [
            center[0] - (hi[0] - lo[0]) * 0.5,
            center[1] - (hi[1] - lo[1]) * 0.5,
        ];
        result.end = [
            center[0] + (hi[0] - lo[0]) * 0.5,
            center[1] + (hi[1] - lo[1]) * 0.5,
        ];
        result.corner_radius = result.corner_radius.min(result.max_corner_radius());
        Ok(result)
    }
    pub fn pan_image_by_drag(&self, from: [f32; 2], to: [f32; 2]) -> Result<Self> {
        let bitmap = self
            .image
            .as_ref()
            .ok_or_else(|| Error::Invalid("Select a screenshot to pan".into()))?;
        let view = bitmap.view();
        let from = self.rotation().inverse_point(from);
        let to = self.rotation().inverse_point(to);
        let (min, max) = self.body_bounds();
        let delta = [0, 1]
            .map(|axis| -(to[axis] - from[axis]) / (max[axis] - min[axis]) * view.rect[axis + 2]);
        self.with_image_view(Some(view.panned(delta)?))
    }
    pub fn with_image_view(&self, view: Option<ImageView>) -> Result<Self> {
        let bitmap = self
            .image
            .as_ref()
            .filter(|_| self.kind == Kind::Image)
            .ok_or_else(|| Error::Invalid("Select an image to frame".into()))?;
        let mut result = self.clone();
        result.image = Some(std::sync::Arc::new(bitmap.with_view(view)?));
        Ok(result)
    }
}

pub(crate) fn round_coverage(local: [f64; 2], extent: [f32; 2], radius: f32, scale: f64) -> f64 {
    if radius <= 0. {
        return 1.;
    }
    let r = f64::from(radius);
    let dx = (r - local[0])
        .max(local[0] - (f64::from(extent[0]) - r))
        .max(0.);
    let dy = (r - local[1])
        .max(local[1] - (f64::from(extent[1]) - r))
        .max(0.);
    (0.5 - (dx.hypot(dy) - r) * scale).clamp(0., 1.)
}

#[cfg(test)]
mod tests;
