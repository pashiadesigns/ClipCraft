use crate::{Annotation, Document, Error, Kind, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    pub detail: u8,
    #[serde(default)]
    pub colors: Vec<[u8; 4]>,
}

/// Clip a single cached analysis to the live area. No image decoding occurs on
/// pointer movement. An invisible anchor keeps an empty area selectable.
pub fn regenerate(
    anchor: &Annotation,
    candidates: &[Annotation],
    bounds: ([f32; 2], [f32; 2]),
) -> Vec<Annotation> {
    let (lo, hi) = bounds;
    let mut region = anchor.clone();
    region.start = lo;
    region.end = hi;
    let mut result = vec![region];
    for candidate in candidates {
        let mut block = candidate.clone();
        block.group = anchor.group;
        block.analysis = None;
        for i in 0..2 {
            block.start[i] = block.start[i].max(lo[i]);
            block.end[i] = block.end[i].min(hi[i]);
        }
        if block.end[0] - block.start[0] > 1. && block.end[1] - block.start[1] > 1. {
            block.corner_radius = block.corner_radius.min(block.max_corner_radius());
            result.push(block);
        }
    }
    result
}
impl Document {
    pub fn simplify_source(&self, group: u64) -> Result<image::RgbaImage> {
        let mut source = self.clone();
        source.annotations.retain(|a| a.group != Some(group));
        source
            .object_states
            .retain(|id, _| source.annotations.iter().any(|a| a.id == *id));
        source.reconcile_guide();
        source.render()
    }
    pub fn replace_simplify_group(&mut self, group: u64, objects: Vec<Annotation>) -> Result<()> {
        let old: Vec<_> = self
            .annotations
            .iter()
            .filter(|a| a.group == Some(group))
            .collect();
        let Some(anchor) = old.iter().find(|a| a.analysis.is_some()) else {
            return Err(Error::Invalid("Select an automatic Simplify group".into()));
        };
        if old.iter().any(|a| !self.object_editable(a.id)) {
            return Err(Error::Invalid(
                "Show and unlock the Simplify group before resizing it".into(),
            ));
        }
        if objects
            .iter()
            .any(|a| a.group != Some(group) || a.kind != Kind::Simplify)
            || objects.iter().filter(|a| a.analysis.is_some()).count() != 1
            || !objects
                .iter()
                .any(|a| a.id == anchor.id && a.analysis.is_some())
        {
            return Err(Error::Invalid("Invalid Simplify replacement".into()));
        }
        let at = self
            .annotations
            .iter()
            .position(|a| a.group == Some(group))
            .unwrap();
        let mut next = self.clone();
        next.annotations.retain(|a| a.group != Some(group));
        next.annotations.splice(at..at, objects);
        next.object_states
            .retain(|id, _| next.annotations.iter().any(|a| a.id == *id));
        next.reconcile_guide();
        next.validate()?;
        *self = next;
        Ok(())
    }
}
