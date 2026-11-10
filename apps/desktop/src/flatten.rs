use crate::{Command, Editor, egui, shortcuts};
impl Editor {
    pub fn flatten_all(&mut self) {
        let count = self.session.document.annotations.len();
        if count == 0 {
            return;
        }
        if let Some(guide) = &self.session.document.guide {
            let point = self
                .selected_annotation()
                .map(|a| a.start)
                .or_else(|| {
                    self.view
                        .current_transform()
                        .map(|t| t.image_point(t.viewport.center()))
                })
                .unwrap_or([0.; 2]);
            let page = guide.page_at(point);
            self.execute(Command::FlattenPage { page });
            self.status = format!(
                "Flattened page {}. Other pages stay editable; Undo restores this operation.",
                page + 1
            );
            return;
        }
        self.execute(Command::FlattenAll);
        if self.session.document.annotations.is_empty() {
            self.status = format!(
                "Flattened {count} objects into the image. Undo restores their editability."
            );
        }
    }
    pub fn flatten_shortcut(&mut self, ctx: &egui::Context) {
        let available = self.pending.is_none()
            && self.template_picker.is_none()
            && self.new_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.crop.is_none()
            && self.text_draft.is_none()
            && self.group_drag.is_none()
            && self.edit_drag.is_none()
            && self.property_preview.is_none()
            && self.drag_start.is_none()
            && !self.capture_requested
            && self.capture_result.is_none()
            && !self.shell.help
            && !self.view.panning
            && self.shell.canvas_enabled()
            && !ctx.egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ctx);
        if available
            && ctx.input_mut(|i| {
                shortcuts::consume_exact(
                    i,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::T,
                )
            })
        {
            self.flatten_all();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_cut_then_flatten_and_undo_rebuilds_complete_page_display() {
        use capture_core::{cut_out::CutAxis, guide_sections::Action, templates};
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = crate::Session::new(
            templates::build("three-steps", "Guide", &[], templates::Scale::Fit).unwrap(),
        )
        .unwrap();
        e.execute(Command::Guide {
            action: Action::AddPage {
                page: 0,
                copy: true,
            },
        });
        e.refresh_canvas_display(&ctx);
        let original = e.session.document.clone();
        e.execute(Command::CutOutPages {
            axis: CutAxis::Horizontal,
            edge: Default::default(),
            start: 180,
            end: 200,
            page: 0,
            all_pages: false,
        });
        e.refresh_canvas_display(&ctx);
        let cut = e.session.document.clone();
        e.flatten_all();
        assert_eq!(e.session.document.guide.as_ref().unwrap().page_count(), 2);
        e.refresh_canvas_display(&ctx);
        e.execute(Command::Undo);
        e.refresh_canvas_display(&ctx);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&cut).unwrap()
        );
        e.execute(Command::Undo);
        e.refresh_canvas_display(&ctx);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert_eq!(
            e.session.document.render().unwrap(),
            original.render().unwrap()
        );
        // Switching projects after Undo must not substitute a stale saved snapshot.
        e.history.active = Some("original".into());
        e.switch_session(
            crate::Session::new(crate::Document::blank(40, 40).unwrap()).unwrap(),
            Some("other"),
            None,
        );
        e.switch_session(
            crate::Session::new(crate::Document::blank(40, 40).unwrap()).unwrap(),
            Some("original"),
            None,
        );
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert!(e.session.can_redo());
    }
}
