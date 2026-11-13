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
    /// File › Info (approved File menu): one details card, with Rename,
    /// Duplicate and Show in File Explorer as cards beside it.
    pub(crate) fn file_info(&mut self, ui: &mut egui::Ui) {
        let video = self.is_video();
        crate::backstage::heading(ui, "General", "Info", None);
        ui.add_space(10.);
        let width = ui.available_width().min(1000.);
        let side = (width * 0.38).clamp(200., 320.);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(width - side - 16.);
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(36, 43, 46))
                    .stroke(egui::Stroke::new(1., egui::Color32::from_rgb(52, 61, 64)))
                    .corner_radius(9)
                    .inner_margin(egui::Margin::symmetric(16, 14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        self.info_name(ui);
                        ui.add_space(8.);
                        let d = &self.session.document;
                        let kept = match &self.path {
                            Some(p) => p.display().to_string(),
                            None => "Library (not saved as a file yet)".into(),
                        };
                        let kind = if video {
                            "Video project".to_owned()
                        } else if let Some(g) = d.guide.as_ref().filter(|g| g.pages.is_some()) {
                            format!("Image project · guide, {} pages", g.page_count())
                        } else {
                            "Image project".to_owned()
                        };
                        let shared = match self.locked_by_other() {
                            Some(lock) => format!("{} is editing", lock.name),
                            None => "No one else is editing".into(),
                        };
                        let (saved, _) = self.autosave_summary();
                        egui::Grid::new("file-info-details")
                            .num_columns(2)
                            .spacing([16., 6.])
                            .show(ui, |ui| {
                                for (label, value) in [
                                    ("Type", kind),
                                    ("Kept in", kept),
                                    ("Size", format!("{} × {} px", d.width, d.height)),
                                    ("Objects", format!("{} editable objects", d.annotations.len())),
                                    ("Last saved", saved.to_owned()),
                                    ("Shared", shared),
                                ] {
                                    ui.label(RichText::new(label).color(egui::Color32::from_gray(150)));
                                    ui.add(egui::Label::new(value).wrap());
                                    ui.end_row();
                                }
                            });
                    });
                ui.add_space(8.);
                ui.label(
                    RichText::new(if video {
                        "A video project keeps its media in the .vclip-media folder beside it. Rename moves both together."
                    } else {
                        "Editable projects keep the original picture and every object separate. Exports combine them into one file."
                    })
                    .color(egui::Color32::from_gray(150)),
                );
            });
            ui.add_space(16.);
            ui.vertical(|ui| {
                ui.set_width(side);
                let explorer = if video {
                    self.recording_file()
                } else {
                    self.path.clone()
                };
                let show_detail = if explorer.is_some() {
                    if video {
                        "The original recording"
                    } else {
                        "The project file"
                    }
                } else {
                    "Available once saved as a file"
                };
                match crate::backstage::cards(
                    ui,
                    &[
                        ("Text", "Rename", "F2, or double-click the title", false),
                        ("Duplicate", "Duplicate", "A copy named \u{201c}(copy)\u{201d}", false),
                        ("Folder", "Show in File Explorer", show_detail, false),
                    ],
                ) {
                    Some(0) => self.begin_rename(),
                    Some(1) => self.duplicate_project(),
                    Some(_) => {
                        if let Some(path) = explorer
                            && let Err(e) = crate::export_ui::show_in_explorer(&path)
                        {
                            self.fail(e);
                        }
                    }
                    None => {}
                }
            });
        });
    }
    /// The project's name, or the rename field while renaming.
    fn info_name(&mut self, ui: &mut egui::Ui) {
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
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
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
                crate::guided::dialog_message(ui, crate::guided::Kind::Error, error, false);
            }
        } else {
            ui.label(RichText::new(self.project_name()).size(17.).strong());
        }
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
