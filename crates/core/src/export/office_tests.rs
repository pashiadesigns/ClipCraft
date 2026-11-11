use super::*;

/// Reads every entry of a ZIP written by `Zip` back out.
fn unzip(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    use std::io::Read as _;
    let mut out = vec![];
    let mut at = 0;
    while at + 30 <= bytes.len() && bytes[at..at + 4] == [0x50, 0x4b, 3, 4] {
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]) as usize;
        let u32_at = |i: usize| {
            u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize
        };
        let crc = u32_at(at + 14) as u32;
        let packed = u32_at(at + 18);
        let size = u32_at(at + 22);
        let name_len = u16_at(at + 26);
        let extra = u16_at(at + 28);
        let name = String::from_utf8(bytes[at + 30..at + 30 + name_len].to_vec()).unwrap();
        let start = at + 30 + name_len + extra;
        let mut data = vec![];
        flate2::read::DeflateDecoder::new(&bytes[start..start + packed])
            .read_to_end(&mut data)
            .unwrap();
        assert_eq!(data.len(), size, "{name}");
        let mut check = flate2::Crc::new();
        check.update(&data);
        assert_eq!(check.sum(), crc, "{name}");
        out.push((name, data));
        at = start + packed;
    }
    // The central directory and its end record follow.
    assert_eq!(&bytes[at..at + 4], &[0x50, 0x4b, 1, 2]);
    assert_eq!(
        &bytes[bytes.len() - 22..bytes.len() - 18],
        &[0x50, 0x4b, 5, 6]
    );
    out
}
fn text(entries: &[(String, Vec<u8>)], name: &str) -> String {
    let found = entries
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("missing {name}"));
    String::from_utf8(found.1.clone()).unwrap()
}
/// Every XML part parses as well-formed XML.
fn well_formed(entries: &[(String, Vec<u8>)]) {
    for (name, data) in entries {
        if name.ends_with(".xml") || name.ends_with(".rels") {
            let xml = String::from_utf8(data.clone()).unwrap();
            roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}

fn object(id: u64, kind: Kind, start: [f32; 2], end: [f32; 2]) -> Annotation {
    Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        focus: None,
        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.0,
        mirror_x: false,
        id,
        kind,
        start,
        end,
        color: [40, 40, 40, 255],
        width: 3,
        text: None,
        corner_radius: 0.0,
        fill: None,
        points: Vec::new(),
        callout: None,
    }
}

fn sample() -> Document {
    let mut document = Document::blank(800, 600).unwrap();
    let mut rect = object(1, Kind::Rectangle, [100., 100.], [300., 200.]);
    rect.fill = Some([24, 145, 193, 255]);
    document.annotations.push(rect);
    let mut label = object(2, Kind::Text, [100., 300.], [500., 360.]);
    label.text = Some(crate::text::TextStyle {
        content: "Click Save & close".into(),
        ..Default::default()
    });
    document.annotations.push(label);
    document
        .annotations
        .push(object(3, Kind::Arrow, [600., 500.], [400., 250.]));
    document
}

#[test]
fn powerpoint_keeps_shapes_and_text_editable() {
    let bytes = pptx(std::iter::once(Ok(sample())), &SlideOptions::default()).unwrap();
    let entries = unzip(&bytes);
    well_formed(&entries);
    let slide = text(&entries, "ppt/slides/slide1.xml");
    assert!(slide.contains("prst=\"rect\"") && slide.contains("1891C1"));
    assert!(slide.contains("Click Save &amp; close"));
    assert!(slide.contains("triangle"), "the arrow keeps its head");
    assert!(slide.contains("flipH=\"1\""), "the arrow points left");
    let notes = text(&entries, "ppt/notesSlides/notesSlide1.xml");
    assert!(notes.contains("Click Save &amp; close"));
    assert!(text(&entries, "ppt/presentation.xml").contains("cx=\"12192000\""));
    assert!(entries.iter().any(|(n, _)| n == "ppt/media/image1.png"));
}

#[test]
fn pictures_only_and_redactions_flatten() {
    let options = SlideOptions {
        editable: false,
        notes: false,
        size: SlideSize::Standard,
        ..Default::default()
    };
    let entries = unzip(&pptx(std::iter::once(Ok(sample())), &options).unwrap());
    well_formed(&entries);
    let slide = text(&entries, "ppt/slides/slide1.xml");
    assert!(!slide.contains("<p:sp>") && !slide.contains("Click Save"));
    assert!(!entries.iter().any(|(n, _)| n.contains("notesSlide")));
    // A redaction over the text keeps the text out of the slide.
    let mut redacted = sample();
    redacted
        .annotations
        .push(object(9, Kind::Redact, [90., 290.], [520., 370.]));
    let entries = unzip(&pptx(std::iter::once(Ok(redacted)), &SlideOptions::default()).unwrap());
    let slide = text(&entries, "ppt/slides/slide1.xml");
    assert!(!slide.contains("Click Save"), "covered text is flattened");
    assert!(
        slide.contains("prst=\"rect\""),
        "the shape it doesn't touch stays"
    );
}

#[test]
fn every_part_becomes_slides_in_one_file() {
    let parts = vec![Ok(sample()), Ok(sample()), Ok(sample())];
    let entries = unzip(&pptx(parts.into_iter(), &SlideOptions::default()).unwrap());
    well_formed(&entries);
    assert!(entries.iter().any(|(n, _)| n == "ppt/slides/slide3.xml"));
    assert_eq!(
        text(&entries, "ppt/presentation.xml")
            .matches("<p:sldId ")
            .count(),
        3
    );
}

#[test]
fn word_steps_in_built_in_styles() {
    for beside in [false, true] {
        let options = DocumentOptions {
            paper: Paper::A4,
            beside,
            title: "Expense report".into(),
            ..Default::default()
        };
        let entries = unzip(&docx(std::iter::once(Ok(sample())), &options).unwrap());
        well_formed(&entries);
        let body = text(&entries, "word/document.xml");
        assert!(body.contains("Expense report") && body.contains(" TOC "));
        assert!(body.contains("Heading1") && body.contains("1. Click Save &amp; close"));
        assert!(body.contains("w:w=\"11906\""));
        assert_eq!(body.contains("<w:tbl>"), beside);
        assert!(text(&entries, "word/styles.xml").contains("heading 1"));
        assert!(entries.iter().any(|(n, _)| n == "word/media/image1.png"));
    }
}

/// Writes sample files for checking in Office or another reader
/// (CLIPDESIGNER_OFFICE_OUT names the folder).
#[test]
fn write_samples_when_asked() {
    let Ok(dir) = std::env::var("CLIPDESIGNER_OFFICE_OUT") else {
        return;
    };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let parts = || vec![Ok(sample()), Ok(sample())].into_iter();
    std::fs::write(
        dir.join("sample.pptx"),
        pptx(parts(), &SlideOptions::default()).unwrap(),
    )
    .unwrap();
    let docs = DocumentOptions::default();
    std::fs::write(dir.join("sample.docx"), docx(parts(), &docs).unwrap()).unwrap();
    let beside = DocumentOptions {
        beside: true,
        ..docs
    };
    std::fs::write(dir.join("beside.docx"), docx(parts(), &beside).unwrap()).unwrap();
}

#[test]
fn a_video_plays_on_its_slide() {
    let poster = image::RgbaImage::from_pixel(320, 180, image::Rgba([10, 20, 30, 255]));
    let entries =
        unzip(&pptx_movie(b"not really an mp4", false, &poster, SlideSize::Widescreen).unwrap());
    well_formed(&entries);
    let slide = text(&entries, "ppt/slides/slide1.xml");
    assert!(slide.contains("videoFile") && slide.contains("p:timing"));
    assert!(text(&entries, "[Content_Types].xml").contains("video/mp4"));
    assert!(
        entries
            .iter()
            .any(|(n, d)| n == "ppt/media/media1.mp4" && d == b"not really an mp4")
    );
    let entries = unzip(&pptx_movie(b"GIF89a", true, &poster, SlideSize::MatchPage).unwrap());
    well_formed(&entries);
    assert!(entries.iter().any(|(n, _)| n == "ppt/media/image1.gif"));
    assert!(!text(&entries, "ppt/slides/slide1.xml").contains("videoFile"));
}

#[test]
fn summary_counts_without_drawing() {
    assert_eq!(summary(&sample(), Each::Page, true), (1, 3));
    assert_eq!(summary(&sample(), Each::Page, false), (1, 0));
}
