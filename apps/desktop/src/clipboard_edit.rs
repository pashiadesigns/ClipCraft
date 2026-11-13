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
            Err(error) => self.fail(error),
        }
    }
    /// Insert > Image from file (EDIT-084): PNG, JPEG, WebP, BMP, GIF, SVG
    /// or EPS as a new image object. SVG stays a sharp, resizable drawing.
    pub(super) fn insert_image_file(&mut self, path: &std::path::Path) {
        self.insert_image_file_at(path, None);
    }
    /// Places the image centered at `at` in canvas pixels, or in the view.
    pub(super) fn insert_image_file_at(&mut self, path: &std::path::Path, at: Option<[f32; 2]>) {
        let svg = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
        let eps = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("eps"));
        let mut note = String::new();
        // A vector icon is drawn in a square box; size it by the SVG's own size.
        let mut shape: Option<[u32; 2]> = None;
        let bitmap = (|| -> Result<std::sync::Arc<capture_core::bitmap::Bitmap>, String> {
            if svg {
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                // BI11: an SVG becomes an editable vector icon (its colors are
                // separate swatches) when it fits; otherwise it stays a drawing.
                match capture_core::vector_import::svg_vector_icon(&text)
                    .and_then(capture_core::bitmap::Bitmap::from_vector)
                {
                    Ok(icon) => {
                        shape = capture_core::vector_import::svg_size(&text).ok();
                        return Ok(std::sync::Arc::new(icon));
                    }
                    Err(e) => {
                        note = format!(
                            " It stays a drawing ({}), so Recolor changes it as a whole.",
                            e.to_string().trim_end_matches('.')
                        );
                    }
                }
                return capture_core::bitmap::Bitmap::from_svg(&text)
                    .map(std::sync::Arc::new)
                    .map_err(|e| e.to_string());
            }
            let pixels = if eps {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                let (pixels, source) =
                    capture_core::vector_import::eps_image(&bytes).map_err(|e| e.to_string())?;
                if source == capture_core::vector_import::EpsSource::Preview {
                    note = " Ghostscript isn't installed, so this is the preview stored in the EPS; it may look soft when enlarged.".into();
                }
                pixels
            } else {
                capture_core::Document::open_image(path)
                    .and_then(|d| d.render())
                    .map_err(|e| e.to_string())?
            };
            let payload = capture_core::clipboard_payload::ImagePayload::from_rgba(
                pixels.width(),
                pixels.height(),
                pixels.into_raw(),
            )
            .map_err(|e| e.to_string())?;
            capture_core::bitmap::Bitmap::new(payload.image)
                .map(std::sync::Arc::new)
                .map_err(|e| e.to_string())
        })();
        let bitmap = match bitmap {
            Ok(b) => b,
            Err(e) => {
                self.fail(e);
                return;
            }
        };
        if !self.commit_text() {
            return;
        }
        let mut a = bitmap.annotation(1);
        // Fit within most of the canvas, keeping the drawing's shape.
        let canvas = [
            self.session.document.width as f32,
            self.session.document.height as f32,
        ];
        let size = match shape {
            Some([w, h]) => [w.max(h) as f32; 2],
            None => [a.end[0] - a.start[0], a.end[1] - a.start[1]],
        };
        let fit = (canvas[0] * 0.8 / size[0])
            .min(canvas[1] * 0.8 / size[1])
            .min(1.);
        a.end = [size[0] * fit, size[1] * fit];
        let payload = capture_core::clipboard_payload::ObjectPayload {
            document_version: capture_core::MAX_PROJECT_VERSION,
            source_canvas: [self.session.document.width, self.session.document.height],
            objects: vec![a],
        };
        let center = at.unwrap_or_else(|| {
            self.view
                .current_transform()
                .map(|t| t.image_point(t.viewport.center()))
                .unwrap_or([canvas[0] * 0.5, canvas[1] * 0.5])
        });
        let before = self.session.document.annotations.len();
        self.execute(Command::PasteObjects { payload, center });
        if self.session.document.annotations.len() > before {
            let id = self.session.document.annotations.last().map(|a| a.id);
            self.select_only(id);
            self.status = format!(
                "Inserted {}.{}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                note
            );
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
                self.fail(e);
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
