//! A temporary presentation state; pane choices are retained, not rewritten.
use crate::{Editor, egui};

impl Editor {
    pub(super) fn preview_ui_input(&mut self, ctx: &egui::Context) {
        // Crop can remain selected; only an active crop drag owns this key.
        let crop = self.crop.take();
        let idle = self.editor_shortcuts_idle();
        self.crop = crop;
        if !idle
            || !self.shell.canvas_enabled()
            || self.text_draft.is_some()
            || ctx.text_edit_focused()
            || egui::Popup::is_any_open(ctx)
            || self.shell.help
            || self.view.panning
            || self.capture_requested
            || self.stamps.redact_drag.is_some()
            || self.crop.as_ref().is_some_and(|c| c.dragging())
            || ctx.input(|i| i.pointer.any_down() || !i.focused)
        {
            return;
        }
        let tab = ctx.input(|i| {
            i.events.iter().any(|e| {
                matches!(e,
            egui::Event::Key { key: egui::Key::Tab, pressed: true, modifiers, .. }
                if modifiers.matches_exact(egui::Modifiers::NONE))
            })
        });
        if tab {
            // egui schedules focus navigation before the UI consumes events.
            ctx.memory_mut(|m| {
                m.move_focus(egui::FocusDirection::None);
                m.stop_text_input();
            });
            if ctx.input_mut(|i| {
                crate::shortcuts::consume_exact(i, egui::Modifiers::NONE, egui::Key::Tab)
            }) {
                self.shell.preview_ui = !self.shell.preview_ui;
                ctx.request_repaint();
            }
        }
    }

    pub(super) fn show_ui_button(&mut self, ui: &mut egui::Ui) {
        if !self.shell.preview_ui {
            return;
        }
        let area = egui::Rect::from_min_size(
            egui::pos2(ui.max_rect().right() - 86., ui.min_rect().top() + 2.),
            egui::vec2(80., 30.),
        );
        let r = ui
            .scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                ui.add_sized(
                    area.size(),
                    egui::Button::new("Show UI").fill(crate::shell::ACCENT),
                )
            })
            .inner
            .on_hover_text("Restore the ribbon and panes · Tab");
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("show-ui-button"), r.rect));
        if r.clicked() {
            self.shell.preview_ui = false;
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::PaintedOutput;

    fn tab(modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }
    fn release_tab() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn frame(
        ctx: &egui::Context,
        e: &mut Editor,
        events: Vec<egui::Event>,
    ) -> (PaintedOutput, egui::Rect) {
        let mut canvas = egui::Rect::NOTHING;
        let out = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280., 800.),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ui| {
                e.show_shell(ui);
                canvas = ui.available_rect_before_wrap();
                ui.painter()
                    .rect_filled(canvas.shrink(20.), 0., egui::Color32::WHITE);
            },
        ));
        (out, canvas)
    }

    #[test]
    fn tab_reclaims_workspace_restores_prior_panes_and_show_ui_button_recovers() {
        for properties in [false, true] {
            for recent in [false, true] {
                let ctx = egui::Context::default();
                crate::shell::configure(&ctx);
                let mut e = crate::canvas::tests::editor(&ctx);
                e.shell.badges = false;
                e.shell.properties = properties;
                e.history.recent = recent;
                let original = serde_json::to_vec(&e.session.document).unwrap();
                frame(&ctx, &mut e, vec![]);
                let (_, before) = frame(&ctx, &mut e, vec![]);
                frame(&ctx, &mut e, vec![tab(egui::Modifiers::NONE, false)]);
                assert!(e.shell.preview_ui);
                let (_, hidden) = frame(&ctx, &mut e, vec![]);
                assert!(hidden.top() < before.top() - 60.);
                if properties {
                    assert!(hidden.width() > before.width() + 250.);
                }
                if recent {
                    assert!(
                        hidden.bottom() > before.bottom() + 10.,
                        "{before:?} -> {hidden:?}"
                    );
                }
                assert_eq!(e.shell.properties, properties);
                assert_eq!(e.history.recent, recent);
                frame(&ctx, &mut e, vec![tab(egui::Modifiers::NONE, true)]);
                assert!(e.shell.preview_ui, "key repeat must not flicker");
                frame(&ctx, &mut e, vec![release_tab()]);
                frame(&ctx, &mut e, vec![tab(egui::Modifiers::NONE, false)]);
                let (_, restored) = frame(&ctx, &mut e, vec![]);
                assert!(!e.shell.preview_ui);
                assert_eq!(before, restored);
                frame(&ctx, &mut e, vec![release_tab()]);
                frame(&ctx, &mut e, vec![tab(egui::Modifiers::NONE, false)]);
                let rect = ctx.data(|d| {
                    d.get_temp::<egui::Rect>(egui::Id::unique("show-ui-button"))
                        .unwrap()
                });
                assert!(rect.right() > 1250. && rect.top() < 50., "{rect:?}");
                if properties
                    && recent
                    && let Some(dir) = std::env::var_os("CLIPCRAFT_HEADLESS_PREVIEW_DIR")
                {
                    let preview_ctx = egui::Context::default();
                    crate::shell::configure(&preview_ctx);
                    let mut preview = crate::canvas::tests::editor(&preview_ctx);
                    preview.shell.preview_ui = true;
                    preview.shell.badges = false;
                    let first = frame(&preview_ctx, &mut preview, vec![]).0;
                    let second = frame(&preview_ctx, &mut preview, vec![]).0;
                    crate::property_layout::tests::dump_meshes(
                        &preview_ctx,
                        &dir.to_string_lossy(),
                        "preview-ui",
                        first,
                        second,
                    );
                }
                let button = |pressed| egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                frame(
                    &ctx,
                    &mut e,
                    vec![egui::Event::PointerMoved(rect.center()), button(true)],
                );
                frame(&ctx, &mut e, vec![button(false)]);
                assert!(!e.shell.preview_ui);
                assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
                assert!(!e.session.can_undo());
            }
        }
    }

    #[test]
    fn tab_retains_normal_focus_navigation_for_fields_dialogs_and_modifier_chords() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        frame(&ctx, &mut e, vec![tab(egui::Modifiers::SHIFT, false)]);
        assert!(!e.shell.preview_ui);
        e.begin_export(false);
        frame(&ctx, &mut e, vec![tab(egui::Modifiers::NONE, false)]);
        assert!(!e.shell.preview_ui);
        e.export_dialog = None;
        let mut text = String::new();
        let mut run = |events| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let r = ui.text_edit_singleline(&mut text);
                    r.request_focus();
                    e.preview_ui_input(ui.ctx());
                },
            ))
        };
        run(vec![]);
        run(vec![tab(egui::Modifiers::NONE, false)]);
        assert!(!e.shell.preview_ui);
    }
}
