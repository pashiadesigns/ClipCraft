//! Branded project filenames. Image clips remain validated JSON documents.
use std::path::{Path, PathBuf};

pub const IMAGE_EXTENSION: &str = "iclip";
/// Editable video clips use a validated JSON manifest and adjacent media folder.
pub const VIDEO_EXTENSION: &str = "vclip";

pub fn is_image_project(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case(IMAGE_EXTENSION) || v.eq_ignore_ascii_case("json"))
}
pub fn is_video_project(path: &Path) -> bool {
    path.extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case(VIDEO_EXTENSION))
}
pub fn image_path(path: &Path) -> PathBuf {
    if path
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case(IMAGE_EXTENSION))
    {
        return path.to_path_buf();
    }
    let mut result = path.to_path_buf();
    if path
        .file_name()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.ends_with(".capture.json"))
    {
        result.set_extension("");
    }
    result.set_extension(IMAGE_EXTENSION);
    result
}
pub fn open(path: &Path) -> crate::Result<crate::Document> {
    if is_video_project(path) {
        let doc = crate::Document::load(path)?;
        if doc.video.is_none() {
            return Err(crate::Error::Invalid("This is not a video clip.".into()));
        }
        return Ok(doc);
    }
    if is_image_project(path) {
        crate::Document::load(path)
    } else {
        crate::Document::open_image(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branded_projects_are_json_and_roundtrip_without_losing_objects() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Guide with spaces.ICLIP");
        let document: crate::Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        document.save(&path).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(json["annotations"].is_array());
        assert_eq!(
            serde_json::to_value(open(&path).unwrap()).unwrap(),
            serde_json::to_value(&document).unwrap()
        );
        assert!(open(&dir.path().join("future.vclip")).is_err());
        std::fs::write(&path, b"not json").unwrap();
        assert!(open(&path).is_err());
    }
    #[test]
    fn suggested_names_use_only_one_branded_suffix() {
        for (before, after) in [
            ("guide", "guide.iclip"),
            ("guide.capture.json", "guide.iclip"),
            ("guide.json", "guide.iclip"),
            ("guide.ICLIP", "guide.ICLIP"),
            ("guide.png", "guide.iclip"),
        ] {
            assert_eq!(image_path(Path::new(before)), Path::new(after));
        }
    }
}
