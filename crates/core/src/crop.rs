//! Editable crop: raster deletion is isolated to raster sources. Native objects
//! remain whole on the pasteboard and output clipping never mutates their paths.
use crate::{Document, Error, RasterImage, Result, validate_dimensions};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CropMode {
    Delete,
    #[default]
    Preserve,
}

impl Document {
    pub(crate) fn crop_image(
        &mut self,
        rect: [u32; 4],
        mode: CropMode,
        page: usize,
    ) -> Result<bool> {
        let [x, y, width, height] = rect;
        validate_dimensions(width, height)?;
        let mut old = self.guide.clone();
        if let Some(g) = &mut old {
            g.pages.get_or_insert(crate::guide_sections::PageLayout {
                gap: crate::guide_sections::PAGE_GAP,
                setup: vec![],
                horizontal: true,
                size: [self.width, self.height],
                count: 1,
                auto_overflow: false,
            });
        }
        let count = old.as_ref().map_or(1, |g| g.page_count());
        if page >= count {
            return Err(Error::Invalid("Choose an existing page to crop.".into()));
        }
        let size = old.as_ref().map_or([self.width, self.height], |g| {
            g.page_size(page, [self.width, self.height])
        });
        if x.checked_add(width).is_none_or(|n| n > size[0])
            || y.checked_add(height).is_none_or(|n| n > size[1])
        {
            return Err(Error::Invalid(
                "Drag the crop so it stays inside this page.".into(),
            ));
        }
        if rect == [0, 0, size[0], size[1]] {
            return Ok(false);
        }
        let base = self.base_image()?;
        let mut owners: BTreeMap<_, _> = self
            .annotations
            .iter()
            .map(|a| (a.id, self.object_page(a)))
            .collect();
        let mut layout = old.clone();
        if let Some(g) = &mut layout {
            let p = g.pages.as_mut().unwrap();
            if p.setup.is_empty() {
                p.setup = vec![crate::guide_sections::PageSetup::from_pixels(p.size); p.count];
            }
            let s = &mut p.setup[page];
            s.size = [width, height];
            s.physical_points = None;
            s.cropped = true;
            s.content_shift[0] += x as f32;
            s.content_shift[1] += y as f32;
            s.validate().map_err(|_|Error::Invalid("Keep the cropped page at least 128 × 128 pixels with room for its margins. Draw a larger crop or reduce margins in Page Setup.".into()))?;
            p.size = p.setup[0].size;
        }
        let dimensions = layout.as_ref().map_or([width, height], |g| g.canvas_size());
        validate_dimensions(dimensions[0], dimensions[1])?;
        let mut next_base = image::RgbaImage::new(dimensions[0], dimensions[1]);
        let mut next = self.annotations.clone();
        let mut id = next.iter().map(|a| a.id).max().unwrap_or(0);
        for i in 0..count {
            let origin = old.as_ref().map_or([0.; 2], |g| g.page_offset(i));
            let to = layout.as_ref().map_or([0.; 2], |g| g.page_offset(i));
            let old_size = old.as_ref().map_or([self.width, self.height], |g| {
                g.page_size(i, [self.width, self.height])
            });
            let crop = if i == page {
                rect
            } else {
                [0, 0, old_size[0], old_size[1]]
            };
            let source = image::imageops::crop_imm(
                &base,
                origin[0] as u32,
                origin[1] as u32,
                old_size[0],
                old_size[1],
            )
            .to_image();
            let preserve = (layout.is_some() || (i == page && mode == CropMode::Preserve))
                && source.pixels().any(|p| p != source.get_pixel(0, 0));
            if preserve {
                id = id
                    .checked_add(1)
                    .ok_or_else(|| Error::Invalid("Object identifier overflow".into()))?;
                let b = Arc::new(crate::bitmap::Bitmap::new(Arc::new(
                    RasterImage::from_image(&source)?,
                ))?);
                let a = crate::canvas_expansion::translated(&[b.annotation(id)], origin)?.remove(0);
                next.insert(0, a);
                owners.insert(id, i);
            } else {
                let image = image::imageops::crop_imm(&source, crop[0], crop[1], crop[2], crop[3])
                    .to_image();
                image::imageops::replace(&mut next_base, &image, to[0] as i64, to[1] as i64);
            }
            let world = [
                origin[0] + crop[0] as f32,
                origin[1] + crop[1] as f32,
                crop[2] as f32,
                crop[3] as f32,
            ];
            let delta = [to[0] - world[0], to[1] - world[1]];
            for a in next.iter_mut().filter(|a| owners[&a.id] == i) {
                if i == page
                    && mode == CropMode::Delete
                    && let Some(b) = a.image.clone()
                    && b.vector().is_none()
                    && !b.is_placeholder()
                {
                    a.image = Some(Arc::new(b.trimmed_to(a, world)?));
                }
                *a = crate::canvas_expansion::translated(std::slice::from_ref(a), delta)?.remove(0);
            }
        }
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        [self.width, self.height] = dimensions;
        self.raster = Some(Arc::new(RasterImage::from_image(&next_base)?));
        self.annotations = next;
        self.guide = layout;
        self.auto_canvas_base = None;
        self.canvas_behavior.allow_overflow = true;
        if self.guide.is_some() {
            self.canvas_behavior.page_owners = owners;
        }
        self.hold_outside_objects();
        self.version = crate::MAX_PROJECT_VERSION;
        self.validate()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
