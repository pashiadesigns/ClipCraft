//! ClipDesigner was called ClipCraft through 1.0.0-beta.3. Profiles from those
//! releases keep their Library, preferences, styles and presets in
//! `%LOCALAPPDATA%\ClipCraft`; this moves them to `%LOCALAPPDATA%\ClipDesigner`
//! on first launch so nothing is left behind by the rename.
use std::path::{Path, PathBuf};

const LEGACY: &str = "ClipCraft";
const CURRENT: &str = "ClipDesigner";

/// Moves the legacy data folder before any other code creates the new one.
/// Entries that already exist under the new name are left in the legacy
/// folder untouched, and anything that cannot be moved now (for example while
/// an old ClipCraft is still running) is retried on the next launch.
pub(crate) fn migrate_local_data() -> Option<String> {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    migrate(&base)
}

fn migrate(base: &Path) -> Option<String> {
    let legacy = base.join(LEGACY);
    let current = base.join(CURRENT);
    if !legacy.is_dir() {
        return None;
    }
    if !current.exists() {
        return Some(match std::fs::rename(&legacy, &current) {
            Ok(()) => format!("Moved {} to {}", legacy.display(), current.display()),
            Err(e) => format!("Could not move {}: {e}", legacy.display()),
        });
    }
    let entries = match std::fs::read_dir(&legacy) {
        Ok(entries) => entries,
        Err(e) => return Some(format!("Could not read {}: {e}", legacy.display())),
    };
    let (mut moved, mut kept) = (0, Vec::new());
    for entry in entries.flatten() {
        let target = current.join(entry.file_name());
        if target.exists() {
            kept.push(entry.file_name().to_string_lossy().into_owned());
        } else if std::fs::rename(entry.path(), &target).is_ok() {
            moved += 1;
        } else {
            kept.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    if kept.is_empty() {
        let _ = std::fs::remove_dir(&legacy);
    }
    (moved > 0 || !kept.is_empty()).then(|| {
        format!(
            "Moved {moved} item(s) from {} to {}; kept: {}",
            legacy.display(),
            current.display(),
            if kept.is_empty() {
                "none".into()
            } else {
                kept.join(", ")
            }
        )
    })
}

/// A folder chosen in Settings inside the legacy data folder now lives under
/// the new name; other folders are returned unchanged.
pub(crate) fn relocate(path: &Path) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .and_then(|base| relocate_from(Path::new(&base), path))
        .unwrap_or_else(|| path.to_path_buf())
}

fn relocate_from(base: &Path, path: &Path) -> Option<PathBuf> {
    let rest = path.strip_prefix(base.join(LEGACY)).ok()?;
    Some(base.join(CURRENT).join(rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn moves_the_whole_legacy_profile() {
        let base = tempfile::tempdir().unwrap();
        write(&base.path().join("ClipCraft/history-v1/index"), "library");
        write(&base.path().join("ClipCraft/preferences-v1/prefs"), "prefs");
        assert!(migrate(base.path()).unwrap().starts_with("Moved "));
        assert!(!base.path().join("ClipCraft").exists());
        let read = |p: &str| std::fs::read_to_string(base.path().join(p)).unwrap();
        assert_eq!(read("ClipDesigner/history-v1/index"), "library");
        assert_eq!(read("ClipDesigner/preferences-v1/prefs"), "prefs");
        assert_eq!(migrate(base.path()), None);
    }

    #[test]
    fn merges_into_an_existing_folder_without_overwriting() {
        let base = tempfile::tempdir().unwrap();
        write(&base.path().join("ClipCraft/history-v1/index"), "library");
        write(&base.path().join("ClipCraft/logs/old.log"), "old");
        write(&base.path().join("ClipDesigner/logs/new.log"), "new");
        let report = migrate(base.path()).unwrap();
        assert!(report.contains("kept: logs"), "{report}");
        let read = |p: &str| std::fs::read_to_string(base.path().join(p)).unwrap();
        assert_eq!(read("ClipDesigner/history-v1/index"), "library");
        assert_eq!(read("ClipDesigner/logs/new.log"), "new");
        assert_eq!(read("ClipCraft/logs/old.log"), "old");
    }

    #[test]
    fn fresh_profiles_are_untouched() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(migrate(base.path()), None);
        assert!(!base.path().join("ClipDesigner").exists());
    }

    #[test]
    fn chosen_folders_follow_the_move() {
        let base = Path::new("C:/Users/a/AppData/Local");
        assert_eq!(
            relocate_from(base, &base.join("ClipCraft/history-v1/custom")),
            Some(base.join("ClipDesigner/history-v1/custom"))
        );
        assert_eq!(relocate_from(base, Path::new("D:/Clips")), None);
    }
}
