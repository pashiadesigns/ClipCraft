//! Local reusable layouts use the same atomic project store as the Library.
use super::*;
use crate::history::{Entry, History, Origin};
use std::path::Path;

pub struct Library(pub History);
impl Library {
    pub fn open(root: &Path) -> Result<Self> {
        Ok(Self(History::open(root)?))
    }
    pub fn save(&mut self, source: &Document, name: &str, empty_images: bool) -> Result<Entry> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "Enter a template name of 1–120 characters.".into(),
            ));
        }
        let mut d = source.clone();
        if d.height < 48 {
            return Err(Error::Invalid(
                "A template must be at least 48 pixels high.".into(),
            ));
        }
        if d.video.is_some() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let folder = self.0.root().join("video-templates");
            std::fs::create_dir_all(&folder)?;
            let file = folder.join(format!("Template-{stamp}.vclip"));
            d.save(&file)?;
            d = Document::load(&file)?;
            let entry = self.0.create(&d, name, Origin::Project)?;
            return self.0.edit_metadata(&entry, name, &[]);
        }
        if empty_images {
            for a in &mut d.annotations {
                if a.image
                    .as_ref()
                    .is_some_and(|b| !b.is_icon() && !b.is_cursor())
                {
                    let placeholder = placeholder()?;
                    let placeholder = if a.is_shape() {
                        Arc::new(placeholder.with_frame_opacity(a.image_opacity()))
                    } else {
                        placeholder
                    };
                    a.image = image_in_frame(
                        placeholder,
                        a.id,
                        [
                            a.start[0],
                            a.start[1],
                            a.end[0] - a.start[0],
                            a.end[1] - a.start[1],
                        ],
                        Scale::Fill,
                    )?
                    .image;
                }
            }
        }
        if let Some(guide) = &mut d.guide {
            guide.template = Some("local".into());
        } else {
            d.guide = Some(Layout {
                pages: None,
                template: Some("local".into()),
                top: 0.,
                sections: vec![Section {
                    page: 0,
                    id: 1,
                    kind: SectionKind::Image,
                    height: d.height as f32,
                    objects: d.annotations.iter().map(|a| a.id).collect(),
                    marker: None,
                }],
            });
        }
        d.promote_features();
        d.validate()?;
        let entry = self.0.create(&d, name, Origin::Project)?;
        self.0.edit_metadata(&entry, name, &[])
    }
}
pub fn slots(document: &Document) -> usize {
    document
        .annotations
        .iter()
        .filter(|a| a.image.as_ref().is_some_and(|b| b.is_placeholder()))
        .count()
}
pub fn instantiate(
    source: &Document,
    title: &str,
    images: &[Arc<Bitmap>],
    scale: Scale,
) -> Result<Document> {
    if images.len() > slots(source) {
        return Err(Error::Invalid(
            "Choose a template with enough empty image frames.".into(),
        ));
    }
    let mut d = source.clone();
    let mut images = images.iter();
    for a in &mut d.annotations {
        if a.image.as_ref().is_some_and(|b| b.is_placeholder())
            && let Some(b) = images.next()
        {
            // Saved frame geometry belongs to the layout; only its source/crop changes.
            let bitmap = if a.is_shape() {
                Arc::new(b.with_frame_opacity(a.image_opacity()))
            } else {
                b.clone()
            };
            let fitted = image_in_frame(
                bitmap,
                a.id,
                [
                    a.start[0],
                    a.start[1],
                    a.end[0] - a.start[0],
                    a.end[1] - a.start[1],
                ],
                scale,
            )?;
            if a.kind == Kind::Polygon {
                let (lo, hi) = a.body_bounds();
                a.points = a
                    .points
                    .iter()
                    .map(|p| {
                        [0, 1].map(|i| {
                            fitted.start[i]
                                + (p[i] - lo[i]) / (hi[i] - lo[i])
                                    * (fitted.end[i] - fitted.start[i])
                        })
                    })
                    .collect();
            }
            a.image = fitted.image;
            a.start = fitted.start;
            a.end = fitted.end;
        }
    }
    if let Some(header) = d
        .guide
        .as_ref()
        .and_then(|g| g.sections.first())
        .filter(|s| s.kind == SectionKind::Header)
        && let Some(a) = d
            .annotations
            .iter_mut()
            .find(|a| header.objects.contains(&a.id) && a.kind == Kind::Text)
        && let Some(text) = &mut a.text
        && !title.trim().is_empty()
    {
        let old = text.content.clone();
        text.content = title.into();
        crate::text::rich::rebase(text, &old, None);
    }
    d.promote_features();
    d.validate()?;
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_paged_polygon_frames_keep_mask_opacity_and_fit_geometry() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Library::open(dir.path()).unwrap();
        let mut source = build("two-steps", "Frame layout", &[], Scale::Fill).unwrap();
        for a in &mut source.annotations {
            if a.is_image_frame() {
                a.kind = Kind::Polygon;
                a.corner_radius = 0.;
                a.points = crate::polygon::outline(a.start, a.end, 6);
                a.image = Some(Arc::new(a.image.as_ref().unwrap().with_frame_opacity(170)));
            }
        }
        source.promote_features();
        source = source
            .edit_guide(&crate::guide_sections::Action::PageSize {
                width: 900,
                height: 550,
                auto_overflow: true,
            })
            .unwrap();
        let entry = store.save(&source, "Paged mask layout", true).unwrap();
        let saved = store.0.load(&entry.id).unwrap();
        let bitmap = Arc::new(
            Bitmap::new(Arc::new(
                crate::RasterImage::from_image(&image::RgbaImage::from_pixel(
                    80,
                    180,
                    image::Rgba([50, 150, 200, 255]),
                ))
                .unwrap(),
            ))
            .unwrap(),
        );
        let document = instantiate(&saved, "Filled pages", &[bitmap], Scale::Fit).unwrap();
        let a = document
            .annotations
            .iter()
            .find(|a| a.is_image_frame() && !a.image.as_ref().unwrap().is_placeholder())
            .unwrap();
        assert_eq!(a.kind, Kind::Polygon);
        assert_eq!(a.image_opacity(), 170);
        assert_eq!(a.body_bounds(), (a.start, a.end));
        assert_eq!(document.guide.as_ref().unwrap().page_count(), 2);
        document.validate().unwrap();
    }
    #[test]
    fn saved_layout_is_independent_portable_and_reusable() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Library::open(dir.path()).unwrap();
        let source = build("two-steps", "Original", &[], Scale::Fill).unwrap();
        let before = serde_json::to_vec(&source).unwrap();
        let entry = store.save(&source, "Team checklist", true).unwrap();
        assert_eq!(entry.label, "Team checklist");
        assert_eq!(serde_json::to_vec(&source).unwrap(), before);
        drop(store);
        let reopened = Library::open(dir.path()).unwrap();
        let saved = reopened.0.load(&entry.id).unwrap();
        assert_eq!(slots(&saved), 2);
        let image = Arc::new(
            Bitmap::new(Arc::new(
                crate::RasterImage::from_image(&image::RgbaImage::from_pixel(
                    40,
                    20,
                    image::Rgba([10, 80, 130, 255]),
                ))
                .unwrap(),
            ))
            .unwrap(),
        );
        let created = instantiate(
            &saved,
            "New guide",
            std::slice::from_ref(&image),
            Scale::Fill,
        )
        .unwrap();
        assert_eq!(slots(&created), 1);
        if let Ok(root) = std::env::var("CLIPCRAFT_TEMPLATE_FIXTURES") {
            let root = Path::new(&root);
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(
                root.join("reusable-empty.capture.json"),
                serde_json::to_vec_pretty(&saved).unwrap(),
            )
            .unwrap();
            std::fs::write(
                root.join("reusable-filled.capture.json"),
                serde_json::to_vec_pretty(&created).unwrap(),
            )
            .unwrap();
        }
        assert_eq!(slots(&reopened.0.load(&entry.id).unwrap()), 2);
        assert_eq!(
            created.guide.as_ref().unwrap().template.as_deref(),
            Some("local")
        );
        let mut session = crate::Session::new(created.clone()).unwrap();
        let id = created
            .annotations
            .iter()
            .find(|a| a.image.as_ref().is_some_and(|b| !b.is_placeholder()))
            .unwrap()
            .id;
        session.execute(crate::Command::Remove { id }).unwrap();
        session.execute(crate::Command::Undo).unwrap();
        assert_eq!(
            serde_json::to_vec(&session.document).unwrap(),
            serde_json::to_vec(&created).unwrap()
        );
        assert!(
            instantiate(
                &saved,
                "Too many",
                &[image.clone(), image.clone(), image],
                Scale::Fit
            )
            .is_err()
        );
        let mut invalid = saved;
        invalid.version = 24;
        assert!(invalid.validate().is_err());
    }
}
