//! Page flattening retains the guide, its physical setup and independent pages.
use crate::{Document, Error, RasterImage, Result};
use std::sync::Arc;

impl Document {
    pub(crate) fn flatten_pages(&self, selected: Option<usize>) -> Result<Self> {
        let guide = self
            .guide
            .as_ref()
            .ok_or_else(|| Error::Invalid("Select a guide page first.".into()))?;
        if selected.is_some_and(|p| p >= guide.page_count()) {
            return Err(Error::Invalid("Select an existing page.".into()));
        }
        let mut next = self.clone();
        let mut pixels = self.base_image()?;
        for page in 0..guide.page_count() {
            if selected.is_some_and(|p| p != page) {
                continue;
            }
            let [x, y, w, h] = self.page_rect(page).map(|n| n as u32);
            let mut content = image::imageops::crop_imm(&pixels, x, y, w, h).to_image();
            let objects: Vec<_> = self
                .annotations
                .iter()
                .filter(|a| self.object_page(a) == page && self.object_visible(a.id))
                .cloned()
                .collect();
            for a in crate::canvas_expansion::translated(&objects, [-(x as f32), -(y as f32)])? {
                crate::render_annotation(&mut content, &a);
            }
            image::imageops::replace(&mut pixels, &content, x as i64, y as i64);
        }
        next.annotations
            .retain(|a| selected.is_some_and(|page| self.object_page(a) != page));
        // Empty sections remain usable insertion targets. Header/footer furniture
        // is generated separately, so it must not be baked twice into the raster.
        for section in &mut next.guide.as_mut().unwrap().sections {
            if selected.is_none_or(|p| p == section.page) {
                section.objects.clear();
                section.marker = None;
            }
        }
        next.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        next.raster = Some(Arc::new(RasterImage::from_image(&pixels)?));
        next.auto_canvas_base = None;
        next.prune_groups();
        next.prune_object_states();
        next.prune_canvas_behavior();
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Command, Session, cut_out::CutAxis, guide_sections::Action, templates};
    #[test]
    fn mixed_page_cut_flatten_undo_restores_every_object_and_pixel() {
        let d = templates::build("three-steps", "Guide", &[], templates::Scale::Fit).unwrap();
        let mut s = Session::new(d).unwrap();
        s.execute(Command::Guide {
            action: Action::AddPage {
                page: 0,
                copy: true,
            },
        })
        .unwrap();
        let original = serde_json::to_vec(&s.document).unwrap();
        let original_pixels = s.document.render().unwrap();
        if let Ok(root) = std::env::var("CLIPDESIGNER_NEXT_FIXTURES") {
            std::fs::create_dir_all(&root).unwrap();
            s.save(&std::path::Path::new(&root).join("undo-guide.capture.json"))
                .unwrap();
        }
        s.execute(Command::CutOutPages {
            axis: CutAxis::Horizontal,
            edge: Default::default(),
            start: 180,
            end: 200,
            page: 0,
            all_pages: false,
        })
        .unwrap();
        let cut = serde_json::to_vec(&s.document).unwrap();
        let cut_pixels = s.document.render().unwrap();
        let untouched: Vec<_> = s
            .document
            .annotations
            .iter()
            .filter(|a| s.document.object_page(a) == 1)
            .cloned()
            .collect();
        s.execute(Command::FlattenPage { page: 0 }).unwrap();
        if let Ok(root) = std::env::var("CLIPDESIGNER_NEXT_FIXTURES") {
            s.save(&std::path::Path::new(&root).join("flattened-page.capture.json"))
                .unwrap();
        }
        assert_eq!(s.document.guide.as_ref().unwrap().page_count(), 2);
        assert_eq!(s.document.render().unwrap(), cut_pixels);
        assert!(untouched.iter().all(|a| s.document.annotations.contains(a)));
        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), cut);
        assert_eq!(s.document.render().unwrap(), cut_pixels);
        s.execute(Command::Undo).unwrap();
        assert_eq!(serde_json::to_vec(&s.document).unwrap(), original);
        assert_eq!(s.document.render().unwrap(), original_pixels);
        s.execute(Command::Redo).unwrap();
        s.execute(Command::Redo).unwrap();
        assert_eq!(s.document.render().unwrap(), cut_pixels);
        s.execute(Command::FlattenAll).unwrap();
        assert_eq!(s.document.guide.as_ref().unwrap().page_count(), 2);
        assert_eq!(s.document.render().unwrap(), cut_pixels);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pages.iclip");
        s.save(&path).unwrap();
        assert_eq!(
            crate::Document::load(&path).unwrap().render().unwrap(),
            cut_pixels
        );
    }
}
