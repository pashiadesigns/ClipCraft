//! Exact RGBA swatches, isolated from document history and other preferences.
use crate::{Error, Result, atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}
#[derive(Serialize, Deserialize)]
struct Shelf {
    version: u32,
    colors: Vec<[u8; 4]>,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let store = Self {
            root: fs::canonicalize(root)?,
        };
        store.read()?;
        Ok(store)
    }
    pub fn read(&self) -> Result<Vec<[u8; 4]>> {
        let path = self.root.join("colors-v1.json");
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.into()),
        };
        if metadata.len() > 4096 {
            return Err(Error::Invalid(
                "Saved colors file is too large; file preserved".into(),
            ));
        }
        let shelf: Shelf = serde_json::from_slice(&fs::read(path)?)?;
        if shelf.version != 1
            || shelf.colors.len() > 32
            || shelf
                .colors
                .iter()
                .enumerate()
                .any(|(i, c)| shelf.colors[..i].contains(c))
        {
            return Err(Error::Invalid(
                "Unsupported saved colors; file preserved".into(),
            ));
        }
        Ok(shelf.colors)
    }
    pub fn set(&self, color: [u8; 4], add: bool) -> Result<Vec<[u8; 4]>> {
        self.change(|colors| {
            if add && !colors.contains(&color) {
                if colors.len() == 32 {
                    return Err(Error::Invalid(
                        "My Colors is full (32 colors). Remove one to add another.".into(),
                    ));
                }
                colors.push(color);
            } else if !add {
                colors.retain(|c| *c != color);
            }
            Ok(())
        })
    }
    pub fn replace(&self, old: [u8; 4], new: [u8; 4]) -> Result<Vec<[u8; 4]>> {
        self.change(|colors| {
            if !colors.contains(&old) {
                return Err(Error::Invalid(
                    "This saved color was removed in another window.".into(),
                ));
            }
            if old != new {
                colors.retain(|c| *c != new);
                *colors.iter_mut().find(|c| **c == old).unwrap() = new;
            }
            Ok(())
        })
    }
    fn change(&self, edit: impl FnOnce(&mut Vec<[u8; 4]>) -> Result<()>) -> Result<Vec<[u8; 4]>> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("colors.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Saved colors are busy in another window".into()))?;
        let mut colors = self.read()?;
        edit(&mut colors)?;
        atomic_write(
            &self.root.join("colors-v1.json"),
            &serde_json::to_vec(&Shelf {
                version: 1,
                colors: colors.clone(),
            })?,
        )?;
        Ok(colors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replace_swatch_preserves_order_alpha_merges_writers_and_rejects_stale_or_locked_edits() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let b = Store::open(temp.path()).unwrap();
        let old = [12, 34, 56, 255];
        let other = [99, 89, 79, 128];
        let new = [21, 43, 65, 0];
        a.set(old, true).unwrap();
        b.set(other, true).unwrap();
        assert_eq!(a.replace(old, new).unwrap(), [new, other]);
        assert_eq!(
            Store::open(temp.path()).unwrap().read().unwrap(),
            [new, other]
        );
        assert!(b.replace(old, other).is_err());
        assert_eq!(b.replace(other, new).unwrap(), [new]);
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp.path().join("colors.lock"))
            .unwrap();
        lock.lock().unwrap();
        assert!(a.replace(new, old).is_err());
        assert_eq!(a.read().unwrap(), [new]);
    }
    #[test]
    fn saved_colors_merge_exact_hidden_rgba_reopen_remove_and_preserve_invalid_files() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let b = Store::open(temp.path()).unwrap();
        a.set([17, 85, 201, 0], true).unwrap();
        b.set([23, 45, 67, 128], true).unwrap();
        a.set([17, 85, 201, 0], true).unwrap();
        assert_eq!(
            Store::open(temp.path()).unwrap().read().unwrap(),
            [[17, 85, 201, 0], [23, 45, 67, 128]]
        );
        assert_eq!(b.set([17, 85, 201, 0], false).unwrap(), [[23, 45, 67, 128]]);
        for n in 0..31 {
            a.set([n, 0, 0, 255], true).unwrap();
        }
        let path = temp.path().join("colors-v1.json");
        let before = fs::read(&path).unwrap();
        assert!(b.set([255; 4], true).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, br#"{"version":2,"colors":[]}"#).unwrap();
        assert!(a.set([1; 4], true).is_err());
        assert_eq!(fs::read(&path).unwrap(), br#"{"version":2,"colors":[]}"#);
    }
}
