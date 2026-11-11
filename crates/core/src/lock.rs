//! Edit locks for projects in shared folders (approved beta 6 tab 3).
//!
//! Opening a project from a shared folder leaves a small lock file beside
//! it saying who has it and since when. Anyone else who opens it sees that
//! and gets it read-only, so two people never overwrite each other. The lock
//! is removed when the project is closed; one older than 12 hours (a PC that
//! went offline) no longer counts.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STALE_MS: u64 = 12 * 60 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// The name shown to others ("Sam").
    pub name: String,
    pub computer: String,
    pub since_ms: u64,
}
impl Lock {
    fn same_holder(&self, other: &Lock) -> bool {
        self.computer.eq_ignore_ascii_case(&other.computer) && self.name == other.name
    }
}

/// `~Onboarding guide.iclip.cdlock` beside `Onboarding guide.iclip`.
pub fn path(project: &Path) -> PathBuf {
    let name = project
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    project.with_file_name(format!("~{name}.cdlock"))
}

/// The lock on `project`, if one is there and not stale.
pub fn read(project: &Path, now_ms: u64) -> Option<Lock> {
    let bytes = std::fs::read(path(project)).ok()?;
    if bytes.len() > 4096 {
        return None;
    }
    let lock: Lock = serde_json::from_slice(&bytes).ok()?;
    let fresh = now_ms.saturating_sub(lock.since_ms) < STALE_MS && lock.since_ms <= now_ms + 60_000;
    (fresh && lock.name.chars().count() <= 60 && lock.computer.chars().count() <= 64)
        .then_some(lock)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opened {
    /// The lock is ours now (or already was).
    Mine,
    /// Someone else has it; open read-only.
    Theirs(Lock),
}

/// Takes the lock on `project` for `me`, unless someone else holds it.
/// A folder that can't take a lock file (read-only) opens as `Mine`, since
/// nobody can save there anyway.
pub fn take(project: &Path, me: &Lock) -> Opened {
    if let Some(held) = read(project, me.since_ms)
        && !held.same_holder(me)
    {
        return Opened::Theirs(held);
    }
    if let Ok(bytes) = serde_json::to_vec(me) {
        let _ = crate::atomic_write(&path(project), &bytes);
    }
    Opened::Mine
}

/// Removes `me`'s lock from `project`; someone else's is left alone.
pub fn release(project: &Path, me: &Lock) {
    if read(project, me.since_ms).is_none_or(|held| held.same_holder(me)) {
        let _ = std::fs::remove_file(path(project));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock(name: &str, computer: &str, since_ms: u64) -> Lock {
        Lock {
            name: name.into(),
            computer: computer.into(),
            since_ms,
        }
    }

    #[test]
    fn someone_elses_lock_opens_read_only_until_it_clears_or_goes_stale() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Onboarding guide.iclip");
        std::fs::write(&project, b"{}").unwrap();
        let sam = lock("Sam", "SAM-LAPTOP", 1_000);
        assert_eq!(take(&project, &sam), Opened::Mine);
        assert_eq!(
            path(&project).file_name().unwrap(),
            "~Onboarding guide.iclip.cdlock"
        );
        let alex = lock("Alex", "ALEX-PC", 5_000);
        assert_eq!(take(&project, &alex), Opened::Theirs(sam.clone()));
        // Alex closing doesn't remove Sam's lock.
        release(&project, &alex);
        assert_eq!(read(&project, 5_000), Some(sam.clone()));
        // Sam reopening on the same PC keeps it.
        assert_eq!(
            take(&project, &lock("Sam", "sam-laptop", 9_000)),
            Opened::Mine
        );
        release(&project, &sam);
        assert_eq!(read(&project, 9_000), None);
        assert_eq!(take(&project, &alex), Opened::Mine);
        // Twelve hours later the lock no longer counts.
        let later = lock("Sam", "SAM-LAPTOP", 5_000 + STALE_MS);
        assert_eq!(take(&project, &later), Opened::Mine);
    }

    #[test]
    fn a_damaged_lock_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("a.iclip");
        std::fs::write(path(&project), b"not json").unwrap();
        assert_eq!(read(&project, 1), None);
        assert_eq!(take(&project, &lock("Sam", "PC", 1)), Opened::Mine);
    }
}
