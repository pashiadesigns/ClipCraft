use super::*;

pub const PAGE_GAP: u32 = 48;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageLayout {
    #[serde(default)]
    pub horizontal: bool,
    pub size: [u32; 2],
    pub count: usize,
    pub auto_overflow: bool,
}

impl Layout {
    pub fn page_count(&self) -> usize {
        self.pages.as_ref().map_or(1, |p| p.count.max(1))
    }
    pub fn page_offset(&self, page: usize) -> [f32; 2] {
        self.pages.as_ref().map_or([0.; 2], |p| {
            let axis = usize::from(!p.horizontal);
            let mut offset = [0.; 2];
            offset[axis] = page as f32 * (p.size[axis] + PAGE_GAP) as f32;
            offset
        })
    }
    pub fn page_origin(&self, page: usize) -> f32 {
        self.page_offset(page)[1]
    }
    pub fn page_width(&self, document_width: u32) -> u32 {
        self.pages.as_ref().map_or(document_width, |p| p.size[0])
    }
    pub fn canvas_size(&self) -> [u32; 2] {
        self.pages.as_ref().map_or([0; 2], |p| {
            let mut size = p.size;
            let axis = usize::from(!p.horizontal);
            size[axis] = (size[axis].saturating_add(PAGE_GAP))
                .saturating_mul(p.count as u32)
                .saturating_sub(PAGE_GAP);
            size
        })
    }
    pub fn page_at(&self, point: [f32; 2]) -> usize {
        self.pages.as_ref().map_or(0, |p| {
            let axis = usize::from(!p.horizontal);
            ((point[axis].max(0.) / (p.size[axis] + PAGE_GAP) as f32) as usize).min(p.count - 1)
        })
    }
    pub fn section_origins(&self) -> BTreeMap<u64, [f32; 2]> {
        self.spans()
            .into_iter()
            .zip(&self.sections)
            .map(|((id, top, _), s)| (id, [self.page_offset(s.page)[0], top]))
            .collect()
    }
    pub(super) fn validate_pages(&self, document: &Document) -> Result<()> {
        if let Some(p) = &self.pages
            && (document.version < 27
                || !(1..=128).contains(&p.count)
                || (p.horizontal && document.version < 28)
                || p.size.iter().any(|n| !(128..=32768).contains(n))
                || self.top + 48. > p.size[1] as f32
                || [document.width, document.height] != self.canvas_size())
        {
            return Err(Error::Invalid(
                "Invalid guide page dimensions or count".into(),
            ));
        }
        let mut previous = 0;
        for s in &self.sections {
            if s.page >= self.page_count() || s.page < previous {
                return Err(Error::Invalid("Invalid guide section page".into()));
            }
            previous = s.page;
        }
        if let Some(p) = &self.pages {
            for (i, (_, _, bottom)) in self.spans().iter().enumerate() {
                if *bottom > self.page_origin(self.sections[i].page) + p.size[1] as f32 + 0.01 {
                    return Err(Error::Invalid("Section exceeds its page; enable automatic overflow or increase page height".into()));
                }
            }
        }
        Ok(())
    }
    pub(super) fn overflow_sections(&mut self) -> Result<()> {
        let Some(pages) = &mut self.pages else {
            return Ok(());
        };
        self.sections.sort_by_key(|s| s.page);
        let available = pages.size[1] as f32 - self.top;
        let mut page = 0;
        let mut used = 0.;
        for section in &mut self.sections {
            if section.height > available + 0.01 {
                return Err(Error::Invalid("A section is taller than the page. Increase page height or shorten that section before paginating.".into()));
            }
            if section.page > page {
                page = section.page;
                used = 0.;
            }
            if used + section.height > available + 0.01 {
                if !pages.auto_overflow {
                    return Err(Error::Invalid(
                        "Page is full. Enable automatic overflow or move sections to another page."
                            .into(),
                    ));
                }
                page += 1;
                used = 0.;
            }
            section.page = page;
            used += section.height;
        }
        pages.count = pages.count.max(page + 1);
        if pages.count > 128 {
            return Err(Error::Invalid(
                "Guides support up to 128 pages within the canvas pixel limit".into(),
            ));
        }
        Ok(())
    }
}

impl Document {
    pub(super) fn edit_pages(&self, action: &Action) -> Result<Self> {
        let mut next = self.clone();
        let mut layout = next
            .guide
            .take()
            .ok_or_else(|| Error::Invalid("This image has no guide sections".into()))?;
        let mut old_origins = layout.section_origins();
        let old_layout = layout.clone();
        layout.pages.get_or_insert(PageLayout {
            horizontal: true,
            size: [self.width, self.height],
            count: 1,
            auto_overflow: true,
        });
        layout.pages.as_mut().unwrap().horizontal = true;
        let count = layout.page_count();
        let check_page = |page: usize| -> Result<()> {
            if page < count {
                Ok(())
            } else {
                Err(Error::Invalid("Guide page does not exist".into()))
            }
        };
        let mut next_object = self
            .annotations
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
        let mut next_section = layout
            .sections
            .iter()
            .map(|s| s.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Section identifier overflow".into()))?;
        if !next.source_png.is_empty() && next.source_size.is_none() {
            next.source_size = Some([self.width, self.height]);
        }
        if old_layout.pages.is_none() && (!self.source_png.is_empty() || self.raster.is_some()) {
            let base = self.base_image()?;
            if base.pixels().any(|pixel| pixel != base.get_pixel(0, 0)) {
                // A photographic canvas is a page object, not a background that
                // may be stretched across all subsequent pages.
                let bitmap = crate::bitmap::Bitmap::new(std::sync::Arc::new(
                    crate::RasterImage::from_image(&base)?,
                ))?;
                next.annotations
                    .insert(0, std::sync::Arc::new(bitmap).annotation(next_object));
                next_object = next_object
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
                next.raster = Some(std::sync::Arc::new(crate::RasterImage::from_image(
                    &image::RgbaImage::new(1, 1),
                )?));
            }
        }
        let before_count = next.annotations.len();
        match *action {
            Action::AddPage { page, copy } => {
                check_page(page)?;
                let originals: Vec<_> = layout
                    .sections
                    .iter()
                    .filter(|s| s.page == page)
                    .cloned()
                    .collect();
                for s in &mut layout.sections {
                    if s.page > page {
                        s.page += 1;
                    }
                }
                layout.pages.as_mut().unwrap().count += 1;
                if copy {
                    let owned: BTreeSet<_> = old_layout
                        .sections
                        .iter()
                        .flat_map(|s| s.objects.iter().copied())
                        .collect();
                    let free: Vec<_> = next
                        .annotations
                        .iter()
                        .filter(|a| !owned.contains(&a.id))
                        .filter(|a| old_layout.page_at(a.start) == page)
                        .cloned()
                        .collect();
                    for mut a in free {
                        let state = self.object_state(a.id);
                        a.id = next_object;
                        next_object = next_object
                            .checked_add(1)
                            .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
                        if state != Default::default() {
                            next.object_states.insert(a.id, state);
                        }
                        next.annotations.push(
                            crate::canvas_expansion::translated(
                                &[a],
                                [0, 1].map(|i| {
                                    layout.page_offset(page + 1)[i]
                                        - old_layout.page_offset(page)[i]
                                }),
                            )?
                            .remove(0),
                        );
                    }
                    for section in originals {
                        let mut duplicate = duplicate_section(
                            &mut next,
                            &section,
                            &mut next_object,
                            &mut next_section,
                        )?;
                        duplicate.page = page + 1;
                        old_origins.insert(duplicate.id, old_origins[&section.id]);
                        layout.sections.push(duplicate);
                    }
                } else {
                    let height =
                        (layout.pages.as_ref().unwrap().size[1] as f32 - layout.top).min(160.);
                    layout.sections.push(Section {
                        page: page + 1,
                        id: next_section,
                        kind: SectionKind::Blank,
                        height,
                        objects: vec![],
                        marker: None,
                    });
                    old_origins.insert(
                        next_section,
                        [
                            layout.page_offset(page + 1)[0],
                            layout.page_origin(page + 1) + layout.top,
                        ],
                    );
                }
            }
            Action::DeletePage { page } => {
                check_page(page)?;
                if count == 1 {
                    return Err(Error::Invalid("Keep at least one page".into()));
                }
                let removed: BTreeSet<_> = layout
                    .sections
                    .iter()
                    .filter(|s| s.page == page)
                    .flat_map(|s| s.objects.iter().copied())
                    .collect();
                next.annotations.retain(|a| !removed.contains(&a.id));
                layout.sections.retain(|s| s.page != page);
                for s in &mut layout.sections {
                    if s.page > page {
                        s.page -= 1;
                    }
                }
                layout.pages.as_mut().unwrap().count -= 1;
            }
            Action::MovePage { page, index } => {
                check_page(page)?;
                check_page(index)?;
                for section in &mut layout.sections {
                    section.page = reordered_page(section.page, page, index);
                }
            }
            Action::SectionPage { id, page, copy } => {
                check_page(page)?;
                let at = layout
                    .sections
                    .iter()
                    .position(|s| s.id == id)
                    .ok_or_else(|| Error::Invalid("Guide section does not exist".into()))?;
                if copy {
                    let original = layout.sections[at].clone();
                    let mut duplicate = duplicate_section(
                        &mut next,
                        &original,
                        &mut next_object,
                        &mut next_section,
                    )?;
                    duplicate.page = page;
                    old_origins.insert(duplicate.id, old_origins[&id]);
                    layout.sections.push(duplicate);
                } else {
                    let mut section = layout.sections.remove(at);
                    section.page = page;
                    layout.sections.push(section);
                }
            }
            Action::PageSize {
                width,
                height,
                auto_overflow,
            } => {
                if !(128..=32768).contains(&width) || !(128..=32768).contains(&height) {
                    return Err(Error::Invalid(
                        "Page dimensions must be between 128 and 32768 pixels".into(),
                    ));
                }
                let scale = width as f32 / old_layout.page_width(self.width) as f32;
                // Keep fonts within the editor's supported range when narrowing a page.
                // Geometry and image crops still scale proportionally.
                for a in &mut next.annotations {
                    if let Some(t) = &mut a.text {
                        t.size_points = (t.size_points * scale).clamp(6., 144.) / scale;
                    }
                }
                next.annotations = crate::groups::scaled(&next.annotations, [0., 0.], scale)?;
                layout.top *= scale;
                for s in &mut layout.sections {
                    s.height = (s.height * scale).max(48.);
                }
                for origin in old_origins.values_mut() {
                    for n in origin {
                        *n *= scale;
                    }
                }
                next.width = width;
                let p = layout.pages.as_mut().unwrap();
                p.size = [width, height];
                p.auto_overflow = auto_overflow;
            }
            Action::Paginate => {
                for s in &mut layout.sections {
                    s.page = 0;
                }
                let p = layout.pages.as_mut().unwrap();
                p.count = 1;
                p.auto_overflow = true;
            }
            _ => unreachable!(),
        }
        let copied_start = before_count.min(next.annotations.len());
        crate::groups::freshen(&mut next.annotations[copied_start..], self)?;
        // Free objects outside section bounds still belong to their physical page.
        let owned: BTreeSet<_> = old_layout
            .sections
            .iter()
            .flat_map(|s| s.objects.iter().copied())
            .collect();
        let free: Vec<_> = self
            .annotations
            .iter()
            .filter(|a| !owned.contains(&a.id))
            .cloned()
            .collect();
        for a in free {
            let page = old_layout.page_at(a.start);
            let destination = match *action {
                Action::DeletePage { page: removed } if page == removed => None,
                Action::DeletePage { page: removed } => {
                    Some(if page > removed { page - 1 } else { page })
                }
                Action::AddPage { page: after, .. } => {
                    Some(if page > after { page + 1 } else { page })
                }
                Action::MovePage { page: from, index } => Some(reordered_page(page, from, index)),
                _ => Some(page),
            };
            if let Some(dest) = destination {
                let scale =
                    layout.page_width(next.width) as f32 / old_layout.page_width(self.width) as f32;
                let delta = [0, 1]
                    .map(|i| layout.page_offset(dest)[i] - old_layout.page_offset(page)[i] * scale);
                if let Some(slot) = next.annotations.iter_mut().find(|n| n.id == a.id) {
                    *slot = crate::canvas_expansion::translated(std::slice::from_ref(slot), delta)?
                        .remove(0);
                }
            } else {
                next.annotations.retain(|n| n.id != a.id);
            }
        }
        self.finish_guide(next, layout, old_origins)
    }
}

fn duplicate_section(
    document: &mut Document,
    original: &Section,
    next_object: &mut u64,
    next_section: &mut u64,
) -> Result<Section> {
    let mut section = original.clone();
    section.id = *next_section;
    *next_section = next_section
        .checked_add(1)
        .ok_or_else(|| Error::Invalid("Section identifier overflow".into()))?;
    let mut ids = BTreeMap::new();
    for id in &original.objects {
        let mut object = document
            .annotations
            .iter()
            .find(|a| a.id == *id)
            .ok_or_else(|| Error::Invalid("Section object is missing".into()))?
            .clone();
        object.id = *next_object;
        ids.insert(*id, *next_object);
        let state = document.object_state(*id);
        if state != Default::default() {
            document.object_states.insert(*next_object, state);
        }
        *next_object = next_object
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
        document.annotations.push(object);
    }
    section.objects = original.objects.iter().map(|id| ids[id]).collect();
    section.marker = original.marker.map(|id| ids[&id]);
    Ok(section)
}

fn reordered_page(value: usize, from: usize, to: usize) -> usize {
    if value == from {
        to
    } else if from < to && value > from && value <= to {
        value - 1
    } else if to < from && value >= to && value < from {
        value + 1
    } else {
        value
    }
}
