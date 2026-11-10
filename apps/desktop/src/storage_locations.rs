use crate::tips::Tip as _;
use crate::{Editor, egui};
use capture_core::quick_preferences::{Choice, Folder};
use std::path::PathBuf;

pub(crate) fn configured(kind: Folder) -> Option<PathBuf> {
    // Test instances and review harnesses retain their isolated explicit roots.
    #[cfg(test)]
    {
        let _ = kind;
        None
    }
    #[cfg(not(test))]
    {
        crate::quick_controls::QuickControls::new().folder(kind)
    }
}
impl Editor {
    pub(crate) fn template_root(&self) -> Option<PathBuf> {
        self.quick_controls
            .folder(Folder::Templates)
            .or_else(|| self.history.root().map(|p| p.join("templates")))
    }
    pub(crate) fn managed_recording_path(&self) -> Result<PathBuf, String> {
        let root = self
            .history
            .root()
            .ok_or("Choose a project storage folder in Settings before recording.")?
            .join("recordings");
        std::fs::create_dir_all(&root).map_err(|e| {
            format!(
                "Cannot save recordings here: {e}. Choose a writable project folder in Settings."
            )
        })?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        Ok(root.join(format!("ClipCraft-{stamp}.mp4")))
    }
    pub(crate) fn storage_location_options(&mut self, ui: &mut egui::Ui) {
        crate::panel_ui::heading(ui, "Library", "Storage locations");
        ui.label("New recordings and projects are saved automatically. Folder changes take effect after restarting ClipCraft; existing files stay in their current folders.");
        for (kind, label, fallback) in [
            (
                Folder::ManagedProjects,
                "Projects & recordings",
                self.history.root(),
            ),
            (Folder::Templates, "Template library", self.template_root()),
            (
                Folder::Presets,
                "Preset library",
                std::env::var_os("CLIPCRAFT_STYLE_DIR")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("LOCALAPPDATA")
                            .map(|p| PathBuf::from(p).join("ClipCraft/styles-v1"))
                    }),
            ),
        ] {
            ui.add_space(8.);
            ui.label(crate::panel_ui::title(label, 13.));
            let current = self.quick_controls.folder(kind).or(fallback);
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 92.).max(80.);
                let text = current
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Not configured".into());
                ui.add_sized([width, 28.], egui::Label::new(&text).truncate())
                    .tip(&text);
                if ui
                    .add_sized([80., 28.], egui::Button::new("Browse…"))
                    .clicked()
                {
                    self.suspend_global_capture();
                    let mut dialog = rfd::FileDialog::new().set_title(label);
                    if let Some(p) = &current {
                        dialog = dialog.set_directory(p);
                    }
                    if let Some(path) = dialog.pick_folder() {
                        match self
                            .quick_controls
                            .save_lifecycle(Choice::Folder(kind, path))
                        {
                            Ok(()) => {
                                self.status =
                                    "Storage location saved. Restart ClipCraft to use it.".into()
                            }
                            Err(e) => self.status = e,
                        }
                    }
                }
            });
        }
        ui.add_space(10.);
        crate::panel_ui::divider(ui);
    }
}
