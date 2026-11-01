//! Bounded local snapshots shared by desktop workflows and CLI tooling.
//! The index is atomic; immutable documents and thumbnails never reference
//! external user files. A process lock serializes index updates.
pub mod import;
mod manage;
use crate::{Document, Error, Result, atomic_write};
use image::{ImageFormat, RgbaImage};
pub use manage::{Batch, Limits, unique_names};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_ENTRIES: usize = 50;
pub const MAX_BYTES: u64 = 512_000_000;
const MAX_INDEX_BYTES: u64 = 1_000_000;
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    #[default]
    Image,
    Steps,
    Template,
}
impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Steps => "Step capture",
            Self::Template => "From Template",
        }
    }
    fn is_image(&self) -> bool {
        *self == Self::Image
    }
    fn of(document: &Document) -> Self {
        match document.guide.as_ref() {
            Some(guide) if guide.template.is_some() => Self::Template,
            Some(_) => Self::Steps,
            None => Self::Image,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Capture,
    Clipboard,
    Image,
    Project,
    Canvas,
    Saved,
}
impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Self::Capture => "Capture",
            Self::Clipboard => "Clipboard",
            Self::Image => "Image",
            Self::Project => "Project",
            Self::Canvas => "Canvas",
            Self::Saved => "Saved project",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "MediaKind::is_image")]
    pub kind: MediaKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub id: String,
    pub label: String,
    pub origin: Origin,
    pub width: u32,
    pub height: u32,
    pub created_ms: u64,
    pub bytes: u64,
    /// Immutable file key for an editable Library revision. Legacy snapshots
    /// use their entry ID. Version2 indexes prevent old apps overwriting them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_ms: Option<u64>,
}
impl Entry {
    pub fn storage_id(&self) -> &str {
        self.revision.as_deref().unwrap_or(&self.id)
    }
    /// UTC Gregorian calendar date for this snapshot's creation timestamp.
    pub fn created_date(&self) -> [u32; 3] {
        // Count complete 400-year eras, then the year/month/day within an era.
        let days = self.created_ms / 86_400_000 + 719_468;
        let era = days / 146_097;
        let day_of_era = days % 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_index = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_index + 2) / 5 + 1;
        let month = if month_index < 10 {
            month_index + 3
        } else {
            month_index - 9
        };
        let year = year_of_era + era * 400 + u64::from(month <= 2);
        [year as u32, month as u32, day as u32]
    }
    pub fn created_utc(&self) -> String {
        let [year, month, day] = self.created_date();
        let seconds = self.created_ms / 1000 % 86400;
        format!(
            "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
    /// Literal, case-insensitive words across the label, origin and dimensions.
    /// Empty/whitespace queries retain every entry; no files are read or changed.
    pub fn matches_query(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {} {}x{}",
            self.label,
            self.origin.label(),
            self.kind.label(),
            self.tags.join(" "),
            self.width,
            self.height
        )
        .to_lowercase();
        query
            .to_lowercase()
            .replace('×', "x")
            .split_whitespace()
            .all(|word| haystack.contains(word))
    }
}
#[derive(Serialize, Deserialize)]
struct Index {
    version: u32,
    entries: Vec<Entry>,
}
impl Default for Index {
    fn default() -> Self {
        Self {
            version: 1,
            entries: vec![],
        }
    }
}
pub struct History {
    root: PathBuf,
    index: Index,
    cleanup_warning: Option<String>,
}
enum WriteMode<'a> {
    Snapshot,
    Create,
    Update(&'a Entry),
    Duplicate(&'a Entry),
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn normalize_tags(tags: &[String]) -> Result<Vec<String>> {
    if tags.len() > 24 {
        return Err(Error::Invalid("Use up to 24 tags".into()));
    }
    let mut result = Vec::<String>::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.chars().count() > 48 || tag.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "Each tag can contain up to 48 characters without line breaks".into(),
            ));
        }
        if !tag.is_empty() && !result.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
            result.push(tag.into());
        }
    }
    Ok(result)
}
fn index_at(root: &Path) -> Result<Index> {
    let path = root.join("index.json");
    let metadata = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Index::default()),
        Err(e) => return Err(e.into()),
    };
    if metadata.len() > MAX_INDEX_BYTES {
        return Err(Error::Invalid(
            "History index is too large; existing files were preserved.".into(),
        ));
    }
    let index: Index = serde_json::from_slice(&fs::read(path)?)?;
    let mut ids = std::collections::HashSet::new();
    let mut storage = std::collections::HashSet::new();
    if !matches!(index.version, 1..=3)
        || index.entries.len() > manage::HARD_ENTRIES
        || index.entries.iter().any(|e| {
            !valid_id(&e.id)
                || !ids.insert(e.id.clone())
                || !valid_id(e.storage_id())
                || !storage.insert(e.storage_id().to_owned())
                || (index.version == 1 && (e.revision.is_some() || e.updated_ms.is_some()))
                || e.label.chars().count() > 120
                || e.label.chars().any(char::is_control)
                || (index.version < 3 && (e.kind != MediaKind::Image || !e.tags.is_empty()))
                || normalize_tags(&e.tags).is_err()
                || e.bytes == 0
                || e.bytes > MAX_BYTES
                || crate::validate_dimensions(e.width, e.height).is_err()
        })
        || index.entries.iter().map(|e| e.bytes).sum::<u64>() > manage::HARD_BYTES
    {
        return Err(Error::Invalid(
            "History index is invalid; existing files were preserved.".into(),
        ));
    }
    Ok(index)
}
impl History {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        for name in ["images", "thumbnails"] {
            let folder = root.join(name);
            fs::create_dir_all(&folder)?;
            if fs::canonicalize(&folder)?.parent() != Some(root.as_path()) {
                return Err(Error::Invalid(
                    "History folders must stay inside their data directory.".into(),
                ));
            }
        }
        let mut history = Self {
            index: index_at(&root)?,
            root,
            cleanup_warning: None,
        };
        history.classify_legacy_guides()?;
        Ok(history)
    }
    pub fn entries(&self) -> &[Entry] {
        &self.index.entries
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// A committed snapshot remains successful if obsolete files could not be
    /// removed. Expose that maintenance warning separately from save failure.
    pub fn cleanup_warning(&self) -> Option<&str> {
        self.cleanup_warning.as_deref()
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.index = index_at(&self.root)?;
        self.classify_legacy_guides()?;
        Ok(())
    }
    fn paths(&self, id: &str) -> Result<[PathBuf; 2]> {
        if !valid_id(id) {
            return Err(Error::Invalid("Invalid history identifier.".into()));
        }
        Ok([
            self.root.join("images").join(format!("{id}.capture.json")),
            self.root.join("thumbnails").join(format!("{id}.png")),
        ])
    }
    fn known_paths(&self, id: &str) -> Result<[PathBuf; 2]> {
        let entry = self
            .index
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| Error::Invalid("This history item is no longer available.".into()))?;
        self.paths(entry.storage_id())
    }
    pub fn available(&self, id: &str) -> bool {
        self.known_paths(id).is_ok_and(|p| p[0].is_file())
    }
    pub fn load(&self, id: &str) -> Result<Document> {
        Document::load(&self.known_paths(id)?[0])
    }
    pub fn thumbnail(&self, id: &str) -> Result<RgbaImage> {
        let path = &self.known_paths(id)?[1];
        if fs::metadata(path)?.len() > 200_000 {
            return Err(Error::Invalid("History thumbnail is too large.".into()));
        }
        let bytes = fs::read(path)?;
        let reader = image::ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Png);
        let (w, h) = reader.into_dimensions()?;
        if w == 0 || h == 0 || w > 160 || h > 100 {
            return Err(Error::Invalid(
                "Invalid history thumbnail dimensions.".into(),
            ));
        }
        Ok(image::load_from_memory_with_format(&bytes, ImageFormat::Png)?.into_rgba8())
    }
    /// Store an immutable snapshot. A failed write leaves the previous index
    /// and document intact. Missing old files do not prevent new captures.
    pub fn record(&mut self, document: &Document, label: &str, origin: Origin) -> Result<Entry> {
        self.record_limited(document, label, origin, MAX_ENTRIES, MAX_BYTES)
    }
    /// Create a durable editable item. Unlike bounded recent snapshots, this
    /// never evicts another item. Full storage is an explicit save failure.
    pub fn create(&mut self, document: &Document, label: &str, origin: Origin) -> Result<Entry> {
        self.persist(
            document,
            label,
            origin,
            self.limits()?.entries,
            self.limits()?.bytes,
            WriteMode::Create,
        )
    }
    /// Replace one Library item's revision without replacing its identity or
    /// original capture time. The expected revision prevents lost updates from
    /// another process. No other Library item is evicted to make room.
    pub fn update(&mut self, expected: &Entry, document: &Document) -> Result<Entry> {
        self.persist(
            document,
            &expected.label,
            expected.origin,
            self.limits()?.entries,
            self.limits()?.bytes,
            WriteMode::Update(expected),
        )
    }
    /// Metadata updates never rewrite the project or thumbnail. The current
    /// revision and metadata must still match the item the user was editing.
    pub fn edit_metadata(
        &mut self,
        expected: &Entry,
        label: &str,
        tags: &[String],
    ) -> Result<Entry> {
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 120 || label.chars().any(char::is_control) {
            return Err(Error::Invalid(
                "Enter a name of 1–120 characters without line breaks".into(),
            ));
        }
        let tags = normalize_tags(tags)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("history.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Library is busy; try again after saving".into()))?;
        let mut next = index_at(&self.root)?;
        let current = next
            .entries
            .iter_mut()
            .find(|e| e.id == expected.id)
            .ok_or_else(|| Error::Invalid("This Library item is no longer available".into()))?;
        if current != expected {
            return Err(Error::Invalid(
                "This Library item changed; reopen its menu to use the latest version".into(),
            ));
        }
        current.label = label.into();
        current.tags = tags;
        let entry = current.clone();
        next.version = 3;
        atomic_write(
            &self.root.join("index.json"),
            &serde_json::to_vec_pretty(&next)?,
        )?;
        self.index = next;
        Ok(entry)
    }
    pub fn duplicate(&mut self, expected: &Entry) -> Result<Entry> {
        let document = self.load(&expected.id)?;
        let label = format!(
            "Copy of {}",
            expected.label.chars().take(112).collect::<String>()
        );
        self.persist(
            &document,
            &label,
            expected.origin,
            self.limits()?.entries,
            self.limits()?.bytes,
            WriteMode::Duplicate(expected),
        )
    }
    fn classify_legacy_guides(&mut self) -> Result<()> {
        if self.index.version >= 3 || self.index.entries.is_empty() {
            return Ok(());
        }
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("history.lock"))?;
        // A writer will promote the index itself. Do not block launch on migration.
        if lock.try_lock().is_err() {
            return Ok(());
        }
        let mut next = index_at(&self.root)?;
        if next.version >= 3 {
            self.index = next;
            return Ok(());
        }
        let mut found = false;
        for entry in &mut next.entries {
            if let Ok(doc) = Document::load(&self.paths(entry.storage_id())?[0]) {
                entry.kind = MediaKind::of(&doc);
                found |= entry.kind != MediaKind::Image;
            }
        }
        // Empty legacy recent stores retain their bounded snapshot semantics.
        if found || next.version == 2 {
            next.version = 3;
            atomic_write(
                &self.root.join("index.json"),
                &serde_json::to_vec_pretty(&next)?,
            )?;
            self.index = next;
        }
        Ok(())
    }
    /// Discard exactly the revision owned by an active capture session. Never
    /// delete a newer revision another editor has saved in the meantime.
    pub fn discard(&mut self, expected: &Entry) -> Result<()> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("history.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Library is busy; the recording was preserved.".into()))?;
        let mut next = index_at(&self.root)?;
        let current = next
            .entries
            .iter()
            .find(|e| e.id == expected.id)
            .ok_or_else(|| Error::Invalid("The capture is no longer in Library".into()))?;
        if current.storage_id() != expected.storage_id() {
            return Err(Error::Invalid(
                "The capture was edited in another window; its newer revision was preserved."
                    .into(),
            ));
        }
        let paths = self.paths(current.storage_id())?;
        next.entries.retain(|e| e.id != expected.id);
        atomic_write(
            &self.root.join("index.json"),
            &serde_json::to_vec_pretty(&next)?,
        )?;
        self.index = next;
        self.cleanup_warning = None;
        for path in paths {
            if let Err(e) = fs::remove_file(path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                self.cleanup_warning = Some(format!(
                    "Capture removed from Library, but an obsolete file could not be removed: {e}"
                ));
            }
        }
        Ok(())
    }
    fn record_limited(
        &mut self,
        document: &Document,
        label: &str,
        origin: Origin,
        max_entries: usize,
        max_bytes: u64,
    ) -> Result<Entry> {
        self.persist(
            document,
            label,
            origin,
            max_entries,
            max_bytes,
            WriteMode::Snapshot,
        )
    }
    fn persist(
        &mut self,
        document: &Document,
        label: &str,
        origin: Origin,
        max_entries: usize,
        max_bytes: u64,
        mode: WriteMode<'_>,
    ) -> Result<Entry> {
        document.validate()?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("history.lock"))?;
        lock.try_lock().map_err(|_| {
            Error::Invalid(
                "History is busy in another ClipCraft window. The open image is unchanged.".into(),
            )
        })?;
        // Another process may have recorded an item since this instance opened.
        let mut next = index_at(&self.root)?;
        if let WriteMode::Duplicate(expected) = mode
            && !next.entries.iter().any(|current| current == expected)
        {
            return Err(Error::Invalid(
                "This Library item changed; refresh before duplicating it".into(),
            ));
        }
        let replaced = if let WriteMode::Update(expected) = mode {
            let current = next
                .entries
                .iter()
                .find(|e| e.id == expected.id)
                .ok_or_else(|| {
                    Error::Invalid(
                        "This Library item is no longer available; save a new copy.".into(),
                    )
                })?;
            if current.storage_id() != expected.storage_id() {
                return Err(Error::Invalid("This Library item changed in another window. Reopen it or save a new copy; the latest version was preserved.".into()));
            }
            Some(current.clone())
        } else {
            None
        };
        let thumbnail = image::DynamicImage::ImageRgba8(document.render()?)
            .thumbnail(160, 100)
            .into_rgba8();
        let mut thumbnail_bytes = Cursor::new(Vec::new());
        thumbnail.write_to(&mut thumbnail_bytes, ImageFormat::Png)?;
        let project = serde_json::to_vec(document)?;
        let bytes = project.len() as u64 + thumbnail_bytes.get_ref().len() as u64;
        if project.len() as u64 > crate::MAX_FILE_BYTES || bytes > max_bytes || max_entries == 0 {
            return Err(Error::Invalid(
                "This image exceeds the local history storage limit.".into(),
            ));
        }
        if let Some(old) = &replaced
            && next.entries.iter().map(|e| e.bytes).sum::<u64>() - old.bytes + bytes > max_bytes
        {
            return Err(Error::Invalid(
                "Library storage is full; existing items were preserved.".into(),
            ));
        }
        // A version2 Library is durable, including when an older API caller
        // requests a recent snapshot. Never silently retire editable work.
        if replaced.is_none()
            && (!matches!(mode, WriteMode::Snapshot) || next.version >= 2)
            && (next.entries.len() >= max_entries
                || next.entries.iter().map(|e| e.bytes).sum::<u64>() + bytes > max_bytes)
        {
            return Err(Error::Invalid("Library storage is full; existing items were preserved. Save a project copy to another location.".into()));
        }
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Invalid("System time precedes the history epoch.".into()))?;
        let storage_id = loop {
            let id = format!(
                "{:016x}{:08x}{:08x}",
                elapsed.as_millis() as u64,
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            );
            if !self.paths(&id)?.iter().any(|p| p.exists()) {
                break id;
            }
        };
        let mut entry = Entry {
            kind: MediaKind::of(document),
            tags: replaced
                .as_ref()
                .map(|e| e.tags.clone())
                .unwrap_or_else(|| {
                    if let WriteMode::Duplicate(e) = mode {
                        e.tags.clone()
                    } else {
                        vec![]
                    }
                }),
            id: replaced
                .as_ref()
                .map_or_else(|| storage_id.clone(), |e| e.id.clone()),
            label: replaced
                .as_ref()
                .map_or(label, |e| e.label.as_str())
                .chars()
                .filter(|c| !c.is_control())
                .take(120)
                .collect(),
            origin,
            width: document.width,
            height: document.height,
            created_ms: replaced
                .as_ref()
                .map_or(elapsed.as_millis() as u64, |e| e.created_ms),
            bytes,
            revision: (!matches!(mode, WriteMode::Snapshot)).then(|| storage_id.clone()),
            updated_ms: (!matches!(mode, WriteMode::Snapshot))
                .then_some(elapsed.as_millis() as u64),
        };
        if matches!(mode, WriteMode::Create) {
            entry.label = format!(
                "{} · {}",
                entry.label.chars().take(90).collect::<String>(),
                entry.created_utc()
            );
        }
        if !matches!(mode, WriteMode::Snapshot) || entry.kind != MediaKind::Image {
            next.version = 3;
        }
        let paths = self.paths(&storage_id)?;
        let mut removed = Vec::new();
        if let Some(old) = replaced {
            next.version = 3;
            next.entries.retain(|e| e.id != old.id);
            removed.push(old);
        }
        next.entries.insert(0, entry.clone());
        while next.entries.len() > max_entries
            || next.entries.iter().map(|e| e.bytes).sum::<u64>() > max_bytes
        {
            removed.push(next.entries.pop().unwrap());
        }
        let result = (|| {
            atomic_write(&paths[0], &project)?;
            atomic_write(&paths[1], thumbnail_bytes.get_ref())?;
            atomic_write(
                &self.root.join("index.json"),
                &serde_json::to_vec_pretty(&next)?,
            )
        })();
        if let Err(error) = result {
            for path in &paths {
                let _ = fs::remove_file(path);
            }
            return Err(error);
        }
        self.index = next;
        self.cleanup_warning = None;
        for old in removed {
            for path in self.paths(old.storage_id())? {
                match fs::remove_file(path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        self.cleanup_warning = Some(format!(
                            "Snapshot saved, but an obsolete Library file could not be removed: {e}"
                        ));
                    }
                }
            }
        }
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guide_type_migration_metadata_duplicate_and_autosave_preserve_library_identity() {
        use crate::step_capture::{Button, Gesture, Guide};
        let mut guide = Guide::new([80, 50]).unwrap();
        guide
            .push(
                RgbaImage::from_pixel(80, 50, image::Rgba([50, 80, 100, 255])),
                Gesture {
                    start: [20, 20],
                    end: [20, 20],
                    button: Button::Left,
                },
            )
            .unwrap();
        let doc = guide.document().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut store = History::open(dir.path()).unwrap();
        let entry = store.create(&doc, "Step guide", Origin::Capture).unwrap();
        assert_eq!(entry.kind, MediaKind::Steps);
        let paths = store.known_paths(&entry.id).unwrap();
        let original = paths.map(|p| fs::read(p).unwrap());
        let updated = store
            .edit_metadata(
                &entry,
                "Setup guide",
                &[" Training ".into(), "training".into(), "Product".into()],
            )
            .unwrap();
        assert_eq!(updated.tags, ["Training", "Product"]);
        assert!(updated.matches_query("training step setup"));
        assert!(store.edit_metadata(&entry, "stale", &[]).is_err());
        assert!(store.edit_metadata(&updated, "", &[]).is_err());
        assert_eq!(
            store
                .known_paths(&entry.id)
                .unwrap()
                .map(|p| fs::read(p).unwrap()),
            original
        );
        // An autosave queued before rename/tags must keep the new metadata.
        let saved = store.update(&entry, &doc).unwrap();
        assert_eq!(saved.label, "Setup guide");
        assert_eq!(saved.tags, updated.tags);
        let copy = store.duplicate(&saved).unwrap();
        assert_ne!(copy.id, saved.id);
        assert_ne!(copy.storage_id(), saved.storage_id());
        assert_eq!(copy.kind, MediaKind::Steps);
        assert_eq!(copy.tags, updated.tags);
        assert_eq!(
            serde_json::to_vec(&store.load(&copy.id).unwrap()).unwrap(),
            serde_json::to_vec(&doc).unwrap()
        );
        // Emulate an existing v2 guide saved before capture types were recorded.
        let mut legacy = index_at(dir.path()).unwrap();
        legacy.version = 2;
        for e in &mut legacy.entries {
            e.kind = MediaKind::Image;
            e.tags.clear();
        }
        fs::write(
            dir.path().join("index.json"),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let reopened = History::open(dir.path()).unwrap();
        assert!(
            reopened
                .entries()
                .iter()
                .all(|e| e.kind == MediaKind::Steps)
        );
        assert_eq!(reopened.entries()[0].label, copy.label);
        assert_eq!(reopened.index.version, 3);
    }
    #[test]
    fn discard_removes_only_the_expected_revision_and_preserves_other_or_newer_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let d = Document::blank(30, 20).unwrap();
        let other = h.create(&d, "Other project", Origin::Canvas).unwrap();
        let capture = h.create(&d, "Current recording", Origin::Capture).unwrap();
        let paths = h.known_paths(&capture.id).unwrap();
        let fresh = h.update(&capture, &d).unwrap();
        let index = fs::read(dir.path().join("index.json")).unwrap();
        assert!(h.discard(&capture).is_err());
        assert_eq!(fs::read(dir.path().join("index.json")).unwrap(), index);
        let current_paths = h.known_paths(&fresh.id).unwrap();
        h.discard(&fresh).unwrap();
        assert_eq!(h.entries().len(), 1);
        assert!(h.load(&other.id).is_ok());
        assert!(
            paths
                .iter()
                .chain(current_paths.iter())
                .all(|p| !p.exists())
        );
        assert!(History::open(dir.path()).unwrap().load(&fresh.id).is_err());
    }

    #[test]
    fn durable_library_creates_timestamped_items_without_retiring_old_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::canvas(24, 20, [30, 50, 70, 255]).unwrap();
        let old = h.record(&doc, "Legacy capture", Origin::Capture).unwrap();
        let current = h.create(&doc, "Region capture", Origin::Capture).unwrap();
        assert!(current.label.starts_with("Region capture · "));
        assert!(current.label.ends_with(" UTC"));
        assert!(current.revision.is_some());
        let before = fs::read(dir.path().join("index.json")).unwrap();
        for mode in [WriteMode::Create, WriteMode::Snapshot] {
            assert!(
                h.persist(&doc, "full", Origin::Canvas, 2, MAX_BYTES, mode)
                    .is_err()
            );
            assert_eq!(fs::read(dir.path().join("index.json")).unwrap(), before);
            assert_eq!(h.load(&old.id).unwrap().source_png, doc.source_png);
        }
        let updated = h.update(&current, &doc).unwrap();
        assert_eq!(updated.label, current.label);
        assert_eq!(updated.created_ms, current.created_ms);
        assert_eq!(h.entries().len(), 2);
    }

    #[test]
    fn editable_revisions_keep_identity_original_time_and_objects_and_detect_stale_windows() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = History::open(dir.path()).unwrap();
        let original = Document::blank(160, 100).unwrap();
        let entry = first
            .record(&original, "Capture2026-10-30", Origin::Capture)
            .unwrap();
        let old_paths = first.known_paths(&entry.id).unwrap();
        let mut second = History::open(dir.path()).unwrap();
        let mut edited = original.clone();
        edited.annotations.push(
            crate::steps::marker(
                1,
                [60.0, 50.0],
                44,
                [70, 90, 120, 255],
                (1, false),
                [160, 100],
            )
            .unwrap(),
        );
        let revision = first.update(&entry, &edited).unwrap();
        assert_eq!(revision.id, entry.id);
        assert_eq!(revision.created_ms, entry.created_ms);
        assert_eq!(revision.label, entry.label);
        assert_ne!(revision.storage_id(), entry.storage_id());
        assert!(revision.updated_ms.is_some());
        assert_eq!(first.entries().len(), 1);
        assert_eq!(first.index.version, 3);
        assert!(old_paths.iter().all(|p| !p.exists()));
        assert_eq!(
            first.load(&entry.id).unwrap().annotations,
            edited.annotations
        );
        assert_eq!(
            first.load(&entry.id).unwrap().base_image().unwrap(),
            original.base_image().unwrap()
        );
        assert_eq!(first.thumbnail(&entry.id).unwrap().dimensions(), (160, 100));
        let index = fs::read(dir.path().join("index.json")).unwrap();
        assert!(second.update(&entry, &original).is_err());
        assert_eq!(fs::read(dir.path().join("index.json")).unwrap(), index);
        second.refresh().unwrap();
        assert_eq!(
            second.load(&entry.id).unwrap().render().unwrap(),
            edited.render().unwrap()
        );
        let next = second.update(&revision, &original).unwrap();
        assert_ne!(next.storage_id(), revision.storage_id());
        first.refresh().unwrap();
        assert!(first.load(&entry.id).unwrap().annotations.is_empty());
        first
            .record(&original, "Another capture", Origin::Capture)
            .unwrap();
        let reopened = History::open(dir.path()).unwrap();
        assert_eq!(reopened.entries().len(), 2);
        assert!(reopened.available(&entry.id));
        assert_eq!(fs::read_dir(dir.path().join("images")).unwrap().count(), 2);
    }

    #[test]
    fn failed_or_oversized_revision_preserves_previous_index_files_and_other_entries() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::blank(80, 60).unwrap();
        let e = h.record(&doc, "First", Origin::Capture).unwrap();
        h.record(&doc, "Second", Origin::Capture).unwrap();
        let before = fs::read(dir.path().join("index.json")).unwrap();
        let original_paths = h.known_paths(&e.id).unwrap();
        let project = fs::read(&original_paths[0]).unwrap();
        assert!(
            h.persist(
                &doc,
                &e.label,
                e.origin,
                MAX_ENTRIES,
                e.bytes,
                WriteMode::Update(&e)
            )
            .is_err()
        );
        assert_eq!(fs::read(dir.path().join("index.json")).unwrap(), before);
        // A real second-file write failure after the new project was written.
        // All paths are owned by this temporary test directory.
        fs::rename(
            dir.path().join("thumbnails"),
            dir.path().join("saved-thumbnails"),
        )
        .unwrap();
        fs::write(dir.path().join("thumbnails"), b"blocked folder").unwrap();
        assert!(h.update(&e, &doc).is_err());
        assert_eq!(fs::read(dir.path().join("index.json")).unwrap(), before);
        assert_eq!(fs::read(&original_paths[0]).unwrap(), project);
        assert_eq!(fs::read_dir(dir.path().join("images")).unwrap().count(), 2);
        fs::remove_file(dir.path().join("thumbnails")).unwrap();
        fs::rename(
            dir.path().join("saved-thumbnails"),
            dir.path().join("thumbnails"),
        )
        .unwrap();
        assert_eq!(
            h.load(&e.id).unwrap().render().unwrap(),
            doc.render().unwrap()
        );
        assert!(h.thumbnail(&e.id).is_ok());
        assert!(h.cleanup_warning().is_none());
    }

    #[test]
    fn revision_file_keys_cannot_escape_or_alias_another_entry_and_legacy_indexes_still_load() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::blank(20, 20).unwrap();
        let a = h.record(&doc, "a", Origin::Image).unwrap();
        let b = h.record(&doc, "b", Origin::Image).unwrap();
        let legacy = fs::read(dir.path().join("index.json")).unwrap();
        assert_eq!(History::open(dir.path()).unwrap().index.version, 1);
        for key in ["../outside", b.storage_id()] {
            let mut index: Index = serde_json::from_slice(&legacy).unwrap();
            index.version = 2;
            index
                .entries
                .iter_mut()
                .find(|e| e.id == a.id)
                .unwrap()
                .revision = Some(key.into());
            fs::write(
                dir.path().join("index.json"),
                serde_json::to_vec(&index).unwrap(),
            )
            .unwrap();
            assert!(History::open(dir.path()).is_err());
        }
        fs::write(dir.path().join("index.json"), legacy).unwrap();
        assert!(History::open(dir.path()).unwrap().available(&a.id));
    }

    #[test]
    fn search_matches_words_across_fields_unicode_and_dimensions_without_changing_entries() {
        let entry = Entry {
            kind: Default::default(),
            tags: vec![],
            id: "00000000000000000000000000000000".into(),
            label: "Résumé Draft [1]".into(),
            origin: Origin::Saved,
            width: 640,
            height: 400,
            created_ms: 1,
            bytes: 1,
            revision: None,
            updated_ms: None,
        };
        let before = entry.clone();
        for query in [
            "",
            " \t",
            "RÉSUMÉ",
            "draft saved",
            "640x400",
            "640 × 400",
            "PROJECT [1]",
        ] {
            assert!(entry.matches_query(query), "{query:?}");
        }
        for query in ["capture", "draft 800", "640x40*", ".*", "[2]"] {
            assert!(!entry.matches_query(query), "{query:?}");
        }
        assert_eq!(entry, before);
    }

    #[test]
    fn snapshot_calendar_handles_epoch_leap_days_and_day_boundaries() {
        let mut entry = Entry {
            kind: Default::default(),
            tags: vec![],
            id: String::new(),
            label: String::new(),
            origin: Origin::Canvas,
            width: 1,
            height: 1,
            created_ms: 0,
            bytes: 1,
            revision: None,
            updated_ms: None,
        };
        for (ms, date, expected) in [
            (0, [1970, 1, 1], "1970-01-01 00:00:00 UTC"),
            (86_399_999, [1970, 1, 1], "1970-01-01 23:59:59 UTC"),
            (86_400_000, [1970, 1, 2], "1970-01-02 00:00:00 UTC"),
            (951_782_400_000, [2000, 2, 29], "2000-02-29 00:00:00 UTC"),
            (1_793_354_469_427, [2026, 10, 30], "2026-10-30 10:01:09 UTC"),
        ] {
            entry.created_ms = ms;
            assert_eq!(entry.created_date(), date);
            assert_eq!(entry.created_utc(), expected);
        }
        entry.created_ms = u64::MAX;
        let [_, month, day] = entry.created_date();
        assert!((1..=12).contains(&month) && (1..=31).contains(&day));
    }
    fn fixture() -> Document {
        serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json")).unwrap()
    }
    #[test]
    fn snapshots_reopen_with_editable_objects_thumbnails_and_independent_original() {
        let dir = tempfile::tempdir().unwrap();
        let mut history = History::open(dir.path()).unwrap();
        let mut doc = fixture();
        let expected = doc.render().unwrap();
        let entry = history
            .record(&doc, "Synthetic\nfixture", Origin::Capture)
            .unwrap();
        assert_eq!(entry.label, "Syntheticfixture");
        doc.annotations.clear();
        let reopened = History::open(dir.path()).unwrap();
        let restored = reopened.load(&entry.id).unwrap();
        assert_eq!(restored.annotations.len(), 7);
        assert_eq!(restored.render().unwrap(), expected);
        let thumb = reopened.thumbnail(&entry.id).unwrap();
        assert_eq!(thumb.dimensions(), (160, 100));
        assert_eq!(
            thumb,
            image::DynamicImage::ImageRgba8(expected)
                .thumbnail(160, 100)
                .into_rgba8()
        );
        assert!(reopened.available(&entry.id));
    }
    #[test]
    fn retention_enforces_count_and_bytes_and_does_not_touch_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::blank(10, 10).unwrap();
        let untouched = dir.path().join("unrelated.txt");
        fs::write(&untouched, "keep").unwrap();
        let first = h
            .record_limited(&doc, "first", Origin::Canvas, 2, MAX_BYTES)
            .unwrap();
        let second = h
            .record_limited(&doc, "second", Origin::Canvas, 2, MAX_BYTES)
            .unwrap();
        let third = h
            .record_limited(&doc, "third", Origin::Canvas, 2, MAX_BYTES)
            .unwrap();
        assert_eq!(
            h.entries()
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            [third.id.as_str(), second.id.as_str()]
        );
        assert!(h.paths(&first.id).unwrap().iter().all(|p| !p.exists()));
        h.record_limited(&doc, "limited", Origin::Canvas, 50, third.bytes + 1)
            .unwrap();
        assert_eq!(h.entries().len(), 1);
        assert_eq!(fs::read_to_string(untouched).unwrap(), "keep");
    }
    #[test]
    fn stale_instances_merge_and_busy_or_failed_writes_preserve_existing_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = History::open(dir.path()).unwrap();
        let mut b = History::open(dir.path()).unwrap();
        let doc = Document::blank(10, 10).unwrap();
        a.record(&doc, "a", Origin::Canvas).unwrap();
        b.record(&doc, "b", Origin::Canvas).unwrap();
        a.refresh().unwrap();
        assert_eq!(a.entries().len(), 2);
        let before = fs::read(dir.path().join("index.json")).unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("history.lock"))
            .unwrap();
        lock.lock().unwrap();
        assert!(a.record(&doc, "busy", Origin::Canvas).is_err());
        drop(lock);
        assert!(
            a.record_limited(&doc, "oversize", Origin::Canvas, 50, 1)
                .is_err()
        );
        let mut bad = doc;
        bad.source_png = "broken".into();
        assert!(a.record(&bad, "bad", Origin::Canvas).is_err());
        assert_eq!(fs::read(dir.path().join("index.json")).unwrap(), before);
    }
    #[test]
    fn missing_images_invalid_ids_and_corrupt_indexes_fail_without_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let e = h
            .record(&Document::blank(4, 4).unwrap(), "small", Origin::Image)
            .unwrap();
        fs::remove_file(&h.paths(&e.id).unwrap()[0]).unwrap();
        assert!(!h.available(&e.id));
        assert!(h.load(&e.id).is_err());
        assert!(h.load("../../outside").is_err());
        let path = dir.path().join("index.json");
        fs::write(&path, b"invalid json").unwrap();
        assert!(History::open(dir.path()).is_err());
        assert!(
            h.record(&Document::blank(4, 4).unwrap(), "next", Origin::Image)
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"invalid json");
        let mut invalid = Index::default();
        let mut entry = e;
        entry.id = "../not-an-owned-image".into();
        invalid.entries.push(entry);
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(History::open(dir.path()).is_err());
    }
}
