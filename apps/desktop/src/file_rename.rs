//! File > Info: Rename and Duplicate for the open project (FILE-007,
//! approved bi-proposal 1). Rename moves the file, a video's media folder,
//! the Library label and the window title together; Duplicate saves a copy
//! next to the project and opens it.
use super::*;
use crate::tips::Tip as _;
use egui::RichText;

impl Editor {
    /// The name shown and edited for the open project, without extension.
    pub(crate) fn project_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .or_else(|| self.shell.untitled_name.clone())
            .unwrap_or_else(|| "Untitled".into())
    }
    /// Opens the rename field on File > Info.
    pub(crate) fn begin_rename(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.shell.backstage = Some(crate::backstage::Page::Info);
        self.shell.rename = Some((self.project_name(), String::new()));
    }
    /// Renames the open project; on failure nothing changes and the reason is returned.
    pub(crate) fn rename_project(&mut self, name: &str) -> Result<(), String> {
        let name = capture_core::project_file::check_name(name)
            .map_err(|e| e.to_string())?
            .to_owned();
        let Some(path) = self.path.clone() else {
            self.shell.untitled_name = Some(name.clone());
            self.status = format!("Renamed to {name}. The file is created when you save.");
            return Ok(());
        };
        let video = capture_core::project_file::is_video_project(&path);
        let target = if path.exists() {
            capture_core::project_file::rename(&path, &name).map_err(|e| e.to_string())?
        } else {
            capture_core::project_file::renamed(&path, &name).map_err(|e| e.to_string())?
        };
        if video {
            self.session.relocate_media(
                &capture_core::project_file::media_folder(&path),
                &capture_core::project_file::media_folder(&target),
            );
        }
        let entry = self
            .history
            .active
            .as_deref()
            .and_then(|id| self.history.entry(id));
        if let Some(entry) = entry
            && let Some(store) = self.history.store.as_mut()
        {
            let _ = store.batch(&[entry], capture_core::history::Batch::Rename(&name));
            self.history.refresh();
        }
        self.path = Some(target.clone());
        self.status = format!("Renamed to {}", target.display());
        Ok(())
    }
    /// Saves a copy named "<name> (copy)" next to the project and opens it.
    pub(crate) fn duplicate_project(&mut self) {
        if !self.commit_text() {
            return;
        }
        let Some(path) = self.path.clone().filter(|p| {
            capture_core::project_file::is_image_project(p)
                || capture_core::project_file::is_video_project(p)
        }) else {
            self.status = "Save the project first, then duplicate it.".into();
            return;
        };
        let copy = capture_core::project_file::copy_path(&path);
        match self.session.save(&copy) {
            Ok(()) => {
                self.path = Some(copy.clone());
                self.status = format!(
                    "Opened the copy {}. The original is unchanged.",
                    copy.display()
                );
            }
            Err(e) => self.fail(e),
        }
    }
    /// Palette menu > New from image colors: the open image's main colors.
    pub(crate) fn palette_from_image(&mut self, ctx: &egui::Context) {
        let pixels = if self.is_video() {
            // The frame on screen; the editor's still fallback has no video in it (BI6).
            match self.video_frame_pixels() {
                Some(p) => Ok(p),
                None => Err("Wait for the video frame to load, then try again.".into()),
            }
        } else {
            self.session.document.render().map_err(|e| e.to_string())
        };
        match pixels {
            Ok(pixels) => {
                let colors = capture_core::palettes::from_image(&pixels, 8);
                crate::color_widgets::create_palette(
                    ctx,
                    &format!("From image: {}", self.project_name()),
                    colors,
                );
            }
            Err(e) => self.status = format!("Couldn't read the image colors: {e}"),
        }
    }
    /// F2 or a double-click on the title bar name starts Rename.
    pub(crate) fn rename_shortcuts(&mut self, ctx: &egui::Context) {
        let requested = ctx.data_mut(|d| {
            d.remove_temp::<bool>(crate::chrome::rename_request_id())
                .unwrap_or(false)
        });
        let f2 = self.shell.rename.is_none()
            && self.text_draft.is_none()
            && ctx.memory(|m| m.focused().is_none())
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2));
        if requested || f2 {
            self.begin_rename();
        }
    }
    pub(crate) fn file_info(&mut self, ui: &mut egui::Ui) {
        let video = self.is_video();
        ui.heading(if video {
            "Video project"
        } else {
            "Image project"
        });
        ui.add_space(14.0);
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(36, 43, 46))
            .corner_radius(8)
            .inner_margin(12)
            .show(ui, |ui| {
                ui.set_max_width(640.);
                let mut apply = None;
                let mut cancel = false;
                if let Some((draft, error)) = self.shell.rename.as_mut() {
                    ui.horizontal(|ui| {
                        let field = ui.add(
                            egui::TextEdit::singleline(draft)
                                .id(egui::Id::unique("project_rename"))
                                .desired_width(260.),
                        );
                        if !field.has_focus() && error.is_empty() && !field.lost_focus() {
                            field.request_focus();
                        }
                        let ext = self
                            .path
                            .as_ref()
                            .and_then(|p| p.extension())
                            .map(|e| format!(".{}", e.to_string_lossy()))
                            .unwrap_or_default();
                        ui.label(RichText::new(ext).weak());
                        let enter =
                            field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui
                            .button("Rename")
                            .tip("Rename the project file, its Library entry and the window title.")
                            .clicked()
                            || enter
                        {
                            apply = Some(draft.clone());
                        }
                        if ui.button("Cancel").tip("Keep the current name.").clicked()
                            || ui.input(|i| i.key_pressed(egui::Key::Escape))
                        {
                            cancel = true;
                        }
                    });
                    if !error.is_empty() {
                        ui.colored_label(egui::Color32::from_rgb(228, 135, 127), error.as_str());
                    }
                } else {
                    ui.label(RichText::new(self.project_name()).size(16.).strong());
                }
                ui.label(
                    RichText::new(
                        self.path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "Not saved yet".into()),
                    )
                    .weak()
                    .monospace(),
                );
                if let Some(name) = apply {
                    match self.rename_project(&name) {
                        Ok(()) => self.shell.rename = None,
                        Err(e) => {
                            if let Some((_, error)) = self.shell.rename.as_mut() {
                                *error = e;
                            }
                        }
                    }
                }
                if cancel {
                    self.shell.rename = None;
                }
            });
        ui.add_space(10.);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.shell.rename.is_none(), egui::Button::new("Rename"))
                .tip("Rename the project. Shortcut: F2, or double-click the name in the title bar.")
                .clicked()
            {
                self.begin_rename();
            }
            if ui
                .button("Duplicate")
                .tip("Save a copy named \"(copy)\" next to this project and open it.")
                .clicked()
            {
                self.duplicate_project();
            }
            if video
                && let Some(path) = self.recording_file()
                && ui
                    .button("Show recording file")
                    .tip("Open File Explorer with the original recording selected.")
                    .clicked()
                && let Err(e) = crate::export_ui::show_in_explorer(&path)
            {
                self.status = e;
            }
        });
        ui.add_space(18.0);
        ui.label(format!(
            "{} × {} pixels",
            self.session.document.width, self.session.document.height
        ));
        ui.label(format!(
            "{} editable objects",
            self.session.document.annotations.len()
        ));
        ui.add_space(16.0);
        self.autosave_status(ui);
        ui.label(if video {
            "A video project keeps its media in the .vclip-media folder beside it. Rename moves both together."
        } else {
            "Editable projects retain the original image and separate annotations. PNG exports combine them into one image."
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_from_file_info_moves_the_file_and_keeps_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let dir = tempfile::tempdir().unwrap();
        // Unsaved: the name is kept for the first Save.
        e.path = None;
        e.rename_project("Expense report").unwrap();
        assert_eq!(e.project_name(), "Expense report");
        assert!(e.window_title().contains("Expense report"));
        let path = dir.path().join("Expense report.iclip");
        e.session.save(&path).unwrap();
        e.path = Some(path.clone());
        e.execute(Command::Undo);
        let undo = e.session.can_undo();
        assert!(e.rename_project("bad/name").is_err());
        assert!(path.exists(), "a refused name changes nothing");
        e.rename_project("Expense steps").unwrap();
        let renamed = dir.path().join("Expense steps.iclip");
        assert_eq!(e.path.as_deref(), Some(renamed.as_path()));
        assert!(renamed.exists() && !path.exists());
        assert_eq!(e.session.can_undo(), undo);
        e.duplicate_project();
        assert_eq!(
            e.path.as_deref(),
            Some(dir.path().join("Expense steps (copy).iclip").as_path())
        );
        assert!(renamed.exists());
    }
}
