//! Opt-in cleanup only for files explicitly created and marked by ClipDesigner.
use super::*;
use std::collections::BTreeSet;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OwnedFile {
    pub path: PathBuf,
    pub bytes: u64,
    modified: u128,
}
fn identity(path: &Path) -> Result<OwnedFile> {
    let path = path.canonicalize()?;
    let m = fs::metadata(&path)?;
    if !m.is_file() {
        return Err(Error::Invalid("Media is not a regular file.".into()));
    }
    Ok(OwnedFile {
        path,
        bytes: m.len(),
        modified: m
            .modified()?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    })
}
fn marker(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".clipdesigner-owned");
    PathBuf::from(p)
}
/// Called only after successfully creating a recording, take or still, never on import.
pub fn mark_created(path: &Path) -> Result<()> {
    let record = identity(path)?;
    atomic_write(&marker(&record.path), &serde_json::to_vec(&record)?)
}
fn owned(root: &Path, path: &Path) -> Result<Option<OwnedFile>> {
    let Ok(record) = identity(path) else {
        return Ok(None);
    };
    let root = root.canonicalize()?;
    let allowed = ["recordings", "voiceovers", "stills", "backgrounds"]
        .iter()
        .any(|dir| record.path.starts_with(root.join(dir)));
    if !allowed {
        return Ok(None);
    }
    let m = marker(&record.path);
    if !m.is_file() || fs::metadata(&m)?.len() > 32768 {
        return Ok(None);
    }
    let saved: OwnedFile = serde_json::from_slice(&fs::read(m)?)?;
    Ok((record == saved).then_some(record))
}
fn media(doc: &Document) -> BTreeSet<PathBuf> {
    doc.video
        .iter()
        .flat_map(|v| v.media_paths())
        .filter_map(|p| Path::new(p).canonicalize().ok())
        .collect()
}
impl History {
    pub fn media_cleanup_plan(
        &self,
        entries: &[Entry],
        protect: &[PathBuf],
    ) -> Result<Vec<OwnedFile>> {
        let ids: BTreeSet<_> = entries.iter().map(|e| e.id.clone()).collect();
        let mut candidates = BTreeSet::new();
        for entry in entries {
            candidates.extend(media(&self.load(&entry.id)?));
            for version in self.revisions(&entry.id)? {
                let path = self
                    .root
                    .join("revisions")
                    .join(&entry.id)
                    .join(format!("{}.capture.json", version.storage_id()));
                candidates.extend(media(&Document::load(&path)?));
            }
        }
        let retained = self.retained_media(&ids, protect)?;
        candidates
            .difference(&retained)
            .filter_map(|p| match owned(&self.root, p) {
                Ok(Some(f)) => Some(Ok(f)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
    fn retained_media(
        &self,
        exclude: &BTreeSet<String>,
        protect: &[PathBuf],
    ) -> Result<BTreeSet<PathBuf>> {
        let mut paths = BTreeSet::new();
        for entry in index_at(&self.root)?.entries {
            if exclude.contains(&entry.id) {
                continue;
            }
            paths.extend(media(&Document::load(&self.paths(entry.storage_id())?[0])?));
            for version in self.revisions(&entry.id)? {
                paths.extend(media(&Document::load(
                    &self
                        .root
                        .join("revisions")
                        .join(&entry.id)
                        .join(format!("{}.capture.json", version.storage_id())),
                )?));
            }
        }
        // Template/saved-project libraries may be configured outside this root.
        let mut folders = vec![self.root.join("templates")];
        folders.extend(protect.iter().filter(|p| p.is_dir()).cloned());
        let mut count = 0;
        while let Some(folder) = folders.pop() {
            if !folder.exists() {
                continue;
            }
            for entry in fs::read_dir(folder)? {
                let entry = entry?;
                count += 1;
                if count > 20_000 {
                    return Err(Error::Invalid("Too many template files to verify safe media cleanup. Keep media for this deletion.".into()));
                }
                if entry.file_type()?.is_symlink() {
                    return Err(Error::Invalid("A linked template folder cannot be checked safely. Keep media for this deletion.".into()));
                }
                let p = entry.path();
                if p.is_dir() {
                    folders.push(p);
                    continue;
                }
                let name = p.file_name().unwrap().to_string_lossy();
                if name.ends_with(".capture.json")
                    || name.ends_with(".vclip")
                    || name.ends_with(".iclip")
                {
                    paths.extend(media(&Document::load(&p)?));
                }
            }
        }
        paths.extend(
            protect
                .iter()
                .filter(|p| p.is_file())
                .filter_map(|p| p.canonicalize().ok()),
        );
        Ok(paths)
    }
    /// Rescan after Library deletion under its lock. A changed, shared, unknown
    /// or replaced asset is kept, even if it was eligible in the dialog preview.
    pub fn remove_unused_media(
        &mut self,
        planned: &[OwnedFile],
        protect: &[PathBuf],
    ) -> Result<(usize, u64)> {
        let _lock = self.management_lock()?;
        let retained = self.retained_media(&BTreeSet::new(), protect)?;
        let mut removed = (0, 0);
        for file in planned {
            if retained.contains(&file.path)
                || owned(&self.root, &file.path)?.as_ref() != Some(file)
            {
                continue;
            }
            fs::remove_file(&file.path)?;
            let _ = fs::remove_file(marker(&file.path));
            removed.0 += 1;
            removed.1 += file.bytes;
        }
        Ok(removed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cleanup_keeps_imports_shared_revisions_and_replaced_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        fs::create_dir(dir.path().join("recordings")).unwrap();
        let path = dir.path().join("recordings/test.mp4");
        fs::write(&path, b"created").unwrap();
        let mut doc = Document::blank(320, 180).unwrap();
        doc.video = Some(crate::video::editor::Project::new(
            path.to_string_lossy().into(),
            [320, 180],
            1000,
            30,
        ));
        let first = h.record(&doc, "First", Origin::Capture).unwrap();
        assert!(
            h.media_cleanup_plan(std::slice::from_ref(&first), &[])
                .unwrap()
                .is_empty()
        );
        mark_created(&path).unwrap();
        let second = h.duplicate(&first).unwrap();
        assert!(
            h.media_cleanup_plan(std::slice::from_ref(&first), &[])
                .unwrap()
                .is_empty()
        );
        let all = vec![first, second];
        let plan = h.media_cleanup_plan(&all, &[]).unwrap();
        assert_eq!(plan.len(), 1);
        h.batch(&all, Batch::Delete).unwrap();
        fs::write(&path, b"replaced original").unwrap();
        assert_eq!(h.remove_unused_media(&plan, &[]).unwrap().0, 0);
        assert!(path.exists());
        mark_created(&path).unwrap();
        let fresh = owned(dir.path(), &path).unwrap().unwrap();
        assert_eq!(h.remove_unused_media(&[fresh], &[]).unwrap().0, 1);
        assert!(!path.exists());
    }
}
