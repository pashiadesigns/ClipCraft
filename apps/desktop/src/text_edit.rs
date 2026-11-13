use crate::{Annotation, Command, Editor, Kind, egui};
use capture_core::text::{MAX_TEXT_CHARS, TextStyle, VerticalAlignment};

pub(super) fn cancel_on_escape(modal: &egui::ModalResponse<()>) -> bool {
    // Capture popup ownership from before the modal was drawn: the popup may
    // already have handled this Escape and closed during this same frame.
    modal.response.should_close()
        || modal.is_top_modal
            && !modal.any_popup_open
            && modal
                .response
                .ctx
                .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
}

pub(super) struct TextDraft {
    original: Annotation,
    pub preview: Annotation,
    focus: bool,
    surface: Option<(egui::Rect, egui::Rect)>,
    pub selection: std::ops::Range<usize>,
    pub selection_active: bool,
    pub typing: Option<capture_core::text::rich::Format>,
    history: Vec<TextSnapshot>,
    history_at: usize,
}

#[derive(Clone, PartialEq)]
struct TextSnapshot {
    annotation: Annotation,
    selection: std::ops::Range<usize>,
    typing: Option<capture_core::text::rich::Format>,
}
impl TextDraft {
    fn remember(&mut self) {
        let snapshot = TextSnapshot {
            annotation: self.preview.clone(),
            selection: self.selection.clone(),
            typing: self.typing.clone(),
        };
        if self
            .history
            .get(self.history_at)
            .is_some_and(|v| v.annotation == snapshot.annotation && v.typing == snapshot.typing)
        {
            return;
        }
        self.history.truncate(self.history_at + 1);
        self.history.push(snapshot);
        if self.history.len() > 100 {
            self.history.remove(0);
        }
        self.history_at = self.history.len() - 1;
    }
    fn restore(&mut self, redo: bool) {
        self.remember();
        self.history_at = if redo {
            (self.history_at + 1).min(self.history.len() - 1)
        } else {
            self.history_at.saturating_sub(1)
        };
        let value = &self.history[self.history_at];
        self.preview = value.annotation.clone();
        self.selection = value.selection.clone();
        self.typing = value.typing.clone();
    }
    fn changed(&self) -> bool {
        self.original != self.checkpoint_annotation()
    }
    pub(super) fn checkpoint_annotation(&self) -> Annotation {
        let mut annotation = self.preview.clone();
        annotation.rotation_degrees = self.original.rotation_degrees;
        annotation
    }
}

impl Editor {
    pub(super) fn is_modified(&self) -> bool {
        self.session.is_modified() || self.text_draft.as_ref().is_some_and(TextDraft::changed)
    }

    pub(super) fn begin_text_edit(&mut self) {
        if let Some(a) = self.selected_annotation().filter(|a| {
            a.focus.is_none()
                && matches!(
                    a.kind,
                    Kind::Text
                        | Kind::Callout
                        | Kind::Step
                        | Kind::Rectangle
                        | Kind::Ellipse
                        | Kind::Polygon
                )
        }) {
            if self.shell.raster_preview {
                self.shell.raster_preview = false;
                self.needs_render = true;
            }
            self.cancel_gesture();
            self.select_only(Some(a.id));
            let mut preview = a.clone();
            // egui's editable caret frame is axis-aligned. Keep an upright
            // editing frame during entry and restore the stored orientation
            // when finishing; autosave checkpoints retain that orientation too.
            preview.rotation_degrees = 0.0;
            if a.rotation_degrees != 0.0 {
                self.status="Text entry uses an upright editing frame. Finish text to restore the object's rotation.".into();
            }
            preview.text.get_or_insert_with(|| TextStyle {
                content: String::new(),
                padding: 6.0,
                color: Some([35, 35, 35, 255]),
                ..Default::default()
            });
            self.text_draft = Some(TextDraft {
                original: a,
                preview,
                focus: true,
                surface: None,
                selection: 0..0,
                selection_active: false,
                typing: None,
                history: vec![],
                history_at: 0,
            });
            self.text_draft.as_mut().unwrap().remember();
            self.shell.properties = true;

            self.shell.property_tab = crate::shell::PropertyTab::Tool;
            self.needs_render = true;
        }
    }

    pub(super) fn commit_text(&mut self) -> bool {
        let Some(draft) = &self.text_draft else {
            return true;
        };
        if let Some(style) = &draft.preview.text
            && let Err(error) = style.validate()
        {
            self.status = error.to_string();
            return false;
        }
        let annotation = match draft
            .checkpoint_annotation()
            .fit_text_shape([self.session.document.width, self.session.document.height])
        {
            Ok(annotation) => annotation,
            Err(error) => {
                crate::guided::set_error(&mut self.status, &mut self.notice, error);
                return false;
            }
        };
        if draft.changed()
            && let Err(error) = self
                .keyframe_command(Command::Update {
                    annotation: annotation.clone(),
                })
                .and_then(|command| self.session.execute(command))
        {
            self.status = error.to_string();
            return false;
        }
        self.text_draft = None;
        self.remember_annotation_style(&annotation);
        self.needs_render = true;
        true
    }

    pub(super) fn finish_text_on_outside_click(&mut self, ctx: &egui::Context) {
        if egui::Popup::is_any_open(ctx) {
            return;
        }
        let Some((rect, viewport)) = self.text_draft.as_ref().and_then(|d| d.surface) else {
            return;
        };
        if ctx.input(|i| {
            i.pointer.button_pressed(egui::PointerButton::Primary)
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !rect.contains(p) && viewport.contains(p))
        }) {
            self.commit_text();
        }
    }

    pub(super) fn inline_text_properties(&mut self, ui: &mut egui::Ui) {
        let Some(draft) = self.text_draft.as_ref() else {
            return;
        };
        if draft.preview.kind == Kind::Text {
            crate::property_layout::section_help(
                ui,
                "Properties",
                crate::property_layout::TEXT_HELP,
            );
        } else if crate::property_layout::text_section_with_back(ui).clicked() && self.commit_text()
        {
            return;
        }
        self.dedicated_text_properties(ui);
        if let Some(error) = self
            .text_draft
            .as_ref()
            .and_then(|d| d.preview.text.as_ref())
            .and_then(|t| t.validate().err())
        {
            ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
        }
    }

    pub(super) fn show_inline_text(
        &mut self,
        ui: &mut egui::Ui,
        transform: crate::canvas_view::Transform,
    ) {
        self.inline_format_shortcuts(ui.ctx());
        let Some(draft) = &mut self.text_draft else {
            return;
        };
        let ctx = ui.ctx().clone();
        let before = draft.preview.clone();
        let input_id = egui::Id::unique(("text_content", draft.preview.id));
        // Enter on an empty list item ends the list without inserting another
        // bullet. Keep this in the same inline edit/Undo transaction.
        if ctx.memory(|m| m.has_focus(input_id))
            && draft.selection.is_empty()
            && draft.preview.text.as_mut().is_some_and(|s| {
                let caret = draft.selection.start;
                let chars: Vec<_> = s.content.chars().collect();
                let start = chars[..caret.min(chars.len())]
                    .iter()
                    .rposition(|c| *c == '\n')
                    .map_or(0, |i| i + 1);
                let end = chars[caret.min(chars.len())..]
                    .iter()
                    .position(|c| *c == '\n')
                    .map_or(chars.len(), |i| caret + i);
                let listed = s
                    .rich
                    .as_ref()
                    .and_then(|r| r.paragraphs.iter().rev().find(|p| p.start <= caret))
                    .is_some_and(|p| p.list != capture_core::text::rich::List::None);
                if listed
                    && start == end
                    && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                {
                    capture_core::text::rich::paragraph_range(s, caret..caret, |p| {
                        p.list = capture_core::text::rich::List::None;
                        p.level = 0;
                    });
                    true
                } else {
                    false
                }
            })
        {
            self.needs_render = true;
        }
        if ctx.memory(|m| m.has_focus(input_id)) {
            let redo = ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                ) || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            });
            let undo = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
            if redo || undo {
                draft.restore(redo);
                crate::text_clipboard::set_cursor(&ctx, input_id, draft.selection.end);
                self.needs_render = true;
            }
        }
        if let Err(error) = crate::text_clipboard::events(&ctx, draft) {
            crate::guided::set_error(&mut self.status, &mut self.notice, error);
        }
        let navigating = self.view.panning
            || ui.input(|i| i.pointer.button_released(egui::PointerButton::Middle));
        let input_id = egui::Id::unique(("text_content", draft.preview.id));
        let restore_focus = navigating && ctx.memory(|memory| memory.has_focus(input_id));
        let navigation_cursor = navigating
            .then(|| egui::TextEdit::load_state(&ctx, input_id).map(|state| state.cursor))
            .flatten();
        match draft
            .preview
            .fit_text_shape([self.session.document.width, self.session.document.height])
        {
            Ok(fitted) => draft.preview = fitted,
            Err(error) => crate::guided::set_error(&mut self.status, &mut self.notice, error),
        }
        let (min, max) = capture_core::text::content_bounds(&draft.preview);
        let rect =
            egui::Rect::from_min_max(transform.screen_point(min), transform.screen_point(max));
        // A collapsed content box needs a reachable input surface; content stays
        // clipped by the shared renderer until the user enlarges the object.
        let rect = if rect.width() < 12.0 || rect.height() < 12.0 {
            egui::Rect::from_center_size(
                rect.center(),
                egui::vec2(rect.width().max(12.0), rect.height().max(12.0)),
            )
        } else {
            rect
        };
        draft.surface = Some((rect, transform.viewport));
        let mut layout_annotation = draft.preview.clone();
        let inherited_color = if layout_annotation.kind == Kind::Step {
            capture_core::steps::foreground(&layout_annotation).color
        } else {
            layout_annotation.color
        };
        let Some(style) = &mut draft.preview.text else {
            return;
        };
        let align = egui::Align2([
            // Horizontal offsets are already resolved by the shared engine.
            egui::Align::Min,
            match style.vertical {
                VerticalAlignment::Top => egui::Align::Min,
                VerticalAlignment::Middle => egui::Align::Center,
                VerticalAlignment::Bottom => egui::Align::Max,
            },
        ]);
        let color = style.color.unwrap_or(inherited_color);
        let family = crate::text_layout::available_family(ui.ctx(), style);
        let mut layout_style = style.clone();
        let original_content = style.content.clone();
        let typing = draft.typing.clone();
        let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _: f32| {
            let before = layout_style.content.clone();
            layout_style.content = text.as_str().to_owned();
            capture_core::text::rich::rebase(&mut layout_style, &before, typing.as_ref());
            // Recalculate Step sizing for the text in this exact input frame.
            // Keep the requested font size untouched in the editable object.
            let resolved_style = if layout_annotation.kind == Kind::Step {
                layout_annotation.text = Some(layout_style.clone());
                capture_core::steps::foreground(&layout_annotation)
                    .text
                    .unwrap()
            } else {
                layout_style.clone()
            };
            crate::text_layout::galley(
                ui,
                &resolved_style,
                [(max[0] - min[0]).max(0.0), (max[1] - min[1]).max(0.0)],
                transform.scale,
                egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
            )
        };
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(rect.intersect(transform.viewport));
                if style.decorated() {
                    // Selection is above the canvas-rendered ink. A translucent
                    // highlight keeps selected letters visible without a second
                    // glyph pass (which would double translucent text opacity).
                    ui.visuals_mut().selection.bg_fill =
                        ui.visuals().selection.bg_fill.gamma_multiply(0.28);
                }
                let input = ui.add_sized(
                    rect.size(),
                    crate::unicode_input::TextEdit::multiline(&mut style.content)
                        .id(input_id)
                        .font(egui::FontId::new(
                            style.size_points * 4.0 / 3.0 * transform.scale,
                            family,
                        ))
                        .layouter(&mut layouter)
                        .text_color(egui::Color32::from_rgba_unmultiplied(
                            color[0], color[1], color[2], color[3],
                        ))
                        .align(align)
                        .frame(egui::Frame::NONE)
                        .margin(0)
                        .min_size(rect.size())
                        .desired_rows(1)
                        .desired_width(rect.width())
                        .char_limit(MAX_TEXT_CHARS),
                );
                if draft.focus || restore_focus {
                    input.request_focus();
                    draft.focus = false;
                }
                // egui also repositions its caret on non-primary presses.
                // Keep the text selection intact while the canvas owns this
                // middle gesture; do not disable the input and lose focus.
                if let Some(cursor) = navigation_cursor
                    && let Some(mut state) = egui::TextEdit::load_state(&ctx, input_id)
                {
                    state.cursor = cursor;
                    state.store(&ctx, input_id);
                }
            },
        );
        capture_core::text::rich::rebase(style, &original_content, draft.typing.as_ref());
        if ctx.memory(|m| m.has_focus(input_id))
            && let Some(range) =
                egui::TextEdit::load_state(&ctx, input_id).and_then(|s| s.cursor.char_range())
        {
            let r = range.as_sorted_char_range();
            let next = r.start.0..r.end.0;
            if next != draft.selection && original_content == style.content {
                draft.typing = None;
            }
            draft.selection = next;
            draft.selection_active = true;
        }
        if self.rich_text.autocorrect
            && original_content != style.content
            && let Some(caret) = crate::rich_text::autocorrect(
                style,
                draft.selection.end,
                &self.rich_text.corrections,
            )
        {
            crate::text_clipboard::set_cursor(&ctx, input_id, caret);
            draft.selection = caret..caret;
        }
        match draft
            .preview
            .fit_text_shape([self.session.document.width, self.session.document.height])
        {
            Ok(fitted) => draft.preview = fitted,
            Err(error) => crate::guided::set_error(&mut self.status, &mut self.notice, error),
        }
        if draft.preview != before {
            self.needs_render = true;
            ctx.request_repaint();
        }
        draft.remember();
        let cancel = !egui::Popup::is_any_open(&ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let apply = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
        if cancel {
            self.text_draft = None;
            self.tool = None;
            self.icon_library.active = false;

            self.needs_render = true;
            ctx.request_repaint();
        } else if apply {
            self.commit_text();
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_sidebar_formatting_retains_range_and_whole_block_changes_apply_immediately() {
        for kind in [Kind::Text, Kind::Step, Kind::Rectangle] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            crate::text_layout::configure(&ctx);
            let mut e = crate::canvas::tests::editor(&ctx);
            let mut a = capture_core::steps::marker(
                1,
                [220., 180.],
                140,
                [20, 150, 190, 255],
                (1, false),
                [640, 400],
            )
            .unwrap();
            a.kind = kind;
            let text = a.text.as_mut().unwrap();
            text.content = "First second".into();
            text.bold = false;
            text.italic = false;
            e.session.document.annotations = vec![a];
            e.select_only(Some(1));
            e.tool = None;
            e.begin_text_edit();
            let draw = |e: &mut Editor, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1200., 900.),
                        )),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        e.finish_text_on_outside_click(&ctx);
                        egui::Panel::right("test-properties")
                            .exact_size(390.)
                            .show(ui, |ui| e.image_properties_pane(ui));
                        e.show_canvas(ui);
                    },
                );
                output.textures_delta.clear();
            };
            for _ in 0..3 {
                draw(&mut e, vec![]);
            }
            let input_id = egui::Id::unique(("text_content", 1u64));
            let mut state = egui::TextEdit::load_state(&ctx, input_id).unwrap();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(5),
                )));
            state.store(&ctx, input_id);
            draw(&mut e, vec![]);
            for label in ["Bold", "Italic"] {
                let pos = ctx
                    .data(|d| {
                        d.get_temp::<(egui::Rect, bool)>(egui::Id::unique((
                            "quick_text_button",
                            label,
                        )))
                    })
                    .unwrap()
                    .0
                    .center();
                for pressed in [true, false] {
                    draw(
                        &mut e,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    );
                }
            }
            let draft = e.text_draft.as_ref().unwrap();
            assert_eq!(draft.selection, 0..5, "{kind:?}");
            let text = draft.preview.text.as_ref().unwrap();
            let f = capture_core::text::rich::format_at(text, 1);
            assert!(f.bold && f.italic, "{kind:?}: {f:?}");
            assert!(!capture_core::text::rich::format_at(text, 8).bold);
            // A caret with whole-block mode immediately affects every run.
            let draft = e.text_draft.as_mut().unwrap();
            draft.selection = 5..5;
            draft.selection_active = true;
            e.rich_text.caret_formatting = false;
            let pos = ctx
                .data(|d| {
                    d.get_temp::<(egui::Rect, bool)>(egui::Id::unique((
                        "quick_text_button",
                        "Underline",
                    )))
                })
                .unwrap()
                .0
                .center();
            for pressed in [true, false] {
                draw(
                    &mut e,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            let text = e
                .text_draft
                .as_ref()
                .unwrap()
                .preview
                .text
                .as_ref()
                .unwrap();
            assert!(capture_core::text::rich::format_at(text, 1).underline);
            assert!(capture_core::text::rich::format_at(text, 8).underline);
        }
    }

    #[test]
    fn enter_on_empty_list_item_returns_to_plain_paragraph() {
        let ctx = egui::Context::default();
        let mut e = crate::rich_text::tests::editor(&ctx);
        let text = e.session.document.annotations[0].text.as_mut().unwrap();
        text.content = "Item\n".into();
        capture_core::text::rich::paragraph_range(text, 0..6, |p| {
            p.list = capture_core::text::rich::List::Bullet
        });
        e.select_only(Some(e.session.document.annotations[0].id));
        e.begin_text_edit();
        inline_frame(&ctx, &mut e, vec![]);
        let id = egui::Id::unique(("text_content", e.text_draft.as_ref().unwrap().preview.id));
        crate::text_clipboard::set_cursor(&ctx, id, 5);
        inline_frame(&ctx, &mut e, vec![]);
        inline_frame(
            &ctx,
            &mut e,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let text = e
            .text_draft
            .as_ref()
            .unwrap()
            .preview
            .text
            .as_ref()
            .unwrap();
        assert_eq!(text.content, "Item\n");
        assert_eq!(
            text.rich.as_ref().unwrap().paragraphs.last().unwrap().list,
            capture_core::text::rich::List::None
        );
    }

    #[test]
    fn mixed_selection_typing_and_formatting_undo_restore_runs() {
        let ctx = egui::Context::default();
        let mut e = crate::rich_text::tests::editor(&ctx);
        e.begin_text_edit();
        inline_frame(&ctx, &mut e, vec![]);
        let id = egui::Id::unique(("text_content", e.text_draft.as_ref().unwrap().preview.id));
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        };
        // Selection is kept when the formatting ribbon takes focus.
        let draft = e.text_draft.as_mut().unwrap();
        draft.selection = 0..6;
        draft.selection_active = true;
        inline_frame(&ctx, &mut e, vec![key(egui::Key::B)]);
        let text = e
            .text_draft
            .as_ref()
            .unwrap()
            .preview
            .text
            .as_ref()
            .unwrap();
        assert!(capture_core::text::rich::format_at(text, 0).bold);
        assert!(!capture_core::text::rich::format_at(text, 9).bold);
        inline_frame(&ctx, &mut e, vec![key(egui::Key::Z)]);
        assert!(
            !capture_core::text::rich::format_at(
                e.text_draft
                    .as_ref()
                    .unwrap()
                    .preview
                    .text
                    .as_ref()
                    .unwrap(),
                0
            )
            .bold
        );
        inline_frame(&ctx, &mut e, vec![key(egui::Key::Y)]);
        assert!(
            capture_core::text::rich::format_at(
                e.text_draft
                    .as_ref()
                    .unwrap()
                    .preview
                    .text
                    .as_ref()
                    .unwrap(),
                0
            )
            .bold
        );
        crate::text_clipboard::set_cursor(&ctx, id, 6);
        inline_frame(&ctx, &mut e, vec![]);
        inline_frame(&ctx, &mut e, vec![key(egui::Key::I)]);
        inline_frame(&ctx, &mut e, vec![egui::Event::Text("!".into())]);
        let text = e
            .text_draft
            .as_ref()
            .unwrap()
            .preview
            .text
            .as_ref()
            .unwrap();
        assert_eq!(text.content.chars().nth(6), Some('!'));
        assert!(capture_core::text::rich::format_at(text, 6).italic);
        assert!(e.commit_text());
        e.session.document.validate().unwrap();
        e.execute(Command::Undo);
        assert!(
            e.session.document.annotations[0]
                .text
                .as_ref()
                .unwrap()
                .rich
                .is_none()
        );
    }

    fn inline_frame(
        ctx: &egui::Context,
        e: &mut Editor,
        events: Vec<egui::Event>,
    ) -> crate::property_layout::tests::PaintedOutput {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                e.finish_text_on_outside_click(ctx);
                if e.needs_render {
                    e.refresh_canvas_display(ctx);
                }
                e.show_canvas(ui);
            },
        );
        crate::property_layout::tests::PaintedOutput(output)
    }

    #[test]
    fn step_typing_paints_contrasting_or_custom_text_before_commit() {
        fn find_label(shape: &egui::Shape) -> Option<&egui::epaint::TextShape> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == "89" => Some(text),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(find_label),
                _ => None,
            }
        }
        for (fill, custom, expected) in [
            ([220, 40, 40, 255], None, [255, 255, 255, 255]),
            ([250, 225, 80, 255], None, [25, 25, 25, 255]),
            ([20, 50, 80, 180], None, [255, 255, 255, 180]),
            (
                [30, 70, 100, 255],
                Some([255, 230, 30, 255]),
                [255, 230, 30, 255],
            ),
        ] {
            let ctx = egui::Context::default();
            let mut e = crate::canvas::tests::editor(&ctx);
            let mut original =
                capture_core::steps::marker(1, [240.0, 180.0], 100, fill, (8, false), [640, 400])
                    .unwrap();
            original.text.as_mut().unwrap().color = custom;
            e.session.document.annotations.push(original.clone());
            e.selected = Some(1);
            e.tool = Some(Kind::Step);
            e.begin_text_edit();
            let first = inline_frame(&ctx, &mut e, vec![]);
            let output = inline_frame(&ctx, &mut e, vec![egui::Event::Text("9".into())]);
            let painted = output
                .shapes
                .iter()
                .find_map(|s| find_label(&s.shape))
                .expect("typed Step is painted immediately");
            let expected = egui::Color32::from_rgba_unmultiplied(
                expected[0],
                expected[1],
                expected[2],
                expected[3],
            );
            assert!(painted.override_text_color.is_none_or(|c| c == expected));
            let vertices: Vec<_> = painted
                .galley
                .rows
                .iter()
                .flat_map(|r| &r.visuals.mesh.vertices)
                .collect();
            assert!(!vertices.is_empty());
            assert!(vertices.iter().all(|v| v.color == expected));
            assert_eq!(e.session.document.annotations[0], original);
            assert!(!e.session.can_undo());
            let foreground =
                capture_core::steps::foreground(&e.text_draft.as_ref().unwrap().preview);
            let transform = e.view.transform(
                e.text_draft.as_ref().unwrap().surface.unwrap().1,
                egui::vec2(640.0, 400.0),
                1.0,
            );
            let style = foreground.text.as_ref().unwrap();
            assert!(
                (painted.galley.job.sections[0].format.font_id.size
                    - style.size_points * 4.0 / 3.0 * transform.scale)
                    .abs()
                    < 0.01
            );
            let rendered = e.preview_document().render().unwrap();
            assert!(e.commit_text());
            assert_eq!(e.session.document.render().unwrap(), rendered);
            assert_eq!(
                e.session.document.annotations[0]
                    .text
                    .as_ref()
                    .unwrap()
                    .size_points,
                original.text.as_ref().unwrap().size_points
            );
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("step.capture.json");
            e.session.save(&path).unwrap();
            assert_eq!(
                crate::Document::load(&path).unwrap().render().unwrap(),
                rendered
            );
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations[0], original);
            assert!(!e.session.can_undo());
            if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_STEP_DIR") {
                crate::property_layout::tests::dump_meshes(
                    &ctx,
                    &root,
                    &format!(
                        "fill-{}-{}-{}-custom-{}",
                        fill[0],
                        fill[1],
                        fill[2],
                        custom.is_some()
                    ),
                    first,
                    output,
                );
            }
        }
    }
    #[test]
    fn decorated_inline_typing_keeps_vector_ink_caret_and_atomic_undo() {
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        for mut a in fixture.annotations.into_iter().filter(|a| a.text.is_some()) {
            let ctx = egui::Context::default();
            let mut e = crate::canvas::tests::editor(&ctx);
            let text = a.text.as_mut().unwrap();
            text.content.clear();
            text.underline = true;
            text.strikethrough = true;
            text.color = Some([30, 80, 140, 96]);
            e.session.document.version = capture_core::MAX_PROJECT_VERSION;
            e.session.document.annotations = vec![a.clone()];
            e.selected = Some(a.id);
            e.tool = Some(a.kind);
            e.begin_text_edit();
            let mut first = inline_frame(&ctx, &mut e, vec![]);
            for ch in ["8", "9"] {
                let output = inline_frame(&ctx, &mut e, vec![egui::Event::Text(ch.into())]);
                for (id, deltas) in &output.0.textures_delta.set {
                    first
                        .0
                        .textures_delta
                        .set
                        .entry(*id)
                        .or_default()
                        .extend(deltas.clone());
                }
            }
            let output = inline_frame(&ctx, &mut e, vec![]);
            for (id, deltas) in &output.0.textures_delta.set {
                first
                    .0
                    .textures_delta
                    .set
                    .entry(*id)
                    .or_default()
                    .extend(deltas.clone());
            }
            let draft = e.text_draft.as_ref().unwrap();
            assert_eq!(draft.preview.text.as_ref().unwrap().content, "89");
            let id = egui::Id::unique(("text_content", a.id));
            let cursor = egui::TextEdit::load_state(&ctx, id)
                .unwrap()
                .cursor
                .char_range()
                .unwrap();
            assert_eq!(cursor.primary.index.0, 2);
            let canvas = e.canvas_document();
            assert_eq!(canvas.annotations[0].text.as_ref().unwrap().content, "89");
            assert_eq!(
                canvas.render().unwrap(),
                e.preview_document().render().unwrap()
            );
            assert_eq!(e.session.document.annotations, vec![a.clone()]);
            assert!(!e.session.can_undo());
            if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_TYPOGRAPHY_DIR") {
                let selected = inline_frame(
                    &ctx,
                    &mut e,
                    vec![egui::Event::Key {
                        key: egui::Key::A,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::CTRL,
                    }],
                );
                crate::property_layout::tests::dump_meshes(
                    &ctx,
                    &root,
                    &format!("{:?}", a.kind),
                    first,
                    selected,
                );
            }
            assert!(e.commit_text());
            assert_eq!(
                e.session.document.render().unwrap(),
                canvas.render().unwrap()
            );
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, vec![a]);
            assert!(!e.session.can_undo());
        }
    }

    #[test]
    fn rotated_text_entry_retains_angle_in_checkpoint_commit_and_one_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut a = capture_core::steps::marker(
            1,
            [240.0, 180.0],
            100,
            [40, 80, 100, 255],
            (8, false),
            [640, 400],
        )
        .unwrap();
        a.rotation_degrees = 37.0;
        e.session.document.version = capture_core::MAX_PROJECT_VERSION;
        e.session.document.annotations.push(a.clone());
        e.session = crate::Session::new(e.session.document.clone()).unwrap();
        e.tool = Some(Kind::Step);
        e.select_only(Some(1));
        e.begin_text_edit();
        assert!(
            !e.is_modified(),
            "opening upright editing frame is not an edit"
        );
        inline_frame(&ctx, &mut e, vec![]);
        inline_frame(&ctx, &mut e, vec![egui::Event::Text("9".into())]);
        assert_eq!(e.text_draft.as_ref().unwrap().preview.rotation_degrees, 0.0);
        let checkpoint = e.preview_document();
        assert_eq!(checkpoint.annotations[0].rotation_degrees, 37.0);
        assert_eq!(
            checkpoint.annotations[0].text.as_ref().unwrap().content,
            "89"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkpoint.capture.json");
        checkpoint.save(&path).unwrap();
        let reopened = crate::Document::load(&path).unwrap();
        assert_eq!(reopened.annotations, checkpoint.annotations);
        assert!(e.commit_text());
        assert_eq!(e.session.document.annotations, reopened.annotations);
        assert_eq!(
            e.session.document.render().unwrap(),
            checkpoint.render().unwrap()
        );
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, vec![a]);
        assert!(!e.session.can_undo());
    }

    #[test]
    fn middle_pan_during_inline_entry_keeps_draft_caret_and_single_undo() {
        use egui::{Event, Modifiers, PointerButton};
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        for kind in [
            Kind::Text,
            Kind::Callout,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
        ] {
            for inside_text in [true, false] {
                let ctx = egui::Context::default();
                let mut e = crate::canvas::tests::editor(&ctx);
                let mut object = fixture
                    .annotations
                    .iter()
                    .find(|a| a.kind == kind)
                    .unwrap()
                    .clone();
                object.text = Some(TextStyle {
                    content: String::new(),
                    ..Default::default()
                });
                let original = object.clone();
                e.session.document.annotations.push(object);
                e.selected = Some(original.id);
                e.tool = Some(kind);
                e.begin_text_edit();
                inline_frame(&ctx, &mut e, vec![]);
                inline_frame(&ctx, &mut e, vec![Event::Text("Before".into())]);
                let (before, viewport) = e.text_draft.as_ref().unwrap().surface.unwrap();
                let p = if inside_text {
                    before.center()
                } else {
                    viewport.min + egui::vec2(10.0, 10.0)
                };
                let q = p + egui::vec2(25.0, 20.0);
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Middle,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                inline_frame(&ctx, &mut e, vec![Event::PointerMoved(p), button(p, true)]);
                assert_eq!(
                    e.text_draft.as_ref().unwrap().surface.unwrap().0,
                    before,
                    "press cannot jump the canvas"
                );
                inline_frame(&ctx, &mut e, vec![Event::PointerMoved(q)]);
                assert!(e.view.panning, "{kind:?}, inside={inside_text}");
                let during = e.text_draft.as_ref().unwrap().surface.unwrap().0;
                assert!((during.min - before.min - (q - p)).length() < 0.01);
                assert_eq!(e.session.document.annotations[0], original);
                assert!(!e.session.can_undo());
                inline_frame(&ctx, &mut e, vec![button(q, false)]);
                assert!(!e.view.panning);
                inline_frame(&ctx, &mut e, vec![Event::Text(" after".into())]);
                assert_eq!(
                    e.text_draft
                        .as_ref()
                        .unwrap()
                        .preview
                        .text
                        .as_ref()
                        .unwrap()
                        .content,
                    "Before after",
                    "typing focus survives panning {kind:?}, inside={inside_text}"
                );
                assert_eq!(e.selected, Some(original.id));
                assert_eq!(e.tool, Some(kind));
                assert!(e.commit_text());
                assert!(e.session.can_undo());
                e.execute(Command::Undo);
                assert_eq!(e.session.document.annotations[0], original);
                assert!(!e.session.can_undo());
            }
        }
    }

    #[test]
    fn inline_resize_fit_is_live_and_commits_once_with_requested_font_preserved() {
        use capture_core::text::TextFit;
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut a = capture_core::steps::marker(
            1,
            [200.0, 200.0],
            44,
            [20, 60, 90, 255],
            (1, false),
            [640, 400],
        )
        .unwrap();
        a.kind = Kind::Text;
        a.start = [100.0, 100.0];
        a.end = [180.0, 140.0];
        a.text = Some(TextStyle {
            content: String::new(),
            fit: TextFit::ResizeShape,
            wrap: false,
            ..Default::default()
        });
        e.session.document.annotations.push(a.clone());
        e.tool = Some(Kind::Text);
        e.selected = Some(1);
        e.begin_text_edit();
        inline_frame(&ctx, &mut e, vec![]);
        inline_frame(
            &ctx,
            &mut e,
            vec![egui::Event::Text("A growing label".into())],
        );
        inline_frame(&ctx, &mut e, vec![]);
        let preview = e.text_draft.as_ref().unwrap().preview.clone();
        assert!(preview.end[0] - preview.start[0] > 80.0);
        assert_eq!(preview.text.as_ref().unwrap().size_points, 18.0);
        assert_eq!(e.session.document.annotations[0], a);
        assert!(!e.session.can_undo());
        let pixels = e.preview_document().render().unwrap();
        assert!(e.commit_text());
        assert_eq!(e.session.document.annotations[0], preview);
        assert_eq!(e.session.document.render().unwrap(), pixels);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations[0], a);
        assert!(!e.session.can_undo());
    }

    #[test]
    fn inline_shape_text_accepts_input_sidebar_alignment_outside_commit_and_one_undo() {
        for kind in [
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Text,
            Kind::Callout,
        ] {
            let ctx = egui::Context::default();
            let mut e = crate::canvas::tests::editor(&ctx);
            let original = Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.0,
                mirror_x: false,
                id: 1,
                kind,
                start: [100.0, 100.0],
                end: [300.0, 240.0],
                width: 3,
                color: [20, 90, 180, 255],
                fill: None,
                corner_radius: 0.0,
                points: if kind == Kind::Polygon {
                    capture_core::polygon::outline([100.0, 100.0], [300.0, 240.0], 3)
                } else {
                    vec![]
                },
                callout: (kind == Kind::Callout).then(|| {
                    capture_core::callout::CalloutStyle::for_box(
                        [100.0, 100.0],
                        [300.0, 240.0],
                        [640, 400],
                    )
                }),
                text: matches!(kind, Kind::Text | Kind::Callout).then(|| TextStyle {
                    content: String::new(),
                    ..Default::default()
                }),
            };
            e.session.document.annotations.push(original.clone());
            e.tool = Some(kind);
            e.selected = Some(1);
            e.begin_text_edit();
            inline_frame(&ctx, &mut e, vec![]);
            inline_frame(&ctx, &mut e, vec![egui::Event::Text("Inline".into())]);
            inline_frame(&ctx, &mut e, vec![egui::Event::Text(" label".into())]);
            assert_eq!(
                e.text_draft
                    .as_ref()
                    .unwrap()
                    .preview
                    .text
                    .as_ref()
                    .unwrap()
                    .content,
                "Inline label"
            );
            assert_eq!(e.session.document.annotations[0], original);
            assert!(!e.session.can_undo());
            e.apply_quick_justification(
                capture_core::quick_preferences::Justification::BottomRight,
            );
            assert_eq!(
                e.text_draft
                    .as_ref()
                    .unwrap()
                    .preview
                    .text
                    .as_ref()
                    .unwrap()
                    .vertical,
                VerticalAlignment::Bottom
            );
            assert!(!e.session.can_undo());
            let (rect, viewport) = e.text_draft.as_ref().unwrap().surface.unwrap();
            assert!(viewport.contains_rect(rect));
            let outside = viewport.min + egui::vec2(10.0, 10.0);
            inline_frame(
                &ctx,
                &mut e,
                vec![
                    egui::Event::PointerMoved(outside),
                    egui::Event::PointerButton {
                        pos: outside,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            assert!(e.text_draft.is_none());
            assert_eq!(
                e.session.document.annotations[0]
                    .text
                    .as_ref()
                    .unwrap()
                    .content,
                "Inline label"
            );
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("inline.capture.json");
            e.session.save(&path).unwrap();
            assert_eq!(
                crate::Document::load(&path).unwrap().annotations,
                e.session.document.annotations
            );
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations[0], original);
            assert!(!e.session.can_undo());
        }
    }

    #[test]
    fn inline_invalid_text_cannot_commit_or_destroy_redo_and_ctrl_enter_finishes() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = crate::Session::new(
            serde_json::from_str(include_str!(
                "../../../crates/core/tests/fixtures/translation.capture.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let id = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Text)
            .unwrap()
            .id;
        e.select_only(Some(id));
        e.begin_text_edit();
        inline_frame(&ctx, &mut e, vec![]);
        let previous = e
            .text_draft
            .as_ref()
            .unwrap()
            .preview
            .text
            .as_ref()
            .unwrap()
            .content
            .clone();
        e.text_draft
            .as_mut()
            .unwrap()
            .preview
            .text
            .as_mut()
            .unwrap()
            .content = "\u{0001}".into();
        assert!(!e.commit_text());
        assert!(e.text_draft.is_some());
        assert!(!e.session.can_undo());
        e.text_draft
            .as_mut()
            .unwrap()
            .preview
            .text
            .as_mut()
            .unwrap()
            .content = previous + " edited";
        inline_frame(
            &ctx,
            &mut e,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        assert!(e.text_draft.is_none());
        assert!(e.session.can_undo());
        e.execute(Command::Undo);
        assert!(e.session.can_redo());
        e.begin_text_edit();
        e.text_draft
            .as_mut()
            .unwrap()
            .preview
            .text
            .as_mut()
            .unwrap()
            .content = "\u{0001}".into();
        assert!(!e.commit_text());
        assert!(e.session.can_redo());
    }

    #[test]
    fn inline_keyboard_selection_replace_and_cancel_keep_original_and_redo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = crate::Session::new(
            serde_json::from_str(include_str!(
                "../../../crates/core/tests/fixtures/translation.capture.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let object = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Text)
            .unwrap()
            .clone();
        e.select_only(Some(object.id));
        e.begin_text_edit();
        inline_frame(&ctx, &mut e, vec![]);
        inline_frame(
            &ctx,
            &mut e,
            vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }],
        );
        inline_frame(&ctx, &mut e, vec![egui::Event::Text("Replacement".into())]);
        assert_eq!(
            e.text_draft
                .as_ref()
                .unwrap()
                .preview
                .text
                .as_ref()
                .unwrap()
                .content,
            "Replacement"
        );
        inline_frame(
            &ctx,
            &mut e,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(e.text_draft.is_none());
        assert_eq!(
            e.session
                .document
                .annotations
                .iter()
                .find(|a| a.id == object.id)
                .unwrap(),
            &object
        );
        assert!(!e.session.can_undo());
    }

    fn frame(ctx: &egui::Context, escape: bool) -> (egui::Id, bool) {
        let events = if escape {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        } else {
            vec![]
        };
        let mut result = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(981.0, 687.0),
                )),
                ..Default::default()
            },
            |_| {
                let mut popup_id = None;
                let mut color = [25, 85, 160, 200];
                let modal =
                    crate::controls::Modal::new(egui::Id::unique("escape_test")).show(ctx, |ui| {
                        ui.label("Keep the enclosing text draft");
                        let response = crate::color_widgets::rgba(ui, &mut color);
                        popup_id = Some(response.id.with("rgba-popup"));
                    });
                assert_eq!(color, [25, 85, 160, 200]);
                result = Some((popup_id.unwrap(), cancel_on_escape(&modal)));
            },
        );
        output.textures_delta.clear();
        result.unwrap()
    }

    #[test]
    fn escape_closes_color_popup_before_enclosing_text_draft() {
        let ctx = egui::Context::default();
        frame(&ctx, false);
        let (popup, cancel) = frame(&ctx, false);
        assert!(!cancel);
        egui::Popup::open_id(&ctx, popup);
        assert!(!frame(&ctx, false).1);
        assert!(egui::Popup::is_id_open(&ctx, popup));
        assert!(
            !frame(&ctx, true).1,
            "the enclosing text draft must survive"
        );
        assert!(!egui::Popup::is_id_open(&ctx, popup));
        assert!(
            frame(&ctx, true).1,
            "a second Escape cancels the text draft"
        );
    }
}

#[cfg(test)]
mod capture_refinement_tests {
    #[test]
    fn controls_below_canvas_do_not_end_inline_text_editing() {
        let ctx = egui::Context::default();
        let mut e = crate::rich_text::tests::editor(&ctx);
        e.begin_text_edit();
        let viewport = egui::Rect::from_min_max(egui::pos2(0., 0.), egui::pos2(800., 500.));
        let text = egui::Rect::from_min_max(egui::pos2(100., 100.), egui::pos2(300., 150.));
        e.text_draft.as_mut().unwrap().surface = Some((text, viewport));
        for pos in [
            egui::pos2(850., 300.),
            egui::pos2(850., 700.),
            egui::pos2(400., 700.),
        ] {
            let _output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |_| e.finish_text_on_outside_click(&ctx),
            ));
            assert!(e.text_draft.is_some());
            let _output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |_| {},
            ));
        }
    }
}
