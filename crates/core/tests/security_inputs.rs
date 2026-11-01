//! Synthetic trust-boundary regression checks; no desktop or OS clipboard access.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use capture_core::{
    Document,
    clipboard_payload::ClipboardPayload,
    export::{self, Format, Options},
    history::{History, Origin},
    stamps::vector::{Path as StampPath, VectorStamp},
};
use image::{Rgba, RgbaImage};
use serde_json::{Value, json};
use std::{fs, io::Read};

fn seed() -> Value {
    serde_json::to_value(
        Document::from_image(RgbaImage::from_pixel(16, 12, Rgba([255; 4]))).unwrap(),
    )
    .unwrap()
}

#[test]
fn malformed_projects_fail_without_panicking_or_changing_input() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("untrusted.capture.json");
    let mut cases = vec![];
    for (key, value) in [
        ("width", json!(0)),
        ("width", json!(u32::MAX)),
        ("height", json!(-1)),
        ("version", json!(u32::MAX)),
        ("source_png", json!("not base64")),
        ("source_size", json!([1, 1])),
        ("raster", json!({"width":1,"height":1,"png":""})),
    ] {
        let mut case = seed();
        case[key] = value;
        cases.push(serde_json::to_vec(&case).unwrap());
    }
    cases.push(b"{\"width\":1e9999}".to_vec());
    cases.push(format!("{}0{}", "[".repeat(200), "]".repeat(200)).into_bytes());
    for bytes in cases {
        fs::write(&file, &bytes).unwrap();
        let result = std::panic::catch_unwind(|| Document::load(&file));
        assert!(result.expect("untrusted project panicked").is_err());
        assert_eq!(fs::read(&file).unwrap(), bytes);
    }
}

#[test]
fn truncated_and_mutated_clipboard_inputs_are_bounded_and_do_not_panic() {
    let doc = seed();
    let payload = json!({"schema_version":1,"content":"image","image":{
        "width":16,"height":12,"png":doc["source_png"]
    }});
    let valid = serde_json::to_vec(&payload).unwrap();
    assert!(ClipboardPayload::decode(&valid).is_ok());
    for n in 0..valid.len() {
        assert!(ClipboardPayload::decode(&valid[..n]).is_err());
        let mut mutated = valid.clone();
        mutated[n] = 0xff;
        assert!(
            std::panic::catch_unwind(|| ClipboardPayload::decode(&mutated))
                .unwrap()
                .is_err()
        );
    }
    for (key, value) in [
        ("width", json!(u32::MAX)),
        ("height", json!(0)),
        ("png", json!("AA==")),
    ] {
        let mut p = payload.clone();
        p["image"][key] = value;
        assert!(ClipboardPayload::decode(&serde_json::to_vec(&p).unwrap()).is_err());
    }
}

#[test]
fn oversized_png_header_is_rejected_before_pixel_allocation() {
    let mut png = STANDARD
        .decode(seed()["source_png"].as_str().unwrap())
        .unwrap();
    png[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    // Valid IHDR checksum: rejection must come from dimensions, not a bad CRC.
    let mut crc = !0u32;
    for byte in &png[12..29] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    png[29..33].copy_from_slice(&(!crc).to_be_bytes());
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("large.png");
    fs::write(&file, &png).unwrap();
    let err = Document::open_image(&file).unwrap_err().to_string();
    assert!(
        err.contains("25 megapixels") || err.contains("Memory limit exceeded"),
        "{err}"
    );
}

#[test]
fn library_index_rejects_path_traversal_before_accessing_user_files() {
    let dir = tempfile::tempdir().unwrap();
    let sentinel = dir.path().join("outside.capture.json");
    fs::write(&sentinel, b"private fixture").unwrap();
    let root = dir.path().join("library");
    let mut history = History::open(&root).unwrap();
    history
        .create(&Document::blank(8, 8).unwrap(), "Fixture", Origin::Canvas)
        .unwrap();
    let index: Value = serde_json::from_slice(&fs::read(root.join("index.json")).unwrap()).unwrap();
    for key in ["id", "revision"] {
        for invalid in [
            "../../outside",
            "..\\..\\outside",
            "C:\\outside",
            "https://example.invalid/data",
        ] {
            let mut bad = index.clone();
            bad["version"] = json!(3);
            bad["entries"][0][key] = json!(invalid);
            fs::write(root.join("index.json"), serde_json::to_vec(&bad).unwrap()).unwrap();
            assert!(History::open(&root).is_err());
            assert_eq!(fs::read(&sentinel).unwrap(), b"private fixture");
        }
    }
}

#[test]
fn vector_stamp_data_cannot_be_external_svg_or_unbounded_geometry() {
    for data in [
        "<svg><script>alert(1)</script></svg>",
        "https://example.invalid/image.svg",
        "M 0 0 L 1e300 1e300",
        "M 0 0 L -100 50",
    ] {
        let stamp = VectorStamp {
            colors: vec![[0, 0, 0, 255]],
            paths: vec![StampPath {
                data: data.into(),
                color: 0,
                stroke: None,
            }],
        };
        assert!(stamp.validate().is_err(), "accepted {data}");
    }
}

#[test]
fn partial_redaction_removes_source_pixels_from_png_and_pdf_streams() {
    let secret = [117, 43, 201, 255];
    let pixels = RgbaImage::from_fn(64, 48, |x, y| {
        if (20..30).contains(&x) && (16..24).contains(&y) {
            Rgba(secret)
        } else {
            Rgba([240, 240, 240, 255])
        }
    });
    let mut doc = Document::from_capture_image(pixels).unwrap();
    doc.annotations.push(serde_json::from_value(json!({"id":2,"kind":"redact","start":[16,12],"end":[36,28],"color":[0,0,0,255],"width":1})).unwrap());
    doc.promote_features();
    let before = serde_json::to_vec(&doc).unwrap();
    let png = export::encode_document(&doc, &Options::default()).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().into_rgba8();
    assert_eq!(decoded.get_pixel(0, 0).0, [240, 240, 240, 255]);
    assert!(!decoded.pixels().any(|p| p.0 == secret));
    let pdf = export::encode_document(
        &doc,
        &Options {
            format: Format::Pdf,
            ..Default::default()
        },
    )
    .unwrap();
    let mut at = 0;
    let mut streams = 0;
    while let Some(start) = pdf[at..].windows(7).position(|v| v == b"stream\n") {
        let start = at + start + 7;
        let end = start
            + pdf[start..]
                .windows(10)
                .position(|v| v == b"\nendstream")
                .unwrap();
        let mut raw = vec![];
        flate2::read::ZlibDecoder::new(&pdf[start..end])
            .read_to_end(&mut raw)
            .unwrap();
        assert!(!raw.windows(3).any(|v| v == &secret[..3]));
        streams += 1;
        at = end + 10;
    }
    assert!(streams > 0);
    assert_eq!(
        serde_json::to_vec(&doc).unwrap(),
        before,
        "export must not mutate original editable data"
    );
}
