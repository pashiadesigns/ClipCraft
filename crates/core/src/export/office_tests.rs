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

/// BW2: an editable shape that the document also records (held on the
/// pasteboard or owned by a guide page) made the slide picture fail with
/// "Invalid pasteboard object ownership", so nothing was saved.
#[test]
fn shapes_with_page_records_still_export() {
    let mut document = sample();
    document.canvas_behavior.held.insert(1);
    document.canvas_behavior.held.insert(3);
    let entries = unzip(&pptx(std::iter::once(Ok(document)), &SlideOptions::default()).unwrap());
    well_formed(&entries);
    assert!(text(&entries, "ppt/slides/slide1.xml").contains("prst=\"rect\""));
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
        let entries = unzip(&docx(std::iter::once(Ok(step_guide())), &options).unwrap());
        well_formed(&entries);
        let body = text(&entries, "word/document.xml");
        assert!(body.contains("Expense report") && body.contains(" TOC "));
        assert!(body.contains("Heading1") && body.contains("<w:keepNext/>"));
        assert_eq!(body.contains("<w:cantSplit/>"), beside);
        assert!(body.contains("w:w=\"11906\""));
        assert_eq!(body.contains("<w:tbl>"), beside);
        assert!(text(&entries, "word/styles.xml").contains("heading 1"));
        assert!(entries.iter().any(|(n, _)| n == "word/media/image1.png"));
    }
}

fn step_guide() -> Document {
    use crate::step_capture::{Button, Gesture, Guide};
    let mut g = Guide::new([320, 180]).unwrap();
    for color in [[230, 20, 40, 255], [20, 180, 220, 255]] {
        g.push(
            image::RgbaImage::from_pixel(320, 180, image::Rgba(color)),
            Gesture {
                start: [50, 40],
                end: [50, 40],
                button: Button::Left,
            },
        )
        .unwrap();
    }
    g.document().unwrap()
}

/// BX2: a recorded step guide (sections, markers, steps) exports to
/// PowerPoint with editable shapes instead of failing validation.
#[test]
fn step_guides_export_to_powerpoint() {
    let entries =
        unzip(&pptx(std::iter::once(Ok(step_guide())), &SlideOptions::default()).unwrap());
    well_formed(&entries);
    let sections = SlideOptions {
        each: Each::Section,
        ..Default::default()
    };
    let entries = unzip(&pptx(std::iter::once(Ok(step_guide())), &sections).unwrap());
    well_formed(&entries);
    assert!(entries.iter().any(|(n, _)| n == "ppt/slides/slide2.xml"));
}

/// BX4/BW4: the title block is the document's title, not step 1; steps
/// number from 1; plain pictures and pages get no step numbers.
#[test]
fn word_uses_the_title_block_and_numbers_only_steps() {
    let entries = unzip(
        &docx(
            std::iter::once(Ok(step_guide())),
            &DocumentOptions::default(),
        )
        .unwrap(),
    );
    well_formed(&entries);
    let body = text(&entries, "word/document.xml");
    let headings: Vec<&str> = body
        .split("<w:pStyle w:val=\"Heading1\"/>")
        .skip(1)
        .map(|s| {
            s.split("</w:t>")
                .next()
                .unwrap()
                .rsplit('>')
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(headings.len(), 2, "{headings:?}");
    assert!(
        headings[0].starts_with("1. ") && headings[1].starts_with("2. "),
        "{headings:?}"
    );
    assert!(
        !body.contains(">1. Step guide<"),
        "the title block isn't a step"
    );
    assert!(body.contains("w:val=\"Title\""));
    // A plain picture: no numbered heading and no contents field.
    let plain = unzip(&docx(std::iter::once(Ok(sample())), &DocumentOptions::default()).unwrap());
    let plain = text(&plain, "word/document.xml");
    assert!(!plain.contains("Heading1") && !plain.contains(" TOC "));
}

/// BW4: text under a redaction and the REDACTED labels never become Word
/// paragraphs or slide notes.
#[test]
fn redacted_text_stays_out_of_word_and_notes() {
    let mut d = sample();
    d.annotations
        .push(object(9, Kind::Redact, [90., 290.], [520., 370.]));
    let d = crate::redaction_labels::decorate(d, true).unwrap();
    let word = text(
        &unzip(&docx(std::iter::once(Ok(d.clone())), &DocumentOptions::default()).unwrap()),
        "word/document.xml",
    );
    assert!(!word.contains("Click Save") && !word.contains("REDACTED"));
    let slides = unzip(&pptx(std::iter::once(Ok(d)), &SlideOptions::default()).unwrap());
    assert!(
        !slides
            .iter()
            .any(|(n, b)| n.contains("notesSlide")
                && String::from_utf8_lossy(b).contains("REDACTED"))
    );
}

/// BW4: a tall step picture is made small enough to sit on one page with
/// its heading, and the heading stays with it.
#[test]
fn word_pictures_fit_on_the_page_with_their_heading() {
    let tall = Document::blank(800, 3200).unwrap();
    for (paper, beside) in [(Paper::Letter, false), (Paper::A4, true)] {
        let options = DocumentOptions {
            paper,
            beside,
            ..Default::default()
        };
        let entries = unzip(&docx(std::iter::once(Ok(tall.clone())), &options).unwrap());
        let body = text(&entries, "word/document.xml");
        let cy: i64 = body
            .split("<wp:extent cx=\"")
            .nth(1)
            .and_then(|s| s.split("cy=\"").nth(1))
            .and_then(|s| s.split('"').next())
            .unwrap()
            .parse()
            .unwrap();
        let page_h = if paper == Paper::Letter {
            15_840
        } else {
            16_838
        };
        let text_h = (page_h - 2 * 1_440) * 635;
        assert!(
            cy + 731_520 <= text_h,
            "{cy} doesn't leave room for the heading"
        );
        let sheets = word_sheets(&tall, &options);
        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0].place[3] as i64, cy);
    }
}

/// The export preview shows one sheet per slide at the chosen slide shape.
#[test]
fn slide_sheets_match_the_slides() {
    let sheets = slide_sheets(&sample(), &SlideOptions::default());
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].sheet, [12_192_000., 6_858_000.]);
    let [x, y, w, h] = sheets[0].place;
    assert!(x >= 0. && y >= 0. && x + w <= 12_192_001. && h <= 6_858_001.);
    assert!(((w / h) - 800. / 600.).abs() < 0.01);
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
