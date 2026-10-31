//! Persistent flat groups. Membership belongs to annotations, so snapshots and
//! clipboard payloads retain it without baking transforms into image pixels.
use crate::{Annotation, Document, Error, Result};
use std::collections::{BTreeMap, BTreeSet};
impl Document {
    pub fn group_members(&self, id: u64) -> Vec<u64> {
        let group = self
            .annotations
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.group);
        self.annotations
            .iter()
            .filter(|a| a.id == id || (group.is_some() && a.group == group))
            .map(|a| a.id)
            .collect()
    }
    pub fn expand_groups(&self, ids: &[u64]) -> Vec<u64> {
        let selected: BTreeSet<_> = ids.iter().flat_map(|id| self.group_members(*id)).collect();
        self.annotations
            .iter()
            .filter(|a| selected.contains(&a.id))
            .map(|a| a.id)
            .collect()
    }
    pub fn arranged_groups(&self, ids: &[u64], order: crate::StackOrder) -> Result<Vec<u64>> {
        if ids.is_empty()
            || ids
                .iter()
                .any(|id| !self.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Select existing objects to arrange".into()));
        }
        let selected = self.expand_groups(ids);
        let current: Vec<_> = self.annotations.iter().map(|a| a.id).collect();
        let first = current.iter().position(|id| selected.contains(id)).unwrap();
        let last = current
            .iter()
            .rposition(|id| selected.contains(id))
            .unwrap();
        let mut rest: Vec<_> = current
            .iter()
            .copied()
            .filter(|id| !selected.contains(id))
            .collect();
        let at = match order {
            crate::StackOrder::BringToFront => rest.len(),
            crate::StackOrder::SendToBack => 0,
            crate::StackOrder::BringForward => {
                if last + 1 == current.len() {
                    return Ok(current);
                }
                let neighbor = self.group_members(current[last + 1]);
                rest.iter().rposition(|id| neighbor.contains(id)).unwrap() + 1
            }
            crate::StackOrder::SendBackward => {
                if first == 0 {
                    return Ok(current);
                }
                let neighbor = self.group_members(current[first - 1]);
                rest.iter().position(|id| neighbor.contains(id)).unwrap()
            }
        };
        rest.splice(at..at, selected);
        Ok(rest)
    }
    pub(crate) fn validate_groups(&self) -> Result<()> {
        let mut counts = BTreeMap::new();
        for a in &self.annotations {
            if let Some(g) = a.group {
                *counts.entry(g).or_insert(0) += 1;
            }
        }
        if (!counts.is_empty() && self.version < 16)
            || counts.iter().any(|(id, n)| {
                *id == 0
                    || (*n < 2
                        && !self
                            .annotations
                            .iter()
                            .any(|a| a.group == Some(*id) && a.analysis.is_some()))
            })
        {
            return Err(Error::Invalid(
                "Groups require version16, nonzero identifiers and at least two members".into(),
            ));
        }
        for group in counts.keys() {
            if self
                .annotations
                .iter()
                .filter(|a| a.group == Some(*group) && a.analysis.is_some())
                .count()
                > 1
            {
                return Err(Error::Invalid(
                    "A Simplify group has only one analysis area".into(),
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn prune_groups(&mut self) {
        let mut counts = BTreeMap::new();
        for a in &self.annotations {
            if let Some(g) = a.group {
                *counts.entry(g).or_insert(0) += 1;
            }
        }
        for a in &mut self.annotations {
            if a.analysis.is_none() && a.group.is_some_and(|g| counts[&g] < 2) {
                a.group = None;
            }
        }
        self.annotations
            .retain(|a| a.analysis.is_none() || a.group.is_some());
    }
    pub(crate) fn group_objects(&mut self, ids: &[u64], ungroup: bool) -> Result<bool> {
        if ids.is_empty()
            || ids
                .iter()
                .any(|id| !self.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Select existing objects to group".into()));
        }
        let ids: BTreeSet<_> = self.expand_groups(ids).into_iter().collect();
        if ids.iter().any(|id| !self.object_editable(*id)) {
            return Err(Error::Invalid(
                "Show and unlock group members before editing the group".into(),
            ));
        }
        let current: Vec<_> = self
            .annotations
            .iter()
            .filter(|a| ids.contains(&a.id))
            .collect();
        if ungroup {
            if current.iter().all(|a| a.group.is_none()) {
                return Ok(false);
            }
        } else {
            if current.len() < 2 {
                return Err(Error::Invalid("Select two or more objects to group".into()));
            }
            if current[0].group.is_some() && current.iter().all(|a| a.group == current[0].group) {
                return Ok(false);
            }
        }
        let next = if ungroup {
            None
        } else {
            Some(
                self.annotations
                    .iter()
                    .filter_map(|a| a.group)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("Group identifiers exhausted".into()))?,
            )
        };
        for a in &mut self.annotations {
            if ids.contains(&a.id) {
                a.group = next;
            }
        }
        // Ungrouping or combining analysis groups bakes their current blocks
        // into ordinary editable shapes; the invisible analysis areas disappear.
        self.annotations
            .retain(|a| !ids.contains(&a.id) || a.analysis.is_none());
        self.prune_groups();
        Ok(true)
    }
}
/// Duplicate/import groups independently. Partial groups are deliberately ungrouped.
pub(crate) fn freshen(objects: &mut [Annotation], target: &Document) -> Result<()> {
    let mut counts = BTreeMap::new();
    for a in objects.iter() {
        if let Some(g) = a.group {
            *counts.entry(g).or_insert(0) += 1;
        }
    }
    let mut next = target
        .annotations
        .iter()
        .filter_map(|a| a.group)
        .max()
        .unwrap_or(0);
    let mut mapping = BTreeMap::new();
    for (id, n) in counts {
        if n >= 2
            || objects
                .iter()
                .any(|a| a.group == Some(id) && a.analysis.is_some())
        {
            next = next
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("Group identifiers exhausted".into()))?;
            mapping.insert(id, next);
        }
    }
    for a in objects {
        a.group = a.group.and_then(|g| mapping.get(&g).copied());
    }
    Ok(())
}
pub fn bounds(objects: &[Annotation]) -> Result<([f32; 2], [f32; 2])> {
    if objects.len() < 2 && !objects.iter().any(|a| a.analysis.is_some()) {
        return Err(Error::Invalid("Select two or more objects".into()));
    }
    let mut low = [f32::INFINITY; 2];
    let mut high = [f32::NEG_INFINITY; 2];
    for a in objects {
        let (lo, hi) = a.bounds();
        for i in 0..2 {
            low[i] = low[i].min(lo[i]);
            high[i] = high[i].max(hi[i]);
        }
    }
    Ok((low, high))
}
pub fn scaled(objects: &[Annotation], anchor: [f32; 2], scale: f32) -> Result<Vec<Annotation>> {
    if !scale.is_finite()
        || !(0.01..=100.).contains(&scale)
        || anchor.iter().any(|v| !v.is_finite())
    {
        return Err(Error::Invalid("Invalid group scale".into()));
    }
    let point = |p: &mut [f32; 2]| {
        for i in 0..2 {
            p[i] = anchor[i] + (p[i] - anchor[i]) * scale;
        }
    };
    let mut result = objects.to_vec();
    for a in &mut result {
        point(&mut a.start);
        point(&mut a.end);
        for p in &mut a.points {
            point(p);
        }
        a.corner_radius = (a.corner_radius * scale).min(a.max_corner_radius());
        if a.kind != crate::Kind::Redact && a.kind != crate::Kind::Image {
            let width = (a.width as f32 * scale).round().max(1.);
            if width > 32. {
                return Err(Error::Invalid(
                    "Group size exceeds the supported stroke width".into(),
                ));
            }
            a.width = width as u32;
        }
        if let Some(t) = &mut a.text {
            if let Some(typo) = &mut t.typography {
                typo.spacing *= scale;
            }
            t.size_points *= scale;
            t.padding *= scale;
            if let Some(o) = &mut t.outline {
                o.width *= scale;
            }
            t.validate()?;
        }
        if let Some(c) = &mut a.callout {
            point(&mut c.pointer);
            c.padding *= scale;
        }
        if let Some(o) = &mut a.step_outline {
            o.width *= scale;
            o.validate()?;
        }
        if let Some(s) = &mut a.shadow {
            s.distance *= scale;
            s.blur *= scale;
            s.validate()?;
        }
    }
    Ok(result)
}
pub fn rotated(objects: &[Annotation], center: [f32; 2], degrees: f32) -> Result<Vec<Annotation>> {
    let r = crate::rotation::Rotation::new(center, degrees)?;
    if objects.iter().any(|a| !a.can_rotate()) {
        return Err(Error::Invalid(
            "Groups containing Blur, Pixelate or Eraser regions cannot rotate yet".into(),
        ));
    }
    objects
        .iter()
        .map(|a| {
            let old = a.rotation().center;
            let new = r.point(old);
            let mut b = crate::canvas_expansion::translated(
                std::slice::from_ref(a),
                [new[0] - old[0], new[1] - old[1]],
            )?
            .remove(0);
            b = b.with_rotation(a.rotation_degrees + degrees)?;
            Ok(b)
        })
        .collect()
}
#[cfg(test)]
mod tests;
