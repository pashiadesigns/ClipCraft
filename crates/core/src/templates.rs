//! Original editable guide layouts; all images retain their complete source.
pub mod library;
use crate::{
    Annotation, Document, Error, Kind, Result,
    bitmap::Bitmap,
    guide_sections::{Layout, Section, SectionKind},
};
use std::sync::Arc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Fit,
    Fill,
}
#[derive(Clone, Copy)]
pub struct Template {
    pub id: &'static str,
    pub label: &'static str,
    pub category: &'static str,
    pub slots: usize,
    pub columns: usize,
    pub width: u32,
    pub row_height: f32,
    pub side_text: bool,
    pub timeline: bool,
}
pub const ALL: &[Template] = &[
    Template {
        id: "two-steps",
        label: "Two steps · Portrait",
        category: "Steps",
        slots: 2,
        columns: 1,
        width: 900,
        row_height: 370.,
        side_text: false,
        timeline: false,
    },
    Template {
        id: "three-steps",
        label: "Three steps · Portrait",
        category: "Steps",
        slots: 3,
        columns: 1,
        width: 900,
        row_height: 270.,
        side_text: true,
        timeline: false,
    },
    Template {
        id: "four-steps",
        label: "Four steps · Grid",
        category: "Steps",
        slots: 4,
        columns: 2,
        width: 900,
        row_height: 320.,
        side_text: false,
        timeline: false,
    },
    Template {
        id: "comparison",
        label: "Before / After · Landscape",
        category: "Comparison",
        slots: 2,
        columns: 2,
        width: 1200,
        row_height: 420.,
        side_text: false,
        timeline: false,
    },
    Template {
        id: "overview",
        label: "Overview · Landscape",
        category: "Overview",
        slots: 1,
        columns: 1,
        width: 1200,
        row_height: 440.,
        side_text: true,
        timeline: false,
    },
    Template {
        id: "timeline",
        label: "Timeline · Portrait",
        category: "Timeline",
        slots: 5,
        columns: 1,
        width: 900,
        row_height: 240.,
        side_text: true,
        timeline: true,
    },
];
pub fn find(id: &str) -> Option<&'static Template> {
    ALL.iter().find(|t| t.id == id)
}
pub fn placeholder() -> Result<Arc<Bitmap>> {
    let image = image::RgbaImage::from_fn(240, 150, |x, y| {
        let mountain = (40..205).contains(&x)
            && y > 115_u32.saturating_sub((x as i32 - 110).unsigned_abs() / 2)
            && y < 125;
        let sun = (x as i32 - 179).pow(2) + (y as i32 - 43).pow(2) < 12 * 12;
        image::Rgba(if mountain || sun {
            [160, 174, 180, 255]
        } else {
            [232, 238, 241, 255]
        })
    });
    Ok(Arc::new(
        Bitmap::new(Arc::new(crate::RasterImage::from_image(&image)?))?.into_placeholder(),
    ))
}
/// Reusable for creation and replacing an existing frame; crop stays nondestructive.
pub fn image_in_frame(
    bitmap: Arc<Bitmap>,
    id: u64,
    rect: [f32; 4],
    scale: Scale,
) -> Result<Annotation> {
    let [x, y, w, h] = rect;
    if !rect.into_iter().all(f32::is_finite) || w <= 0. || h <= 0. {
        return Err(Error::Invalid("Invalid image frame".into()));
    }
    let [iw, ih] = bitmap.size().map(|v| v as f32);
    let mut a = bitmap.clone().annotation(id);
    match scale {
        Scale::Fit => {
            let factor = (w / iw).min(h / ih);
            let [dw, dh] = [iw * factor, ih * factor];
            a.start = [x + (w - dw) * 0.5, y + (h - dh) * 0.5];
            a.end = [a.start[0] + dw, a.start[1] + dh];
        }
        Scale::Fill => {
            let ratio = (w / h) / (iw / ih);
            let [cw, ch] = if ratio < 1. {
                [ratio, 1.]
            } else {
                [1., 1. / ratio]
            };
            a.image = Some(Arc::new(bitmap.with_view(Some(
                crate::image_frame::ImageView {
                    rect: [(1. - cw) * 0.5, (1. - ch) * 0.5, cw, ch],
                },
            ))?));
            a.start = [x, y];
            a.end = [x + w, y + h];
        }
    }
    a.corner_radius = 8.;
    Ok(a)
}
pub(crate) fn text(id: u64, rect: [f32; 4], content: &str, size: f32, bold: bool) -> Annotation {
    Annotation {
        id,
        kind: Kind::Text,
        start: [rect[0], rect[1]],
        end: [rect[0] + rect[2], rect[1] + rect[3]],
        color: [40, 47, 52, 255],
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
            content: content.into(),
            size_points: size,
            bold,
            horizontal: crate::text::HorizontalAlignment::Left,
            vertical: crate::text::VerticalAlignment::Top,
            ..Default::default()
        }),
    }
}
pub fn build(id: &str, title: &str, images: &[Arc<Bitmap>], scale: Scale) -> Result<Document> {
    let t = find(id).ok_or_else(|| Error::Invalid("Unknown image template".into()))?;
    if images.len() > t.slots || title.chars().count() > 120 {
        return Err(Error::Invalid(format!(
            "This template accepts at most {} images and a title of 120 characters.",
            t.slots
        )));
    }
    let rows = t.slots.div_ceil(t.columns);
    let height = 144. + rows as f32 * t.row_height + 64.;
    let mut d = Document::canvas(t.width, height.ceil() as u32, [255; 4])?;
    let mut sections = vec![];
    let mut next = 1;
    let mut add = |d: &mut Document, mut a: Annotation| {
        a.id = next;
        next += 1;
        let id = a.id;
        d.annotations.push(a);
        id
    };
    let header = vec![
        add(
            &mut d,
            text(
                0,
                [32., 25., t.width as f32 - 64., 58.],
                if title.trim().is_empty() {
                    "Untitled guide"
                } else {
                    title
                },
                32.,
                true,
            ),
        ),
        add(
            &mut d,
            text(
                0,
                [32., 92., t.width as f32 - 64., 35.],
                "Add a subtitle or introduction",
                17.,
                false,
            ),
        ),
    ];
    sections.push(Section {
        page: 0,
        id: 1,
        kind: SectionKind::Header,
        height: 144.,
        objects: header,
        marker: None,
    });
    let placeholder = placeholder()?;
    for row in 0..rows {
        let top = 144. + row as f32 * t.row_height;
        let mut objects = vec![];
        let mut marker = None;
        for col in 0..t.columns {
            let index = row * t.columns + col;
            if index >= t.slots {
                break;
            }
            let cw = (t.width as f32 - 64. - (t.columns - 1) as f32 * 24.) / t.columns as f32;
            let left = 32. + col as f32 * (cw + 24.);
            let side = t.side_text;
            let image_w = if side { cw * 0.58 } else { cw };
            let image_h = if side {
                t.row_height - 40.
            } else {
                t.row_height - 90.
            };
            let image_left = if t.timeline && row % 2 == 1 {
                t.width as f32 - 32. - image_w
            } else {
                left
            };
            let bitmap = images.get(index).unwrap_or(&placeholder).clone();
            objects.push(add(
                &mut d,
                image_in_frame(bitmap, 0, [image_left, top + 16., image_w, image_h], scale)?,
            ));
            let caption_x = if side {
                if t.timeline && row % 2 == 1 {
                    left
                } else {
                    left + image_w + 24.
                }
            } else {
                left
            };
            let caption_y = if side { top + 30. } else { top + image_h + 30. };
            let caption_w = if side { cw - image_w - 24. } else { cw };
            let label = if t.id == "comparison" {
                if index == 0 {
                    "Before".into()
                } else {
                    "After".into()
                }
            } else {
                format!("Step {}", index + 1)
            };
            let marker_id = if t.category == "Steps" || t.timeline {
                let a = crate::steps::marker(
                    1,
                    [caption_x + 14., caption_y + 14.],
                    28,
                    [24, 145, 193, 255],
                    ((index + 1) as u32, false),
                    [d.width, d.height],
                )?;
                Some(add(&mut d, a))
            } else {
                None
            };
            if let Some(id) = marker_id {
                objects.push(id);
                if t.columns == 1 {
                    marker = Some(id);
                }
            }
            let offset = if marker_id.is_some() { 38. } else { 0. };
            objects.push(add(
                &mut d,
                text(
                    0,
                    [caption_x + offset, caption_y, caption_w - offset, 32.],
                    if t.id == "overview" {
                        "Key information"
                    } else {
                        &label
                    },
                    20.,
                    true,
                ),
            ));
            if side {
                objects.push(add(
                    &mut d,
                    text(
                        0,
                        [caption_x, caption_y + 46., caption_w, t.row_height - 110.],
                        "Describe the action or key point.",
                        17.,
                        false,
                    ),
                ));
            }
        }
        sections.push(Section {
            page: 0,
            id: row as u64 + 2,
            kind: SectionKind::Image,
            height: t.row_height,
            objects,
            marker,
        });
    }
    let footer = add(
        &mut d,
        text(
            0,
            [32., height - 44., t.width as f32 - 64., 28.],
            "Created with ClipCraft",
            13.,
            false,
        ),
    );
    sections.push(Section {
        page: 0,
        id: rows as u64 + 2,
        kind: SectionKind::Text,
        height: 64.,
        objects: vec![footer],
        marker: None,
    });
    d.guide = Some(Layout {
        pages: None,
        template: Some(t.id.into()),
        top: 0.,
        sections,
    });
    d.promote_features();
    d.validate()?;
    Ok(d)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_keep_editable_sections_sources_and_exact_roundtrips() {
        for t in ALL {
            for scale in [Scale::Fit, Scale::Fill] {
                let source = Arc::new(
                    Bitmap::new(Arc::new(
                        crate::RasterImage::from_image(&image::RgbaImage::from_pixel(
                            640,
                            180,
                            image::Rgba([38, 110, 180, 255]),
                        ))
                        .unwrap(),
                    ))
                    .unwrap(),
                );
                let d = build(
                    t.id,
                    "Deployment guide",
                    std::slice::from_ref(&source),
                    scale,
                )
                .unwrap();
                assert_eq!(d.version, if 1 < t.slots { 25 } else { 24 });
                assert_eq!(
                    d.annotations
                        .iter()
                        .filter(|a| a.kind == Kind::Image)
                        .count(),
                    t.slots
                );
                let saved = serde_json::to_vec(&d).unwrap();
                let copy: Document = serde_json::from_slice(&saved).unwrap();
                copy.validate().unwrap();
                assert_eq!(d.render().unwrap(), copy.render().unwrap());
                let first = d
                    .annotations
                    .iter()
                    .find(|a| a.kind == Kind::Image)
                    .unwrap();
                assert_eq!(first.image.as_ref().unwrap().size(), source.size());
                let mut s = crate::Session::new(d.clone()).unwrap();
                s.execute(crate::Command::Guide {
                    action: crate::guide_sections::Action::Duplicate { id: 2 },
                })
                .unwrap();
                s.execute(crate::Command::Undo).unwrap();
                assert_eq!(serde_json::to_vec(&s.document).unwrap(), saved);
                let root = tempfile::tempdir().unwrap();
                let mut h = crate::history::History::open(root.path()).unwrap();
                let e = h
                    .create(&d, "Template", crate::history::Origin::Canvas)
                    .unwrap();
                assert_eq!(e.kind, crate::history::MediaKind::Template);
                let mut reopened = crate::history::History::open(root.path()).unwrap();
                assert_eq!(
                    reopened.entries()[0].kind,
                    crate::history::MediaKind::Template
                );
                let duplicated = reopened.duplicate(&e).unwrap();
                assert_eq!(duplicated.kind, crate::history::MediaKind::Template);
                let mut legacy = d.clone();
                legacy.version = 23;
                assert!(legacy.validate().is_err());
                if scale == Scale::Fill
                    && let Ok(root) = std::env::var("CLIPCRAFT_TEMPLATE_FIXTURES")
                {
                    std::fs::create_dir_all(&root).unwrap();
                    d.save(
                        &std::path::Path::new(&root)
                            .join(format!("template-{}.capture.json", t.id)),
                    )
                    .unwrap();
                }
            }
        }
        assert!(build("unknown", "", &[], Scale::Fit).is_err());
        assert!(build("overview", "", &vec![placeholder().unwrap(); 2], Scale::Fit).is_err());
    }
}
