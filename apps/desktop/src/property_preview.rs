//! Selected-property transactions follow DesignCraft's held-preview/commit pattern.
//! See docs/designcraft-reuse-audit.md. ClipCraft keeps the original document intact
//! until release, so Escape never needs to rewrite history or restore an image.
use crate::{Annotation, Command, Document, Editor, egui};

pub(super) struct PropertyPreview {
    original: Annotation,
    pub preview: Annotation,
    pub batch_originals: Vec<Annotation>,
    pub batch_preview: Vec<Annotation>,
}

impl Editor {
    pub(super) fn toggle_annotations(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.shell.annotations_hidden = !self.shell.annotations_hidden;
        self.shell.raster_preview = false;
        self.needs_render = true;
        self.status = if self.shell.annotations_hidden {
            "Annotations hidden for comparison. Copy and export still include all objects."
        } else {
            "Annotations shown. Continue editing."
        }
        .into();
    }

    pub(super) fn canvas_document(&self) -> Document {
        let mut document = self.preview_document();
        if self.shell.annotations_hidden {
            document.annotations.clear();
            document.object_states.clear();
            document.guide = None;
        } else if let Some(draft) = &self.text_draft
            && let Some(annotation) = document
                .annotations
                .iter_mut()
                .find(|a| a.id == draft.preview.id)
        {
            // Decorated labels use the shared vector coverage layer even during
            // entry, avoiding doubled ink where a rule intersects a glyph.
            // The widget retains caret/selection geometry without repainting ink.
            annotation.rotation_degrees = draft.preview.rotation_degrees;
            if let Some(text) = annotation.text.as_mut()
                && !text.decorated()
            {
                text.content.clear();
            }
        }
        document
    }
    pub(super) fn prepare_property_input(&mut self, ctx: &egui::Context) {
        if crate::color_widgets::eyedropper::consume_escape(ctx) {
            return;
        }
        let (focused, escape) = ctx.input(|i| (i.focused, i.key_pressed(egui::Key::Escape)));
        if self.property_preview.is_some()
            && !escape
            && crate::color_widgets::dismissing_live_popup(ctx)
        {
            self.commit_property_preview();
            crate::color_widgets::close_live_popup(ctx);
        } else if self.property_preview.is_some() && (!focused || escape) {
            self.cancel_gesture();
            self.property_suppressed = true;
            self.status = "Property preview cancelled. The object is unchanged.".into();
        }
    }

    #[cfg(test)]
    pub(super) fn preview_property_edit(&mut self, edited: Annotation) {
        self.preview_property_edit_with_rotation(edited, false);
    }

    pub(super) fn preview_property_edit_with_rotation(
        &mut self,
        edited: Annotation,
        rotation_requested: bool,
    ) {
        if self.property_suppressed {
            return;
        }
        let Some(original) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| Some(a.id) == self.selected && a.id == edited.id)
            .cloned()
        else {
            return;
        };
        let current = self
            .property_preview
            .as_ref()
            .map(|p| &p.preview)
            .unwrap_or(&original);
        if &edited == current && !rotation_requested {
            return;
        }
        let canvas = [self.session.document.width, self.session.document.height];
        let batch_originals = self.selected_objects();
        let batch_preview: capture_core::Result<Vec<_>> = batch_originals
            .iter()
            .map(|a| {
                let mut formatted = if a.id == edited.id {
                    edited.clone()
                } else {
                    capture_core::multi_selection::format_like_change(a, &original, &edited)
                };
                if rotation_requested && formatted.kind == edited.kind {
                    formatted.rotation_degrees = edited.rotation_degrees;
                }
                if edited.kind == crate::Kind::Step && a.id != edited.id {
                    let (old_min, old_max) = original.body_bounds();
                    let (min, max) = edited.body_bounds();
                    if old_max[0] - old_min[0] != max[0] - min[0] {
                        formatted = capture_core::steps::resized_marker(
                            &formatted,
                            max[0] - min[0],
                            canvas,
                        )?;
                    }
                }
                formatted.fit_text_shape(canvas)
            })
            .collect();
        let batch_preview = match batch_preview {
            Ok(values) => values,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        self.preview_property_batch(batch_preview);
    }

    /// Store a validated live batch while keeping the document and Undo untouched.
    /// Ribbon text controls use this across text-bearing object types; sidebar
    /// geometry/style controls still choose their own compatible targets above.
    pub(super) fn preview_property_batch(&mut self, batch_preview: Vec<Annotation>) {
        if self.property_suppressed {
            return;
        }
        let Some(original) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| Some(a.id) == self.selected)
            .cloned()
        else {
            return;
        };
        let batch_originals = self.selected_objects();
        if batch_preview.len() != batch_originals.len()
            || !batch_preview
                .iter()
                .zip(&batch_originals)
                .all(|(a, b)| a.id == b.id)
        {
            return;
        }
        let edited = batch_preview
            .iter()
            .find(|a| a.id == original.id)
            .unwrap()
            .clone();
        let mut document = self.session.document.clone();
        for value in &batch_preview {
            *document
                .annotations
                .iter_mut()
                .find(|a| a.id == value.id)
                .unwrap() = value.clone();
        }
        if document
            .annotations
            .iter()
            .any(|a| a.rotation_degrees != 0.0)
        {
            document.version = document.version.max(3);
        }
        document.promote_features();
        if let Err(error) = document.validate() {
            self.status = error.to_string();
            return;
        }
        self.property_preview = Some(PropertyPreview {
            original,
            preview: edited,
            batch_originals,
            batch_preview,
        });
        self.needs_render = true;
    }

    pub(super) fn settle_property_preview(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.pointer.any_down())
            || ctx.egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        if self.property_suppressed {
            if ctx.input(|i| i.focused) {
                self.property_suppressed = false;
            }
            return;
        }
        self.commit_property_preview();
    }

    pub(super) fn commit_property_preview(&mut self) {
        let Some(preview) = self.property_preview.take() else {
            return;
        };
        self.needs_render = true;
        // A file replacement/deletion must never resurrect an old preview.
        if self
            .session
            .document
            .annotations
            .iter()
            .any(|a| a == &preview.original)
            && self.selected == Some(preview.original.id)
            && preview.batch_originals.iter().all(|a| {
                self.session
                    .document
                    .annotations
                    .iter()
                    .any(|current| current == a)
            })
            && preview.batch_preview != preview.batch_originals
        {
            self.execute(Command::UpdateMany {
                annotations: preview.batch_preview,
            });
            self.status = "Object updated. Undo reverses the whole property change.".into();
        }
    }

    pub(super) fn preview_document(&self) -> Document {
        if let Some(drag) = &self.guide_drag {
            return drag.preview.clone();
        }
        let mut document = self.session.document.clone();
        if let Some(drag) = &self.group_drag
            && let Some(crate::group_transform::TransformDrag::Analyze { region, .. }) =
                &drag.transform
        {
            let _ = document.replace_simplify_group(region.group.unwrap(), drag.preview.clone());
            return document;
        }

        let preview = self
            .edit_drag
            .as_ref()
            .map(|d| &d.preview)
            .or_else(|| self.property_preview.as_ref().map(|p| &p.preview));
        if let Some(preview) = preview
            && let Some(annotation) = document.annotations.iter_mut().find(|a| a.id == preview.id)
        {
            *annotation = preview.clone();
        }
        if let Some(values) = self
            .group_drag
            .as_ref()
            .map(|d| &d.preview)
            .or_else(|| self.property_preview.as_ref().map(|p| &p.batch_preview))
        {
            for value in values {
                if let Some(target) = document.annotations.iter_mut().find(|a| a.id == value.id) {
                    *target = value.clone();
                } else if self.group_drag.as_ref().is_some_and(|d| d.duplicate) {
                    document.annotations.push(value.clone());
                }
            }
        }
        if let Some(draft) = &self.text_draft
            && let Some(annotation) = document
                .annotations
                .iter_mut()
                .find(|a| a.id == draft.preview.id)
            && draft
                .preview
                .text
                .as_ref()
                .is_some_and(|style| style.validate().is_ok())
        {
            *annotation = draft.checkpoint_annotation();
        }
        if let Some(drag) = &self.rotation_drag
            && let Some(a) = document
                .annotations
                .iter_mut()
                .find(|a| a.id == drag.preview.id)
        {
            *a = drag.preview.clone();
            document.version = document.version.max(3);
        }
        if self.tool == Some(crate::Kind::Eraser)
            && self.drag_start.is_some()
            && self.stroke_points.len() >= 2
        {
            document
                .annotations
                .push(self.eraser_annotation(self.stroke_points.clone()));
        }
        if let Some(step) = &self.step_placement {
            document.annotations.push(step.clone());
        }
        if document
            .annotations
            .iter()
            .any(|a| a.rotation_degrees != 0.0)
        {
            document.version = document.version.max(3);
        }
        document.promote_features();
        if let Some(plan) = self.expanding_move_plan()
            && (plan.dimensions != [self.session.document.width, self.session.document.height]
                || plan.offset != [0, 0])
        {
            // Autosave/copy get a valid editable checkpoint. Interactive display
            // uses the plan directly and avoids encoding PNGs on drag frames.
            return plan
                .apply(&self.session.document)
                .unwrap_or_else(|_| self.session.document.clone());
        }
        document
    }

    pub(super) fn eraser_annotation(&self, points: Vec<[f32; 2]>) -> Annotation {
        Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id: self.session.next_id(),
            kind: crate::Kind::Eraser,
            start: points[0],
            end: *points.last().unwrap(),
            color: [0, 0, 0, self.quick_styles.alpha],
            width: self.width,
            points,
            corner_radius: 0.0,
            fill: None,
            text: None,
            callout: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Kind, Session};
    use egui::{Event, Modifiers, PointerButton, Pos2};

    #[test]
    fn annotation_comparison_hides_only_ui_objects_preserves_selection_pixels_and_history() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = Session::new(
            serde_json::from_str(include_str!(
                "../../../crates/core/tests/fixtures/translation.capture.json"
            ))
            .unwrap(),
        )
        .unwrap();
        e.tool = Some(Kind::Step);
        e.select_only(Some(e.session.document.annotations[0].id));
        let selection = e.selected;
        let original = serde_json::to_vec(&e.session.document).unwrap();
        let complete = e.session.document.render().unwrap();
        let base = e.session.document.base_image().unwrap();
        assert_ne!(complete, base);
        e.toggle_annotations();
        assert_eq!(e.canvas_document().render().unwrap(), base);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                focused: true,
                events: vec![
                    Event::PointerMoved(egui::pos2(400.0, 300.0)),
                    Event::PointerButton {
                        pos: egui::pos2(400.0, 300.0),
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                    Event::PointerButton {
                        pos: egui::pos2(400.0, 300.0),
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| e.show_canvas(ui),
        );
        output.textures_delta.clear();
        assert_eq!(e.selected, selection);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert_eq!(e.session.document.render().unwrap(), complete);
        assert!(!e.session.can_undo());
        e.toggle_annotations();
        assert_eq!(e.canvas_document().render().unwrap(), complete);
        assert!(!e.session.can_undo());
    }

    fn editor(ctx: &egui::Context) -> Editor {
        let mut editor = crate::canvas::tests::editor(ctx);
        let mut document = Document::blank(640, 400).unwrap();
        document.annotations.push(Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id: 1,
            kind: Kind::Ellipse,
            start: [100.0, 100.0],
            end: [300.0, 240.0],
            color: [30, 120, 220, 180],
            width: 3,
            corner_radius: 0.0,
            fill: None,
            points: vec![],
            text: None,
            callout: None,
        });
        editor.session = Session::new(document).unwrap();
        editor.selected = Some(1);
        editor.tool = None;
        editor
    }

    #[test]
    fn numeric_rotation_batches_preview_commit_reset_and_reopen() {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let mut second = e.session.document.annotations[0]
            .translated([240.0, 0.0], [640, 400])
            .unwrap();
        second.id = 2;
        e.session.document.annotations.push(second);
        e.extra_selected.insert(2);
        let before = serde_json::to_vec(&e.session.document).unwrap();
        for angle in [10.0, 23.0, 37.5] {
            let edited = e
                .selected_annotation()
                .unwrap()
                .with_rotation(angle)
                .unwrap();
            e.preview_property_edit(edited);
            assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
            let preview = e.preview_document();
            assert_eq!(preview.version, 3);
            assert!(
                preview
                    .annotations
                    .iter()
                    .all(|a| a.rotation_degrees == angle)
            );
            preview.validate().unwrap();
            preview.render().unwrap();
        }
        e.commit_property_preview();
        assert_eq!(e.session.document.version, 3);
        let path = std::env::temp_dir().join(format!(
            "clipcraft-numeric-rotation-{}.json",
            std::process::id()
        ));
        e.session.document.save(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        assert_eq!(
            reopened.render().unwrap(),
            e.session.document.render().unwrap()
        );
        std::fs::remove_file(path).unwrap();
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert!(!e.session.can_undo());
        e.execute(Command::Redo);
        let rotated = serde_json::to_vec(&e.session.document).unwrap();
        e.preview_property_edit(e.selected_annotation().unwrap().with_rotation(0.0).unwrap());
        e.commit_property_preview();
        assert!(
            e.session
                .document
                .annotations
                .iter()
                .all(|a| a.rotation_degrees == 0.0)
        );
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), rotated);
        let mut bad = e.selected_annotation().unwrap();
        bad.rotation_degrees = f32::NAN;
        e.preview_property_edit(bad);
        assert!(e.property_preview.is_none());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), rotated);
        // Explicit zero also resets peers when the primary object is already upright.
        e.session.document.annotations[0].rotation_degrees = 0.0;
        e.preview_property_edit_with_rotation(e.selected_annotation().unwrap(), true);
        e.commit_property_preview();
        assert!(
            e.session
                .document
                .annotations
                .iter()
                .all(|a| a.rotation_degrees == 0.0)
        );
        // Project4 metadata must never be downgraded by an orientation edit.
        e.session.document.version = 4;
        e.preview_property_edit(
            e.selected_annotation()
                .unwrap()
                .with_rotation(15.0)
                .unwrap(),
        );
        e.commit_property_preview();
        assert_eq!(e.session.document.version, 4);
    }

    #[test]
    fn rotation_field_keeps_keyboard_preview_as_one_transaction_and_reset_click_works() {
        use egui::Key;
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let original = serde_json::to_vec(&e.session.document).unwrap();
        let render = |e: &mut Editor, events| {
            let mut controls = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(400.0, 200.0),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.prepare_property_input(&ctx);
                    let mut a = e.selected_annotation().unwrap();
                    controls = crate::property_layout::rotation_controls(ui, &mut a);
                    e.preview_property_edit_with_rotation(
                        a,
                        controls
                            .as_ref()
                            .is_some_and(|r| r[0].changed() || r[1].clicked()),
                    );
                    e.settle_property_preview(&ctx);
                },
            );
            output.textures_delta.clear();
            controls.unwrap()
        };
        render(&mut e, vec![]);
        let controls = render(&mut e, vec![]);
        let pos = controls[0].rect.center();
        for pressed in [true, false] {
            render(&mut e, vec![Event::PointerMoved(pos), button(pos, pressed)]);
        }
        let key = |key, modifiers| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        render(
            &mut e,
            vec![
                key(Key::A, Modifiers::CTRL | Modifiers::COMMAND),
                Event::Text("37.5".into()),
            ],
        );
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert_eq!(e.preview_document().annotations[0].rotation_degrees, 37.5);
        render(&mut e, vec![key(Key::Enter, Modifiers::NONE)]);
        assert_eq!(e.session.document.annotations[0].rotation_degrees, 37.5);
        let rotated = serde_json::to_vec(&e.session.document).unwrap();
        let controls = render(&mut e, vec![]);
        let reset = controls[1].rect.center();
        for pressed in [true, false] {
            render(
                &mut e,
                vec![Event::PointerMoved(reset), button(reset, pressed)],
            );
        }
        assert_eq!(e.session.document.annotations[0].rotation_degrees, 0.0);
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), rotated);
        e.execute(Command::Undo);
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
        assert!(!e.session.can_undo());
        let mut peer = e.session.document.annotations[0].clone();
        peer.id = 2;
        peer.rotation_degrees = 40.0;
        e.session.document.annotations.push(peer);
        e.session.document.version = 3;
        e.extra_selected.insert(2);
        let controls = render(&mut e, vec![]);
        assert!(controls[1].enabled());
        let reset = controls[1].rect.center();
        for pressed in [true, false] {
            render(
                &mut e,
                vec![Event::PointerMoved(reset), button(reset, pressed)],
            );
        }
        assert!(
            e.session
                .document
                .annotations
                .iter()
                .all(|a| a.rotation_degrees == 0.0)
        );
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn frame(
        ctx: &egui::Context,
        editor: &mut Editor,
        events: Vec<Event>,
        focused: bool,
    ) -> egui::Rect {
        let mut rect = egui::Rect::NOTHING;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(400.0, 200.0),
                )),
                events,
                focused,
                ..Default::default()
            },
            |ui| {
                editor.prepare_property_input(ctx);
                let mut edited = editor.selected_annotation().unwrap();
                rect = crate::controls::slider(
                    ui,
                    egui::Slider::new(&mut edited.width, 1..=32).text("Stroke width"),
                )
                .rect;
                editor.preview_property_edit(edited);
                editor.settle_property_preview(ctx);
            },
        );
        out.textures_delta.clear();
        rect
    }

    // Same behavioral contract as DesignCraft's held-slider regression: preview
    // each input frame, commit only on drop, and restore the exact starting state.
    #[test]
    fn held_slider_renders_object_each_frame_and_commits_one_undo() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let original = editor.session.document.clone();
        let before_pixels = original.render().unwrap();
        let rect = frame(&ctx, &mut editor, vec![], true);
        frame(&ctx, &mut editor, vec![], true);
        let first = rect.min + egui::vec2(5.0, rect.height() / 2.0);
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(first), button(first, true)],
            true,
        );
        for x in [30.0, 50.0, 70.0, 90.0] {
            frame(
                &ctx,
                &mut editor,
                vec![Event::PointerMoved(first + egui::vec2(x, 0.0))],
                true,
            );
            assert_eq!(editor.session.document.annotations, original.annotations);
            assert!(!editor.session.can_undo());
            assert_ne!(editor.preview_document().render().unwrap(), before_pixels);
        }
        let preview = editor.preview_document().render().unwrap();
        // An unmoving held frame must not split history.
        frame(&ctx, &mut editor, vec![], true);
        assert!(!editor.session.can_undo());
        frame(
            &ctx,
            &mut editor,
            vec![button(first + egui::vec2(90.0, 0.0), false)],
            true,
        );
        assert!(editor.property_preview.is_none());
        assert_eq!(editor.session.document.render().unwrap(), preview);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, original.annotations);
        assert!(!editor.session.can_undo());
        editor.execute(Command::Redo);
        assert_eq!(editor.session.document.render().unwrap(), preview);
        let reopened: Document =
            serde_json::from_slice(&serde_json::to_vec(&editor.session.document).unwrap()).unwrap();
        assert_eq!(reopened.render().unwrap(), preview);
    }

    #[test]
    fn escape_focus_loss_and_stale_selection_cannot_commit_a_preview() {
        for focus_loss in [false, true] {
            let ctx = egui::Context::default();
            let mut editor = editor(&ctx);
            let original = editor.session.document.annotations.clone();
            let rect = frame(&ctx, &mut editor, vec![], true);
            frame(&ctx, &mut editor, vec![], true);
            let p = rect.min + egui::vec2(60.0, rect.height() / 2.0);
            frame(
                &ctx,
                &mut editor,
                vec![Event::PointerMoved(p), button(p, true)],
                true,
            );
            assert!(editor.property_preview.is_some());
            let events = if focus_loss {
                vec![]
            } else {
                vec![Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }]
            };
            frame(&ctx, &mut editor, events, !focus_loss);
            assert!(editor.property_preview.is_none());
            frame(&ctx, &mut editor, vec![button(p, false)], true);
            assert_eq!(editor.session.document.annotations, original);
            assert!(!editor.session.can_undo());
        }
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let original = editor.selected_annotation().unwrap();
        editor.preview_property_edit(original.clone());
        assert!(editor.property_preview.is_none());
        let mut invalid = original.clone();
        invalid.width = 0;
        editor.preview_property_edit(invalid);
        assert!(editor.property_preview.is_none());
        let mut edited = original.clone();
        edited.width = 17;
        editor.preview_property_edit(edited);
        editor.selected = None;
        editor.settle_property_preview(&ctx);
        assert_eq!(editor.session.document.annotations, vec![original]);
        assert!(!editor.session.can_undo());
    }
}
