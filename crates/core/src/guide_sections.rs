//! Editable guide section layout. Source images and independently edited objects survive reflow.
use crate::{Annotation, Document, Error, Kind, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

mod pages;
mod setup;
pub use setup::{Alignment, NumberStyle, PageSetup, RunningText};
#[cfg(test)]
mod tests;
pub use pages::{PAGE_GAP, PageLayout};
fn page_zero(page: &usize) -> bool {
    *page == 0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Header,
    Image,
    Text,
    Blank,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    #[serde(default, skip_serializing_if = "page_zero")]
    pub page: usize,
    pub id: u64,
    pub kind: SectionKind,
    pub height: f32,
    pub objects: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<PageLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    pub top: f32,
    pub sections: Vec<Section>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    AddPage {
        page: usize,
        copy: bool,
    },
    MovePage {
        page: usize,
        index: usize,
    },
    DeletePage {
        page: usize,
    },
    SectionPage {
        id: u64,
        page: usize,
        copy: bool,
    },
    PageSize {
        width: u32,
        height: u32,
        auto_overflow: bool,
    },
    PageSetup {
        settings: Box<PageSetup>,
        pages: Vec<usize>,
        scale_content: bool,
        auto_overflow: bool,
        distribute: bool,
    },
    Paginate,
    Insert {
        after: Option<u64>,
        kind: SectionKind,
    },
    InsertImage {
        after: Option<u64>,
        image: std::sync::Arc<crate::bitmap::Bitmap>,
    },
    Resize {
        id: u64,
        height: f32,
    },
    Remove {
        id: u64,
    },
    Duplicate {
        id: u64,
    },
    Move {
        id: u64,
        index: usize,
    },
    /// Insert at a page-local position after removing the source section.
    Relocate {
        id: u64,
        page: usize,
        index: usize,
    },
}
impl Layout {
    pub fn spans(&self) -> Vec<(u64, f32, f32)> {
        let mut positions: Vec<_> = (0..self.page_count())
            .map(|p| self.content_top(p))
            .collect();
        self.sections
            .iter()
            .map(|s| {
                let slot = s.page.min(positions.len() - 1);
                let top = positions[slot] + self.page_origin(slot);
                positions[slot] += s.height;
                (s.id, top, top + s.height)
            })
            .collect()
    }
    pub fn validate(&self, document: &Document) -> Result<()> {
        if self
            .template
            .as_ref()
            .is_some_and(|name| document.version < 24 || name.is_empty() || name.len() > 120)
            || document.version < 11
            || !self.top.is_finite()
            || self.top < 0.
            || self.sections.len() > 256
        {
            return Err(Error::Invalid("Invalid guide section layout".into()));
        }
        self.validate_pages(document)?;
        let mut sections = BTreeSet::new();
        let mut objects = BTreeSet::new();
        for s in &self.sections {
            if (s.kind == SectionKind::Blank && document.version < 26)
                || !sections.insert(s.id)
                || s.id == 0
                || !s.height.is_finite()
                || !(48.0..=32768.0).contains(&s.height)
            {
                return Err(Error::Invalid(
                    "Invalid guide section dimensions or identity".into(),
                ));
            }
            for id in &s.objects {
                if !objects.insert(*id) || !document.annotations.iter().any(|a| a.id == *id) {
                    return Err(Error::Invalid(
                        "Guide objects must belong to one existing section".into(),
                    ));
                }
            }
            if let Some(id) = s.marker
                && (!s.objects.contains(&id)
                    || !document
                        .annotations
                        .iter()
                        .any(|a| a.id == id && a.kind == Kind::Step))
            {
                return Err(Error::Invalid("Invalid guide step marker".into()));
            }
        }
        if self
            .spans()
            .last()
            .is_some_and(|s| s.2 > document.height as f32 + 0.01)
        {
            return Err(Error::Invalid(
                "Guide sections extend beyond the canvas".into(),
            ));
        }
        Ok(())
    }
}
impl Document {
    pub(crate) fn reconcile_guide(&mut self) {
        if self.annotations.is_empty()
            && self.raster.is_some()
            && self.guide.as_ref().is_none_or(|g| g.pages.is_none())
        {
            self.guide = None;
            return;
        }
        if let Some(layout) = &mut self.guide {
            let ids: BTreeSet<_> = self.annotations.iter().map(|a| a.id).collect();
            for s in &mut layout.sections {
                s.objects.retain(|id| ids.contains(id));
                s.marker = s.marker.filter(|id| ids.contains(id));
            }
            let previous: BTreeMap<_, _> = layout
                .sections
                .iter()
                .flat_map(|s| s.objects.iter().map(move |id| (*id, s.id)))
                .collect();
            let spans = layout.spans();
            for s in &mut layout.sections {
                s.objects.clear();
            }
            for a in &self.annotations {
                let (lo, hi) = a.bounds();
                let mut winner = None;
                let mut area = 0.0_f32;
                for ((id, top, bottom), section) in spans.iter().zip(&layout.sections) {
                    let x = layout.page_offset(section.page)[0];
                    let overlap = (hi[0].min(
                        x + layout.page_size(section.page, [self.width, self.height])[0] as f32,
                    ) - lo[0].max(x))
                    .max(0.)
                        * (hi[1].min(*bottom) - lo[1].max(*top)).max(0.);
                    if overlap > area
                        || (overlap > 0. && overlap == area && previous.get(&a.id) == Some(id))
                    {
                        area = overlap;
                        winner = Some(*id);
                    }
                }
                if let Some(id) = winner {
                    layout
                        .sections
                        .iter_mut()
                        .find(|s| s.id == id)
                        .unwrap()
                        .objects
                        .push(a.id);
                }
            }
            for section in &mut layout.sections {
                section.marker = section.marker.filter(|id| section.objects.contains(id));
            }
        }
    }
    pub fn edit_guide(&self, action: &Action) -> Result<Self> {
        if let Action::PageSetup {
            settings,
            pages,
            scale_content,
            auto_overflow,
            distribute,
        } = action
        {
            return self.edit_page_setup(
                settings,
                pages,
                *scale_content,
                *auto_overflow,
                *distribute,
            );
        }
        self.validate()?;
        if matches!(
            action,
            Action::AddPage { .. }
                | Action::MovePage { .. }
                | Action::DeletePage { .. }
                | Action::SectionPage { .. }
                | Action::PageSize { .. }
                | Action::Paginate
        ) {
            return self.edit_pages(action);
        }
        let mut next = self.clone();
        let mut layout = next
            .guide
            .take()
            .ok_or_else(|| Error::Invalid("This image has no guide sections".into()))?;
        let mut old_origins = layout.section_origins();
        let index = |id| {
            layout
                .sections
                .iter()
                .position(|s| s.id == id)
                .ok_or_else(|| Error::Invalid("Guide section does not exist".into()))
        };
        let mut next_object = next
            .annotations
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
        let next_section = layout
            .sections
            .iter()
            .map(|s| s.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Guide section identifier overflow".into()))?;
        match *action {
            Action::Insert { after, .. } | Action::InsertImage { after, .. } => {
                let (kind, bitmap) = match action {
                    Action::Insert { kind, .. } => (*kind, None),
                    Action::InsertImage { image, .. } => (SectionKind::Image, Some(image.clone())),
                    _ => unreachable!(),
                };
                let at = after.map(index).transpose()?.map_or(0, |i| i + 1);
                let top = if at == 0 {
                    layout.top
                } else {
                    layout.spans()[at - 1].2
                };
                let mut height = match kind {
                    SectionKind::Header => 120.,
                    SectionKind::Text | SectionKind::Blank => 160.,
                    SectionKind::Image => 360.,
                };
                let page = after
                    .and_then(|id| layout.sections.iter().find(|s| s.id == id))
                    .map_or(0, |s| s.page);
                let width = layout.content_width(page, next.width);
                let mut objects = Vec::new();
                let title = match kind {
                    SectionKind::Header => "Heading",
                    SectionKind::Text => "Enter section text",
                    SectionKind::Image => "Image section",
                    SectionKind::Blank => "",
                };
                let text = Annotation {
                    id: next_object,
                    kind: Kind::Text,
                    start: [24., top + 12.],
                    end: [(width - 24.).max(25.), top + height - 16.],
                    color: [45, 45, 45, 255],
                    width: 1,
                    corner_radius: 0.,
                    fill: None,
                    points: vec![],
                    image: None,
                    step_outline: None,
                    line_style: None,
                    shadow: None,
                    analysis: None,
                    highlighter_rect: false,
                    group: None,
                    rotation_degrees: 0.,
                    callout: None,
                    text: Some(crate::text::TextStyle {
                        content: title.into(),
                        size_points: if kind == SectionKind::Header {
                            28.
                        } else {
                            20.
                        },
                        bold: kind == SectionKind::Header,
                        horizontal: crate::text::HorizontalAlignment::Left,
                        vertical: crate::text::VerticalAlignment::Top,
                        ..Default::default()
                    }),
                };
                if let Some(bitmap) = bitmap {
                    if bitmap.is_cursor() {
                        return Err(Error::Invalid(
                            "Choose a screenshot or image for this section".into(),
                        ));
                    }
                    let [w, h] = bitmap.size().map(|v| v as f32);
                    let scale = ((width - 48.).max(1.) / w).min(1200. / h).min(1.);
                    let mut image = bitmap.annotation(next_object);
                    image.start = [24., top + 16.];
                    image.end = [24. + w * scale, top + 16. + h * scale];
                    height = (h * scale + 32.).max(48.);
                    next.annotations.push(image);
                    objects.push(next_object);
                } else if matches!(kind, SectionKind::Header | SectionKind::Text) {
                    objects.push(next_object);
                    next.annotations.push(text);
                }
                layout.sections.insert(
                    at,
                    Section {
                        page: if at == 0 {
                            0
                        } else {
                            layout.sections[at - 1].page
                        },
                        id: next_section,
                        kind,
                        height,
                        objects,
                        marker: None,
                    },
                );
                old_origins.insert(next_section, [0., top]);
            }
            Action::Resize { id, height } => {
                let at = index(id)?;
                if !height.is_finite() || !(48.0..=32768.0).contains(&height) {
                    return Err(Error::Invalid(
                        "Section height must be between 48 and 32768 pixels".into(),
                    ));
                }
                let top = old_origins[&id][1];
                let content_bottom = next
                    .annotations
                    .iter()
                    .filter(|a| layout.sections[at].objects.contains(&a.id))
                    .map(|a| a.bounds().1[1] - top + 12.)
                    .fold(48., f32::max);
                layout.sections[at].height = height.max(content_bottom);
            }
            Action::Remove { id } => {
                let at = index(id)?;
                let removed = layout.sections.remove(at);
                next.annotations
                    .retain(|a| !removed.objects.contains(&a.id));
            }
            Action::Duplicate { id } => {
                let at = index(id)?;
                let mut copy = layout.sections[at].clone();
                copy.id = next_section;
                let mut ids = BTreeMap::new();
                let before_count = next.annotations.len();
                for object in &copy.objects {
                    let mut a = next
                        .annotations
                        .iter()
                        .find(|a| a.id == *object)
                        .unwrap()
                        .clone();
                    ids.insert(*object, next_object);
                    let state = next.object_state(*object);
                    if state != Default::default() {
                        next.object_states.insert(next_object, state);
                    }
                    a.id = next_object;
                    next_object = next_object
                        .checked_add(1)
                        .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
                    next.annotations.push(a);
                }
                crate::groups::freshen(&mut next.annotations[before_count..], self)?;
                copy.objects = copy.objects.iter().map(|id| ids[id]).collect();
                copy.marker = copy.marker.map(|id| ids[&id]);
                old_origins.insert(next_section, old_origins[&id]);
                layout.sections.insert(at + 1, copy);
            }
            Action::Move {
                id,
                index: destination,
            } => {
                let at = index(id)?;
                if destination >= layout.sections.len() {
                    return Err(Error::Invalid("Invalid section destination".into()));
                }
                if layout.sections[at].page != layout.sections[destination].page {
                    return Err(Error::Invalid(
                        "Use Move to to move a section between pages".into(),
                    ));
                }
                let section = layout.sections.remove(at);
                layout.sections.insert(destination, section);
            }
            Action::Relocate {
                id,
                page,
                index: destination,
            } => {
                let at = index(id)?;
                if page >= layout.page_count() {
                    return Err(Error::Invalid("Invalid destination page".into()));
                }
                let mut section = layout.sections.remove(at);
                let positions: Vec<_> = layout
                    .sections
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.page == page)
                    .map(|(i, _)| i)
                    .collect();
                if destination > positions.len() {
                    return Err(Error::Invalid("Invalid section destination".into()));
                }
                let insert = positions.get(destination).copied().unwrap_or_else(|| {
                    layout
                        .sections
                        .iter()
                        .position(|s| s.page > page)
                        .unwrap_or(layout.sections.len())
                });
                section.page = page;
                layout.sections.insert(insert, section);
            }
            _ => unreachable!(),
        }
        self.finish_guide(next, layout, old_origins)
    }
    fn finish_guide(
        &self,
        mut next: Document,
        mut layout: Layout,
        old_origins: BTreeMap<u64, [f32; 2]>,
    ) -> Result<Self> {
        layout.overflow_sections()?;
        for (id, origin) in layout.section_origins() {
            let delta = [0, 1].map(|i| origin[i] - old_origins[&id][i]);
            if delta == [0.; 2] {
                continue;
            }
            let section = layout.sections.iter().find(|s| s.id == id).unwrap();
            let moved: Vec<_> = next
                .annotations
                .iter()
                .filter(|a| section.objects.contains(&a.id))
                .cloned()
                .collect();
            let moved = crate::canvas_expansion::translated(&moved, delta)?;
            for a in moved {
                let slot = next.annotations.iter_mut().find(|s| s.id == a.id).unwrap();
                *slot = a;
            }
        }
        let mut number = 0;
        for section in &layout.sections {
            if let Some(id) = section.marker {
                number += 1;
                if let Some(a) = next.annotations.iter_mut().find(|a| a.id == id)
                    && let Some(text) = &mut a.text
                {
                    text.content = number.to_string();
                }
            }
        }
        for a in &self.annotations {
            if self.object_state(a.id).locked
                && next.annotations.iter().find(|n| n.id == a.id) != Some(a)
            {
                return Err(Error::Invalid(
                    "Unlock the affected objects before changing guide sections".into(),
                ));
            }
        }
        let bottom = layout.spans().last().map_or(layout.top + 48., |s| s.2);
        let objects_bottom = next
            .annotations
            .iter()
            .map(|a| a.bounds().1[1])
            .fold(0., f32::max);
        next.height = layout.pages.as_ref().map_or_else(
            || bottom.max(objects_bottom + 12.).ceil() as u32,
            |_| layout.canvas_size()[1],
        );
        if layout.pages.is_some() {
            next.width = layout.canvas_size()[0];
        }
        crate::validate_dimensions(next.width, next.height)?;
        next.auto_canvas_base = None;
        next.guide = Some(layout);
        next.prune_groups();
        next.prune_object_states();
        next.promote_features();
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod pages_tests;
#[cfg(test)]
mod setup_tests;
