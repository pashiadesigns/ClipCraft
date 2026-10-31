use crate::{Command, Editor, Kind, egui};

impl Editor {
    /// A newly pressed Step is visible immediately, but creation and numbering
    /// form one document transaction when the pointer is released.
    pub(super) fn step_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
    ) -> bool {
        if self.tool != Some(Kind::Step) {
            return false;
        }
        if self.step_placement.is_none() && ui.input(|i|i.events.iter().any(|event|matches!(event,
            egui::Event::PointerButton {button:egui::PointerButton::Primary,pressed:true,modifiers,..} if modifiers.alt))) {
            return false;
        }
        let (focused, pressed, released, pointer, additive, space) = ui.input(|i| {
            (
                i.focused,
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.interact_pos(),
                i.modifiers.shift || i.modifiers.command || i.modifiers.alt,
                i.key_down(egui::Key::Space),
            )
        });
        if !focused {
            let handled = self.step_placement.is_some();
            self.cancel_gesture();
            return handled;
        }
        if pressed
            && !additive
            && !space
            && !self.view.panning
            && self.edit_drag.is_none()
            && self.group_drag.is_none()
            && let Some(pointer) = ui
                .input(|i| {
                    i.events
                        .iter()
                        .find_map(|event| {
                            if let egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                ..
                            } = event
                            {
                                Some(*pos)
                            } else {
                                None
                            }
                        })
                        .or_else(|| i.pointer.press_origin())
                })
                .or(pointer)
            && transform
                .viewport
                .intersect(transform.image)
                .contains(pointer)
        {
            let point = transform.image_point(pointer);
            let tolerance = 7.0 / transform.scale;
            let on_handle = self
                .selected_annotation()
                .is_some_and(|a| a.kind == Kind::Step && a.hit_handle(point, tolerance).is_some());
            if !on_handle && self.tool_hit(point, tolerance).is_none() {
                match self.step_marker(point) {
                    Ok(step) => {
                        self.select_only(None);
                        self.step_placement = Some(step);
                        self.needs_render = true;
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
        }
        let Some(step) = &mut self.step_placement else {
            return false;
        };
        if let Some(pointer) = pointer.or_else(|| response.interact_pointer_pos()) {
            let point = transform.image_point(pointer);
            let center = [
                (step.start[0] + step.end[0]) * 0.5,
                (step.start[1] + step.end[1]) * 0.5,
            ];
            if let Ok(mut result) = crate::canvas_expansion::move_with_snapping(
                &self.session.document,
                std::slice::from_ref(step),
                [point[0] - center[0], point[1] - center[1]],
                self.quick_controls
                    .snapping()
                    .then_some(6.0 / transform.scale),
                true,
            ) {
                self.snapping_guides = result.guides;
                let moved = result.annotations.remove(0);
                if moved != *step {
                    *step = moved;
                    self.needs_render = true;
                }
            }
        }
        if released {
            let step = self.step_placement.take().unwrap();
            self.commit_step(step, Some(transform));
        }
        ui.ctx().request_repaint();
        true
    }

    pub(super) fn step_marker(&self, point: [f32; 2]) -> capture_core::Result<crate::Annotation> {
        capture_core::steps::marker(
            self.session.next_id(),
            point,
            self.step_size,
            [self.color[0], self.color[1], self.color[2], 255],
            (self.next_step_number(), self.step_letters),
            [self.session.document.width, self.session.document.height],
        )
        .map(|a| self.styled_new_annotation(a))
    }

    fn commit_step(
        &mut self,
        annotation: crate::Annotation,
        transform: Option<crate::canvas_view::Transform>,
    ) {
        let id = annotation.id;
        if let Some(transform) = transform {
            self.commit_expanding_move(vec![annotation], true, transform);
        } else {
            self.execute(Command::Add { annotation });
        }
        self.needs_render = true;
        if self.session.document.annotations.iter().any(|a| a.id == id) {
            self.select_only(Some(id));
            self.sync_step_sequence();
        }
    }

    #[cfg(test)]
    pub(super) fn add_step(&mut self, point: [f32; 2]) {
        match self.step_marker(point) {
            Ok(annotation) => {
                self.commit_step(annotation, None);
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    pub(super) fn step_properties(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let numeric = ui
                .selectable_value(&mut self.step_letters, false, "1, 2, 3")
                .clicked();
            let letters = ui
                .selectable_value(&mut self.step_letters, true, "A, B, C")
                .clicked();
            if numeric || letters {
                self.step_number =
                    capture_core::steps::next_number(&self.session.document, self.step_letters);
            }
        });
        let has_sequence =
            capture_core::steps::highest_number(&self.session.document, self.step_letters)
                .is_some();
        if has_sequence {
            self.sync_step_sequence();
        }
        ui.horizontal(|ui| {
            if has_sequence {
                ui.label(if self.step_number <= 999 {
                    format!(
                        "Next step: {}",
                        capture_core::steps::label(self.step_number, self.step_letters).unwrap()
                    )
                } else {
                    "Sequence limit reached (999)".into()
                });
            } else {
                let label = ui.label("Start at");
                ui.add(egui::DragValue::new(&mut self.step_number).range(1..=999))
                    .labelled_by(label.id);
            }
        });
        if self.selected.is_none() {
            crate::property_layout::slider(
                ui,
                "Marker size",
                egui::Slider::new(&mut self.step_size, 24..=160),
            );
        }
    }
    pub(super) fn sync_step_sequence(&mut self) {
        if self.tool == Some(Kind::Step) {
            self.step_number =
                capture_core::steps::next_number(&self.session.document, self.step_letters);
        }
    }

    pub(super) fn next_step_number(&self) -> u32 {
        capture_core::steps::highest_number(&self.session.document, self.step_letters)
            .map_or(self.step_number, |number| number.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn placement_reads_highest_label_after_gaps_manual_edits_reopen_and_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(Kind::Step);
        e.step_number = 8;
        e.add_step([60.0, 60.0]);
        assert_eq!(
            e.session.document.annotations[0]
                .text
                .as_ref()
                .unwrap()
                .content,
            "8"
        );
        e.add_step([120.0, 60.0]);
        assert_eq!(
            e.session.document.annotations[1]
                .text
                .as_ref()
                .unwrap()
                .content,
            "9"
        );
        e.execute(Command::Undo);
        e.execute(Command::Undo);
        for (id, n) in [1, 2, 4, 8].into_iter().enumerate() {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id as u64 + 1,
                    [60.0 + id as f32 * 70.0, 60.0],
                    40,
                    [255, 0, 0, 255],
                    (n, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        // Deliberately stale UI counter cannot override labels on the canvas.
        e.step_number = 1;
        e.add_step([400.0, 60.0]);
        assert_eq!(
            e.session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "9"
        );
        e.execute(Command::Undo);
        assert_eq!(e.step_number, 9);
        let mut edited = e.session.document.annotations[0].clone();
        edited.text.as_mut().unwrap().content = "17".into();
        e.execute(Command::Update { annotation: edited });
        e.add_step([400.0, 120.0]);
        assert_eq!(
            e.session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "18"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sequence.json");
        e.session.save(&path).unwrap();
        e.session =
            capture_core::Session::new(capture_core::Document::load(&path).unwrap()).unwrap();
        e.step_number = 1;
        e.add_step([400.0, 180.0]);
        assert_eq!(
            e.session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "19"
        );
        e.execute(Command::Undo);
        assert_eq!(e.step_number, 19);
        e.execute(Command::Redo);
        assert_eq!(e.step_number, 20);
    }

    #[test]
    fn letter_sequence_is_independent_and_limit_never_duplicates_last_marker() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(Kind::Step);
        e.step_number = 999;
        e.add_step([60.0, 60.0]);
        e.add_step([120.0, 60.0]);
        assert_eq!(e.session.document.annotations.len(), 1);
        assert!(e.status.contains("1–999"));
        e.step_letters = true;
        e.step_number = 26;
        e.add_step([120.0, 60.0]);
        e.add_step([180.0, 60.0]);
        assert_eq!(
            e.session.document.annotations[2]
                .text
                .as_ref()
                .unwrap()
                .content,
            "AA"
        );
    }
}
