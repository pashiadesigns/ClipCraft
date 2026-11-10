//! Library folders, automatic grouping and the Recent strip (LIB-002, LIB-007,
//! approved mockup `artifacts/br-proposal/roadmap-ui.html`). A capture lives in
//! one folder and can still have any number of tags. Deleting a folder keeps
//! its captures; closing an item only takes it out of Recent.
use super::*;

/// How a Library item is organized. Saving a new version keeps it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Organize {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Recorded at capture, for new captures only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    /// Taken out of Recent. The item stays in the Library.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub closed: bool,
}
impl Organize {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}
fn folder_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "Enter a folder name of 1–60 characters without line breaks".into(),
        ));
    }
    Ok(name.into())
}
fn source_name(name: Option<String>) -> Option<String> {
    name.map(|n| n.chars().filter(|c| !c.is_control()).take(80).collect())
        .filter(|n: &String| !n.trim().is_empty())
}
impl History {
    fn edit_index(&mut self, edit: impl FnOnce(&mut Index) -> Result<()>) -> Result<()> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("history.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Library is busy; try again after saving".into()))?;
        let mut next = index_at(&self.root)?;
        edit(&mut next)?;
        next.version = 3;
        atomic_write(
            &self.root.join("index.json"),
            &serde_json::to_vec_pretty(&next)?,
        )?;
        self.index = next;
        Ok(())
    }
    fn edit_entries(&mut self, ids: &[String], edit: impl Fn(&mut Organize)) -> Result<()> {
        self.edit_index(|index| {
            for entry in index.entries.iter_mut().filter(|e| ids.contains(&e.id)) {
                edit(&mut entry.organize);
            }
            Ok(())
        })
    }
    pub fn folders(&self) -> &[String] {
        &self.index.folders
    }
    pub fn create_folder(&mut self, name: &str) -> Result<String> {
        let name = folder_name(name)?;
        let made = name.clone();
        self.edit_index(move |index| {
            if index.folders.iter().any(|f| f.eq_ignore_ascii_case(&name)) {
                return Err(Error::Invalid(
                    "A folder with that name already exists".into(),
                ));
            }
            if index.folders.len() >= 200 {
                return Err(Error::Invalid("Use up to 200 folders".into()));
            }
            index.folders.push(name);
            index.folders.sort_by_key(|f| f.to_lowercase());
            Ok(())
        })?;
        Ok(made)
    }
    pub fn rename_folder(&mut self, old: &str, new: &str) -> Result<()> {
        let new = folder_name(new)?;
        self.edit_index(|index| {
            if index
                .folders
                .iter()
                .any(|f| f != old && f.eq_ignore_ascii_case(&new))
            {
                return Err(Error::Invalid(
                    "A folder with that name already exists".into(),
                ));
            }
            for f in index.folders.iter_mut().filter(|f| *f == old) {
                *f = new.clone();
            }
            for e in &mut index.entries {
                if e.organize.folder.as_deref() == Some(old) {
                    e.organize.folder = Some(new.clone());
                }
            }
            index.folders.sort_by_key(|f| f.to_lowercase());
            Ok(())
        })
    }
    pub fn delete_folder(&mut self, name: &str) -> Result<()> {
        self.edit_index(|index| {
            index.folders.retain(|f| f != name);
            for e in &mut index.entries {
                if e.organize.folder.as_deref() == Some(name) {
                    e.organize.folder = None;
                }
            }
            Ok(())
        })
    }
    pub fn move_to_folder(&mut self, ids: &[String], folder: Option<&str>) -> Result<()> {
        self.edit_entries(ids, |o| o.folder = folder.map(str::to_owned))
    }
    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> Result<()> {
        self.edit_entries(&[id.to_owned()], |o| {
            o.pinned = pinned;
            o.closed &= !pinned;
        })
    }
    pub fn set_closed(&mut self, id: &str, closed: bool) -> Result<()> {
        self.edit_entries(&[id.to_owned()], |o| {
            o.closed = closed;
            o.pinned &= !closed;
        })
    }
    pub fn set_source(
        &mut self,
        id: &str,
        app: Option<String>,
        site: Option<String>,
    ) -> Result<()> {
        let (app, site) = (source_name(app), source_name(site));
        self.edit_entries(&[id.to_owned()], |o| {
            o.app = app.clone();
            o.site = site.clone();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> (tempfile::TempDir, History, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let doc = Document::blank(32, 32).unwrap();
        let ids = ["One", "Two"]
            .map(|n| h.create(&doc, n, Origin::Capture).unwrap().id)
            .to_vec();
        (dir, h, ids)
    }
    fn organize(h: &History, id: &str) -> Organize {
        h.entries()
            .iter()
            .find(|e| e.id == id)
            .unwrap()
            .organize
            .clone()
    }
    #[test]
    fn folders_persist_and_deleting_one_keeps_its_captures() {
        let (dir, mut h, ids) = store();
        h.create_folder("Support replies").unwrap();
        assert!(h.create_folder("support REPLIES").is_err());
        assert!(h.create_folder("  ").is_err());
        h.move_to_folder(&ids[..1], Some("Support replies"))
            .unwrap();
        h.rename_folder("Support replies", "Replies").unwrap();
        assert_eq!(organize(&h, &ids[0]).folder.as_deref(), Some("Replies"));
        let reopened = History::open(dir.path()).unwrap();
        assert_eq!(reopened.folders(), ["Replies"]);
        assert_eq!(
            organize(&reopened, &ids[0]).folder.as_deref(),
            Some("Replies")
        );
        h.delete_folder("Replies").unwrap();
        assert!(h.folders().is_empty());
        assert_eq!(h.entries().len(), 2, "the captures stay");
        assert!(organize(&h, &ids[0]).is_empty());
    }
    #[test]
    fn pin_close_and_source_survive_a_new_version() {
        let (_dir, mut h, ids) = store();
        h.set_pinned(&ids[1], true).unwrap();
        h.set_source(&ids[1], Some("Excel".into()), None).unwrap();
        let entry = h.entries().iter().find(|e| e.id == ids[1]).unwrap().clone();
        let doc = Document::blank(40, 40).unwrap();
        let updated = h.update(&entry, &doc).unwrap();
        assert!(updated.organize.pinned);
        assert_eq!(updated.organize.app.as_deref(), Some("Excel"));
        // Closing takes it out of Recent only, and a pin cannot stay on it.
        h.set_closed(&ids[1], true).unwrap();
        let o = organize(&h, &ids[1]);
        assert!(o.closed && !o.pinned);
        assert_eq!(h.entries().len(), 2);
    }
    #[test]
    fn previous_versions_can_be_turned_off() {
        let (_dir, mut h, ids) = store();
        let doc = Document::blank(40, 40).unwrap();
        let count = |h: &History, id: &str| h.revisions(id).unwrap().len();
        let entry = h.entries().iter().find(|e| e.id == ids[0]).unwrap().clone();
        let entry = h.update(&entry, &doc).unwrap();
        assert_eq!(count(&h, &ids[0]), 1);
        h.keep_versions = false;
        let doc = Document::blank(44, 44).unwrap();
        h.update(&entry, &doc).unwrap();
        assert_eq!(count(&h, &ids[0]), 1, "no new version was kept");
    }
}
