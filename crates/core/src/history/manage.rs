//! Transactional bulk metadata/removal and explicit, non-evicting capacity.
use super::*;
pub const HARD_ENTRIES: usize = 1000;
pub const HARD_BYTES: u64 = 16_000_000_000;
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Limits {
    pub entries: usize,
    pub bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: MAX_ENTRIES,
            bytes: MAX_BYTES,
        }
    }
}
impl Limits {
    fn validate(self) -> Result<Self> {
        if self.entries == 0
            || self.entries > HARD_ENTRIES
            || self.bytes < 1_000_000
            || self.bytes > HARD_BYTES
        {
            return Err(Error::Invalid(
                "Capacity must be 1–1000 items and 1–16000 MB".into(),
            ));
        }
        Ok(self)
    }
}
pub enum Batch<'a> {
    AddTags(&'a [String]),
    RemoveTags(&'a [String]),
    Delete,
}
impl History {
    pub fn limits(&self) -> Result<Limits> {
        let path = self.root.join("limits.json");
        match fs::metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Limits::default()),
            Err(e) => return Err(e.into()),
            Ok(m) if m.len() > 4096 => {
                return Err(Error::Invalid("Invalid Library capacity file".into()));
            }
            _ => (),
        }
        serde_json::from_slice::<Limits>(&fs::read(path)?)?.validate()
    }
    pub fn set_limits(&mut self, limits: Limits) -> Result<()> {
        limits.validate()?;
        let lock = self.management_lock()?;
        let next = index_at(&self.root)?;
        if next.entries.len() > limits.entries
            || next.entries.iter().map(|e| e.bytes).sum::<u64>() > limits.bytes
        {
            return Err(Error::Invalid("Capacity is below current usage. Export or delete selected items first; nothing was removed.".into()));
        }
        atomic_write(
            &self.root.join("limits.json"),
            &serde_json::to_vec_pretty(&limits)?,
        )?;
        drop(lock);
        Ok(())
    }
    fn management_lock(&self) -> Result<fs::File> {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("history.lock"))?;
        file.try_lock()
            .map_err(|_| Error::Invalid("Library is busy; try again after saving".into()))?;
        Ok(file)
    }
    /// Validate every expected revision before committing any change.
    pub fn batch(&mut self, expected: &[Entry], action: Batch<'_>) -> Result<()> {
        if expected.is_empty() {
            return Ok(());
        }
        let _lock = self.management_lock()?;
        let mut next = index_at(&self.root)?;
        let mut ids = std::collections::HashSet::new();
        for e in expected {
            if !ids.insert(e.id.clone()) || !next.entries.iter().any(|current| current == e) {
                return Err(Error::Invalid(
                    "A selected item changed. Refresh and select again; nothing was changed."
                        .into(),
                ));
            }
        }
        let mut removed = vec![];
        match action {
            Batch::Delete => {
                for e in expected {
                    removed.extend(self.paths(e.storage_id())?);
                }
                next.entries.retain(|e| !ids.contains(&e.id));
            }
            Batch::AddTags(tags) | Batch::RemoveTags(tags) => {
                let tags = normalize_tags(tags)?;
                let add = matches!(action, Batch::AddTags(_));
                for e in &mut next.entries {
                    if ids.contains(&e.id) {
                        if add {
                            let mut combined = e.tags.clone();
                            for tag in &tags {
                                if !combined.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                                    combined.push(tag.clone());
                                }
                            }
                            e.tags = normalize_tags(&combined)?;
                        } else {
                            e.tags
                                .retain(|t| !tags.iter().any(|tag| tag.eq_ignore_ascii_case(t)));
                        }
                    }
                }
            }
        }
        next.version = 3;
        atomic_write(
            &self.root.join("index.json"),
            &serde_json::to_vec_pretty(&next)?,
        )?;
        self.index = next;
        self.cleanup_warning = None;
        for path in removed {
            if let Err(e) = fs::remove_file(path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                self.cleanup_warning = Some(format!(
                    "Items removed from Library; an obsolete file could not be removed: {e}"
                ));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_is_atomic_on_conflict_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::blank(10, 10).unwrap();
        let a = h.create(&doc, "A", Origin::Canvas).unwrap();
        let b = h.create(&doc, "B", Origin::Canvas).unwrap();
        h.batch(&[a.clone(), b.clone()], Batch::AddTags(&["Review".into()]))
            .unwrap();
        assert!(h.batch(&[a, b], Batch::Delete).is_err());
        assert_eq!(h.entries().len(), 2);
        let entries = h.entries().to_vec();
        h.batch(&entries, Batch::RemoveTags(&["review".into()]))
            .unwrap();
        assert!(h.entries().iter().all(|e| e.tags.is_empty()));
        let entries = h.entries().to_vec();
        h.batch(&entries, Batch::Delete).unwrap();
        assert!(History::open(dir.path()).unwrap().entries().is_empty());
        assert_eq!(fs::read_dir(dir.path().join("images")).unwrap().count(), 0);
    }
    #[test]
    fn capacity_is_persistent_and_never_evicts() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        h.set_limits(Limits {
            entries: 1,
            bytes: 1_000_000,
        })
        .unwrap();
        let doc = Document::blank(10, 10).unwrap();
        let e = h.create(&doc, "Keep", Origin::Canvas).unwrap();
        assert!(h.create(&doc, "No room", Origin::Canvas).is_err());
        assert_eq!(
            h.load(&e.id).unwrap().render().unwrap(),
            doc.render().unwrap()
        );
        assert_eq!(
            History::open(dir.path()).unwrap().limits().unwrap().entries,
            1
        );
        h.set_limits(Limits {
            entries: 2,
            bytes: 1_000_000,
        })
        .unwrap();
        h.create(&doc, "Room", Origin::Canvas).unwrap();
        assert!(
            h.set_limits(Limits {
                entries: 1,
                bytes: 1_000_000
            })
            .is_err()
        );
        assert_eq!(h.entries().len(), 2);
    }
}
