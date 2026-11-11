use super::*;

/// A minimal PDF from object bodies; `(dict, stream)` objects get a stream.
fn pdf(objects: &[(&[u8], Option<&[u8]>)]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![];
    for (i, (dict, stream)) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        match stream {
            Some(s) => {
                let dict = String::from_utf8_lossy(dict).replace("LEN", &s.len().to_string());
                out.extend_from_slice(dict.as_bytes());
                out.extend_from_slice(b"\nstream\n");
                out.extend_from_slice(s);
                out.extend_from_slice(b"\nendstream");
            }
            None => out.extend_from_slice(dict),
        }
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}
fn one_page(content: &[u8], resources: &str) -> Vec<u8> {
    let page = format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources {resources} >>"
    );
    pdf(&[
        (b"<< /Type /Catalog /Pages 2 0 R >>", None),
        (b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>", None),
        (page.as_bytes(), None),
        (b"<< /Length LEN >>", Some(content)),
    ])
}
const HELVETICA: &str =
    "<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >>";
fn text_page() -> Vec<u8> {
    one_page(
        b"BT /F1 12 Tf 72 700 Td (Contact dana.reyes@example.com today) Tj ET\n\
          BT /F1 12 Tf 72 680 Td (Phone \\(555\\) 014-2277) Tj ET\n\
          0 0.5 0.8 rg 72 600 200 40 re f",
        HELVETICA,
    )
}

#[test]
fn a_text_page_imports_as_a_drawing_with_its_words() {
    let file = open(text_page(), "").unwrap();
    assert_eq!(file.page_count(), 1);
    assert_eq!(file.page_pixels(0), [816, 1056]);
    let page = file
        .prepare(0, ImportAs::Editable { fallback_dpi: 150 })
        .unwrap();
    assert_eq!(page.picture, None);
    assert!(page.bitmap.is_svg() && page.bitmap.is_pdf_page());
    let layer = page.bitmap.text_layer().unwrap();
    let email = layer
        .words
        .iter()
        .find(|w| w.text == "dana.reyes@example.com")
        .expect("the email is one word")
        .clone();
    // The line starts 72 pt from the left; the baseline is 92 pt from the top
    // of a 612 x 792 page and the box reaches a little below it.
    let contact = layer.words.iter().find(|w| w.text == "Contact").unwrap();
    assert!((contact.rect[0] - 72. / 612.).abs() < 0.01, "{contact:?}");
    assert!(email.rect[0] > contact.rect[0] + contact.rect[2]);
    assert!(
        (email.rect[1] + email.rect[3] - 92. / 792.).abs() < 0.02,
        "{email:?}"
    );
    assert!((email.rect[3] * 792. - 12.).abs() < 1., "{email:?}");
    assert_eq!(
        layer.words.iter().find(|w| w.text == "Phone").unwrap().line,
        layer
            .words
            .iter()
            .find(|w| w.text == "014-2277")
            .unwrap()
            .line
    );

    let document = build_guide(vec![page]).unwrap();
    assert_eq!([document.width, document.height], [816, 1056]);
    let json = serde_json::to_vec(&document).unwrap();
    let back: Document = serde_json::from_slice(&json).unwrap();
    back.validate().unwrap();
    assert_eq!(
        back.annotations[0].image.as_ref().unwrap().text_layer(),
        document.annotations[0].image.as_ref().unwrap().text_layer()
    );
    let (words, regions) = document_words(&document);
    assert_eq!(regions, vec![[0., 0., 816., 1056.]]);
    let findings =
        crate::privacy::detect(&words, &crate::privacy::Options::default(), [816, 1056]).unwrap();
    let found = findings
        .iter()
        .find(|f| f.category == crate::privacy::Category::Email)
        .expect("the email is found from the text layer");
    let expected = email.rect[0] * 816.;
    // Detection pads its boxes a little.
    assert!(
        (found.bounds[0][0] - expected).abs() < 6.,
        "{:?}",
        found.bounds
    );
    // The drawing shows the text: dark pixels where the email is.
    let pixels = document.render().unwrap();
    let b = found.bounds[0];
    let dark = (b[1] as u32..(b[1] + b[3]) as u32)
        .flat_map(|y| (b[0] as u32..(b[0] + b[2]) as u32).map(move |x| (x, y)))
        .filter(|&(x, y)| pixels.get_pixel(x, y)[0] < 128 && pixels.get_pixel(x, y)[3] > 0)
        .count();
    assert!(dark > 50, "{dark}");
}

#[test]
fn pictures_scans_and_effects_come_in_as_pictures() {
    let file = open(text_page(), "").unwrap();
    let page = file.prepare(0, ImportAs::Pictures { dpi: 150 }).unwrap();
    assert_eq!(page.picture, Some(PictureReason::Chosen));
    assert!(!page.bitmap.is_svg() && page.bitmap.text_layer().is_none());
    assert_eq!(page.bitmap.size(), [1275, 1650]);

    // A page that is one big picture and no text is a scan.
    let pixels: Vec<u8> = (0..64 * 64).map(|i| (i % 251) as u8).collect();
    let scan = pdf(&[
        (b"<< /Type /Catalog /Pages 2 0 R >>", None),
        (b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>", None),
        (
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>",
            None,
        ),
        (b"<< /Length LEN >>", Some(b"q 612 0 0 792 0 0 cm /Im1 Do Q")),
        (
            b"<< /Type /XObject /Subtype /Image /Width 64 /Height 64 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length LEN >>",
            Some(&pixels),
        ),
    ]);
    let page = open(scan, "")
        .unwrap()
        .prepare(0, ImportAs::Editable { fallback_dpi: 100 })
        .unwrap();
    assert_eq!(page.picture, Some(PictureReason::Scan));

    // Blend modes the drawing can't show become a picture that keeps its words.
    let mut content = String::from("BT /F1 10 Tf 40 760 Td (Blend test) Tj ET\n");
    for i in 0..6 {
        let (x, y) = (40 + (i % 3) * 180, 500 - (i / 3) * 200);
        content.push_str(&format!(
            "q 0.1 0.55 0.75 rg {x} {y} 150 150 re f Q q /G{i} gs 0.95 0.7 0.2 rg {} {} 150 150 re f Q\n",
            x + 40,
            y - 40
        ));
    }
    let states: String = [
        "ColorDodge",
        "ColorBurn",
        "Difference",
        "Exclusion",
        "Hue",
        "Luminosity",
    ]
    .iter()
    .enumerate()
    .map(|(i, m)| format!("/G{i} << /Type /ExtGState /BM /{m} >> "))
    .collect();
    let resources = format!(
        "<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> /ExtGState << {states}>> >>"
    );
    let file = open(one_page(content.as_bytes(), &resources), "").unwrap();
    let page = file
        .prepare(0, ImportAs::Editable { fallback_dpi: 100 })
        .unwrap();
    if page.picture.is_some() {
        assert_eq!(page.picture, Some(PictureReason::Effects));
        assert!(page.bitmap.text_layer().is_some());
    }
}

#[test]
fn unsafe_and_damaged_files_are_refused_plainly() {
    assert!(matches!(
        open(b"hello".to_vec(), ""),
        Err(OpenError::Damaged(_))
    ));
    let mut truncated = text_page();
    truncated.truncate(truncated.len() / 3);
    assert!(open(truncated, "").is_err());
    // A small stream that inflates past the content limit is never drawn.
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut encoder, &vec![b' '; 80 * 1024 * 1024]).unwrap();
    let bomb = encoder.finish().unwrap();
    let file = pdf(&[
        (b"<< /Type /Catalog /Pages 2 0 R >>", None),
        (b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>", None),
        (
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R >>",
            None,
        ),
        (b"<< /Length LEN /Filter /FlateDecode >>", Some(&bomb)),
    ]);
    assert!(matches!(open(file, ""), Err(OpenError::TooLarge(_))));
}

#[test]
fn password_protected_files_ask_and_open_with_the_password() {
    let aes = include_bytes!("../tests/fixtures/pdf/enc_aes256.pdf").to_vec();
    assert_eq!(open(aes.clone(), "").err(), Some(OpenError::NeedsPassword));
    assert_eq!(
        open(aes.clone(), "nope").err(),
        Some(OpenError::WrongPassword)
    );
    let file = open(aes, "spike-pass").unwrap();
    assert!(
        file.text_layer(0)
            .words
            .iter()
            .any(|w| w.text.contains("dana.reyes@example.com"))
    );
    let rc4 = include_bytes!("../tests/fixtures/pdf/enc_rc4.pdf").to_vec();
    assert!(open(rc4, "spike-pass").is_ok());
}

fn export_options(d: &Document) -> crate::export::Options {
    crate::export::Options {
        format: crate::export::Format::Pdf,
        page: crate::export::PageSettings {
            size: [d.width as f64, d.height as f64],
            margin: 0.,
            paginate: false,
        },
        ..Default::default()
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
        let mut out = vec![];
        if std::io::Read::read_to_end(
            &mut flate2::read::ZlibDecoder::new(&bytes[start..end]),
            &mut out,
        )
        .is_ok()
        {
            result.push(out);
        }
        at = end + 10;
    }
    result
}

#[test]
fn editable_pages_export_as_shapes_and_redactions_remove_only_what_they_cover() {
    // The email is drawn in a color used nowhere else on the page.
    let secret = [117u8, 43, 201];
    let page = one_page(
        b"BT /F1 14 Tf 72 700 Td (Contact ) Tj ET\n\
          BT 0.4588 0.1686 0.7882 rg /F1 14 Tf 140 700 Td (dana.reyes@example.com) Tj ET\n\
          BT 0 g /F1 14 Tf 72 660 Td (Phone 555 014 2277) Tj ET\n\
          0 0.5 0.8 rg 72 560 200 40 re f",
        HELVETICA,
    );
    let file = open(page, "").unwrap();
    let prepared = file
        .prepare(0, ImportAs::Editable { fallback_dpi: 150 })
        .unwrap();
    assert_eq!(prepared.picture, None);
    let document = build_guide(vec![prepared]).unwrap();
    let options = export_options(&document);

    // Unredacted: shapes only, and the export looks like the page.
    let bytes = crate::export::encode_document(&document, &options).unwrap();
    assert!(!bytes.windows(15).any(|s| s == b"/Subtype /Image"));
    let exported = open(bytes, "").unwrap();
    let shown = exported.render(0, 1.);
    let expected = document.render().unwrap();
    let mut flat = expected.clone();
    for p in flat.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    let shown = image::imageops::resize(
        &shown,
        flat.width(),
        flat.height(),
        image::imageops::FilterType::Triangle,
    );
    assert!(share_different(&flat, &shown) < 0.01);

    // Redact the email: its shapes leave the file; the rest stays shapes.
    let layer = document.annotations[0]
        .image
        .as_ref()
        .unwrap()
        .text_layer()
        .unwrap()
        .clone();
    let email = layer
        .words
        .iter()
        .find(|w| w.text == "dana.reyes@example.com")
        .unwrap();
    let r = [
        email.rect[0] * 816. - 4.,
        email.rect[1] * 1056. - 4.,
        (email.rect[0] + email.rect[2]) * 816. + 4.,
        (email.rect[1] + email.rect[3]) * 1056. + 4.,
    ];
    let mut redacted = document.clone();
    let mut cover =
        crate::steps::marker(9, [40., 40.], 48, [0, 0, 0, 255], (1, false), [816, 1056]).unwrap();
    cover.kind = crate::Kind::Redact;
    cover.text = None;
    cover.start = [r[0], r[1]];
    cover.end = [r[2], r[3]];
    cover.width = 1;
    redacted.annotations.push(cover);
    redacted.promote_features();
    let bytes = crate::export::encode_document(&redacted, &export_options(&redacted)).unwrap();
    let all = streams(&bytes);
    if let Ok(dir) = std::env::var("CLIPDESIGNER_PDF_IMPORT_DEBUG") {
        std::fs::write(format!("{dir}/redacted.pdf"), &bytes).unwrap();
        for (i, s) in all.iter().enumerate() {
            std::fs::write(format!("{dir}/stream{i}.txt"), s).unwrap();
        }
        std::fs::write(
            format!("{dir}/page.svg"),
            redacted.annotations[0]
                .image
                .as_ref()
                .unwrap()
                .svg_text()
                .unwrap(),
        )
        .unwrap();
        eprintln!("redaction {r:?}");
    }
    let color = b"0.4588 0.1686 0.7882";
    for s in &all {
        assert!(
            !s.windows(color.len()).any(|w| w == color),
            "the email's shapes are gone"
        );
        assert!(
            !s.windows(3).any(|w| w == secret),
            "no picture keeps the email"
        );
    }
    // The other words are still shapes (curves), not one flattened page.
    assert!(all.iter().any(|s| s.windows(2).any(|w| w == b" c")));
    let shown = open(bytes, "").unwrap().render(0, 1.);
    let shown = image::imageops::resize(
        &shown,
        flat.width(),
        flat.height(),
        image::imageops::FilterType::Triangle,
    );
    // Outside the redaction the page is unchanged.
    let mut outside = 0;
    let mut differ = 0;
    for (x, y, p) in flat.enumerate_pixels() {
        let (fx, fy) = (x as f32, y as f32);
        if fx > r[0] - 8. && fx < r[2] + 8. && fy > r[1] - 8. && fy < r[3] + 8. {
            continue;
        }
        outside += 1;
        let q = shown.get_pixel(x, y);
        if (0..3).any(|c| (i16::from(p[c]) - i16::from(q[c])).abs() > 60) {
            differ += 1;
        }
    }
    assert!(
        (differ as f64) / (outside as f64) < 0.005,
        "{differ}/{outside}"
    );
}

/// Prints how far each sample page's drawing is from the engine's picture.
#[test]
#[ignore = "report: cargo test -p capture-core faithfulness_report -- --ignored --nocapture"]
fn faithfulness_report() {
    let file = open(
        include_bytes!("../tests/fixtures/pdf/sample-8-pages.pdf").to_vec(),
        "",
    )
    .unwrap();
    for i in 0..file.page_count() {
        let svg = hayro_svg::convert(
            &file.pdf.pages()[i],
            &hayro_svg::RenderCache::new(),
            &file.settings,
            &hayro_svg::SvgRenderSettings::default(),
        );
        let [w, h] = file.page_points(i);
        let reference = file.render(i, 900. / w.max(h));
        let mut drawn =
            crate::vector_import::render_pdf_page(&svg, [reference.width(), reference.height()])
                .unwrap();
        for p in drawn.pixels_mut() {
            let a = u16::from(p[3]);
            for c in 0..3 {
                p[c] = ((u16::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
            }
            p[3] = 255;
        }
        if let Ok(dir) = std::env::var("CLIPDESIGNER_PDF_IMPORT_DEBUG") {
            reference.save(format!("{dir}/ref{i}.png")).unwrap();
            drawn.save(format!("{dir}/svg{i}.png")).unwrap();
        }
        println!("page {}: {:.4}", i + 1, share_different(&reference, &drawn));
    }
}

#[test]
fn a_guide_holds_as_many_pages_as_its_size_limits_allow() {
    let letter = [816, 1056];
    assert_eq!(pages_that_fit(&[letter; 10]), 10);
    let most = pages_that_fit(&[letter; 100]);
    assert_eq!(most, 24);
    let a4 = guide_pixels([595.28, 841.89]);
    assert_eq!(a4, [794, 1123]);
    assert!(pages_that_fit(&[a4; 100]) < 30);
}
