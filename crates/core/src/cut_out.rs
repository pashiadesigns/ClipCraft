//! Straight strip removal, with exact retained RGBA and original-image retention.
use crate::{Document, Error, RasterImage, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutAxis {
    Horizontal,
    Vertical,
}

impl CutAxis {
    pub fn dimension(self) -> usize {
        if self == Self::Horizontal { 1 } else { 0 }
    }
    pub fn output_size(self, size: [u32; 2], start: u32, end: u32) -> Result<[u32; 2]> {
        let axis = self.dimension();
        if start >= end || end > size[axis] || end - start == size[axis] || size.contains(&0) {
            return Err(Error::Invalid("Cut Out must remove a nonempty strip while keeping at least one pixel row or column.".into()));
        }
        let mut result = size;
        result[axis] -= end - start;
        Ok(result)
    }
}

impl Document {
    pub fn cut_out_page_layout(
        &self,
        axis: CutAxis,
        start: u32,
        end: u32,
        page: usize,
        all_pages: bool,
    ) -> Result<crate::guide_sections::Layout> {
        let mut layout = self
            .guide
            .clone()
            .filter(|g| g.pages.is_some())
            .ok_or_else(|| Error::Invalid("Cut Out needs an existing page.".into()))?;
        let pages = layout.pages.as_mut().unwrap();
        if page >= pages.count {
            return Err(Error::Invalid(
                "Select an existing page before cutting.".into(),
            ));
        }
        if pages.setup.is_empty() {
            pages.setup =
                vec![crate::guide_sections::PageSetup::from_pixels(pages.size); pages.count];
        }
        for (i, settings) in pages.setup.iter_mut().enumerate() {
            if all_pages || i == page {
                settings.size = axis.output_size(settings.size, start, end).map_err(|_| Error::Invalid(format!("The strip does not fit page {}. Draw a smaller strip or choose only the selected page.", i + 1)))?;
                settings.physical_points = None;
                settings.validate().map_err(|_| Error::Invalid(format!("Keep page {} at least 128 × 128 pixels with room for its margins. Draw a smaller strip or reduce its margins in Page Setup.", i + 1)))?;
            }
        }
        pages.size = pages.setup[0].size;
        for i in 0..layout.page_count() {
            if (all_pages || i == page) && layout.content_bottom(i) - layout.content_top(i) < 48. {
                return Err(Error::Invalid(
                    "Keep at least 48 pixels of section height. Draw a smaller strip.".into(),
                ));
            }
        }
        let size = layout.canvas_size();
        crate::validate_dimensions(size[0], size[1])?;
        Ok(layout)
    }

    pub(crate) fn cut_out_pages(
        &mut self,
        axis: CutAxis,
        start: u32,
        end: u32,
        page: usize,
        all_pages: bool,
    ) -> Result<()> {
        use crate::guide_sections::{Section, SectionKind};
        use std::{collections::BTreeMap, sync::Arc};
        let mut layout = self.cut_out_page_layout(axis, start, end, page, all_pages)?;
        let old = self.guide.as_ref().unwrap();
        let owners: BTreeMap<_, _> = self
            .annotations
            .iter()
            .map(|a| {
                let page = old
                    .sections
                    .iter()
                    .find(|s| s.objects.contains(&a.id))
                    .map_or_else(|| old.page_at(a.start), |s| s.page);
                (a.id, page)
            })
            .collect();
        let base = self.base_image()?;
        // Page furniture stays editable and follows the new physical page size.
        let mut source = base.clone();
        for a in self
            .annotations
            .iter()
            .filter(|a| self.object_visible(a.id))
        {
            crate::render_annotation(&mut source, a);
        }
        let size = layout.canvas_size();
        let mut next_base = image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([255; 4]));
        let mut annotations = vec![];
        let mut next_object = self.annotations.iter().map(|a| a.id).max().unwrap_or(0);
        let mut next_section = old.sections.iter().map(|s| s.id).max().unwrap_or(0);
        layout.sections.retain(|s| !all_pages && s.page != page);
        for i in 0..old.page_count() {
            let from = old.page_offset(i).map(|n| n as u32);
            let to = layout.page_offset(i).map(|n| n as u32);
            let page_size = layout.page_size(i, size);
            let affected = all_pages || i == page;
            if affected {
                let pixels = image::RgbaImage::from_fn(page_size[0], page_size[1], |x, y| {
                    let mut p = [x, y];
                    if p[axis.dimension()] >= start {
                        p[axis.dimension()] += end - start;
                    }
                    *source.get_pixel(from[0] + p[0], from[1] + p[1])
                });
                next_object = next_object
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
                next_section = next_section
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("Section identifier overflow".into()))?;
                let image = Arc::new(crate::bitmap::Bitmap::new(Arc::new(
                    RasterImage::from_image(&pixels)?,
                ))?);
                let object = image.annotation(next_object);
                annotations.extend(crate::canvas_expansion::translated(
                    &[object],
                    to.map(|n| n as f32),
                )?);
                layout.sections.push(Section {
                    page: i,
                    id: next_section,
                    kind: SectionKind::Image,
                    height: layout.content_bottom(i) - layout.content_top(i),
                    objects: vec![next_object],
                    marker: None,
                });
            } else {
                let objects: Vec<_> = self
                    .annotations
                    .iter()
                    .filter(|a| owners[&a.id] == i)
                    .cloned()
                    .collect();
                annotations.extend(crate::canvas_expansion::translated(
                    &objects,
                    [to[0] as f32 - from[0] as f32, to[1] as f32 - from[1] as f32],
                )?);
            }
            for y in 0..page_size[1] {
                for x in 0..page_size[0] {
                    next_base.put_pixel(
                        to[0] + x,
                        to[1] + y,
                        if affected {
                            image::Rgba([0; 4])
                        } else {
                            *base.get_pixel(from[0] + x, from[1] + y)
                        },
                    );
                }
            }
        }
        layout.sections.sort_by_key(|s| s.page);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        [self.width, self.height] = size;
        self.raster = Some(Arc::new(RasterImage::from_image(&next_base)?));
        self.annotations = annotations;
        self.guide = Some(layout);
        self.auto_canvas_base = None;
        self.prune_groups();
        self.prune_object_states();
        self.version = crate::MAX_PROJECT_VERSION;
        self.validate()
    }

    pub(crate) fn cut_out(&mut self, axis: CutAxis, start: u32, end: u32) -> Result<()> {
        let size = axis.output_size([self.width, self.height], start, end)?;
        let source = self.render()?;
        let result = image::RgbaImage::from_fn(size[0], size[1], |x, y| {
            let mut p = [x, y];
            if p[axis.dimension()] >= start {
                p[axis.dimension()] += end - start;
            }
            *source.get_pixel(p[0], p[1])
        });
        let raster = std::sync::Arc::new(RasterImage::from_image(&result)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        [self.width, self.height] = size;
        self.raster = Some(raster);
        self.annotations.clear();
        self.auto_canvas_base = None;
        self.version = crate::MAX_PROJECT_VERSION;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};
    use image::{Rgba, RgbaImage};
    #[test]
    fn page_cut_scopes_preserve_other_pages_pixels_history_and_page_setup() {
        use crate::guide_sections::{Layout, PageLayout, PageSetup, Section, SectionKind};
        use std::sync::Arc;
        let mut d = Document::blank(800, 300).unwrap();
        let mut sections = vec![];
        for page in 0..2 {
            let pixels = RgbaImage::from_fn(320, 300, |x, y| {
                Rgba([x as u8, y as u8, if page == 0 { 60 } else { 180 }, 255])
            });
            let bitmap = Arc::new(
                crate::bitmap::Bitmap::new(Arc::new(RasterImage::from_image(&pixels).unwrap()))
                    .unwrap(),
            );
            let a = bitmap.annotation(page as u64 + 1);
            d.annotations.extend(
                crate::canvas_expansion::translated(&[a], [page as f32 * 480., 0.]).unwrap(),
            );
            sections.push(Section {
                page,
                id: page as u64 + 1,
                kind: SectionKind::Image,
                height: 300.,
                objects: vec![page as u64 + 1],
                marker: None,
            });
        }
        d.guide = Some(Layout {
            top: 0.,
            template: None,
            sections,
            pages: Some(PageLayout {
                gap: 160,
                horizontal: true,
                size: [320, 300],
                count: 2,
                auto_overflow: false,
                setup: vec![PageSetup::from_pixels([320, 300]); 2],
            }),
        });
        d.validate().unwrap();
        let original = serde_json::to_vec(&d).unwrap();
        let pixels = d.render().unwrap();
        for axis in [CutAxis::Horizontal, CutAxis::Vertical] {
            for page in 0..2 {
                for all_pages in [false, true] {
                    let mut s = Session::new(d.clone()).unwrap();
                    s.execute(Command::CutOutPages {
                        axis,
                        start: 40,
                        end: 80,
                        page,
                        all_pages,
                    })
                    .unwrap();
                    let layout = s.document.guide.as_ref().unwrap();
                    assert_eq!(layout.page_count(), 2);
                    let actual = s.document.render().unwrap();
                    for i in 0..2 {
                        let offset = layout.page_offset(i);
                        let size = layout.page_size(i, [0; 2]);
                        let affected = all_pages || i == page;
                        assert_eq!(
                            size[axis.dimension()],
                            [320, 300][axis.dimension()] - if affected { 40 } else { 0 }
                        );
                        if !affected {
                            assert!(s.document.annotations.iter().any(|a| a.id == i as u64 + 1));
                        }
                        for y in 0..size[1] {
                            for x in 0..size[0] {
                                let mut from = [x, y];
                                if affected && from[axis.dimension()] >= 40 {
                                    from[axis.dimension()] += 40;
                                }
                                assert_eq!(
                                    actual.get_pixel(offset[0] as u32 + x, offset[1] as u32 + y),
                                    pixels.get_pixel(i as u32 * 480 + from[0], from[1])
                                );
                            }
                        }
                    }
                    let dir = tempfile::tempdir().unwrap();
                    let path = dir.path().join("cut.iclip");
                    s.save(&path).unwrap();
                    assert_eq!(Document::load(&path).unwrap().render().unwrap(), actual);
                    s.execute(Command::Undo).unwrap();
                    assert_eq!(serde_json::to_vec(&s.document).unwrap(), original);
                    assert!(!s.can_undo());
                    assert!(
                        s.execute(Command::CutOutPages {
                            axis,
                            start: 0,
                            end: 280,
                            page,
                            all_pages
                        })
                        .is_err()
                    );
                    assert_eq!(serde_json::to_vec(&s.document).unwrap(), original);
                    assert!(s.can_redo());
                }
            }
        }
    }
    #[test]
    fn strip_seams_and_edges_preserve_exact_rgba_history_persistence_and_original() {
        let image = RgbaImage::from_fn(17, 11, |x, y| {
            Rgba([x as u8 * 13, y as u8 * 21, 89, ((x + y) % 3 * 100) as u8])
        });
        let doc = Document::from_image(image.clone()).unwrap();
        for axis in [CutAxis::Horizontal, CutAxis::Vertical] {
            let length = [17, 11][axis.dimension()];
            for (start, end) in [
                (3, 7),
                (0, 4),
                (length - 4, length),
                (0, length - 1),
                (1, length),
            ] {
                let mut session = Session::new(doc.clone()).unwrap();
                session
                    .execute(Command::CutOut { axis, start, end })
                    .unwrap();
                let actual = session.document.render().unwrap();
                for (x, y, p) in actual.enumerate_pixels() {
                    let src = if axis == CutAxis::Horizontal {
                        [x, if y < start { y } else { y + end - start }]
                    } else {
                        [if x < start { x } else { x + end - start }, y]
                    };
                    assert_eq!(p, image.get_pixel(src[0], src[1]));
                }
                assert_eq!(session.document.source_png, doc.source_png);
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("cut.capture.json");
                session.save(&path).unwrap();
                assert_eq!(Document::load(&path).unwrap().render().unwrap(), actual);
                session.execute(Command::Undo).unwrap();
                assert_eq!(session.document.render().unwrap(), image);
                assert!(!session.can_undo());
                session.execute(Command::Redo).unwrap();
                assert_eq!(session.document.render().unwrap(), actual);
                session.execute(Command::RevertOriginal).unwrap();
                assert_eq!(session.document.render().unwrap(), image);
            }
        }
    }
    #[test]
    fn composite_cut_flattens_once_and_invalid_strips_preserve_redo_and_objects() {
        let doc: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let original = serde_json::to_vec(&doc).unwrap();
        let pixels = doc.render().unwrap();
        let mut session = Session::new(doc).unwrap();
        let command: Command = serde_json::from_str(
            r#"{"command":"cut_out","axis":"horizontal","start":100,"end":180}"#,
        )
        .unwrap();
        session.execute(command).unwrap();
        assert!(session.document.annotations.is_empty());
        let actual = session.document.render().unwrap();
        assert_eq!(actual.dimensions(), (640, 320));
        for (x, y, p) in actual.enumerate_pixels() {
            assert_eq!(p, pixels.get_pixel(x, if y < 100 { y } else { y + 80 }));
        }
        session.execute(Command::Undo).unwrap();
        assert!(!session.can_undo());
        for (axis, start, end) in [
            (CutAxis::Horizontal, 5, 5),
            (CutAxis::Horizontal, 6, 5),
            (CutAxis::Horizontal, 0, 400),
            (CutAxis::Vertical, 0, 641),
            (CutAxis::Vertical, u32::MAX, 0),
            (CutAxis::Horizontal, 0, u32::MAX),
        ] {
            assert!(
                session
                    .execute(Command::CutOut { axis, start, end })
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
            assert!(session.can_redo());
        }
    }
}
