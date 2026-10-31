//! One menu and action path for Library cards and the Recent strip.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Open,
    Copy,
    Duplicate,
    Rename,
    Tags,
    Delete,
}
impl Action {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open in Editor",
            Self::Copy => "Copy to Clipboard",
            Self::Duplicate => "Duplicate",
            Self::Rename => "Rename",
            Self::Tags => "Edit Tags",
            Self::Delete => "Delete from Library",
        }
    }
}
pub(super) struct Dialog {
    entry: Entry,
    action: Action,
    name: String,
    tags: String,
    error: String,
}
fn menu(ui: &mut egui::Ui, available: bool, busy: bool) -> Option<Action> {
    ui.set_min_width(210.);
    let mut chosen = None;
    for action in [
        Action::Open,
        Action::Copy,
        Action::Duplicate,
        Action::Rename,
        Action::Tags,
        Action::Delete,
    ] {
        if action == Action::Delete {
            ui.separator();
        }
        let enabled = match action {
            Action::Open | Action::Copy => available,
            Action::Delete | Action::Rename | Action::Tags => !busy,
            Action::Duplicate => available && !busy,
        };
        let response = ui.add_enabled(
            enabled,
            egui::Button::new("").min_size(egui::vec2(210., 26.)),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, action.label()));
        ui.painter().text(
            egui::pos2(response.rect.left() + 32., response.rect.center().y),
            egui::Align2::LEFT_CENTER,
            action.label(),
            egui::FontId::proportional(13.),
            if enabled {
                egui::Color32::from_gray(232)
            } else {
                egui::Color32::GRAY
            },
        );
        let center = egui::pos2(response.rect.left() + 14., response.rect.center().y);
        let color = if !enabled {
            egui::Color32::GRAY
        } else if action == Action::Delete {
            egui::Color32::LIGHT_RED
        } else {
            egui::Color32::from_rgb(130, 202, 237)
        };
        let name = match action {
            Action::Open => "Editor",
            Action::Copy | Action::Duplicate => "Copy",
            Action::Rename => "Text",
            Action::Tags => "Tag",
            Action::Delete => "Delete",
        };
        match action {
            Action::Rename => {
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "A",
                    egui::FontId::proportional(17.),
                    color,
                );
            }
            Action::Delete => {
                let stroke = egui::Stroke::new(1.4, color);
                ui.painter().rect_stroke(
                    egui::Rect::from_center_size(center + egui::vec2(0., 2.), egui::vec2(10., 12.)),
                    1.,
                    stroke,
                    egui::StrokeKind::Inside,
                );
                ui.painter().line_segment(
                    [center + egui::vec2(-7., -5.), center + egui::vec2(7., -5.)],
                    stroke,
                );
                ui.painter().line_segment(
                    [center + egui::vec2(-2., -7.), center + egui::vec2(2., -7.)],
                    stroke,
                );
            }
            Action::Tags => {
                let r = egui::Rect::from_center_size(center, egui::vec2(14., 10.));
                ui.painter().rect_stroke(
                    r,
                    2.,
                    egui::Stroke::new(1.4, color),
                    egui::StrokeKind::Inside,
                );
                ui.painter()
                    .circle_filled(center - egui::vec2(4., 0.), 1.4, color);
            }
            _ => crate::ribbon::draw_icon(ui.painter(), center, name, color),
        }
        if response.clicked() {
            chosen = Some(action);
            ui.close();
        }
        if busy && !enabled {
            response.on_hover_text("Finishing the current Library save…");
        }
    }
    chosen
}
impl Editor {
    pub(crate) fn history_dialog_open(&self) -> bool {
        self.history.dialog.is_some() || self.history.manage.is_some()
    }
    pub(crate) fn history_context(
        &mut self,
        response: &egui::Response,
        entry: &Entry,
        available: bool,
    ) {
        let busy = self.history.active.as_deref() == Some(&entry.id) && self.library_entry_busy();
        let mut action = None;
        response.context_menu(|ui| {
            action = menu(ui, available, busy);
        });
        if let Some(action) = action {
            self.history_action(entry, action, &response.ctx);
        }
    }
    fn history_action(&mut self, entry: &Entry, action: Action, ctx: &egui::Context) {
        if action == Action::Open {
            self.open_history(&entry.id, ctx);
            return;
        }
        if action == Action::Copy {
            self.copy_history_with(entry, crate::clipboard::copy);
            return;
        }
        if self.history.active.as_deref() == Some(&entry.id) && self.library_entry_busy() {
            self.status = "Finishing the current Library save; try again in a moment.".into();
            return;
        }
        if action == Action::Duplicate {
            let result = self
                .history
                .store
                .as_mut()
                .ok_or_else(|| "Library is unavailable".to_owned())
                .and_then(|store| store.duplicate(entry).map_err(|e| e.to_string()));
            self.status = match result {
                Ok(copy) => format!("Duplicated {} as {}.", entry.label, copy.label),
                Err(error) => error,
            };
            self.history.refresh();
        } else {
            self.history.dialog = Some(Dialog {
                entry: entry.clone(),
                action,
                name: entry.label.clone(),
                tags: entry.tags.join(", "),
                error: String::new(),
            });
        }
    }
    fn copy_history_with(
        &mut self,
        entry: &Entry,
        copy: impl FnOnce(&capture_core::Document) -> Result<(), String>,
    ) {
        let result = if self.history.active.as_deref() == Some(&entry.id) {
            if !self.commit_text() {
                return;
            }
            copy(&self.session.document)
        } else {
            self.history
                .store
                .as_ref()
                .ok_or_else(|| "Library is unavailable".to_owned())
                .and_then(|store| store.load(&entry.id).map_err(|e| e.to_string()))
                .and_then(|document| copy(&document))
        };
        self.status = match result {
            Ok(()) => format!("Copied {} to the clipboard.", entry.label),
            Err(error) => error,
        };
    }
    fn submit_history_dialog(&mut self, dialog: &Dialog) -> Result<(), String> {
        let active = self.history.active.as_deref() == Some(&dialog.entry.id);
        if active && self.library_entry_busy() {
            return Err("Finishing the current Library save; try again in a moment.".into());
        }
        let store = self
            .history
            .store
            .as_mut()
            .ok_or("Library is unavailable")?;
        if dialog.action == Action::Delete {
            store.discard(&dialog.entry).map_err(|e| e.to_string())?;
            if active {
                self.detach_deleted_history();
            }
            self.status = format!("Deleted {} from Library.", dialog.entry.label);
        } else {
            let tags = dialog
                .tags
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let updated = store
                .edit_metadata(&dialog.entry, &dialog.name, &tags)
                .map_err(|e| e.to_string())?;
            if active {
                self.update_history_metadata(updated);
            }
            self.status = format!("Updated {}.", dialog.name);
        }
        self.history.refresh();
        Ok(())
    }
    pub(crate) fn show_history_dialog(&mut self, ctx: &egui::Context) {
        self.show_library_management(ctx);
        let Some(mut dialog) = self.history.dialog.take() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("library_entry_dialog")).show(ctx, |ui| {
            ui.set_width(380.);
            ui.heading(dialog.action.label());
            ui.add_space(8.);
            if dialog.action == Action::Delete {
                ui.label(format!("Delete “{}” from Library?", dialog.entry.label));
                ui.label(
                    "This removes the stored capture. Exported files and the open canvas are kept.",
                );
            } else if dialog.action == Action::Rename {
                ui.label("Name");
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.name)
                        .desired_width(f32::INFINITY)
                        .char_limit(120),
                );
            } else {
                ui.label("Tags (separate with commas)");
                ui.add(
                    egui::TextEdit::singleline(&mut dialog.tags)
                        .desired_width(f32::INFINITY)
                        .char_limit(1200),
                );
            }
            if !dialog.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &dialog.error);
            }
            ui.add_space(10.);
            ui.horizontal(|ui| {
                cancel = ui.button("Cancel").clicked();
                let button = if dialog.action == Action::Delete {
                    egui::Button::new("Delete").fill(egui::Color32::from_rgb(177, 43, 49))
                } else {
                    egui::Button::new("Save").fill(crate::shell::ACCENT)
                };
                submit = ui.add(button).clicked();
            });
        });
        cancel |= ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if submit {
            match self.submit_history_dialog(&dialog) {
                Ok(()) => return,
                Err(error) => dialog.error = error,
            }
        }
        if !cancel {
            self.history.dialog = Some(dialog);
        }
    }
}

#[cfg(test)]
mod tests;
