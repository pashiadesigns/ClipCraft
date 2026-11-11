//! Startup errors must remain visible in the Windows GUI executable.
use std::{io::Write, path::PathBuf, sync::OnceLock};
static LOG: OnceLock<PathBuf> = OnceLock::new();
pub(crate) fn install() {
    let migration = crate::legacy_name::migrate_local_data();
    if let Some(base) = std::env::var_os("LOCALAPPDATA") {
        let folder = PathBuf::from(base).join("ClipDesigner/logs");
        if std::fs::create_dir_all(&folder).is_ok() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = LOG.set(folder.join(format!("startup-{stamp}-{}.log", std::process::id())));
        }
    }
    note(concat!(
        "ClipDesigner ",
        env!("CARGO_PKG_VERSION"),
        " starting"
    ));
    if let Some(migration) = migration {
        note(&migration);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        note(&format!("Panic: {info}"));
        previous(info);
    }));
}
pub(crate) fn note(message: &str) {
    if let Some(path) = LOG.get() {
        let _ = append(path, message);
    }
}
fn append(path: &std::path::Path, message: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{message}")?;
    file.flush()
}
pub(crate) fn failure(message: &str) {
    note(message);
    let location = LOG
        .get()
        .map(|p| format!("\n\nDetails were saved to:\n{}", p.display()))
        .unwrap_or_default();
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("ClipDesigner could not start")
        .set_description(format!("{message}\n\nPlease share these details so we can identify the startup problem.{location}"))
        .show();
}
#[cfg(test)]
mod tests {
    #[test]
    fn diagnostics_preserve_startup_phase_and_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("startup.log");
        super::append(&path, "Starting graphics").unwrap();
        super::append(&path, "No suitable adapter").unwrap();
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "Starting graphics\nNo suitable adapter\n"
        );
    }
}
