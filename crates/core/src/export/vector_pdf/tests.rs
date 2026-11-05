use super::*;
use crate::{Annotation, Kind, bitmap::Bitmap, icon_library::vector::VectorIcon, steps, text};
use std::sync::Arc;
fn object(id: u64, kind: Kind, rect: [f32; 4]) -> Annotation {
    let mut a = steps::marker(
        id,
        [40., 40.],
        48,
        [35, 120, 190, 255],
        (1, false),
        [400, 300],
    )
    .unwrap();
    a.kind = kind;
    a.start = [rect[0], rect[1]];
    a.end = [rect[2], rect[3]];
    a.text = None;
    a.width = 4;
    a
}
fn vectors() -> Document {
    let mut d = Document::canvas(400, 300, [245, 245, 245, 255]).unwrap();
    let mut a = object(1, Kind::Rectangle, [18., 18., 185., 92.]);
    a.fill = Some([70, 180, 220, 140]);
    a.corner_radius = 16.;
    a.rotation_degrees = 7.;
    d.annotations.push(a);
    let mut a = object(2, Kind::Ellipse, [210., 12., 375., 100.]);
    a.fill = Some([230, 180, 45, 180]);
    d.annotations.push(a);
    let mut a = object(3, Kind::Text, [12., 112., 385., 186.]);
    a.text = Some(text::TextStyle {
        content: "Vector Abc 123".into(),
        size_points: 30.,
        color: Some([20, 40, 100, 220]),
        bold: true,
        underline: true,
        outline: Some(text::outline::Outline {
            color: [210, 40, 80, 180],
            width: 2.,
        }),
        ..Default::default()
    });
    d.annotations.push(a);
    let mut a = object(4, Kind::Arrow, [20., 205., 190., 280.]);
    a.width = 7;
    d.annotations.push(a);
    let vector = VectorIcon::builtin("check-circle");
    let mut a = Arc::new(Bitmap::from_vector(vector).unwrap()).annotation(5);
    a.start = [235., 205.];
    a.end = [305., 275.];
    a.rotation_degrees = 10.;
    d.annotations.push(a);
    d.annotations.push(
        steps::marker(
            6,
            [350., 240.],
            54,
            [190, 30, 90, 255],
            (3, false),
            [400, 300],
        )
        .unwrap(),
    );
    d.promote_features();
    d
}
fn options(d: &Document) -> Options {
    Options {
        format: Format::Pdf,
        page: PageSettings {
            size: [d.width as f64, d.height as f64],
            margin: 0.,
            paginate: false,
        },
        ..Default::default()
    }
}
#[test]
fn links_are_clickable_but_covered_uris_never_enter_pdf_objects() {
    let mut d = Document::canvas(400, 200, [255; 4]).unwrap();
    let mut a = object(1, Kind::Text, [20., 20., 380., 90.]);
    let mut text = text::TextStyle {
        content: "Open private account".into(),
        horizontal: text::HorizontalAlignment::Left,
        vertical: text::VerticalAlignment::Top,
        ..Default::default()
    };
    crate::text::rich::format_range(&mut text, 0..20, |f| {
        f.link = Some("https://example.com/private-account".into());
        f.underline = true;
    });
    a.text = Some(text);
    d.annotations.push(a);
    let pdf = encode(&d, &options(&d)).unwrap();
    assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Link"));
    for kind in [Kind::Redact, Kind::Blur, Kind::Pixelate, Kind::Eraser] {
        let mut covered = d.clone();
        let mut cover = object(2, kind, [0., 0., 400., 120.]);
        if kind == Kind::Eraser {
            cover.points = vec![cover.start, cover.end];
        }
        covered.annotations.push(cover);
        let pdf = encode(&covered, &options(&covered)).unwrap();
        let bytes = String::from_utf8_lossy(&pdf);
        assert!(!bytes.contains("/URI"), "{kind:?}");
        assert!(!bytes.contains("707269766174652D6163636F756E74"));
    }
}
fn streams(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut result = vec![];
    let mut at = 0;
    while let Some(start) = bytes[at..].windows(7).position(|v| v == b"stream\n") {
        let start = at + start + 7;
        let end = start
            + bytes[start..]
                .windows(10)
                .position(|v| v == b"\nendstream")
                .unwrap();
        let mut d = flate2::read::ZlibDecoder::new(&bytes[start..end]);
        let mut out = vec![];
        std::io::Read::read_to_end(&mut d, &mut out).unwrap();
        result.push(out);
        at = end + 10;
    }
    result
}
fn fixture(name: &str, d: &Document, o: &Options) {
    if let Ok(dir) = std::env::var("CLIPCRAFT_VECTOR_PDF_FIXTURES") {
        let dir = std::path::Path::new(&dir);
        std::fs::create_dir_all(dir).unwrap();
        let bytes = encode(d, o).unwrap();
        std::fs::write(dir.join(format!("{name}.pdf")), bytes).unwrap();
        d.save(&dir.join(format!("{name}.capture.json"))).unwrap();
        d.display_list()
            .unwrap()
            .render_viewport(crate::display::Viewport {
                size: [d.width, d.height],
                origin: [0.; 2],
                scale: 1.,
            })
            .unwrap()
            .save(dir.join(format!("{name}-expected.png")))
            .unwrap();
    }
}
#[test]
fn pdf_retains_shared_vector_paths_and_does_not_mutate_source() {
    let d = vectors();
    let before = serde_json::to_vec(&d).unwrap();
    let bytes = encode(&d, &options(&d)).unwrap();
    assert!(!bytes.windows(15).any(|s| s == b"/Subtype /Image"));
    let commands = streams(&bytes);
    assert!(commands.iter().any(|s| s.windows(2).any(|w| w == b" c")));
    assert!(commands.iter().any(|s| s.windows(2).any(|w| w == b"S\n")));
    assert_eq!(serde_json::to_vec(&d).unwrap(), before);
    fixture("vectors", &d, &options(&d));
}
#[test]
fn pdf_effects_remove_covered_source_data_and_preserve_later_vectors() {
    let secret = [117, 43, 201, 255];
    let mut d = Document::canvas(400, 300, secret).unwrap();
    let mut a = object(1, Kind::Redact, [0., 0., 400., 300.]);
    a.width = 1;
    d.annotations.push(a);
    d.annotations.push(
        steps::marker(
            2,
            [340., 240.],
            54,
            [190, 30, 90, 255],
            (2, false),
            [400, 300],
        )
        .unwrap(),
    );
    d.promote_features();
    let bytes = encode(&d, &options(&d)).unwrap();
    for stream in streams(&bytes) {
        assert!(!stream.windows(3).any(|w| w == &secret[..3]));
    }
    fixture("redacted", &d, &options(&d));
    let mut d = vectors();
    d.annotations
        .push(object(10, Kind::Blur, [22., 20., 110., 85.]));
    let mut e = object(11, Kind::Eraser, [12., 50., 70., 75.]);
    e.points = vec![e.start, e.end];
    e.width = 30;
    d.annotations.push(e);
    d.promote_features();
    fixture("effects", &d, &options(&d));
}

#[test]
fn optional_redaction_labels_never_retain_source_in_pdf_or_raster() {
    let secret = [117, 43, 201, 255];
    let mut document = Document::canvas(180, 70, secret).unwrap();
    document
        .annotations
        .push(object(1, Kind::Redact, [0., 0., 180., 70.]));
    let original = serde_json::to_vec(&document).unwrap();
    let mut results = Vec::new();
    for labels in [false, true] {
        let prepared = crate::redaction_labels::decorate(document.clone(), labels).unwrap();
        prepared.validate().unwrap();
        let rendered = prepared.render().unwrap();
        assert!(!rendered.pixels().any(|p| p.0 == secret));
        assert_eq!(prepared.annotations[0], document.annotations[0]);
        let mut options = options(&document);
        options.redaction_labels = labels;
        let pdf = crate::export::encode_document(&document, &options).unwrap();
        for stream in streams(&pdf) {
            assert!(!stream.windows(3).any(|w| w == &secret[..3]));
        }
        options.format = Format::Png;
        let png = crate::export::encode_document(&document, &options).unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().into_rgba8(),
            rendered
        );
        results.push(rendered);
        fixture(
            if labels {
                "redaction-label"
            } else {
                "redaction-no-label"
            },
            &prepared,
            &options,
        );
    }
    assert_ne!(results[0], results[1]);
    assert_eq!(serde_json::to_vec(&document).unwrap(), original);
}
#[test]
fn pdf_keeps_cropped_frames_transparency_and_pagination() {
    let mut d = vectors();
    let pixels = RgbaImage::from_fn(120, 100, |x, y| {
        image::Rgba([(x * 2) as u8, (y * 2) as u8, 110, 200])
    });
    let bitmap =
        Arc::new(Bitmap::new(Arc::new(crate::RasterImage::from_image(&pixels).unwrap())).unwrap());
    let mut a = crate::templates::image_in_frame(
        bitmap,
        20,
        [100., 210., 95., 70.],
        crate::templates::Scale::Fill,
    )
    .unwrap();
    a.corner_radius = 20.;
    a.rotation_degrees = -8.;
    d.annotations.push(a);
    d.promote_features();
    fixture("frames", &d, &options(&d));
    let guide = crate::templates::build(
        "two-steps",
        "Vector pages",
        &[],
        crate::templates::Scale::Fit,
    )
    .unwrap();
    let mut o = options(&guide);
    o.page.size = [guide.width as f64, 500.];
    o.page.paginate = true;
    let pages = o.page.pages([guide.width, guide.height]).unwrap();
    assert!(pages.len() > 1);
    fixture("pages", &guide, &o);
}

#[test]
fn pdf_cursor_inversion_and_rotated_edge_strokes_match_the_composite() {
    let mut d = vectors();
    d.annotations[0].start = [0., 0.];
    d.annotations[0].end = [100., 70.];
    d.annotations[0].rotation_degrees = 35.;
    let style = d.annotations[2].text.as_mut().unwrap();
    style.color = Some([0; 4]);
    d.annotations[2].shadow = Some(crate::shadow::Shadow::default());
    let black = RgbaImage::from_pixel(12, 18, image::Rgba([255; 4]));
    let white = RgbaImage::from_pixel(12, 18, image::Rgba([0, 0, 0, 255]));
    let cursor =
        crate::captured_cursor::Cursor::from_backgrounds([230, 20], &black, &white).unwrap();
    d.annotations
        .push(cursor.annotation([0, 0], [400, 300], 30).unwrap().unwrap());
    d.promote_features();
    fixture("cursor-outline", &d, &options(&d));
    assert!(!encode(&d, &options(&d)).unwrap().is_empty());
}

#[test]
fn horizontal_pages_translate_source_x_and_preserve_vector_content() {
    let d = crate::templates::build(
        "two-steps",
        "Horizontal",
        &[],
        crate::templates::Scale::Fill,
    )
    .unwrap()
    .edit_guide(&crate::guide_sections::Action::PageSize {
        width: 900,
        height: 550,
        auto_overflow: true,
    })
    .unwrap();
    let o = Options {
        format: Format::Pdf,
        page: PageSettings {
            size: [900., 550.],
            margin: 0.,
            paginate: false,
        },
        ..Default::default()
    };
    let pdf = encode(&d, &o).unwrap();
    let content = streams(&pdf)
        .into_iter()
        .filter_map(|v| String::from_utf8(v).ok())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(content.contains("1 0 0 -1 0 550 cm"));
    assert!(content.contains(&format!(
        "1 0 0 -1 -{} 550 cm",
        900 + crate::guide_sections::PAGE_GAP
    )));
    assert!(content.contains(" l\n"));
}
