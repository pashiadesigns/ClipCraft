//! Transactional bulk metadata/removal and explicit, non-evicting capacity.
use super::*;
pub const HARD_ENTRIES: usize = 1000;
pub const HARD_BYTES: u64 = 16_000_000_000;
#[cfg(test)]
mod global_tag_tests {
    use super::*;
    #[test]
    fn global_tag_rename_merges_case_variants_atomically_and_keeps_projects() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::blank(20, 20).unwrap();
        let a = h.create(&doc, "One", Origin::Canvas).unwrap();
        let b = h.create(&doc, "Two", Origin::Canvas).unwrap();
        let a = h
            .edit_metadata(&a, "One", &["Draft".into(), "Ready".into()])
            .unwrap();
        let b = h
            .edit_metadata(&b, "Two", &["draft".into(), "Keep".into()])
            .unwrap();
        let files: Vec<_> = [&a, &b]
            .into_iter()
            .map(|e| {
                let p = dir
                    .path()
                    .join("images")
                    .join(format!("{}.capture.json", e.storage_id()));
                (p.clone(), fs::read(p).unwrap())
            })
            .collect();
        let index_before = fs::read(dir.path().join("index.json")).unwrap();
        assert!(h.replace_tag("Draft", Some("  ")).is_err());
        assert_eq!(
            fs::read(dir.path().join("index.json")).unwrap(),
            index_before
        );
        assert_eq!(h.replace_tag("DRAFT", Some("Ready")).unwrap(), 2);
        assert_eq!(
            h.entries().iter().find(|e| e.id == a.id).unwrap().tags,
            vec!["Ready"]
        );
        assert_eq!(h.replace_tag("ready", None).unwrap(), 2);
        let reopened = History::open(dir.path()).unwrap();
        assert!(
            reopened
                .entries()
                .iter()
                .find(|e| e.id == a.id)
                .unwrap()
                .tags
                .is_empty()
        );
        assert_eq!(
            reopened
                .entries()
                .iter()
                .find(|e| e.id == b.id)
                .unwrap()
                .tags,
            vec!["Keep"]
        );
        for (path, bytes) in files {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Limits {
    pub entries: usize,
    pub bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: HARD_ENTRIES,
            bytes: HARD_BYTES,
        }
    }
}
#[test]
fn fresh_library_uses_maximum_capacity_and_keeps_explicit_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = History::open(dir.path()).unwrap();
    assert_eq!(
        h.limits().unwrap(),
        Limits {
            entries: HARD_ENTRIES,
            bytes: HARD_BYTES
        }
    );
    let custom = Limits {
        entries: 70,
        bytes: 2_000_000_000,
    };
    h.set_limits(custom).unwrap();
    assert_eq!(History::open(dir.path()).unwrap().limits().unwrap(), custom);
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
    Rename(&'a str),
    Delete,
}
/// Stable names for Library operations and files. Compare case-insensitively on Windows.
pub fn unique_names(base: &str, count: usize, occupied: &[String]) -> Result<Vec<String>> {
    let base = base.trim();
    if base.is_empty() || base.chars().count() > 100 || base.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "Enter a base name of 1–100 characters.".into(),
        ));
    }
    let mut used: std::collections::HashSet<_> =
        occupied.iter().map(|s| s.to_lowercase()).collect();
    let mut result = Vec::new();
    let mut suffix = 1;
    for _ in 0..count {
        loop {
            let name = format!("{base} {suffix}");
            suffix += 1;
            if used.insert(name.to_lowercase()) {
                result.push(name);
                break;
            }
        }
    }
    Ok(result)
}
impl History {
    /// Rename/merge or remove a tag everywhere in one atomic index transaction.
    /// Project files, pixels and their revisions are unchanged.
    pub fn replace_tag(&mut self, old: &str, new: Option<&str>) -> Result<usize> {
        let old = old.trim();
        if old.is_empty() {
            return Err(Error::Invalid("Choose an existing tag.".into()));
        }
        let replacement = match new {
            Some(name) => Some(
                normalize_tags(&[name.into()])?
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::Invalid("Enter a tag name.".into()))?,
            ),
            None => None,
        };
        let _lock = self.management_lock()?;
        let mut next = index_at(&self.root)?;
        let mut count = 0;
        for entry in &mut next.entries {
            if entry.tags.iter().any(|t| t.eq_ignore_ascii_case(old)) {
                let tags: Vec<_> = entry
                    .tags
                    .iter()
                    .filter_map(|t| {
                        if t.eq_ignore_ascii_case(old) {
                            replacement.clone()
                        } else {
                            Some(t.clone())
                        }
                    })
                    .collect();
                entry.tags = normalize_tags(&tags)?;
                count += 1;
            }
        }
        if count == 0 {
            return Err(Error::Invalid(
                "This tag no longer exists. Refresh the Library.".into(),
            ));
        }
        next.version = 3;
        atomic_write(
            &self.root.join("index.json"),
            &serde_json::to_vec_pretty(&next)?,
        )?;
        self.index = next;
        Ok(count)
    }
    /// Copies become visible together only after every source and capacity check succeeds.
    pub fn duplicate_many(&mut self, expected: &[Entry]) -> Result<Vec<Entry>> {
        if expected.is_empty() {
            return Ok(vec![]);
        }
        let _lock = self.management_lock()?;
        let mut next = index_at(&self.root)?;
        let mut ids = std::collections::HashSet::new();
        if expected
            .iter()
            .any(|e| !ids.insert(&e.id) || !next.entries.contains(e))
        {
            return Err(Error::Invalid(
                "A selected item changed; refresh before duplicating.".into(),
            ));
        }
        let limits = self.limits()?;
        if next.entries.len() + expected.len() > limits.entries
            || next
                .entries
                .iter()
                .chain(expected)
                .map(|e| e.bytes)
                .sum::<u64>()
                > limits.bytes
        {
            return Err(Error::Invalid(
                "Library capacity is insufficient; nothing was duplicated.".into(),
            ));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Invalid("Invalid system time".into()))?
            .as_millis() as u64;
        let mut names: Vec<_> = next.entries.iter().map(|e| e.label.clone()).collect();
        let mut paths = Vec::new();
        let mut copies = Vec::new();
        let result = (|| {
            for entry in expected {
                self.load(&entry.id)?;
                let source = self.paths(entry.storage_id())?;
                let id = loop {
                    let id = format!(
                        "{:016x}{:08x}{:08x}",
                        now,
                        std::process::id(),
                        SEQUENCE.fetch_add(1, Ordering::Relaxed)
                    );
                    if self.paths(&id)?.iter().all(|p| !p.exists()) {
                        break id;
                    }
                };
                let label = unique_names(
                    &format!(
                        "Copy of {}",
                        entry.label.chars().take(92).collect::<String>()
                    ),
                    1,
                    &names,
                )?
                .remove(0);
                names.push(label.clone());
                let target = self.paths(&id)?;
                paths.extend(target.clone());
                for (src, dst) in source.iter().zip(&target) {
                    atomic_write(dst, &fs::read(src)?)?;
                }
                copies.push(Entry {
                    id: id.clone(),
                    revision: Some(id),
                    label,
                    created_ms: now,
                    updated_ms: Some(now),
                    ..entry.clone()
                });
            }
            next.version = 3;
            next.entries.splice(0..0, copies.iter().cloned());
            atomic_write(
                &self.root.join("index.json"),
                &serde_json::to_vec_pretty(&next)?,
            )
        })();
        if let Err(error) = result {
            for path in paths {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        self.index = next;
        Ok(copies)
    }
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
            Batch::Rename(base) => {
                let occupied = next
                    .entries
                    .iter()
                    .filter(|e| !ids.contains(&e.id))
                    .map(|e| e.label.clone())
                    .collect::<Vec<_>>();
                let names = unique_names(base, expected.len(), &occupied)?;
                for (e, name) in expected.iter().zip(names) {
                    next.entries
                        .iter_mut()
                        .find(|current| current.id == e.id)
                        .unwrap()
                        .label = name;
                }
            }
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
        if matches!(action, Batch::Delete) {
            for e in expected {
                if let Err(err) = self.delete_revisions(&e.id) {
                    self.cleanup_warning = Some(format!(
                        "Items removed; previous-version cleanup needs attention: {err}"
                    ));
                }
            }
        }
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
    fn ordered_names_duplicates_and_conflicts_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let d = Document::blank(20, 20).unwrap();
        let a = h.create(&d, "A", Origin::Canvas).unwrap();
        let b = h.create(&d, "B", Origin::Canvas).unwrap();
        let busy = h.create(&d, "Existing", Origin::Canvas).unwrap();
        h.edit_metadata(&busy, "GUIDE 1", &[]).unwrap();
        h.batch(&[b.clone(), a.clone()], Batch::Rename("Guide"))
            .unwrap();
        assert_eq!(
            h.entries().iter().find(|e| e.id == b.id).unwrap().label,
            "Guide 2"
        );
        assert_eq!(
            h.entries().iter().find(|e| e.id == a.id).unwrap().label,
            "Guide 3"
        );
        assert!(h.batch(&[a, b], Batch::Rename("No changes")).is_err());
        let expected = h.entries().to_vec();
        let copies = h.duplicate_many(&expected).unwrap();
        assert_eq!(copies.len(), 3);
        assert_eq!(h.entries().len(), 6);
        for (copy, source) in copies.iter().zip(&expected) {
            assert_ne!(copy.id, source.id);
            assert_eq!(
                h.load(&copy.id).unwrap().render().unwrap(),
                h.load(&source.id).unwrap().render().unwrap()
            );
        }
        h.set_limits(Limits {
            entries: 6,
            bytes: 1_000_000,
        })
        .unwrap();
        let before = h.entries().to_vec();
        assert!(h.duplicate_many(&expected).is_err());
        assert_eq!(h.entries(), before);
        assert_eq!(History::open(dir.path()).unwrap().entries(), before);
        assert!(unique_names("", 2, &[]).is_err());
        assert_eq!(
            unique_names("Guide", 3, &["guide 1".into(), "Guide 3".into()]).unwrap(),
            ["Guide 2", "Guide 4", "Guide 5"]
        );
    }
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
