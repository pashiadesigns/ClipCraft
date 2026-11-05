//! Canvas clipboard commands with an injectable transport for background tests.
use crate::{
    Command, Editor, Kind,
    clipboard::objects::{self, SystemTransfer, Transfer},
    egui,
};
use capture_core::clipboard_payload::Content;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Copy,
    Cut,
    Paste,
}

fn shortcut(input: &mut egui::InputState) -> Option<Action> {
    let mut action = None;
    for (key, value) in [
        (egui::Key::C, Action::Copy),
        (egui::Key::X, Action::Cut),
        (egui::Key::V, Action::Paste),
    ] {
        if crate::shortcuts::consume_exact(input, egui::Modifiers::COMMAND, key) {
            action = Some(value);
        }
    }
    input.events.retain(|event| {
        let value = match event {
            egui::Event::Copy => Some(Action::Copy),
            egui::Event::Cut => Some(Action::Cut),
            egui::Event::Paste(_) | egui::Event::PasteImage(_) => Some(Action::Paste),
            _ => None,
        };
        if let Some(value) = value {
            action = Some(value);
            false
        } else {
            true
        }
    });
    action
}

impl Editor {
    pub(super) fn paste_canvas(&mut self) {
        self.paste_canvas_with(&mut SystemTransfer {
            owner: self.hotkey_owner,
        });
    }
    pub(super) fn copy_canvas_selection(&mut self, cut: bool) {
        if self.selected_objects().is_empty() {
            if !cut {
                self.copy_all();
            }
        } else {
            self.copy_objects_with(
                cut,
                &mut SystemTransfer {
                    owner: self.hotkey_owner,
                },
            );
        }
    }
    pub(super) fn clipboard_shortcuts(&mut self, ctx: &egui::Context) {
        if self.clipboard_owns_input(ctx) {
            match ctx.input_mut(shortcut) {
                Some(Action::Paste) => self.paste_canvas(),
                Some(Action::Copy) => self.copy_canvas_selection(false),
                Some(Action::Cut) => self.copy_canvas_selection(true),
                None => {}
            }
        }
    }
    fn clipboard_owns_input(&self, ctx: &egui::Context) -> bool {
        self.editor_shortcuts_idle()
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && self.text_draft.is_none()
            && self.shell.canvas_enabled()
            && !self.shell.annotations_hidden
            && !self.shell.help
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx)
    }
    fn copy_objects_with(&mut self, cut: bool, transfer: &mut impl Transfer) {
        if !self.commit_text() {
            return;
        }
        let selected = self.selected_objects();
        let ids: Vec<_> = selected.iter().map(|a| a.id).collect();
        let result = objects::selection(&self.session.document, &ids)
            .and_then(|(payload, image)| transfer.write(&payload, &image));
        match result {
            Ok(()) => {
                self.quick_controls.copied_format =
                    if selected.len() == 1 && selected[0].kind != Kind::Image {
                        Some(selected[0].clone())
                    } else {
                        None
                    };
                if cut {
                    self.execute(Command::RemoveMany { ids });
                }
                self.status = format!(
                    "{} {} editable object(s). Paste inserts them into the current canvas.",
                    if cut { "Cut" } else { "Copied" },
                    selected.len()
                );
            }
            Err(error) => self.status = error,
        }
    }
    fn paste_canvas_with(&mut self, transfer: &mut impl Transfer) {
        let result = transfer.read().and_then(|payload| {
            payload.validate().map_err(|e| e.to_string())?;
            match payload.content {
                Content::Objects(objects) => Ok(objects),
                Content::Image(image) => image.into_objects().map_err(|e| e.to_string()),
            }
        });
        let payload = match result {
            Ok(p) => p,
            Err(e) => {
                self.status = e;
                return;
            }
        };
        if !self.commit_text() {
            return;
        }
        let center = self
            .view
            .current_transform()
            .map(|t| t.image_point(t.viewport.center()))
            .unwrap_or([
                self.session.document.width as f32 * 0.5,
                self.session.document.height as f32 * 0.5,
            ]);
        let previous: std::collections::BTreeSet<_> = self
            .session
            .document
            .annotations
            .iter()
            .map(|a| a.id)
            .collect();
        let count = payload.objects.len();
        self.execute(Command::PasteObjects { payload, center });
        let ids: Vec<_> = self
            .session
            .document
            .annotations
            .iter()
            .filter(|a| !previous.contains(&a.id))
            .map(|a| a.id)
            .collect();
        if ids.len() != count {
            return;
        } // Execute has retained the actionable failure.
        self.select_only(ids.first().copied());
        self.extra_selected.extend(ids.iter().skip(1));
        self.selection_order = ids;
        self.tool = None;
        self.shell.properties = true;

        self.shell.annotations_hidden = false;
        self.status = format!(
            "Pasted {count} editable object(s). Drag or resize them; Undo removes this paste."
        );
    }
}

#[cfg(test)]
mod tests;
