//! Straight strip removal, with exact retained RGBA and original-image retention.
use crate::{Document, Error, RasterImage, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutAxis {
    Horizontal,
    Vertical,
}

/// The look of a joined seam, also used by image edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeShape {
    #[default]
    Straight,
    Torn,
    Wave,
    Zigzag,
}
impl EdgeShape {
    pub const ALL: [Self; 4] = [Self::Straight, Self::Torn, Self::Wave, Self::Zigzag];
    pub fn label(self) -> &'static str {
        match self {
            Self::Straight => "Straight",
            Self::Torn => "Torn",
            Self::Wave => "Wave",
            Self::Zigzag => "Zigzag",
        }
    }
    /// How far the edge swings at this distance along it, from -1 to 1.
    pub fn offset(self, along: f32) -> f32 {
        match self {
            Self::Straight => 0.,
            Self::Wave => (along * std::f32::consts::TAU / 48.).sin(),
            Self::Zigzag => {
                let t = (along / 24.).rem_euclid(1.);
                4. * (t - 0.5).abs() - 1.
            }
            Self::Torn => {
                0.5 * (along * 0.13).sin()
                    + 0.3 * (along * 0.37 + 1.).sin()
                    + 0.2 * (along * 0.91 + 2.).sin()
            }
        }
    }
}

/// Where a cut's two sides meet.
#[derive(Clone, Copy, Debug)]
pub struct Seam {
    dimension: usize,
    start: f32,
    removed: u32,
    swing: f32,
    edge: EdgeShape,
}
impl Seam {
    /// `size` is the page before the cut. The seam may only swing into pixels
    /// that exist on both sides.
    pub fn new(axis: CutAxis, start: u32, end: u32, edge: EdgeShape, size: [u32; 2]) -> Self {
        let dimension = axis.dimension();
        let swing = 8
            .min((end - start) / 2)
            .min(start)
            .min(size[dimension] - end);
        Self {
            dimension,
            start: start as f32,
            removed: end - start,
            swing: swing as f32,
            edge,
        }
    }
    /// Where the seam sits, measured along the cut direction.
    pub fn at(&self, along: f32) -> f32 {
        self.start + self.edge.offset(along) * self.swing
    }
    /// The source pixel for a pixel of the joined image, and how strongly the
    /// seam line darkens it. A straight cut has no line.
    pub fn map(&self, p: [u32; 2]) -> ([u32; 2], f32) {
        let along = p[1 - self.dimension] as f32;
        let at = self.at(along);
        let middle = p[self.dimension] as f32 + 0.5;
        let mut source = p;
        if middle >= at {
            source[self.dimension] += self.removed;
        }
        let distance = (middle - at).abs();
        let shade = if self.edge != EdgeShape::Straight && self.swing > 0. && distance < 1. {
            0.35 * (1. - distance)
        } else {
            0.
        };
        (source, shade)
    }
}
fn shaded(mut pixel: image::Rgba<u8>, shade: f32) -> image::Rgba<u8> {
    if shade > 0. {
        for c in 0..3 {
            pixel[c] = (pixel[c] as f32 * (1. - shade)).round() as u8;
        }
    }
    pixel
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
        edge: EdgeShape,
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
                let old_size = old.page_size(i, [self.width, self.height]);
                let seam = Seam::new(axis, start, end, edge, old_size);
                let pixels = image::RgbaImage::from_fn(page_size[0], page_size[1], |x, y| {
                    let (p, shade) = seam.map([x, y]);
                    shaded(*source.get_pixel(from[0] + p[0], from[1] + p[1]), shade)
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

    pub(crate) fn cut_out(
        &mut self,
        axis: CutAxis,
        start: u32,
        end: u32,
        edge: EdgeShape,
    ) -> Result<()> {
        let size = axis.output_size([self.width, self.height], start, end)?;
        let source = self.render()?;
        let seam = Seam::new(axis, start, end, edge, [self.width, self.height]);
        let result = image::RgbaImage::from_fn(size[0], size[1], |x, y| {
            let (p, shade) = seam.map([x, y]);
            shaded(*source.get_pixel(p[0], p[1]), shade)
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
                        edge: Default::default(),
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
                            edge: Default::default(),
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
    fn shaped_seams_stay_inside_the_image_and_a_cut_at_the_border_stays_straight() {
        let image = RgbaImage::from_fn(200, 120, |x, y| Rgba([x as u8, y as u8, 40, 255]));
        let doc = Document::from_image(image.clone()).unwrap();
        for edge in EdgeShape::ALL {
            for axis in [CutAxis::Horizontal, CutAxis::Vertical] {
                for (start, end) in [(30, 60), (40, 41), (0, 30)] {
                    let mut s = Session::new(doc.clone()).unwrap();
                    s.execute(Command::CutOut {
                        axis,
                        start,
                        end,
                        edge,
                    })
                    .unwrap();
                    let expected = [
                        200 - if axis == CutAxis::Vertical {
                            end - start
                        } else {
                            0
                        },
                        120 - if axis == CutAxis::Horizontal {
                            end - start
                        } else {
                            0
                        },
                    ];
                    assert_eq!([s.document.width, s.document.height], expected);
                    if start == 0 {
                        let i = axis.dimension();
                        let out = s.document.render().unwrap();
                        let mut p = [0, 0];
                        p[1 - i] = 7;
                        let mut q = p;
                        q[i] = end;
                        assert_eq!(out.get_pixel(p[0], p[1]), image.get_pixel(q[0], q[1]));
                    }
                    s.execute(Command::Undo).unwrap();
                    assert_eq!(s.document.render().unwrap(), image);
                }
            }
        }
        // The seam moves with the shape, so the two sides meet unevenly.
        let seam = Seam::new(CutAxis::Horizontal, 30, 60, EdgeShape::Wave, [200, 120]);
        let heights: Vec<f32> = (0..48).map(|x| seam.at(x as f32)).collect();
        assert!(heights.iter().any(|h| *h > 36.) && heights.iter().any(|h| *h < 24.));
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
                    .execute(Command::CutOut {
                        axis,
                        start,
                        end,
                        edge: Default::default(),
                    })
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
                    .execute(Command::CutOut {
                        axis,
                        start,
                        end,
                        edge: Default::default()
                    })
                    .is_err()
            );
            assert_eq!(serde_json::to_vec(&session.document).unwrap(), original);
            assert!(session.can_redo());
        }
    }
}
