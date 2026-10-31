use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    AddTags,
    RemoveTags,
    Delete,
    Export,
    Storage,
}
pub(super) struct Dialog {
    action: Action,
    entries: Vec<Entry>,
    tags: String,
    limits: capture_core::history::Limits,
    format: capture_core::export::Format,
    error: String,
}
impl Editor {
    pub(super) fn library_bulk_bar(&mut self, ui: &mut egui::Ui, visible: &[Entry]) {
        self.history.library.selected.retain(|id| {
            self.history
                .store
                .as_ref()
                .is_some_and(|s| s.entries().iter().any(|e| e.id == *id))
        });
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            ui.toggle_value(&mut self.history.library.selecting, "Select items");
            if self.history.library.selecting {
                if ui.button("Select visible").clicked() {
                    self.history
                        .library
                        .selected
                        .extend(visible.iter().map(|e| e.id.clone()));
                }
                if ui.button("Clear").clicked() {
                    self.history.library.selected.clear();
                }
                ui.label(format!("{} selected", self.history.library.selected.len()));
                ui.add_enabled_ui(!self.history.library.selected.is_empty(), |ui| {
                    for (label, a) in [
                        ("Add tags", Action::AddTags),
                        ("Remove tags", Action::RemoveTags),
                        ("Export…", Action::Export),
                        ("Delete…", Action::Delete),
                    ] {
                        if ui.button(label).clicked() {
                            action = Some(a);
                        }
                    }
                });
            }
            if ui.button("Storage…").clicked() {
                action = Some(Action::Storage);
            }
        });
        if let Some(action) = action {
            match self
                .history
                .store
                .as_ref()
                .ok_or("Library is unavailable".into())
                .and_then(|s| s.limits().map_err(|e| e.to_string()))
            {
                Ok(limits) => {
                    let entries = self
                        .history
                        .entries()
                        .into_iter()
                        .filter(|e| self.history.library.selected.contains(&e.id))
                        .collect();
                    self.history.manage = Some(Dialog {
                        action,
                        entries,
                        tags: String::new(),
                        limits,
                        format: Default::default(),
                        error: String::new(),
                    });
                }
                Err(e) => self.history.error = e,
            }
        }
    }
    fn apply_library_management(&mut self, d: &Dialog) -> Result<String, String> {
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
        match d.action {
            Action::Storage => store.set_limits(d.limits),
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
        if d.action == Action::Delete {
            self.history.library.selected.clear();
        }
        Ok(if d.action == Action::Storage {
            "Library capacity saved.".into()
        } else {
            format!("Updated {} Library item(s).", d.entries.len())
        })
    }
    pub(super) fn show_library_management(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.history.manage.take() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        let modal=egui::Modal::new(egui::Id::unique("library-management")).show(ctx,|ui|{
            ui.set_width(420.);let title=match d.action{Action::AddTags=>"Add tags",Action::RemoveTags=>"Remove tags",Action::Delete=>"Delete selected captures",Action::Export=>"Export selected captures",Action::Storage=>"Library storage"};ui.heading(title);ui.add_space(8.);
            match d.action {
                Action::Storage=>{
                    let entries=self.history.entries();let used=entries.iter().map(|e|e.bytes).sum::<u64>();
                    ui.label(format!("{} items · {:.1} MB used",entries.len(),used as f64/1_000_000.));
                    crate::property_layout::section_help(ui,"Capacity","Limits apply to future saves. Reducing a limit never deletes work. Export or explicitly delete selected items to free space. The folder below contains editable Library projects; stamp storage is separate.");
                    crate::property_layout::slider(ui,"Items",egui::Slider::new(&mut d.limits.entries,1..=1000));let mut mb=d.limits.bytes/1_000_000;
                    if crate::property_layout::slider(ui,"Megabytes",egui::Slider::new(&mut mb,1..=16000)).changed(){d.limits.bytes=mb*1_000_000;}
                    if let Some(root)=self.history.root(){ui.label("Folder");ui.add(egui::Label::new(root.display().to_string()).wrap());}
                },
                Action::AddTags|Action::RemoveTags=>{ui.label(format!("{} selected captures",d.entries.len()));ui.label("Tags (comma separated)");ui.text_edit_singleline(&mut d.tags);},
                Action::Delete=>{ui.label(format!("Delete {} selected capture(s) from Library?",d.entries.len()));ui.label("Stored projects are removed permanently. Exported files and the open canvas are kept.");},
                Action::Export=>{ui.label(format!("{} selected captures",d.entries.len()));ui.horizontal(|ui|{ui.label("Format");for f in [capture_core::export::Format::Png,capture_core::export::Format::Jpeg,capture_core::export::Format::Pdf]{ui.selectable_value(&mut d.format,f,f.label());}});ui.label("A new subfolder is created in your chosen folder. PDF fits each capture to one page.");},
            }
            if !d.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&d.error);}
            ui.horizontal(|ui|{cancel=ui.button("Cancel").clicked();let text=match d.action{Action::Delete=>"Delete",Action::Export=>"Choose folder…",_=>"Save"};let mut button=egui::Button::new(text);if d.action==Action::Delete{button=button.fill(egui::Color32::from_rgb(166,42,48));}submit=ui.add(button).clicked();});
        });
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if submit {
            if d.action == Action::Export {
                self.suspend_global_capture();
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    let result = self.export_library_batch(&d.entries, &folder, d.format);
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
    fn export_library_batch(
        &self,
        entries: &[Entry],
        folder: &std::path::Path,
        format: capture_core::export::Format,
    ) -> Result<String, String> {
        let store = self
            .history
            .store
            .as_ref()
            .ok_or("Library is unavailable")?;
        let directory = (1..=10000)
            .map(|n| folder.join(format!("ClipCraft export {n}")))
            .find_map(|p| match std::fs::create_dir(&p) {
                Ok(()) => Some(Ok(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e.to_string())),
            })
            .ok_or("No export folder available")??;
        let options = capture_core::export::Options {
            format,
            ..Default::default()
        };
        for (i, e) in entries.iter().enumerate() {
            let name = e
                .label
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                        c
                    } else {
                        '_'
                    }
                })
                .take(70)
                .collect::<String>();
            let path = directory.join(format!("{:03} {}.{}", i + 1, name, format.extension()));
            let result = store
                .load(&e.id)
                .and_then(|doc| capture_core::export::save(&doc, &path, &options));
            if let Err(error) = result {
                return Err(format!(
                    "Export stopped after {i} items in {}: {error}",
                    directory.display()
                ));
            }
        }
        Ok(format!(
            "Exported {} captures to {}",
            entries.len(),
            directory.display()
        ))
    }
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
            limits: Default::default(),
            format: Default::default(),
            error: String::new(),
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
            e.export_library_batch(&entries, root.path(), capture_core::export::Format::Png)
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
        d.action = Action::Storage;
        e.history.manage = Some(d);
        let frame = |e: &mut Editor| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 500.),
                    )),
                    ..Default::default()
                },
                |ui| e.show_library_management(ui.ctx()),
            ))
        };
        let first = frame(&mut e);
        let output = frame(&mut e);
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_BETA_DIR") {
            dump_meshes(&ctx, &dir, "Storage", first, output);
        }
        let mut d = e.history.manage.take().unwrap();
        d.action = Action::Delete;
        d.entries = entries;
        e.apply_library_management(&d).unwrap();
        assert!(e.history.entries().is_empty());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
    }
}
