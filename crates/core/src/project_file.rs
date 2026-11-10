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
/// Checks a project name typed for Rename (FILE-007) and returns it trimmed.
pub fn check_name(name: &str) -> crate::Result<&str> {
    let name = name.trim();
    let reserved = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 200
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || ["CON", "PRN", "AUX", "NUL"].contains(&reserved.as_str())
        || (reserved.len() == 4
            && (reserved.starts_with("COM") || reserved.starts_with("LPT"))
            && reserved.as_bytes()[3].is_ascii_digit())
    {
        return Err(crate::Error::Invalid(
            "Names can't be empty, end with a dot, or use < > : \" / \\ | ? *.".into(),
        ));
    }
    Ok(name)
}
/// The media folder that travels with a `.vclip`.
pub fn media_folder(path: &Path) -> PathBuf {
    path.with_extension("vclip-media")
}
/// The path `path` gets when renamed to `name`, keeping its extension.
pub fn renamed(path: &Path, name: &str) -> crate::Result<PathBuf> {
    let name = check_name(name)?;
    let mut file = name.to_owned();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        file.push('.');
        file.push_str(ext);
    }
    Ok(path.with_file_name(file))
}
fn same_file(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}
/// Renames a saved project on disk (FILE-007). A `.vclip` takes its media
/// folder along and its manifest is rewritten to point into the renamed
/// folder. Nothing changes when the name is taken or invalid.
pub fn rename(path: &Path, name: &str) -> crate::Result<PathBuf> {
    let target = renamed(path, name)?;
    if target == path {
        return Ok(target);
    }
    let taken = |p: &Path| p.exists() && !same_file(p, path);
    if taken(&target) {
        return Err(crate::Error::Invalid(
            "A project with that name is already in this folder.".into(),
        ));
    }
    if !is_video_project(path) {
        std::fs::rename(path, &target)?;
        return Ok(target);
    }
    let (from, to) = (media_folder(path), media_folder(&target));
    if to.exists() && !same_file(&from, &to) {
        return Err(crate::Error::Invalid(
            "A media folder with that name is already in this folder.".into(),
        ));
    }
    let mut doc = crate::Document::load(path)?;
    let moved = from.is_dir();
    if moved {
        std::fs::rename(&from, &to)?;
        relocate(&mut doc.video, &from, &to);
    }
    let result = doc.save(&target).and_then(|()| {
        if !same_file(&target, path) {
            std::fs::remove_file(path)?;
        }
        Ok(())
    });
    if let Err(e) = result {
        if moved {
            let _ = std::fs::rename(&to, &from);
        }
        return Err(e);
    }
    Ok(target)
}
/// Points media inside `from` at the same files inside `to`.
pub(crate) fn relocate(video: &mut Option<crate::video::editor::Project>, from: &Path, to: &Path) {
    let Some(v) = video else { return };
    let from = from.to_string_lossy().into_owned();
    let to = to.to_string_lossy().into_owned();
    for source in v.media_paths_mut() {
        let rest = source
            .get(..from.len())
            .filter(|head| head.eq_ignore_ascii_case(&from))
            .map(|_| source[from.len()..].to_owned());
        if let Some(rest) = rest.filter(|r| r.starts_with(['\\', '/'])) {
            *source = format!("{to}{rest}");
        }
    }
}
/// The first free "<name> (copy)" path next to `path` (Duplicate).
pub fn copy_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned());
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem} (copy)")
        } else {
            format!("{stem} (copy {n})")
        };
        let mut candidate = path.with_file_name(&name);
        if let Some(ext) = &ext {
            candidate.set_extension(ext);
        }
        if !candidate.exists() && !media_folder(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
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
    fn rename_checks_names_and_never_replaces_another_project() {
        let dir = tempfile::tempdir().unwrap();
        let document: crate::Document =
            serde_json::from_str(include_str!("../tests/fixtures/properties.capture.json"))
                .unwrap();
        let path = dir.path().join("Expense report.iclip");
        document.save(&path).unwrap();
        let other = dir.path().join("Quarterly review.iclip");
        document.save(&other).unwrap();
        for bad in ["", "  ", "a/b", "what?", "trailing.", "CON", "com1"] {
            assert!(rename(&path, bad).is_err(), "{bad:?} is refused");
        }
        assert!(
            rename(&path, "quarterly review").is_err(),
            "names are case-insensitive"
        );
        assert!(path.exists() && other.exists());
        let renamed = rename(&path, "  Expense steps ").unwrap();
        assert_eq!(renamed, dir.path().join("Expense steps.iclip"));
        assert!(!path.exists() && open(&renamed).is_ok());
        // Changing only the case renames in place.
        let cased = rename(&renamed, "expense STEPS").unwrap();
        assert!(cased.exists());
        assert_eq!(
            copy_path(&cased),
            dir.path().join("expense STEPS (copy).iclip")
        );
        document.save(&copy_path(&cased)).unwrap();
        assert_eq!(
            copy_path(&cased),
            dir.path().join("expense STEPS (copy 2).iclip")
        );
    }
    #[test]
    fn relocate_moves_only_media_inside_the_old_folder() {
        let mut v = Some(crate::video::editor::Project::new(
            r"C:\p\Old.vclip-media\source-1.mp4".into(),
            [640, 360],
            1000,
            30,
        ));
        relocate(
            &mut v,
            Path::new(r"C:\p\Old.vclip-media"),
            Path::new(r"C:\p\New.vclip-media"),
        );
        assert_eq!(
            v.as_ref().unwrap().source,
            r"C:\p\New.vclip-media\source-1.mp4"
        );
        let mut w = Some(crate::video::editor::Project::new(
            r"C:\p\Old.vclip-media2\a.mp4".into(),
            [640, 360],
            1000,
            30,
        ));
        relocate(
            &mut w,
            Path::new(r"C:\p\Old.vclip-media"),
            Path::new(r"C:\p\New.vclip-media"),
        );
        assert_eq!(w.unwrap().source, r"C:\p\Old.vclip-media2\a.mp4");
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
