//! Editable pasteboard policy. A held object rearms only on a committed drop
//! wholly inside the output, never while a drag preview passes through it.
use crate::{Annotation, Document};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasBehavior {
    pub dynamic_expansion: bool,
    pub allow_overflow: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub held: BTreeSet<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub page_owners: BTreeMap<u64, usize>,
    /// Whole-image perspective, applied on top of the finished image (FX-003).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perspective: Option<crate::tilt::Tilt>,
    /// Filters, border and edge for the whole picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<crate::picture_finish::Finish>,
}
impl Default for CanvasBehavior {
    fn default() -> Self {
        Self {
            dynamic_expansion: true,
            allow_overflow: false,
            held: BTreeSet::new(),
            page_owners: BTreeMap::new(),
            perspective: None,
            finish: None,
        }
    }
}
impl CanvasBehavior {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
pub fn contained(a: &Annotation, size: [u32; 2]) -> bool {
    inside(a, [0., 0., size[0] as f32, size[1] as f32])
}
pub fn inside(a: &Annotation, rect: [f32; 4]) -> bool {
    let (lo, hi) = a.bounds();
    (0..2).all(|i| lo[i] >= rect[i] - 0.001 && hi[i] <= rect[i] + rect[i + 2] + 0.001)
}
impl Document {
    pub fn page_rect(&self, page: usize) -> [f32; 4] {
        let size = [self.width, self.height];
        self.guide.as_ref().filter(|g| g.pages.is_some()).map_or(
            [0., 0., size[0] as f32, size[1] as f32],
            |g| {
                let p = g.page_offset(page);
                let s = g.page_size(page, size);
                [p[0], p[1], s[0] as f32, s[1] as f32]
            },
        )
    }
    pub fn object_page(&self, a: &Annotation) -> usize {
        self.canvas_behavior
            .page_owners
            .get(&a.id)
            .copied()
            .unwrap_or_else(|| {
                self.guide.as_ref().map_or(0, |g| {
                    g.sections
                        .iter()
                        .find(|s| s.objects.contains(&a.id))
                        .map_or_else(|| g.page_at(a.start), |s| s.page)
                })
            })
    }
    pub fn object_inside_output(&self, a: &Annotation) -> bool {
        inside(a, self.page_rect(self.object_page(a)))
    }
    pub(crate) fn set_canvas_expansion(&mut self, enabled: bool) -> bool {
        if self.canvas_behavior.dynamic_expansion == enabled {
            return false;
        }
        self.canvas_behavior.dynamic_expansion = enabled;
        self.canvas_behavior.allow_overflow = true;
        self.auto_canvas_base = None;
        self.hold_outside_objects();
        true
    }
    pub(crate) fn hold_outside_objects(&mut self) {
        let ids: Vec<_> = self
            .annotations
            .iter()
            .filter(|a| !self.object_inside_output(a))
            .map(|a| a.id)
            .collect();
        self.canvas_behavior.held.extend(ids);
    }
    pub(crate) fn committed_object_moves(&mut self, ids: &BTreeSet<u64>) {
        for a in &self.annotations {
            if !ids.contains(&a.id) {
                continue;
            }
            let destination = self.guide.as_ref().filter(|g| g.pages.is_some()).map(|g| {
                let (lo, hi) = a.bounds();
                g.page_at([(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5])
            });
            let rect = self.page_rect(destination.unwrap_or(0));
            if inside(a, rect) {
                self.canvas_behavior.held.remove(&a.id);
                if let Some(page) = destination {
                    self.canvas_behavior.page_owners.insert(a.id, page);
                }
            }
        }
    }
    pub(crate) fn prune_canvas_behavior(&mut self) {
        self.canvas_behavior
            .held
            .retain(|id| self.annotations.iter().any(|a| a.id == *id));
        let pages = self
            .guide
            .as_ref()
            .filter(|g| g.pages.is_some())
            .map_or(0, |g| g.page_count());
        self.canvas_behavior
            .page_owners
            .retain(|id, p| *p < pages && self.annotations.iter().any(|a| a.id == *id));
    }
}
