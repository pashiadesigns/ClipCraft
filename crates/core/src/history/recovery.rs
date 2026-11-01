//! Bounded local previous versions and a non-mutating integrity scan.
use super::*;
const KEEP: usize = 3;
const BUDGET: u64 = 256_000_000;
#[derive(Clone, Debug)]
pub struct Health {
    pub entry: Entry,
    pub problems: Vec<String>,
    pub revisions: Vec<Entry>,
}
impl History {
    fn revision_dir(&self, id: &str) -> Result<PathBuf> {
        self.paths(id)?;
        Ok(self.root.join("revisions").join(id))
    }
    pub fn revisions(&self, id: &str) -> Result<Vec<Entry>> {
        let dir = self.revision_dir(id)?;
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut result = vec![];
        for item in fs::read_dir(&dir)?.take(1000) {
            let p = item?.path();
            if p.extension().and_then(|s| s.to_str()) != Some("meta") {
                continue;
            }
            if fs::metadata(&p)?.len() > 8192 {
                continue;
            }
            let Ok(e) = serde_json::from_slice::<Entry>(&fs::read(&p)?) else {
                continue;
            };
            if e.id == id
                && self.paths(e.storage_id()).is_ok()
                && p.file_stem().and_then(|n| n.to_str()) == Some(e.storage_id())
                && dir
                    .join(format!("{}.capture.json", e.storage_id()))
                    .is_file()
            {
                result.push(e);
            }
        }
        result.sort_by(|a, b| {
            b.updated_ms
                .unwrap_or(b.created_ms)
                .cmp(&a.updated_ms.unwrap_or(a.created_ms))
                .then(b.storage_id().cmp(a.storage_id()))
        });
        Ok(result)
    }
    pub fn check_integrity(&self) -> Vec<Health> {
        self.entries()
            .iter()
            .map(|e| {
                let mut problems = vec![];
                if let Err(err) = self.load(&e.id) {
                    problems.push(format!("Project: {err}"));
                }
                if let Err(err) = self.thumbnail(&e.id) {
                    problems.push(format!("Thumbnail: {err}"));
                }
                let revisions = match self.revisions(&e.id) {
                    Ok(r) => r,
                    Err(e) => {
                        problems.push(format!("Previous versions: {e}"));
                        vec![]
                    }
                };
                Health {
                    entry: e.clone(),
                    problems,
                    revisions,
                }
            })
            .collect()
    }
    /// Restore to an independent Library item. Never replace an open/changed or
    /// damaged project, and validate the chosen revision again at the boundary.
    pub fn restore_revision(&mut self, revision: &Entry) -> Result<Entry> {
        if !self.revisions(&revision.id)?.contains(revision) {
            return Err(Error::Invalid(
                "This previous version is no longer available.".into(),
            ));
        }
        let d = Document::load(
            &self
                .revision_dir(&revision.id)?
                .join(format!("{}.capture.json", revision.storage_id())),
        )?;
        let new = self.create(
            &d,
            &format!(
                "Recovered {}",
                revision.label.chars().take(90).collect::<String>()
            ),
            revision.origin,
        )?;
        self.edit_metadata(&new, &new.label, &revision.tags)
    }
    pub(super) fn archive_revision(&self, e: &Entry) -> Result<()> {
        let source = self.paths(e.storage_id())?[0].clone();
        // A damaged current revision must not prevent saving repaired work.
        if Document::load(&source).is_err() {
            return Ok(());
        }
        let dir = self.revision_dir(&e.id)?;
        fs::create_dir_all(&dir)?;
        atomic_write(
            &dir.join(format!("{}.capture.json", e.storage_id())),
            &fs::read(source)?,
        )?;
        atomic_write(
            &dir.join(format!("{}.meta", e.storage_id())),
            &serde_json::to_vec(e)?,
        )?;
        Ok(())
    }
    pub(super) fn prune_revisions(&self) -> Result<()> {
        let mut kept = vec![];
        for e in self.entries() {
            for (index, r) in self.revisions(&e.id)?.into_iter().enumerate() {
                if index >= KEEP {
                    self.remove_revision(&r)?;
                } else {
                    kept.push(r);
                }
            }
        }
        kept.sort_by_key(|e| e.updated_ms.unwrap_or(e.created_ms));
        let mut sizes = vec![];
        let mut total = 0;
        for r in kept {
            let len = fs::metadata(
                self.revision_dir(&r.id)?
                    .join(format!("{}.capture.json", r.storage_id())),
            )?
            .len();
            total += len;
            sizes.push((r, len));
        }
        for (r, len) in sizes {
            if total <= BUDGET {
                break;
            }
            self.remove_revision(&r)?;
            total -= len;
        }
        Ok(())
    }
    fn remove_revision(&self, e: &Entry) -> Result<()> {
        let dir = self.revision_dir(&e.id)?;
        for suffix in ["capture.json", "meta"] {
            let p = dir.join(format!("{}.{suffix}", e.storage_id()));
            if p.is_file() {
                fs::remove_file(p)?;
            }
        }
        Ok(())
    }
    pub(super) fn delete_revisions(&self, id: &str) -> Result<()> {
        for e in self.revisions(id)? {
            self.remove_revision(&e)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revisions_restore_independently_and_report_damage_without_modifying_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let original = Document::canvas(40, 30, [200, 30, 20, 255]).unwrap();
        let mut entry = h.create(&original, "Guide", Origin::Canvas).unwrap();
        for n in 0..5 {
            entry = h
                .update(&entry, &Document::canvas(40, 30, [n, 40, 60, 255]).unwrap())
                .unwrap();
        }
        assert_eq!(h.revisions(&entry.id).unwrap().len(), 3);
        assert!(h.check_integrity()[0].problems.is_empty());
        let revision = h.revisions(&entry.id).unwrap()[1].clone();
        let path = h.paths(entry.storage_id()).unwrap()[0].clone();
        fs::write(&path, b"broken fixture").unwrap();
        let report = h.check_integrity();
        assert!(report[0].problems.iter().any(|p| p.starts_with("Project:")));
        let restored = h.restore_revision(&revision).unwrap();
        assert_ne!(restored.id, entry.id);
        assert_eq!(fs::read(&path).unwrap(), b"broken fixture");
        assert_eq!(
            h.load(&restored.id)
                .unwrap()
                .render()
                .unwrap()
                .get_pixel(0, 0)
                .0,
            [2, 40, 60, 255]
        );
        let mut forged = revision.clone();
        forged.revision = Some("../../index".into());
        assert!(h.restore_revision(&forged).is_err());
        h.discard(&entry).unwrap();
        assert!(h.revisions(&entry.id).unwrap().is_empty());
    }
}
