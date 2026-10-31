//! Editable Simplify overlays. Original pixels are never replaced.
mod analysis;
mod live;
pub use analysis::{Block, analyze};
use image::RgbaImage;
pub use live::{Analysis, regenerate};
use std::collections::BTreeMap;

/// Deterministic bounded sampling; transparent pixels do not invent palette
/// colors. Quantized bins reduce antialiasing variants, averaged from real ink.
pub fn palette(image: &RgbaImage) -> Vec<[u8; 4]> {
    let mut bins = BTreeMap::<[u8; 3], (u32, [u64; 3])>::new();
    let [w, h] = [image.width(), image.height()];
    if w == 0 || h == 0 {
        return vec![];
    }
    let nx = w.min(64);
    let ny = h.min(64);
    for row in 0..ny {
        for col in 0..nx {
            let p = image
                .get_pixel(
                    ((2 * col + 1) as u64 * w as u64 / (2 * nx) as u64) as u32,
                    ((2 * row + 1) as u64 * h as u64 / (2 * ny) as u64) as u32,
                )
                .0;
            if p[3] < 128 {
                continue;
            }
            let entry = bins.entry([p[0] / 32, p[1] / 32, p[2] / 32]).or_default();
            entry.0 += 1;
            for (sum, value) in entry.1.iter_mut().zip(p) {
                *sum += u64::from(value);
            }
        }
    }
    let mut entries = bins.into_iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
    entries
        .into_iter()
        .take(6)
        .map(|(_, (count, sum))| {
            [
                (sum[0] / u64::from(count)) as u8,
                (sum[1] / u64::from(count)) as u8,
                (sum[2] / u64::from(count)) as u8,
                255,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Kind, Session, display::Viewport, selection::Handle};
    #[test]
    fn sampled_palette_is_bounded_deterministic_and_ignores_transparent_colors() {
        let image = RgbaImage::from_fn(128, 64, |x, _| {
            image::Rgba(if x < 64 {
                [20, 60, 100, 255]
            } else if x < 96 {
                [200, 20, 30, 255]
            } else {
                [0, 255, 0, 0]
            })
        });
        assert_eq!(
            palette(&image),
            vec![[20, 60, 100, 255], [200, 20, 30, 255]]
        );
        assert!(palette(&RgbaImage::new(4, 4)).is_empty());
        let many = RgbaImage::from_fn(256, 128, |x, y| {
            image::Rgba([x as u8, y as u8, (x * y) as u8, 255])
        });
        assert_eq!(palette(&many).len(), 6);
        assert_eq!(palette(&many), palette(&many));
    }

    #[test]
    fn manual_blocks_preserve_source_and_edit_resize_round_copy_undo_and_reopen() {
        let pixels = RgbaImage::from_fn(300, 180, |x, y| image::Rgba([x as u8, y as u8, 200, 255]));
        let d = Document::from_image(pixels.clone()).unwrap();
        let mut s = Session::new(d.clone()).unwrap();
        let fixture: Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        let mut a = fixture
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Rectangle)
            .unwrap()
            .clone();
        a.kind = Kind::Simplify;
        a.start = [40., 40.];
        a.end = [240., 100.];
        a.text = None;
        a.fill = None;
        a.corner_radius = 12.;
        a.color = [70, 90, 120, 255];
        s.execute(Command::Add {
            annotation: a.clone(),
        })
        .unwrap();
        assert_eq!(s.document.version, 13);
        let rendered = s.document.render().unwrap();
        assert_eq!(rendered.get_pixel(100, 70).0, a.color);
        assert_eq!(rendered.get_pixel(5, 5), pixels.get_pixel(5, 5));
        assert_eq!(s.document.base_image().unwrap(), pixels);
        assert!(a.hit_test([100., 70.], 0.));
        assert!(!a.hit_test([40., 40.], 0.));
        assert_eq!(
            rendered,
            s.document
                .display_list()
                .unwrap()
                .render_viewport(Viewport {
                    size: [300, 180],
                    origin: [0., 0.],
                    scale: 1.
                })
                .unwrap()
        );
        let changed = a
            .resized(Handle::BottomRight, [260., 140.], [300, 180])
            .unwrap()
            .rounded_by_drag(Handle::RadiusTopLeft, [45., 45.], [50., 50.])
            .unwrap();
        s.execute(Command::Update {
            annotation: changed.clone(),
        })
        .unwrap();
        let style =
            crate::object_styles::ObjectStyle::from_annotation("Cover".into(), &changed).unwrap();
        assert_eq!(
            style.apply(&a).unwrap().corner_radius,
            changed.corner_radius.min(a.max_corner_radius())
        );
        let shape = fixture
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Rectangle)
            .unwrap();
        let painted = crate::object_styles::paint_format(&changed, shape).unwrap();
        assert_eq!(painted.fill, Some(changed.color));
        let recolored = crate::object_styles::paint_format(shape, &changed).unwrap();
        assert_eq!(recolored.color, shape.fill.unwrap_or([0; 4]));
        for group in crate::object_styles::groups(Kind::Simplify) {
            for preset in group.styles {
                preset.validate().unwrap();
            }
        }
        let copied =
            crate::clipboard_payload::ObjectPayload::capture(&s.document, &[a.id]).unwrap();
        copied.validate().unwrap();
        assert_eq!(copied.document_version, 13);
        let path = tempfile::tempdir().unwrap();
        let file = path.path().join("simple.capture.json");
        s.save(&file).unwrap();
        let loaded = Document::load(&file).unwrap();
        assert_eq!(loaded.render().unwrap(), s.document.render().unwrap());
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.annotations, [a]);
        s.execute(Command::Undo).unwrap();
        assert_eq!(s.document.render().unwrap(), pixels);
        assert!(!s.can_undo());
        let mut old = loaded.clone();
        old.version = 12;
        assert!(old.validate().is_err());
        if let Ok(root) = std::env::var("CLIPCRAFT_SIMPLIFY_DIR") {
            let root = std::path::PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            loaded.save(&root.join("simplify.capture.json")).unwrap();
            loaded.export_png(&root.join("simplify.png")).unwrap();
        }
    }
}
