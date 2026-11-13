//! Non-destructive object moves with automatic transparent canvas margins. Plans are cheap
//! geometry-only previews; raster padding/PNG encoding occurs only on commit.
use crate::{Annotation, Document, Error, RasterImage, Result, validate_dimensions};
use image::RgbaImage;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Clone)]
pub struct MovePlan {
    pub dimensions: [u32; 2],
    pub offset: [i32; 2],
    base: Option<[u32; 4]>,
    pub annotations: Vec<Annotation>,
    original_dimensions: [u32; 2],
    moved_ids: BTreeSet<u64>,
}

/// Translation without clamping. Only a validated MovePlan may enter a document.
pub fn translated(objects: &[Annotation], delta: [f32; 2]) -> Result<Vec<Annotation>> {
    if !delta.iter().all(|v| v.is_finite()) {
        return Err(Error::Invalid("Invalid movement coordinates".into()));
    }
    let mut moved = objects.to_vec();
    for a in &mut moved {
        for p in std::iter::once(&mut a.start)
            .chain(std::iter::once(&mut a.end))
            .chain(a.points.iter_mut())
            .chain(a.callout.as_mut().map(|c| &mut c.pointer))
        {
            for axis in 0..2 {
                p[axis] += delta[axis];
                if !p[axis].is_finite() {
                    return Err(Error::Invalid("Movement exceeds coordinate limits".into()));
                }
            }
        }
    }
    Ok(moved)
}

impl Document {
    /// Updated objects keep their identities; duplicated objects use fresh IDs.
    /// Validate before any raster allocation or changes to the original document.
    pub fn plan_expanding_move(&self, moved: &[Annotation], duplicate: bool) -> Result<MovePlan> {
        if moved.is_empty() {
            return Err(Error::Invalid("Select an object to move".into()));
        }
        self.plan_object_bounds(moved, duplicate)
    }

    /// Reconcile remaining annotation bounds after edits/deletion, without
    /// touching explicitly sized canvas or guessing provenance in old files.
    pub(crate) fn fit_automatic_canvas(&mut self) -> Result<()> {
        if self.video.is_some() || !self.canvas_behavior.dynamic_expansion {
            return Ok(());
        }
        if self.auto_canvas_base.is_some()
            || self
                .annotations
                .iter()
                .any(|a| a.shadow.is_some() || a.line_style.is_some())
        {
            let plan = self.plan_object_bounds(&[], false)?;
            if plan.dimensions != [self.width, self.height] || plan.offset != [0, 0] {
                *self = plan.apply(self)?;
            }
        }
        Ok(())
    }

    pub(crate) fn plan_object_bounds(
        &self,
        moved: &[Annotation],
        duplicate: bool,
    ) -> Result<MovePlan> {
        self.validate()?;
        let base = self
            .auto_canvas_base
            .unwrap_or([0, 0, self.width, self.height]);
        let mut ids = BTreeSet::new();
        let mut candidate = self.clone();
        let mut min = [base[0] as f32, base[1] as f32];
        let mut max = [(base[0] + base[2]) as f32, (base[1] + base[3]) as f32];
        for a in moved {
            if !ids.insert(a.id) {
                return Err(Error::Invalid("Duplicate move identifier".into()));
            }
            let existing = candidate.annotations.iter_mut().find(|b| b.id == a.id);
            match (duplicate, existing) {
                (false, Some(existing)) => *existing = a.clone(),
                (true, None) => candidate.annotations.push(a.clone()),
                _ => {
                    return Err(Error::Invalid(
                        "Move identifier does not match its operation".into(),
                    ));
                }
            }
            for p in [a.start, a.end]
                .iter()
                .chain(&a.points)
                .chain(a.callout.as_ref().map(|c| &c.pointer))
            {
                if !p.iter().all(|v| v.is_finite()) {
                    return Err(Error::Invalid("Non-finite movement coordinates".into()));
                }
            }
        }
        for a in &candidate.annotations {
            if self.video.is_some()
                || !self.canvas_behavior.dynamic_expansion
                || self.canvas_behavior.held.contains(&a.id)
                || self.guide.as_ref().is_some_and(|g| g.pages.is_some())
            {
                continue;
            }
            let (mut lo, mut hi) = a.bounds();
            if let Some((shadow_lo, shadow_hi)) = crate::shadow::bounds(a) {
                for axis in 0..2 {
                    lo[axis] = lo[axis].min(shadow_lo[axis]);
                    hi[axis] = hi[axis].max(shadow_hi[axis]);
                }
            }
            let reach = match a.kind {
                crate::Kind::Highlighter if a.highlighter_rect => 0.,
                crate::Kind::Arrow => a.width as f32 * 4.0,
                crate::Kind::Line
                | crate::Kind::Pen
                | crate::Kind::Highlighter
                | crate::Kind::Eraser
                | crate::Kind::Rectangle
                | crate::Kind::Ellipse
                | crate::Kind::Polygon
                | crate::Kind::Callout => crate::line_style::stroke_reach(a) + 1.0,
                _ => 0.0,
            };
            for axis in 0..2 {
                // Include stroke coverage on an edge the object crosses. Do not
                // grow unrelated edges simply because an old object touches one.
                let low = base[axis] as f32;
                let limit = (base[axis] + base[axis + 2]) as f32;
                min[axis] =
                    min[axis].min((lo[axis] - if lo[axis] < low { reach } else { 0.0 }).floor());
                max[axis] =
                    max[axis].max((hi[axis] + if hi[axis] > limit { reach } else { 0.0 }).ceil());
            }
        }
        let size = [max[0] - min[0], max[1] - min[1]];
        if size.iter().any(|v| !v.is_finite() || *v > u32::MAX as f32) {
            return Err(Error::Invalid(
                "Canvas expansion exceeds image limits".into(),
            ));
        }
        let dimensions = size.map(|v| v as u32);
        validate_dimensions(dimensions[0], dimensions[1])?;
        let offset = min.map(|v| -(v as i32));
        let shifted_base = [
            (base[0] as i32 + offset[0]) as u32,
            (base[1] as i32 + offset[1]) as u32,
            base[2],
            base[3],
        ];
        let automatic =
            (shifted_base != [0, 0, dimensions[0], dimensions[1]]).then_some(shifted_base);
        candidate.auto_canvas_base = automatic;
        if automatic.is_some() {
            candidate.version = crate::MAX_PROJECT_VERSION;
        }
        candidate.width = dimensions[0];
        candidate.height = dimensions[1];
        candidate.annotations = translated(&candidate.annotations, offset.map(|v| v as f32))?;
        candidate.canvas_behavior.allow_overflow |=
            self.guide.as_ref().is_some_and(|g| g.pages.is_some());
        candidate.promote_features();
        if let Some(v) = &mut candidate.video {
            v.sync(&candidate.annotations);
        }
        candidate.validate()?;
        Ok(MovePlan {
            dimensions,
            offset,
            annotations: candidate.annotations,
            original_dimensions: [self.width, self.height],
            base: automatic,
            moved_ids: ids,
        })
    }
}

impl MovePlan {
    fn padded_base(&self, document: &Document) -> Result<RgbaImage> {
        if self.original_dimensions != [document.width, document.height] {
            return Err(Error::Invalid("Canvas changed during the move".into()));
        }
        let source = document.base_image()?;
        if self.dimensions == self.original_dimensions && self.offset == [0, 0] {
            return Ok(source);
        }
        let mut padded = RgbaImage::new(self.dimensions[0], self.dimensions[1]);
        // Replace copies hidden RGB and alpha exactly, with no resampling.
        image::imageops::replace(
            &mut padded,
            &source,
            i64::from(self.offset[0]),
            i64::from(self.offset[1]),
        );
        Ok(padded)
    }
    pub fn display_list(&self, document: &Document) -> Result<crate::display::DisplayList> {
        let visible: Vec<_> = self
            .annotations
            .iter()
            .filter(|a| document.object_visible(a.id))
            .cloned()
            .collect();
        crate::display::layers_from_base(self.padded_base(document)?, &visible, crate::MAX_PIXELS)
    }
    pub fn apply(&self, document: &Document) -> Result<Document> {
        if self.original_dimensions != [document.width, document.height] {
            return Err(Error::Invalid("Canvas changed during the move".into()));
        }
        let mut result = document.clone();
        let turned = document.can_reset_rotation();
        if self.dimensions != self.original_dimensions || self.offset != [0, 0] {
            result.raster = Some(Arc::new(RasterImage::from_image(
                &self.padded_base(document)?,
            )?));
            result.source_size = Some(document.source_size.unwrap_or(self.original_dimensions));
            result.version = crate::MAX_PROJECT_VERSION;
        }
        result.width = self.dimensions[0];
        result.height = self.dimensions[1];
        result.annotations = self.annotations.clone();
        result.canvas_behavior.allow_overflow |=
            result.guide.as_ref().is_some_and(|g| g.pages.is_some());
        result.committed_object_moves(&self.moved_ids);
        if let Some(guide) = &mut result.guide {
            guide.top += self.offset[1] as f32;
        }
        result.auto_canvas_base = self.base;
        if self.base.is_some() {
            result.version = crate::MAX_PROJECT_VERSION;
        }
        if let Some(v) = &mut result.video {
            v.sync(&result.annotations);
        }
        if turned && let Some(mut record) = result.canvas_behavior.turned.take() {
            record.follow_canvas(self.offset, &result);
            result.canvas_behavior.turned = Some(record);
        }
        result.validate()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
