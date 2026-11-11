//! Pack project (approved beta 6 tab 3): a project and all its media in one
//! `.cdpack` file, for handing work to someone else. Opening one unpacks a
//! normal project into its own folder; the pack itself is never changed.
use crate::{Document, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "cdpack";
const FORMAT: &str = "clipdesigner-pack";
/// A pack unpacks to at most this much (the ZIP format's own limit).
const MAX_TOTAL: u64 = u32::MAX as u64;

#[derive(Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    /// The project file at the top of the pack.
    project: String,
}

/// A file name Windows accepts, from a project's title.
pub fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches('.')
        .chars()
        .take(80)
        .collect();
    if cleaned.is_empty() || crate::zip::check_name(&cleaned).is_err() {
        "Project".into()
    } else {
        cleaned
    }
}

/// Packs `document` (an image or video project) into `out`. `stop` cancels;
/// `progress` gets 0–1.
pub fn pack(
    document: &Document,
    title: &str,
    out: &Path,
    stop: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(f32),
) -> Result<()> {
    let folder = out.parent().unwrap_or(Path::new("."));
    let work = tempfile::tempdir()?;
    let extension = if document.video.is_some() {
        "vclip"
    } else {
        "iclip"
    };
    let project = format!("{}.{extension}", file_stem(title));
    let project_path = work.path().join(&project);
    // Saving a video project copies its media beside it.
    document.save(&project_path)?;
    let mut files = vec![(project.clone(), project_path.clone())];
    let media = project_path.with_extension("vclip-media");
    if media.is_dir() {
        let prefix = media.file_name().unwrap().to_string_lossy().into_owned();
        let mut list: Vec<_> = std::fs::read_dir(&media)?.collect::<std::io::Result<_>>()?;
        list.sort_by_key(|e| e.file_name());
        for e in list {
            if e.file_type()?.is_file() {
                let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
                files.push((name, e.path()));
            }
        }
    }
    let mut total = 0u64;
    for (_, path) in &files {
        total += std::fs::metadata(path)?.len();
    }
    if total > MAX_TOTAL - (1 << 20) {
        return Err(Error::Invalid(
            "This project and its media are over 4 GB, too large for one pack. Share the project folder instead."
                .into(),
        ));
    }
    let temp = tempfile::Builder::new()
        .prefix(".clipdesigner-pack-")
        .suffix(".tmp")
        .tempfile_in(folder)?;
    let mut writer = crate::zip::Writer::new(std::io::BufWriter::new(temp.reopen()?));
    writer.bytes(
        "pack.json",
        &serde_json::to_vec_pretty(&Manifest {
            format: FORMAT.into(),
            version: 1,
            project: project.clone(),
        })?,
    )?;
    let mut done = 0u64;
    for (name, path) in &files {
        let file = std::fs::File::open(path)?;
        writer.stream(name, file, stop, &mut |n| {
            done += n;
            progress(done as f32 / total.max(1) as f32);
        })?;
    }
    writer.finish()?;
    temp.persist(out).map_err(|e| Error::Io(e.error))?;
    progress(1.);
    Ok(())
}

/// Where a pack unpacks by default: a new folder beside it, named after it.
pub fn default_folder(pack: &Path) -> PathBuf {
    let parent = pack.parent().unwrap_or(Path::new("."));
    let stem = pack
        .file_stem()
        .map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    let mut folder = parent.join(&stem);
    let mut n = 2;
    while folder.exists() {
        folder = parent.join(format!("{stem} ({n})"));
        n += 1;
    }
    folder
}

/// Unpacks `pack` into the new folder `into` and returns the project file.
/// Nothing is left behind if any part fails.
pub fn unpack(pack: &Path, into: &Path, stop: &dyn Fn() -> bool) -> Result<PathBuf> {
    if into.exists() {
        return Err(Error::Invalid(format!(
            "{} already exists. Choose another folder.",
            into.display()
        )));
    }
    let mut reader = crate::zip::Reader::new(std::fs::File::open(pack)?, MAX_TOTAL)?;
    let manifest: Manifest =
        serde_json::from_slice(&reader.read("pack.json", 64 << 10)?).map_err(|_| {
            Error::Invalid("This pack is damaged or wasn't made by ClipDesigner.".into())
        })?;
    if manifest.format != FORMAT || manifest.version != 1 {
        return Err(Error::Invalid(
            "This pack was made by a newer ClipDesigner. Update ClipDesigner to open it.".into(),
        ));
    }
    let project = manifest.project;
    let video = project.to_ascii_lowercase().ends_with(".vclip");
    if project.contains('/') || !(video || project.to_ascii_lowercase().ends_with(".iclip")) {
        return Err(Error::Invalid(
            "This pack's project file isn't one ClipDesigner opens.".into(),
        ));
    }
    let media = format!(
        "{}.vclip-media/",
        project.rsplit_once('.').map_or(project.as_str(), |p| p.0)
    );
    // Only the project, its manifest and its media folder may be in a pack.
    for e in &reader.entries {
        let ok = e.name == "pack.json"
            || e.name == project
            || (video && e.name.starts_with(&media) && !e.name[media.len()..].contains('/'));
        if !ok {
            return Err(Error::Invalid(format!(
                "This pack holds {}, which doesn't belong in a project.",
                e.name
            )));
        }
    }
    if reader.find(&project).is_none() {
        return Err(Error::Invalid("This pack has no project in it.".into()));
    }
    let parent = into.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let work = tempfile::Builder::new()
        .prefix(".clipdesigner-unpack-")
        .tempdir_in(parent)?;
    for e in reader.entries.clone() {
        if e.name == "pack.json" {
            continue;
        }
        let target = work.path().join(e.name.replace('/', "\\"));
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(&target)?);
        reader.copy(&e, &mut file, stop)?;
        std::io::Write::flush(&mut file)?;
    }
    // The project must open before it is kept.
    Document::load(&work.path().join(&project))?;
    let work = work.keep();
    std::fs::rename(&work, into).inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&work);
    })?;
    Ok(into.join(project))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_project_packs_and_unpacks() {
        let dir = tempfile::tempdir().unwrap();
        let mut document = Document::blank(120, 80).unwrap();
        document.annotations.clear();
        let out = dir.path().join("Onboarding guide.cdpack");
        let mut last = 0.;
        pack(&document, "Onboarding: guide?", &out, &|| false, &mut |p| {
            last = p
        })
        .unwrap();
        assert_eq!(last, 1.);
        let folder = default_folder(&out);
        assert_eq!(folder, dir.path().join("Onboarding guide"));
        let project = unpack(&out, &folder, &|| false).unwrap();
        assert_eq!(project, folder.join("Onboarding guide.iclip"));
        let back = Document::load(&project).unwrap();
        assert_eq!((back.width, back.height), (120, 80));
        // A second unpack picks a new folder, and the first is never replaced.
        assert_eq!(
            default_folder(&out),
            dir.path().join("Onboarding guide (2)")
        );
        assert!(unpack(&out, &folder, &|| false).is_err());
    }

    #[test]
    fn packs_with_stray_files_are_refused_and_leave_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = br#"{"format":"clipdesigner-pack","version":1,"project":"A.iclip"}"#;
        let doc = serde_json::to_vec(&Document::blank(10, 10).unwrap()).unwrap();
        let bytes = crate::zip::to_vec(&[
            ("pack.json", manifest),
            ("A.iclip", &doc),
            ("startup/run.cmd", b"echo hi"),
        ])
        .unwrap();
        let path = dir.path().join("bad.cdpack");
        std::fs::write(&path, bytes).unwrap();
        let into = dir.path().join("out");
        assert!(unpack(&path, &into, &|| false).is_err());
        assert!(!into.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
