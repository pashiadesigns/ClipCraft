//! Local media resolution and portable .vclip save. Never fetch project URLs.
use crate::{Document, Error, Result};
use std::path::Path;
pub fn resolve(doc: &mut Document, path: &Path) -> Result<()> {
    if let Some(v) = doc.video.as_mut() {
        for source in v.media_paths_mut() {
            if source.contains("://")
                || !super::editor::local_media_path(source)
                || source.contains('\0')
            {
                return Err(Error::Invalid("Video media must be a local file.".into()));
            }
            let p = Path::new(source);
            if p.is_relative() {
                if p.components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    return Err(Error::Invalid(
                        "Video media cannot escape the project folder.".into(),
                    ));
                }
                *source = path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(p)
                    .to_string_lossy()
                    .into_owned();
            }
        }
    }
    Ok(())
}
pub fn portable(doc: &Document, path: &Path) -> Result<Document> {
    let mut out = doc.clone();
    if !crate::project_file::is_video_project(path) {
        return Ok(out);
    }
    let v = out
        .video
        .as_mut()
        .ok_or_else(|| Error::Invalid("Save image projects as .iclip.".into()))?;
    let folder = path.with_extension("vclip-media");
    std::fs::create_dir_all(&folder)?;
    let name = folder.file_name().unwrap();
    let mut copied: std::collections::HashMap<std::path::PathBuf, String> =
        std::collections::HashMap::new();
    for (i, source) in v.media_paths_mut().enumerate() {
        let original = Path::new(source);
        let meta = std::fs::metadata(original).map_err(|e| {
            Error::Invalid(format!(
                "Cannot find video media {}: {e}",
                original.display()
            ))
        })?;
        if !meta.is_file() {
            return Err(Error::Invalid("Video media must be a file.".into()));
        }
        let canonical = original.canonicalize()?;
        if let Some(relative) = copied.get(&canonical) {
            *source = relative.clone();
            continue;
        }
        if canonical.parent() == Some(folder.canonicalize()?.as_path()) {
            let relative = Path::new(name)
                .join(canonical.file_name().unwrap())
                .to_string_lossy()
                .into_owned();
            copied.insert(canonical, relative.clone());
            *source = relative;
            continue;
        }
        // A unique name prevents replacing assets still referenced by an older saved manifest.
        let mut stem = format!(
            "source-{i}-{}-{}.{}",
            meta.len(),
            meta.modified()?
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            original
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("bin")
        );
        let mut target = folder.join(&stem);
        let mut suffix = 0;
        while target.exists() && !same_bytes(original, &target)? {
            suffix += 1;
            let ext = original
                .extension()
                .and_then(|v| v.to_str())
                .unwrap_or("bin");
            stem = format!("source-{i}-{}-{suffix}.{ext}", meta.len());
            target = folder.join(&stem);
        }
        if !target.exists() {
            let mut temp = tempfile::NamedTempFile::new_in(&folder)?;
            let mut input = std::fs::File::open(original)?;
            std::io::copy(&mut input, &mut temp)?;
            temp.as_file().sync_all()?;
            temp.persist_noclobber(&target)
                .map_err(|e| Error::Io(e.error))?;
        }
        *source = Path::new(name).join(stem).to_string_lossy().into_owned();
        copied.insert(canonical, source.clone());
    }
    Ok(out)
}

fn same_bytes(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    if a.metadata()?.len() != b.metadata()?.len() {
        return Ok(false);
    }
    let mut a = std::io::BufReader::new(std::fs::File::open(a)?);
    let mut b = std::io::BufReader::new(std::fs::File::open(b)?);
    let mut left = [0; 65536];
    let mut right = [0; 65536];
    loop {
        let n = a.read(&mut left)?;
        if n == 0 {
            return Ok(true);
        }
        b.read_exact(&mut right[..n])?;
        if left[..n] != right[..n] {
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_media_survives_library_reopen_duplicate_and_portable_save() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("capture.mp4");
        std::fs::write(&source, b"local media fixture").unwrap();
        let path = source
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut doc = Document::blank(320, 180).unwrap();
        doc.video = Some(crate::video::editor::Project::new(
            path,
            [320, 180],
            1000,
            30,
        ));
        let mut library = crate::history::History::open(&dir.path().join("library")).unwrap();
        let entry = library
            .record(&doc, "Recording", crate::history::Origin::Capture)
            .unwrap();
        let loaded = library.load(&entry.id).unwrap();
        assert_eq!(loaded.video, doc.video);
        let copied = library.duplicate(&entry).unwrap();
        assert_eq!(library.load(&copied.id).unwrap().video, doc.video);
        let project = dir.path().join("share.vclip");
        loaded.save(&project).unwrap();
        let reopened = Document::load(&project).unwrap();
        let media = Path::new(&reopened.video.as_ref().unwrap().source);
        assert!(media.is_file());
        assert_eq!(std::fs::read(media).unwrap(), b"local media fixture");
    }
    #[test]
    fn local_extended_drive_paths_are_not_confused_with_network_paths() {
        for (source, valid) in [
            (r"\\?\C:\Clips\capture.mp4", true),
            (r"\\server\share\clip.mp4", false),
            (r"\\?\UNC\server\clip.mp4", false),
            ("https://example.com/clip.mp4", false),
            (r"..\clip.mp4", false),
        ] {
            let mut doc = Document::blank(320, 180).unwrap();
            doc.video = Some(crate::video::editor::Project::new(
                source.into(),
                [320, 180],
                1000,
                30,
            ));
            assert_eq!(
                resolve(&mut doc, Path::new("C:/Projects/project.vclip")).is_ok(),
                valid,
                "{source}"
            );
        }
    }
}
