//! Resolve editor bindings once, then reuse the established command handlers.
use crate::{Editor, egui};
use capture_core::{
    editor_keybinds::{Action, Function, Keybinds},
    keybinds::Chord,
    toolbar::ToolId,
};

fn event(chord: Chord, repeat: bool) -> egui::Event {
    let key = match chord.key {
        0x08 => egui::Key::Backspace,
        0xbb => egui::Key::Equals,
        0xbd => egui::Key::Minus,
        0x2e => egui::Key::Delete,
        0x30 => egui::Key::Num0,
        k => egui::Key::from_name(&char::from_u32(k.into()).unwrap().to_string()).unwrap(),
    };
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers: egui::Modifiers {
            ctrl: chord.modifiers & 2 != 0,
            command: chord.modifiers & 2 != 0,
            alt: chord.modifiers & 1 != 0,
            shift: chord.modifiers & 4 != 0,
            ..Default::default()
        },
    }
}

/// Transform local bindings into canonical command events. Unassigned old keys
/// cannot fall through and invoke their former action. Clipboard platform events
/// are resolved through the same bindings without touching the OS clipboard.
fn resolve(events: &mut Vec<egui::Event>, keys: &Keybinds) -> Vec<Action> {
    let mut direct = Vec::new();
    let mut output = Vec::new();
    let mut emitted = Vec::new();
    let mut used_tool = false;
    for e in events.drain(..) {
        let (chord, repeat) = match &e {
            egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                repeat,
                ..
            } => {
                let Some(mut c) = crate::options::chord(*key, *modifiers) else {
                    output.push(e);
                    continue;
                };
                // Ctrl+'+' is the standard shifted alias for Ctrl+'='.
                if c.key == 0xbb && c.modifiers == 6 && keys.action(c).is_none() {
                    c.modifiers = 2;
                }
                (c, *repeat)
            }
            egui::Event::Copy => (Function::Copy.default_chord(), false),
            egui::Event::Cut => (Function::Cut.default_chord(), false),
            egui::Event::Paste(_) | egui::Event::PasteImage(_) => {
                (Function::Paste.default_chord(), false)
            }
            _ => {
                output.push(e);
                continue;
            }
        };
        match keys.action(chord) {
            Some(a @ Action::Tool(_)) => {
                used_tool = true;
                if !repeat && !direct.contains(&a) {
                    direct.push(a);
                }
            }
            Some(a @ Action::Function(Function::Group | Function::Ungroup)) => {
                if !repeat && !direct.contains(&a) {
                    direct.push(a);
                }
            }
            Some(Action::Function(f)) => {
                if !emitted.contains(&f) {
                    output.push(event(f.default_chord(), repeat));
                    emitted.push(f);
                }
            }
            None if Function::ALL.iter().any(|f| f.default_chord() == chord) => {}
            None => output.push(e),
        }
    }
    if used_tool {
        output.retain(|e| !matches!(e, egui::Event::Text(_)));
    }
    *events = output;
    direct
}
impl Editor {
    pub(super) fn editor_keybind_input(&mut self, ctx: &egui::Context) {
        // A crop selection may be abandoned by choosing another tool; gestures,
        // text/numeric entries, dialogs and popup menus retain their input.
        let crop = self.crop.take();
        let idle = self.editor_shortcuts_idle();
        self.crop = crop;
        if !idle
            || self.text_draft.is_some()
            || ctx.egui_wants_keyboard_input()
            || self.shell.help
            || egui::Popup::is_any_open(ctx)
            || self.view.panning
            || self.capture_requested
        {
            return;
        }
        let keys = self.quick_controls.keybinds().editor;
        let actions = ctx.input_mut(|i| resolve(&mut i.events, &keys));
        for action in actions {
            match action {
                Action::Tool(id) => {
                    if let Some(index) = crate::shell::TOOLS.iter().position(|t| t.id == id) {
                        self.choose_tool(index);
                    }
                }
                Action::Function(f) => {
                    let action = if f == Function::Group {
                        crate::ribbon_settings::Action::Group
                    } else {
                        crate::ribbon_settings::Action::Ungroup
                    };
                    if self.shell.canvas_enabled() {
                        self.run_settings_action(action);
                    }
                }
            }
        }
    }
    pub(super) fn tool_tooltip(&self, id: ToolId) -> String {
        let name = crate::shell::TOOLS
            .iter()
            .find(|t| t.id == id)
            .unwrap()
            .name;
        let shortcut = self
            .quick_controls
            .keybinds()
            .editor
            .get(Action::Tool(id))
            .map(|c| format!(" ({})", c.label()))
            .unwrap_or_default();
        let description = match id {
            ToolId::Move => "Select and move objects.",
            ToolId::Text => "Draw a text box.",
            ToolId::Shape => "Draw a shape.",
            ToolId::Step => "Place the next step marker.",
            ToolId::Blur => "Blur or pixelate an area.",
            ToolId::Selection => "Select image pixels.",
            ToolId::Pen => "Draw a freehand stroke.",
            ToolId::Callout => "Draw a callout.",
            ToolId::Highlighter => "Highlight text or an area.",
            ToolId::Redact => "Cover an area with solid black.",
            ToolId::Arrow => "Draw an arrow.",
            ToolId::Line => "Draw a line.",
            ToolId::Fill => "Fill a connected color area.",
            ToolId::Simplify => "Turn image details into editable blocks.",
            ToolId::Crop => "Crop the image.",
            ToolId::CutOut => "Remove a strip of the image.",
            ToolId::Eraser => "Erase image pixels.",
            _ => "",
        };
        format!("{name}{shortcut}\n{description}").trim().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_group_and_ungroup_keys_use_one_undo_and_persist_membership() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session = capture_core::Session::new(
            serde_json::from_str(include_str!(
                "../../../crates/core/tests/fixtures/properties.capture.json"
            ))
            .unwrap(),
        )
        .unwrap();
        e.tool = None;
        e.select_all_matching();
        let count = e.selected_objects().len();
        assert!(count > 1);
        for f in [Function::Group, Function::Ungroup] {
            let mut o = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![
                        event(f.default_chord(), false),
                        egui::Event::Key {
                            key: egui::Key::G,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| e.editor_keybind_input(ui.ctx()),
            );
            o.textures_delta.clear();
            assert!(
                e.session
                    .document
                    .annotations
                    .iter()
                    .all(|a| a.group.is_some() == (f == Function::Group))
            );
            e.execute(crate::Command::Undo);
            assert!(
                e.session
                    .document
                    .annotations
                    .iter()
                    .all(|a| a.group.is_some() == (f == Function::Ungroup))
            );
            e.execute(crate::Command::Redo);
        }
        assert!(!e.shell.pixel_grid);
    }
    #[test]
    fn remapped_commands_do_not_fall_through_and_clipboard_events_are_deduplicated() {
        let mut keys = Keybinds::default();
        keys.set(
            Action::Function(Function::Copy),
            Some(Chord {
                key: 0x51,
                modifiers: 2,
            }),
        );
        let mut events = vec![
            egui::Event::Copy,
            event(
                Chord {
                    key: 0x43,
                    modifiers: 2,
                },
                false,
            ),
            event(
                Chord {
                    key: 0x51,
                    modifiers: 2,
                },
                false,
            ),
        ];
        assert!(resolve(&mut events, &keys).is_empty());
        assert_eq!(events, vec![event(Function::Copy.default_chord(), false)]);
        keys.set(Action::Function(Function::Copy), None);
        resolve(&mut events, &keys);
        assert!(events.is_empty());
        let mut events = vec![
            event(
                Chord {
                    key: 0x56,
                    modifiers: 0,
                },
                true,
            ),
            egui::Event::Text("v".into()),
        ];
        assert!(resolve(&mut events, &keys).is_empty());
        assert!(events.is_empty());
    }
    #[test]
    fn tools_use_real_selection_handlers_and_text_focus_preserves_input() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for (tool, key) in capture_core::editor_keybinds::TOOL_DEFAULTS {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![event(Chord { key, modifiers: 0 }, false)],
                    focused: true,
                    ..Default::default()
                },
                |ui| e.editor_keybind_input(ui.ctx()),
            );
            output.textures_delta.clear();
            assert_eq!(e.current_tool().id, tool);
        }
        e.cancel_gesture();
        e.shell.planned_tool = None;
        e.tool = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![event(
                    Chord {
                        key: 0x54,
                        modifiers: 0,
                    },
                    false,
                )],
                focused: true,
                ..Default::default()
            },
            |ui| {
                let id = egui::Id::unique("typing-field");
                ui.memory_mut(|m| m.request_focus(id));
                ui.add(egui::TextEdit::singleline(&mut String::new()).id(id));
                e.editor_keybind_input(ui.ctx());
            },
        );
        output.textures_delta.clear();
        assert_eq!(e.tool, None);
        assert!(e.tool_tooltip(ToolId::Move).starts_with("Move (V)"));
    }
}
