//! ENG-009: hostile-file tests for the importers (approved beta 6 batch).
//! Deterministic mutations of real inputs (bit flips, truncation, spliced and
//! duplicated chunks, huge numbers) must fail with an error or import
//! normally: never panic, and never run past a time limit. Inputs are
//! generated or synthetic; nothing personal.
use capture_core::pdf_import::{self, ImportAs};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A small, repeatable random number generator (no extra dependency).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() as usize) % n.max(1)
    }
}

/// One mutated copy of `input`.
fn mutate(input: &[u8], rng: &mut Lcg) -> Vec<u8> {
    let mut out = input.to_vec();
    match rng.below(6) {
        0 => {
            for _ in 0..1 + rng.below(16) {
                let i = rng.below(out.len());
                out[i] ^= 1 << rng.below(8);
            }
        }
        1 => out.truncate(rng.below(out.len())),
        2 => {
            let a = rng.below(out.len());
            let b = (a + rng.below(4096)).min(out.len());
            let chunk = out[a..b].to_vec();
            let at = rng.below(out.len());
            out.splice(
                at..at,
                chunk
                    .iter()
                    .copied()
                    .cycle()
                    .take(chunk.len() * (1 + rng.below(4))),
            );
        }
        3 => {
            // Replace a number with a huge or negative one.
            let digits: Vec<usize> = out
                .iter()
                .enumerate()
                .filter(|(_, b)| b.is_ascii_digit())
                .map(|(i, _)| i)
                .collect();
            if let Some(&i) = digits.get(rng.below(digits.len())) {
                let value: &[u8] = [&b"99999999999"[..], b"-1", b"4294967296", b"0"][rng.below(4)];
                out.splice(i..i + 1, value.iter().copied());
            }
        }
        4 => {
            let a = rng.below(out.len());
            let b = (a + rng.below(512)).min(out.len());
            out.drain(a..b);
        }
        _ => {
            let i = rng.below(out.len());
            for b in out.iter_mut().skip(i).take(64) {
                *b = rng.next() as u8;
            }
        }
    }
    out
}

/// Runs `job` on its own thread; reports a panic or a run past `limit`.
fn bounded<F: FnOnce() + Send + 'static>(label: &str, limit: Duration, job: F) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    let start = Instant::now();
    std::thread::spawn(move || {
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_ok();
        let _ = tx.send(ok);
    });
    match rx.recv_timeout(limit) {
        Ok(true) => None,
        Ok(false) => Some(format!("{label}: panicked")),
        Err(_) => Some(format!(
            "{label}: still running after {:?}",
            start.elapsed()
        )),
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/pdf/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// A small text-and-shapes PDF built here, so mutations hit real structure.
fn small_pdf() -> Vec<u8> {
    let content = b"BT /F1 12 Tf 72 700 Td (Contact dana.reyes@example.com) Tj ET\n0 0.5 0.8 rg 72 600 200 40 re f\nq 0.5 w 72 500 m 300 520 l S Q";
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(), content, b"\nendstream"].concat(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

#[test]
fn mutated_pdfs_fail_cleanly_or_import() {
    let seeds = [
        small_pdf(),
        fixture("enc_rc4.pdf"),
        fixture("enc_aes256.pdf"),
    ];
    let mut rng = Lcg(0x5eed_0f_9df);
    let mut failures = vec![];
    let mut opened = 0;
    for round in 0..72 {
        let seed = &seeds[round % seeds.len()];
        let bytes = mutate(seed, &mut rng);
        let password = if round % 3 == 0 { "" } else { "spike-pass" };
        let (tx, rx) = mpsc::channel();
        if let Some(f) = bounded(
            &format!("pdf round {round}"),
            Duration::from_secs(30),
            move || {
                if let Ok(file) = pdf_import::open(bytes, password) {
                    let _ = tx.send(());
                    for i in 0..file.page_count().min(2) {
                        let _ = file.text_layer(i);
                        let _ = file.prepare(i, ImportAs::Editable { fallback_dpi: 72 });
                        let _ = file.thumbnail(i, 96);
                    }
                }
            },
        ) {
            failures.push(f);
        }
        opened += rx.try_iter().count();
    }
    assert!(failures.is_empty(), "{failures:#?}");
    // Some mutations still open: the test exercises the page code too.
    assert!(opened > 5, "only {opened} mutated PDFs opened");
}

#[test]
fn hostile_pdf_structures_are_refused_or_bounded() {
    let deep = format!(
        "<< /Type /Catalog /Pages 2 0 R /Deep {}{} >>",
        "[".repeat(100_000),
        "]".repeat(100_000)
    );
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("header only", b"%PDF-1.7\n".to_vec()),
        ("not a pdf", b"PK\x03\x04 this is a zip".to_vec()),
        ("deep nesting", small_pdf_with_root(&deep)),
        (
            "page tree cycle",
            small_pdf_with_pages("<< /Type /Pages /Kids [3 0 R 2 0 R] /Count 2 >>"),
        ),
        (
            "huge count",
            small_pdf_with_pages("<< /Type /Pages /Kids [3 0 R] /Count 2000000000 >>"),
        ),
        (
            "negative media box",
            String::from_utf8_lossy(&small_pdf())
                .replace("[0 0 612 792]", "[0 0 -612 -792]")
                .into_bytes(),
        ),
        (
            "giant media box",
            String::from_utf8_lossy(&small_pdf())
                .replace("[0 0 612 792]", "[0 0 14400000 14400000]")
                .into_bytes(),
        ),
    ];
    let mut failures = vec![];
    for (label, bytes) in cases {
        if let Some(f) = bounded(label, Duration::from_secs(30), move || {
            if let Ok(file) = pdf_import::open(bytes, "") {
                for i in 0..file.page_count().min(2) {
                    let _ = file.prepare(i, ImportAs::Pictures { dpi: 150 });
                }
            }
        }) {
            failures.push(f);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

fn small_pdf_with_root(root: &str) -> Vec<u8> {
    String::from_utf8_lossy(&small_pdf())
        .replace("<< /Type /Catalog /Pages 2 0 R >>", root)
        .into_bytes()
}
fn small_pdf_with_pages(pages: &str) -> Vec<u8> {
    String::from_utf8_lossy(&small_pdf())
        .replace("<< /Type /Pages /Kids [3 0 R] /Count 1 >>", pages)
        .into_bytes()
}

#[test]
fn mutated_svgs_fail_cleanly_or_import() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="120" viewBox="0 0 200 120">
<defs><linearGradient id="g"><stop offset="0" stop-color="#1891c1"/><stop offset="1" stop-color="#f2b134"/></linearGradient>
<clipPath id="c"><circle cx="60" cy="60" r="50"/></clipPath><filter id="f"><feGaussianBlur stdDeviation="3"/></filter></defs>
<rect x="5" y="5" width="190" height="110" rx="12" fill="url(#g)"/>
<g clip-path="url(#c)" filter="url(#f)"><path d="M10 100 C 40 10, 90 10, 120 100 S 180 60, 190 20" stroke="#222" stroke-width="6" fill="none"/></g>
<use href="#c"/><text x="20" y="30" font-size="14">Sample</text></svg>"##;
    let mut rng = Lcg(0x5eed_5a9);
    let mut failures = vec![];
    for round in 0..150 {
        let bytes = mutate(svg, &mut rng);
        if let Some(f) = bounded(
            &format!("svg round {round}"),
            Duration::from_secs(20),
            move || {
                let text = String::from_utf8_lossy(&bytes);
                if let Ok(size) = capture_core::vector_import::svg_size(&text) {
                    let _ = capture_core::vector_import::render_svg(&text, size, None);
                }
            },
        ) {
            failures.push(f);
        }
    }
    // Entity and reference bombs stay bounded.
    let bomb = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink'><defs><rect id='a0' width='1' height='1'/>{}</defs><use xlink:href='#a20'/></svg>",
        (1..=20)
            .map(|i| format!(
                "<g id='a{i}'><use xlink:href='#a{0}'/><use xlink:href='#a{0}'/></g>",
                i - 1
            ))
            .collect::<String>()
    );
    if let Some(f) = bounded("svg use bomb", Duration::from_secs(20), move || {
        if let Ok(size) = capture_core::vector_import::svg_size(&bomb) {
            let _ = capture_core::vector_import::render_svg(&bomb, size, None);
        }
    }) {
        failures.push(f);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn mutated_images_fail_cleanly_or_open() {
    let mut picture = image::RgbaImage::new(64, 48);
    for (x, y, p) in picture.enumerate_pixels_mut() {
        *p = image::Rgba([(x * 4) as u8, (y * 5) as u8, ((x ^ y) * 3) as u8, 255]);
    }
    let mut seeds = vec![];
    for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
        let mut bytes = std::io::Cursor::new(vec![]);
        let converted = image::DynamicImage::ImageRgba8(picture.clone());
        let converted = if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(converted.to_rgb8())
        } else {
            converted
        };
        converted.write_to(&mut bytes, format).unwrap();
        seeds.push((format, bytes.into_inner()));
    }
    let dir = tempfile::tempdir().unwrap();
    let mut rng = Lcg(0x5eed_1a6);
    let mut failures = vec![];
    for round in 0..160 {
        let (format, seed) = &seeds[round % seeds.len()];
        let bytes = mutate(seed, &mut rng);
        let path = dir
            .path()
            .join(format!("case{round}.{}", format.extensions_str()[0]));
        std::fs::write(&path, &bytes).unwrap();
        if let Some(f) = bounded(
            &format!("{format:?} round {round}"),
            Duration::from_secs(20),
            move || {
                let _ = capture_core::Document::open_image(&path).and_then(|d| d.render());
            },
        ) {
            failures.push(f);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
