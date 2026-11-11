//! OS transfer is isolated here; rendering and imported document validation use
//! the same engine as files and the CLI. Never inspect the clipboard at startup.
use arboard::{Clipboard, ImageData};
use capture_core::{Document, Session};
use std::borrow::Cow;
pub mod objects;

pub fn copy(document: &Document) -> Result<(), String> {
    let image = document.render().map_err(|error| error.to_string())?;
    copy_image(&image)
}

pub fn copy_image(image: &image::RgbaImage) -> Result<(), String> {
    Clipboard::new()
        .and_then(|mut clipboard| {
            clipboard.set_image(ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: Cow::Borrowed(image.as_raw()),
            })
        })
        .map_err(|error| format!("Could not copy the image: {error}"))
}

pub fn read() -> Result<Session, String> {
    let image = Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_image())
        .map_err(|error| format!("Could not read a clipboard image: {error}"))?;
    import(image).and_then(|document| Session::unsaved(document).map_err(|e| e.to_string()))
}

fn import(image: ImageData<'_>) -> Result<Document, String> {
    let width = u32::try_from(image.width).map_err(|_| "Clipboard image is too wide")?;
    let height = u32::try_from(image.height).map_err(|_| "Clipboard image is too tall")?;
    capture_core::validate_dimensions(width, height).map_err(|e| e.to_string())?;
    let expected = u64::from(width) * u64::from(height) * 4;
    if image.bytes.len() as u64 != expected {
        return Err("Clipboard image has an invalid RGBA pixel buffer".into());
    }
    let image = image::RgbaImage::from_raw(width, height, image.bytes.into_owned())
        .ok_or("Clipboard image has an invalid pixel buffer")?;
    Document::from_image(image).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_preserves_transparency_and_becomes_a_portable_unsaved_image() {
        let pixels = vec![17, 85, 201, 0, 18, 52, 171, 128, 250, 124, 9, 255];
        let document = import(ImageData {
            width: 3,
            height: 1,
            bytes: Cow::Borrowed(&pixels),
        })
        .unwrap();
        assert_eq!(document.render().unwrap().as_raw(), &pixels);
        assert!(document.annotations.is_empty());
        let json = serde_json::to_string(&document).unwrap();
        let reopened: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(reopened.render().unwrap().as_raw(), &pixels);
        let session = Session::unsaved(reopened).unwrap();
        assert!(session.is_modified());
        assert!(!session.can_undo());
    }

    #[test]
    fn rejects_oversized_dimensions_and_truncated_or_extra_rgba_bytes() {
        for (width, height, bytes) in [
            (0, 1, vec![]),
            (usize::MAX, 2, vec![]),
            (32769, 1, vec![]),
            (5001, 5000, vec![]),
            (2, 1, vec![0; 7]),
            (1, 1, vec![0; 5]),
        ] {
            assert!(
                import(ImageData {
                    width,
                    height,
                    bytes: Cow::Owned(bytes)
                })
                .is_err()
            );
        }
    }

    // Opt-in local integration check. Always replace clipboard contents with our
    // synthetic pixels before reading; never examine pre-existing user data.
    #[cfg(windows)]
    #[test]
    #[ignore = "writes synthetic content to the Windows clipboard"]
    fn windows_synthetic_copy_and_import_preserve_rendered_pixels() {
        use capture_core::{Annotation, Command, Kind};
        let mut pixels = image::RgbaImage::new(128, 64);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = image::Rgba([
                (x * 2) as u8,
                (y * 4) as u8,
                171,
                [0, 64, 128, 255][(x % 4) as usize],
            ]);
        }
        let document = Document::from_image(pixels).unwrap();
        let mut original = Session::new(document).unwrap();
        original
            .execute(Command::Add {
                annotation: Annotation {
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
                    id: 1,
                    kind: Kind::Arrow,
                    start: [8.0, 8.0],
                    end: [119.0, 55.0],
                    color: [239, 85, 55, 255],
                    width: 3,
                    text: None,
                    corner_radius: 0.0,
                    fill: None,
                    points: Vec::new(),
                    callout: None,
                },
            })
            .unwrap();
        let expected = original.document.render().unwrap();
        copy(&original.document).unwrap();
        let snapshot = read().unwrap();
        assert_eq!(snapshot.document.render().unwrap(), expected);
        assert!(snapshot.is_modified());
        assert!(snapshot.document.annotations.is_empty());
        assert!(original.is_modified());
        assert!(original.can_undo());
        assert_eq!(original.document.annotations.len(), 1);

        // The captured import is independent of subsequent clipboard changes.
        Clipboard::new()
            .unwrap()
            .set_text("ClipDesigner synthetic clipboard test".to_owned())
            .unwrap();
        assert!(read().is_err());
        assert_eq!(snapshot.document.render().unwrap(), expected);
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/clipboard");
        std::fs::create_dir_all(&directory).unwrap();
        snapshot
            .document
            .save(&directory.join("windows-roundtrip.capture.json"))
            .unwrap();
        let reopened = Document::load(&directory.join("windows-roundtrip.capture.json")).unwrap();
        assert_eq!(reopened.render().unwrap(), expected);
        expected.save(directory.join("expected.png")).unwrap();
        reopened
            .export_png(&directory.join("windows-roundtrip.png"))
            .unwrap();
        // Leave only the synthetic image available for later native UI checks.
        copy(&original.document).unwrap();
    }

    // Run only immediately after Copy All in the isolated native fixture app.
    // It deliberately does not call copy(): this checks the actual UI-to-OS path.
    #[cfg(windows)]
    #[test]
    #[ignore = "requires an explicit native Copy All of the synthetic callout fixture"]
    fn windows_native_copy_matches_saved_fixture() {
        assert_eq!(
            std::env::var("CLIPDESIGNER_NATIVE_CLIPBOARD_TEST").as_deref(),
            Ok("1"),
            "Copy the synthetic native fixture first, then opt in explicitly"
        );
        let artifacts = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../artifacts")
            .canonicalize()
            .unwrap();
        let fixture = artifacts
            .join(
                std::env::var("CLIPDESIGNER_NATIVE_CLIPBOARD_FIXTURE")
                    .unwrap_or_else(|_| "native-alpha7/native-callouts.capture.json".into()),
            )
            .canonicalize()
            .unwrap();
        assert!(
            fixture.starts_with(&artifacts),
            "Use a generated artifact fixture"
        );
        let directory = fixture.parent().unwrap();
        let expected = Document::load(&fixture).unwrap().render().unwrap();
        let imported = read().unwrap();
        let actual = imported.document.render().unwrap();
        assert_eq!(actual.dimensions(), expected.dimensions());
        assert!(
            actual == expected,
            "Native clipboard pixels differ from the saved project: {} mismatches; first {:?}",
            actual
                .pixels()
                .zip(expected.pixels())
                .filter(|(a, b)| a != b)
                .count(),
            actual
                .enumerate_pixels()
                .find_map(|(x, y, p)| (*p != *expected.get_pixel(x, y)).then_some((
                    x,
                    y,
                    p.0,
                    expected.get_pixel(x, y).0
                )))
        );
        assert!(imported.is_modified());
        assert!(imported.document.annotations.is_empty());
        // Persist only after proving this is our original synthetic content.
        actual.save(directory.join("native-clipboard.png")).unwrap();
        imported
            .document
            .save(&directory.join("native-clipboard.capture.json"))
            .unwrap();
        let reopened = Document::load(&directory.join("native-clipboard.capture.json")).unwrap();
        assert!(reopened.render().unwrap() == expected);
        let report = serde_json::json!({
            "passed": true, "source": "native Copy All", "width": actual.width(),
            "height": actual.height(), "exact_pixels": actual.width() * actual.height(),
            "import_save_reopen": true
        });
        std::fs::write(
            directory.join("native-clipboard-check.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
    }
}
