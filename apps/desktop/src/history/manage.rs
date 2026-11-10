use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    AddTags,
    RemoveTags,
    Delete,
    Export,
    Rename,
    Duplicate,
    RenameTag,
    DeleteTag,
}
pub(super) struct Dialog {
    action: Action,
    entries: Vec<Entry>,
    tags: String,
    name: String,
    editable: bool,
    format: capture_core::export::Format,
    error: String,
    focus_pending: bool,
    cleanup: bool,
    cleanup_summary: String,
}
impl Editor {
    fn cleanup_protected(&self, entries: &[Entry]) -> Vec<std::path::PathBuf> {
        let ids: Vec<_> = entries.iter().map(|e| e.id.clone()).collect();
        let mut paths = self.project_sessions.retained_media(&ids);
        if !self
            .history
            .active
            .as_ref()
            .is_some_and(|id| ids.contains(id))
        {
            paths.extend(self.session.retained_media());
        }
        paths.extend(self.template_root());
        paths
    }
    pub(super) fn begin_global_tag(&mut self, tag: &str, delete: bool) {
        let entries = self
            .history
            .entries()
            .into_iter()
            .filter(|e| e.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            .collect();
        self.begin_library_management(
            if delete {
                Action::DeleteTag
            } else {
                Action::RenameTag
            },
            entries,
        );
        if let Some(d) = &mut self.history.manage {
            d.tags = tag.into();
            d.name = tag.into();
        }
    }
    pub(super) fn library_selection_context(
        &mut self,
        response: &egui::Response,
        entry: &Entry,
    ) -> bool {
        if !self.shell.library
            || self.history.library.selected.len() < 2
            || !self.history.library.selected.contains(&entry.id)
        {
            return false;
        }
        let mut action = None;
        response.context_menu(|ui| {
            ui.label(format!("{} selected", self.history.library.selected.len()));
            for (label, a, icon) in [
                ("Save All As…", Action::Export, "Save As"),
                ("Duplicate All", Action::Duplicate, "Copy"),
                ("Rename All…", Action::Rename, "Text"),
                ("Edit tags…", Action::AddTags, "Tag"),
                ("Delete from Library…", Action::Delete, "Delete"),
            ] {
                let response = ui.add(egui::Button::new("").min_size(egui::vec2(215., 26.)));
                response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
                ui.painter().text(
                    response.rect.left_center() + egui::vec2(32., 0.),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(13.),
                    egui::Color32::from_gray(232),
                );
                super::actions::icon(
                    ui,
                    response.rect.left_center() + egui::vec2(15., 0.),
                    icon,
                    crate::shell::ACCENT,
                );
                if response.clicked() {
                    action = Some(a);
                    ui.close();
                }
            }
            let ids: Vec<String> = self.history.library.selected.iter().cloned().collect();
            self.folder_menu(ui, &ids);
        });
        if let Some(action) = action {
            let entries = self
                .history
                .library
                .filtered(&self.history.entries(), &self.history.query)
                .into_iter()
                .filter(|e| self.history.library.selected.contains(&e.id))
                .collect();
            self.begin_library_management(action, entries);
        }
        true
    }
    pub(super) fn begin_library_management(&mut self, action: Action, entries: Vec<Entry>) {
        if action == Action::Export && !self.commit_text() {
            return;
        }
        match self
            .history
            .store
            .as_ref()
            .ok_or("Library is unavailable".into())
            .and_then(|s| s.limits().map_err(|e| e.to_string()))
        {
            Ok(_) => {
                let dialog = Dialog {
                    action,
                    tags: String::new(),
                    name: if entries.len() == 1 {
                        entries[0].label.chars().take(100).collect()
                    } else {
                        "Capture".into()
                    },
                    entries: entries.clone(),
                    editable: self.export_preferences.library_format.is_none(),
                    format: self.export_preferences.library_format.unwrap_or_default(),
                    error: String::new(),
                    focus_pending: true,
                    cleanup: false,
                    cleanup_summary: if action == Action::Delete {
                        self.history
                            .store
                            .as_ref()
                            .unwrap()
                            .media_cleanup_plan(&entries, &self.cleanup_protected(&entries))
                            .map(|files| {
                                format!(
                                    "{} unshared managed file(s) · {:.1} MB eligible",
                                    files.len(),
                                    files.iter().map(|f| f.bytes).sum::<u64>() as f64 / 1048576.
                                )
                            })
                            .unwrap_or_else(|e| format!("Media will be kept: {e}"))
                    } else {
                        String::new()
                    },
                };
                if action == Action::Duplicate {
                    self.status = self.apply_library_management(&dialog).unwrap_or_else(|e| e);
                } else {
                    self.history.manage = Some(dialog);
                }
            }
            Err(e) => self.history.error = e,
        }
    }
    pub(super) fn apply_library_management(&mut self, d: &Dialog) -> Result<String, String> {
        let protected = self.cleanup_protected(&d.entries);
        let active = self
            .history
            .active
            .as_ref()
            .is_some_and(|id| d.entries.iter().any(|e| &e.id == id));
        if active && self.library_entry_busy() {
            return Err("Finishing the current Library save; try again in a moment.".into());
        }
        let store = self
            .history
            .store
            .as_mut()
            .ok_or("Library is unavailable")?;
        let tags = d
            .tags
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let cleanup = if d.action == Action::Delete && d.cleanup {
            store
                .media_cleanup_plan(&d.entries, &protected)
                .map_err(|e| e.to_string())?
        } else {
            vec![]
        };
        match d.action {
            Action::RenameTag | Action::DeleteTag => store
                .replace_tag(
                    &d.tags,
                    if d.action == Action::RenameTag {
                        Some(&d.name)
                    } else {
                        None
                    },
                )
                .map(|_| ()),
            Action::Duplicate => store.duplicate_many(&d.entries).map(|_| ()),
            Action::Rename => {
                store.batch(&d.entries, capture_core::history::Batch::Rename(&d.name))
            }
            Action::AddTags => {
                if tags.is_empty() {
                    return Err("Enter at least one tag.".into());
                }
                store.batch(&d.entries, capture_core::history::Batch::AddTags(&tags))
            }
            Action::RemoveTags => {
                if tags.is_empty() {
                    return Err("Enter at least one tag.".into());
                }
                store.batch(&d.entries, capture_core::history::Batch::RemoveTags(&tags))
            }
            Action::Delete => store.batch(&d.entries, capture_core::history::Batch::Delete),
            Action::Export => unreachable!(),
        }
        .map_err(|e| e.to_string())?;
        let cleanup_message = if d.action == Action::Delete && d.cleanup {
            match store.remove_unused_media(&cleanup, &protected) {
                Ok((n, bytes)) => format!(
                    " Removed {n} unshared media file(s), {:.1} MB.",
                    bytes as f64 / 1048576.
                ),
                Err(e) => format!(" Project deletion completed; some media was kept: {e}"),
            }
        } else {
            String::new()
        };
        if d.action == Action::Delete {
            self.project_sessions
                .forget_deleted(&d.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>());
        }
        if active {
            if d.action == Action::Delete {
                self.detach_deleted_history();
            } else if let Some(entry) = self
                .history
                .active
                .as_ref()
                .and_then(|id| self.history.entry(id))
            {
                self.update_history_metadata(entry);
            }
        }
        self.history.refresh();
        if matches!(d.action, Action::RenameTag | Action::DeleteTag)
            && self
                .history
                .library
                .tag
                .as_ref()
                .is_some_and(|tag| tag.eq_ignore_ascii_case(&d.tags))
        {
            self.history.library.tag = if d.action == Action::RenameTag {
                Some(d.name.trim().into())
            } else {
                None
            };
        }
        if d.action == Action::Delete {
            self.history.library.selected.clear();
        }
        Ok(format!(
            "Updated {} Library item(s).{}",
            d.entries.len(),
            cleanup_message
        ))
    }
    pub(super) fn show_library_management(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.history.manage.take() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        let modal=crate::controls::Modal::new(egui::Id::unique("library-management")).show(ctx,|ui|{
            ui.set_width(420.);let title=match d.action{Action::AddTags=>"Add tags",Action::RemoveTags=>"Remove tags",Action::Delete=>"Delete selected captures",Action::Export=>"Save As",Action::Rename=>"Rename All",Action::Duplicate=>"Duplicate All",Action::RenameTag=>"Rename tag everywhere",Action::DeleteTag=>"Delete tag everywhere"};ui.heading(title);ui.add_space(8.);
            match d.action {
                Action::RenameTag|Action::DeleteTag=>{
                    ui.label(format!("Tag: {} · {} captures",d.tags,d.entries.len()));
                    if d.action==Action::RenameTag {
                        ui.label("New name");let input=ui.add(crate::unicode_input::TextEdit::singleline(&mut d.name).char_limit(48).desired_width(f32::INFINITY));crate::controls::focus_once(&input,&mut d.focus_pending);
                        ui.small("An existing tag with this name will be merged.");
                    } else {ui.label("Remove this tag from every Library item? Images and projects are kept.");}
                },
                Action::AddTags|Action::RemoveTags=>{ui.horizontal(|ui|{ui.label("Operation");ui.selectable_value(&mut d.action,Action::AddTags,"Add tags");ui.selectable_value(&mut d.action,Action::RemoveTags,"Remove tags");});ui.label(format!("{} selected captures",d.entries.len()));ui.label("Tags (comma separated)");let input=ui.text_edit_singleline(&mut d.tags);crate::controls::focus_once(&input,&mut d.focus_pending);},
                Action::Delete=>{ui.label(format!("Delete {} selected capture(s) from Library?",d.entries.len()));ui.label("Stored projects are removed permanently. Exported files are kept. Deleting the open project clears its canvas.");ui.add_space(12.);crate::controls::toggle(ui,&mut d.cleanup,"Also delete unused ClipCraft media");ui.small(&d.cleanup_summary);ui.small("Imported originals, shared files and media needed by other projects or Undo are always kept. Leave this off to keep all separate media.");},
                Action::Export|Action::Rename=>{
                    ui.label(format!("{} selected captures",d.entries.len()));
                    ui.label(if d.entries.len()>1 {"Base name"}else{"Name"});
                    let input=ui.add(crate::unicode_input::TextEdit::singleline(&mut d.name).char_limit(100).desired_width(f32::INFINITY));crate::controls::focus_once(&input,&mut d.focus_pending);
                    if d.entries.len()>1 { ui.small(format!("{} 1, {} 2, … · existing names are skipped",d.name,d.name)); }
                    if d.action==Action::Export {
                        ui.horizontal(|ui| {
                            ui.label("Format");
                            egui::ComboBox::from_id_salt("library-save-format").selected_text(if d.editable { "ClipCraft project" } else { d.format.label() }).show_ui(ui,|ui| {
                                ui.selectable_value(&mut d.editable,true,"ClipCraft project");
                                for format in [capture_core::export::Format::Png,capture_core::export::Format::Jpeg,capture_core::export::Format::Pdf] {
                                    if ui.selectable_label(!d.editable && d.format==format,format.label()).clicked(){d.editable=false;d.format=format;}
                                }
                            });
                        });
                        if !d.editable {
                            crate::controls::toggle(ui, &mut self.export_preferences.options.redaction_labels, "Include redaction labels");
                        }
                        self.export_preferences.after.controls(ui);
                        if d.entries.len()>1 {ui.small("A new subfolder keeps existing files untouched.");}
                    }
                },
                Action::Duplicate=>{},
            }
            if !d.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&d.error);}
            (submit,cancel) = crate::controls::dialog_actions(ui,if matches!(d.action,Action::Delete|Action::DeleteTag) {"Delete"} else {"Save"},"Cancel",true,matches!(d.action,Action::Delete|Action::DeleteTag));
        });
        if d.action == Action::Export {
            self.export_preferences.library_format = (!d.editable).then_some(d.format);
        }
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if submit {
            if d.action == Action::Export {
                if d.entries.is_empty() || d.name.trim().is_empty() {
                    d.error = "Enter a name and select at least one capture.".into();
                    self.history.manage = Some(d);
                    return;
                }
                self.suspend_global_capture();
                let format = (!d.editable).then_some(d.format);
                let result = if d.entries.len() == 1 {
                    let video = d.entries[0].kind == capture_core::history::MediaKind::Video;
                    let extension = if d.editable {
                        if video { "vclip" } else { "iclip" }
                    } else {
                        d.format.extension()
                    };
                    rfd::FileDialog::new()
                        .set_directory(
                            self.dialog_folder(capture_core::quick_preferences::Folder::Export),
                        )
                        .add_filter("Selected format", &[extension])
                        .set_file_name(format!("{}.{}", safe_name(&d.name), extension))
                        .save_file()
                        .map(|path| {
                            let path = if d.editable {
                                if video {
                                    let mut p = path;
                                    p.set_extension("vclip");
                                    p
                                } else {
                                    capture_core::project_file::image_path(&path)
                                }
                            } else {
                                path
                            };
                            self.save_library_item(&d.entries[0], &path, format)?;
                            if let Some(parent) = path.parent() {
                                self.last_directory = parent.to_path_buf();
                                self.quick_controls.remember_folder(
                                    capture_core::quick_preferences::Folder::Export,
                                    parent.to_path_buf(),
                                );
                            }
                            let warning = self
                                .export_preferences
                                .after
                                .open(&path, format.is_some())
                                .err();
                            Ok(format!(
                                "Saved {}{}",
                                path.display(),
                                warning
                                    .map(|e| format!("; could not open: {e}"))
                                    .unwrap_or_default()
                            ))
                        })
                } else {
                    rfd::FileDialog::new()
                        .set_directory(
                            self.dialog_folder(capture_core::quick_preferences::Folder::Export),
                        )
                        .pick_folder()
                        .map(|folder| {
                            self.last_directory = folder.clone();
                            self.quick_controls.remember_folder(
                                capture_core::quick_preferences::Folder::Export,
                                folder.clone(),
                            );
                            self.export_library_batch(&d.entries, &folder, format, &d.name)
                        })
                };
                if let Some(result) = result {
                    match result {
                        Ok(message) => {
                            self.status = message;
                            return;
                        }
                        Err(e) => d.error = e,
                    }
                }
            } else {
                match self.apply_library_management(&d) {
                    Ok(message) => {
                        self.status = message;
                        return;
                    }
                    Err(e) => d.error = e,
                }
            }
        }
        self.history.manage = Some(d);
    }
    fn save_library_item(
        &self,
        entry: &Entry,
        path: &std::path::Path,
        format: Option<capture_core::export::Format>,
    ) -> Result<(), String> {
        let store = self
            .history
            .store
            .as_ref()
            .ok_or("Library is unavailable")?;
        let doc = if self.history.active.as_deref() == Some(&entry.id) {
            self.session.document.clone()
        } else {
            store.load(&entry.id).map_err(|e| e.to_string())?
        };
        if let Some(format) = format {
            let doc = if doc.video.is_some() {
                crate::video_editor::still(&doc)?
            } else {
                doc
            };
            capture_core::export::save(
                &doc,
                path,
                &capture_core::export::Options {
                    format,
                    ..self.export_preferences.options.clone()
                },
            )
        } else {
            doc.save(path)
        }
        .map_err(|e| e.to_string())
    }

    fn export_library_batch(
        &self,
        entries: &[Entry],
        folder: &std::path::Path,
        format: Option<capture_core::export::Format>,
        base: &str,
    ) -> Result<String, String> {
        let names = capture_core::history::unique_names(&safe_name(base), entries.len(), &[])
            .map_err(|e| e.to_string())?;
        let directory = (1..=10000)
            .map(|n| folder.join(format!("ClipCraft export {n}")))
            .find_map(|p| match std::fs::create_dir(&p) {
                Ok(()) => Some(Ok(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e.to_string())),
            })
            .ok_or("No export folder available")??;
        for (i, (e, name)) in entries.iter().zip(names).enumerate() {
            let extension = format.map_or(
                if e.kind == capture_core::history::MediaKind::Video {
                    "vclip"
                } else {
                    "iclip"
                },
                |f| f.extension(),
            );
            let path = directory.join(format!("{name}.{extension}"));
            let result = self.save_library_item(e, &path, format);
            if let Err(error) = result {
                return Err(format!(
                    "Export stopped after {i} items in {}: {error}",
                    directory.display()
                ));
            }
        }
        let after = self.export_preferences.after;
        let opened = (|| -> Result<(), String> {
            if after.folder {
                crate::export_ui::AfterExport {
                    folder: true,
                    image: false,
                }
                .open(&directory.join("capture"), false)?;
            }
            if after.image && format.is_some() {
                for path in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
                    crate::export_ui::AfterExport {
                        folder: false,
                        image: true,
                    }
                    .open(&path.map_err(|e| e.to_string())?.path(), true)?;
                }
            }
            Ok(())
        })();
        let warning = opened
            .err()
            .map(|e| format!("; saved, but could not open: {e}"))
            .unwrap_or_default();
        Ok(format!(
            "Exported {} captures to {}{}",
            entries.len(),
            directory.display(),
            warning
        ))
    }
}

fn safe_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bulk_management_preserves_open_canvas_and_exports_unique_folders() {
        use crate::property_layout::tests::{PaintedOutput, dump_meshes};
        let root = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |style| style.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.history = HistoryPanel::isolated(&root.path().join("history"));
        let a = e
            .history
            .store
            .as_mut()
            .unwrap()
            .create(
                &capture_core::Document::canvas(50, 40, [20, 80, 160, 255]).unwrap(),
                "Capture",
                Origin::Capture,
            )
            .unwrap();
        let b = e
            .history
            .store
            .as_mut()
            .unwrap()
            .create(
                &capture_core::Document::blank(50, 40).unwrap(),
                "Other",
                Origin::Canvas,
            )
            .unwrap();
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let mut d = Dialog {
            action: Action::AddTags,
            entries: vec![a, b],
            tags: "approved, beta".into(),
            name: "Capture".into(),
            editable: true,
            format: Default::default(),
            error: String::new(),
            focus_pending: true,
            cleanup: false,
            cleanup_summary: String::new(),
        };
        e.apply_library_management(&d).unwrap();
        assert!(
            e.history
                .entries()
                .iter()
                .all(|e| e.tags == ["approved", "beta"])
        );
        let entries = e.history.entries();
        for _ in 0..2 {
            e.export_library_batch(
                &entries,
                root.path(),
                Some(capture_core::export::Format::Png),
                "Capture",
            )
            .unwrap();
        }
        assert_eq!(
            std::fs::read_dir(root.path().join("ClipCraft export 1"))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("ClipCraft export 2"))
                .unwrap()
                .count(),
            2
        );
        e.export_library_batch(&entries[..1], root.path(), None, "Guide")
            .unwrap();
        let exported =
            capture_core::Document::load(&root.path().join("ClipCraft export 3/Guide 1.iclip"))
                .unwrap();
        assert_eq!(
            exported.render().unwrap(),
            e.history
                .store
                .as_ref()
                .unwrap()
                .load(&entries[0].id)
                .unwrap()
                .render()
                .unwrap()
        );
        assert_eq!(
            std::fs::read_dir(root.path().join("ClipCraft export 3"))
                .unwrap()
                .count(),
            1
        );

        let frame = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 500.),
                    )),
                    ..Default::default()
                },
                |ui| e.library_storage_settings(ui),
            ))
        };
        let first = frame(&mut e);
        let output = frame(&mut e);
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_BETA_DIR") {
            dump_meshes(&ctx, &dir, "Storage", first, output);
        }
        d.action = Action::Delete;
        d.entries = entries;
        e.apply_library_management(&d).unwrap();
        assert!(e.history.entries().is_empty());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    }
}
