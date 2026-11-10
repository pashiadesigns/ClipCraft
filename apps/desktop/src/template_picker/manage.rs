use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Rename,
    Duplicate,
    Delete,
}
#[derive(Clone)]
struct Edit {
    entry: capture_core::history::Entry,
    action: Action,
    name: String,
    focus: bool,
    error: String,
}
fn key() -> egui::Id {
    egui::Id::unique("manage-local-template")
}
impl Editor {
    pub(super) fn template_context(&mut self, response: &egui::Response, choice: &Choice) {
        let Some(entry) = choice.entry.as_ref() else {
            return;
        };
        response.context_menu(|ui| {
            for (label, action) in [
                ("Rename template…", Action::Rename),
                ("Duplicate template", Action::Duplicate),
                ("Delete template…", Action::Delete),
            ] {
                if crate::controls::menu_action(ui, label).clicked() {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            key(),
                            Edit {
                                entry: entry.clone(),
                                action,
                                name: entry.label.clone(),
                                focus: true,
                                error: String::new(),
                            },
                        )
                    });
                    ui.close();
                }
            }
        });
    }
    fn apply_template_edit(&mut self, d: &Edit) -> Result<(), String> {
        let root = self
            .template_root()
            .ok_or("Local templates are unavailable.")?;
        let mut store = templates::library::Library::open(&root).map_err(|e| e.to_string())?;
        match d.action {
            Action::Rename => store
                .0
                .edit_metadata(&d.entry, &d.name, &d.entry.tags)
                .map(|_| ()),
            Action::Duplicate => store
                .0
                .duplicate_many(std::slice::from_ref(&d.entry))
                .map(|_| ()),
            Action::Delete => store.0.discard(&d.entry),
        }
        .map_err(|e| e.to_string())
    }
    pub(crate) fn show_template_management(&mut self, ctx: &egui::Context) {
        let Some(mut d) = ctx.data_mut(|data| {
            let value = data.get_temp::<Edit>(key());
            data.remove::<Edit>(key());
            value
        }) else {
            return;
        };
        let mut submit = d.action == Action::Duplicate;
        let mut cancel = false;
        if !submit {
            let modal = crate::controls::Modal::new(key()).show(ctx, |ui| {
                ui.set_width(380.);
                ui.heading(if d.action == Action::Rename {
                    "Rename template"
                } else {
                    "Delete template"
                });
                if d.action == Action::Rename {
                    ui.label("Name");
                    let r = ui.add(
                        crate::unicode_input::TextEdit::singleline(&mut d.name)
                            .desired_width(f32::INFINITY)
                            .char_limit(120),
                    );
                    crate::controls::focus_once(&r, &mut d.focus);
                } else {
                    ui.label(format!("Delete “{}” from My templates?", d.entry.label));
                    ui.label("Projects created from it are kept.");
                }
                if !d.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
                }
                (submit, cancel) = crate::controls::dialog_actions(
                    ui,
                    if d.action == Action::Rename {
                        "Save"
                    } else {
                        "Delete"
                    },
                    "Cancel",
                    !d.name.trim().is_empty(),
                    d.action == Action::Delete,
                );
            });
            if crate::text_edit::cancel_on_escape(&modal) {
                return;
            }
        }
        if cancel {
            return;
        }
        if submit {
            match self.apply_template_edit(&d) {
                Ok(()) => {
                    ctx.data_mut(|data| {
                        data.remove::<Catalog>(egui::Id::unique("template-catalog"))
                    });
                    self.status = "Local templates updated.".into();
                    return;
                }
                Err(e) => {
                    d.error = e;
                    if d.action == Action::Duplicate {
                        self.status = d.error;
                        return;
                    }
                }
            }
        }
        ctx.data_mut(|data| data.insert_temp(key(), d));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn template_rename_dialog_focuses_name_and_applies_actual_save_button() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let root = tempfile::tempdir().unwrap();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.history = crate::history::HistoryPanel::isolated(root.path());
        let doc = templates::build("two-steps", "Guide", &[], Scale::Fit).unwrap();
        let mut store = templates::library::Library::open(&root.path().join("templates")).unwrap();
        let entry = store.save(&doc, "Original", true).unwrap();
        ctx.data_mut(|data| {
            data.insert_temp(
                key(),
                Edit {
                    entry,
                    action: Action::Rename,
                    name: String::new(),
                    focus: true,
                    error: String::new(),
                },
            )
        });
        let frame = |e: &mut Editor, events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 580.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.show_template_management(ui.ctx()),
            ))
        };
        let mut first = frame(&mut e, vec![]);
        let out = frame(&mut e, vec![egui::Event::Text("Business guide".into())]);
        assert_eq!(
            ctx.data(|d| d.get_temp::<Edit>(key())).unwrap().name,
            "Business guide"
        );
        first.0.textures_delta.append(out.0.textures_delta.clone());
        let out = frame(&mut e, vec![]);
        let save = crate::color_widgets::tests::text_rect(&out.shapes, "Save")
            .unwrap()
            .center();
        if let Ok(dir) = std::env::var("CLIPCRAFT_HEADLESS_RECOVERY_DIR") {
            dump_meshes(&ctx, &dir, "template-rename", first, out);
        }
        for pressed in [true, false] {
            frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(save),
                    egui::Event::PointerButton {
                        pos: save,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        store = templates::library::Library::open(&root.path().join("templates")).unwrap();
        assert_eq!(store.0.entries()[0].label, "Business guide");
        assert!(ctx.data(|d| d.get_temp::<Edit>(key())).is_none());
    }
    #[test]
    fn template_management_preserves_source_and_created_projects() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = crate::canvas::tests::editor(&egui::Context::default());
        e.history = crate::history::HistoryPanel::isolated(dir.path());
        let doc = templates::build("two-steps", "Original", &[], Scale::Fit).unwrap();
        let source = serde_json::to_vec(&doc).unwrap();
        let mut store = templates::library::Library::open(&dir.path().join("templates")).unwrap();
        let entry = store.save(&doc, "Mine", true).unwrap();
        let mut d = Edit {
            entry,
            action: Action::Rename,
            name: "Renamed".into(),
            focus: true,
            error: String::new(),
        };
        e.apply_template_edit(&d).unwrap();
        store = templates::library::Library::open(&dir.path().join("templates")).unwrap();
        assert_eq!(store.0.entries()[0].label, "Renamed");
        d.entry = store.0.entries()[0].clone();
        d.action = Action::Duplicate;
        e.apply_template_edit(&d).unwrap();
        d.action = Action::Delete;
        e.apply_template_edit(&d).unwrap();
        store = templates::library::Library::open(&dir.path().join("templates")).unwrap();
        assert_eq!(store.0.entries().len(), 1);
        assert_eq!(serde_json::to_vec(&doc).unwrap(), source);
    }
}
