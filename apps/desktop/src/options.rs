use crate::{Editor, egui};
use capture_core::keybinds::{Action, Chord, Keybinds};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum ShortcutAction {
    Capture(Action),
    Editor(capture_core::editor_keybinds::Action),
}
impl ShortcutAction {
    fn label(self) -> &'static str {
        match self {
            Self::Capture(a) => a.label(),
            Self::Editor(capture_core::editor_keybinds::Action::Tool(id)) => {
                crate::shell::TOOLS
                    .iter()
                    .find(|t| t.id == id)
                    .unwrap()
                    .name
            }
            Self::Editor(capture_core::editor_keybinds::Action::Function(f)) => f.label(),
        }
    }
    fn get(self, keys: &Keybinds) -> Option<Chord> {
        match self {
            Self::Capture(a) => keys.get(a),
            Self::Editor(a) => keys.editor.get(a),
        }
    }
    fn set(self, keys: &mut Keybinds, chord: Option<Chord>) {
        match self {
            Self::Capture(a) => keys.set(a, chord),
            Self::Editor(a) => keys.editor.set(a, chord),
        }
    }
}

#[derive(Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    General,
    Keybinds,
}
#[derive(Default)]
pub(super) struct Options {
    pub tab: Tab,
    pub recording: Option<ShortcutAction>,
    draft: Option<Keybinds>,
    message: String,
    error: bool,
}
impl Options {
    pub fn record(&mut self, chord: Chord) {
        if let Some(action) = self.recording {
            let mut next = self.draft.clone().unwrap_or_default();
            action.set(&mut next, Some(chord));
            match next.validate() {
                Ok(()) => {
                    self.draft = Some(next);
                    self.recording = None;
                    self.message = "Combination recorded. Save shortcuts to apply it.".into();
                    self.error = false;
                }
                Err(e) => {
                    self.message = e.to_string();
                    self.error = true;
                }
            }
        }
    }
}
pub(super) fn status(keys: &Keybinds) -> String {
    if !keys.enabled {
        "Capture keybinds are disabled.".into()
    } else {
        format!(
            "{} capture keybind(s) active while ClipCraft is running.",
            keys.bindings.len()
        )
    }
}
impl Editor {
    pub(super) fn process_keybind_input(&mut self, ctx: &egui::Context) {
        if self.shell.backstage != Some(crate::backstage::Page::Options)
            || self.options.tab != Tab::Keybinds
        {
            self.options.recording = None;
            return;
        }
        if self.options.recording.is_none() {
            return;
        }
        let events = ctx.input_mut(|i| {
            let events = i.events.clone();
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { .. }
                        | egui::Event::Text(_)
                        | egui::Event::Copy
                        | egui::Event::Cut
                        | egui::Event::Paste(_)
                )
            });
            events
        });
        for event in events {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            {
                if key == egui::Key::Escape {
                    self.options.recording = None;
                    self.options.message.clear();
                    break;
                }
                if let Some(chord) = chord(key, modifiers) {
                    self.options.record(chord);
                } else {
                    self.options.message =
                        "Use a letter, number, function key, Print Screen or navigation key."
                            .into();
                    self.options.error = true;
                }
                break;
            }
        }
    }
    fn save_keybinds(&mut self, ctx: &egui::Context) {
        let Some(keys) = self.options.draft.clone() else {
            return;
        };
        let saved = self.quick_controls.keybinds();
        if keys.enabled == saved.enabled && keys.bindings == saved.bindings {
            match keys
                .validate()
                .map_err(|e| e.to_string())
                .and_then(|()| self.quick_controls.save_keybinds(keys))
            {
                Ok(()) => {
                    self.options.message = "Shortcuts saved.".into();
                    self.options.error = false;
                }
                Err(e) => {
                    self.options.message = e;
                    self.options.error = true;
                }
            }
            return;
        }
        self.suspend_global_capture();
        let result = (|| {
            if self.global_capture.is_none() {
                // Start an empty worker; old preferences must not prevent repairing
                // an existing conflict. The staged update performs registration.
                let empty = Keybinds {
                    enabled: false,
                    bindings: vec![],
                    ..Default::default()
                };
                self.global_capture = Some(crate::hotkey::CaptureHotkey::start(
                    ctx.clone(),
                    self.hotkey_owner,
                    &empty,
                )?);
            }
            crate::hotkey::apply(self.global_capture.as_mut().unwrap(), &keys, || {
                self.quick_controls.save_keybinds(keys.clone())
            })
        })();
        match result {
            Ok(()) => {
                self.global_capture_status = status(&keys);
                self.options.message = "Shortcuts saved.".into();
                self.options.error = false;
                self.status = self.global_capture_status.clone();
            }
            Err(e) => {
                self.options.message = e;
                self.options.error = true;
            }
        }
    }
    fn save_lifecycle_option(&mut self, choice: capture_core::quick_preferences::Choice) {
        match self.quick_controls.save_lifecycle(choice) {
            Ok(()) => self.status = "Startup and closing preferences saved.".into(),
            Err(e) => {
                self.lifecycle.error = e;
            }
        }
    }
    pub(super) fn show_options(&mut self, ui: &mut egui::Ui) {
        ui.heading("Options");
        ui.add_space(12.0);
        crate::tabs::group(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [(Tab::General, "General"), (Tab::Keybinds, "Keybinds")] {
                    if crate::tabs::tab(ui, label, self.options.tab == tab, 128.0).clicked() {
                        self.options.tab = tab;
                        self.options.recording = None;
                    }
                }
            })
        });
        ui.add_space(18.0);
        if self.options.tab == Tab::General {
            ui.strong("Startup & closing");
            ui.add_space(6.);
            let mut close = self.quick_controls.close_to_tray();
            if crate::controls::toggle(ui, &mut close, "Close to tray").on_hover_text("Keep ClipCraft running and capture shortcuts active when its windows close. Use File > Exit or the tray menu to quit.").changed() {
                self.save_lifecycle_option(capture_core::quick_preferences::Choice::CloseToTray(close));
            }
            ui.horizontal(|ui| {
                ui.label("Open on startup");
                let mut startup = self.quick_controls.startup_window();
                let before = startup;
                use capture_core::quick_preferences::StartupWindow;
                crate::tabs::group(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (value, label) in [
                            (StartupWindow::Editor, "Editor"),
                            (StartupWindow::CaptureController, "Capture controller"),
                        ] {
                            if crate::tabs::tab(ui, label, startup == value, 148.).clicked() {
                                startup = value;
                            }
                        }
                    })
                });
                if startup != before {
                    self.save_lifecycle_option(
                        capture_core::quick_preferences::Choice::StartupWindow(startup),
                    );
                }
            });
            if !self.lifecycle.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.lifecycle.error);
            }
            ui.add_space(14.);
            ui.separator();
            ui.add_space(6.);
            ui.strong("Editor");
            let mut auto = self.quick_controls.auto_switch_tool();
            if crate::controls::toggle(
                ui,
                &mut auto,
                "Automatically switch tools when selecting objects",
            )
            .changed()
            {
                self.quick_controls.set_auto_switch_tool(auto);
            }
            let mut snap = self.quick_controls.snapping();
            if crate::controls::toggle(ui, &mut snap, "Canvas snapping").changed() {
                self.quick_controls.set_snapping(snap);
            }
            crate::controls::toggle(ui, &mut self.shell.badges, "Show development badges");
            return;
        }
        let saved = self.quick_controls.keybinds();
        let draft = self.options.draft.get_or_insert_with(|| saved.clone());
        crate::controls::toggle(ui, &mut draft.enabled, "Enable global capture keybinds");
        ui.label("Select a shortcut, press your combination, then Save shortcuts.");
        ui.small("Mode shortcuts use your saved cursor, delay, clipboard and preview settings.");
        ui.add_space(10.0);
        let columns = [
            (
                "Capture",
                Action::ALL
                    .into_iter()
                    .map(ShortcutAction::Capture)
                    .collect::<Vec<_>>(),
            ),
            (
                "Tools",
                crate::shell::TOOLS
                    .iter()
                    .filter(|t| t.id != capture_core::toolbar::ToolId::Favorites)
                    .map(|t| {
                        ShortcutAction::Editor(capture_core::editor_keybinds::Action::Tool(t.id))
                    })
                    .collect(),
            ),
            (
                "Functions",
                capture_core::editor_keybinds::Function::ALL
                    .into_iter()
                    .map(|f| {
                        ShortcutAction::Editor(capture_core::editor_keybinds::Action::Function(f))
                    })
                    .collect(),
            ),
        ];
        let height = (ui.available_height() - 170.).clamp(130., 570.);
        egui::ScrollArea::vertical()
            .id_salt("shortcut_columns")
            .auto_shrink([false, false])
            .max_height(height)
            .show(ui, |ui| {
                ui.columns(3, |uis| {
                    for (ui, (title, actions)) in uis.iter_mut().zip(columns) {
                        ui.spacing_mut().item_spacing.y = 5.;
                        ui.strong(title);
                        ui.separator();
                        for action in actions {
                            ui.push_id(action, |ui| {
                                ui.label(action.label());
                                let listening = self.options.recording == Some(action);
                                let label = if listening {
                                    "Press keys…".into()
                                } else {
                                    action
                                        .get(draft)
                                        .map(Chord::label)
                                        .unwrap_or_else(|| "Not assigned".into())
                                };
                                let (record, clear) = shortcut_field(
                                    ui,
                                    &label,
                                    action.label(),
                                    listening,
                                    action.get(draft).is_some(),
                                );
                                if record.clicked() {
                                    self.options.recording = Some(action);
                                    self.options.message.clear();
                                    self.options.error = false;
                                }
                                if clear.clicked() {
                                    action.set(draft, None);
                                    self.options.recording = None;
                                }
                                ui.add_space(8.);
                            });
                        }
                    }
                });
            });
        let dirty = *draft != saved;
        if self.options.recording.is_some() {
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label("Listening · Esc to cancel");
                if ui
                    .button("Use Print Screen")
                    .on_hover_text(
                        "Assign Print Screen if your keyboard or Windows intercepts the key press",
                    )
                    .clicked()
                {
                    self.options.record(Chord {
                        key: 0x2c,
                        modifiers: 0,
                    });
                }
                if ui.button("Cancel recording").clicked() {
                    self.options.recording = None;
                }
            });
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.options.recording.is_none(),
                    egui::Button::new("Save shortcuts").min_size(egui::vec2(134.0, 34.0)),
                )
                .clicked()
            {
                self.save_keybinds(ui.ctx());
            }
            if ui
                .add_enabled(dirty, egui::Button::new("Discard changes"))
                .clicked()
            {
                self.options.draft = Some(saved);
                self.options.recording = None;
                self.options.message.clear();
            }
            if ui.button("Restore defaults").clicked() {
                self.options.draft = Some(Keybinds::default());
                self.options.recording = None;
                self.options.message = "Default shortcuts loaded. Save shortcuts to apply.".into();
                self.options.error = false;
            }
            if dirty {
                ui.label("Unsaved changes");
            }
        });
        if !self.options.message.is_empty() {
            ui.add_space(8.0);
            ui.colored_label(
                if self.options.error {
                    egui::Color32::from_rgb(255, 153, 153)
                } else {
                    egui::Color32::from_rgb(153, 220, 180)
                },
                &self.options.message,
            );
        }
        ui.add_space(12.0);
        ui.label(&self.global_capture_status);
        ui.small("Print Screen may require turning off Windows’ screen-snipping shortcut in Settings → Accessibility → Keyboard. ClipCraft does not change Windows settings.");
    }
}
pub(super) fn chord(key: egui::Key, m: egui::Modifiers) -> Option<Chord> {
    use egui::Key;
    let name = key.name();
    let key = match key {
        Key::Equals | Key::Plus => 0xbb,
        Key::Minus => 0xbd,
        Key::PrintScreen => 0x2c,
        Key::Space => 0x20,
        Key::Backspace => 0x08,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::End => 0x23,
        Key::Home => 0x24,
        Key::ArrowLeft => 0x25,
        Key::ArrowUp => 0x26,
        Key::ArrowRight => 0x27,
        Key::ArrowDown => 0x28,
        Key::Insert => 0x2d,
        Key::Delete => 0x2e,
        _ if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() => {
            u16::from(name.as_bytes()[0].to_ascii_uppercase())
        }
        _ => {
            let f = name.strip_prefix('F')?.parse::<u16>().ok()?;
            if !(1..=24).contains(&f) {
                return None;
            }
            0x6f + f
        }
    };
    Some(Chord {
        key,
        modifiers: u8::from(m.alt)
            | (u8::from(m.ctrl) * 2)
            | (u8::from(m.shift) * 4)
            | (u8::from(m.mac_cmd) * 8),
    })
}

#[cfg(test)]
mod tests;

/// Two disjoint hit targets share one compact field, so Clear cannot start recording.
fn shortcut_field(
    ui: &mut egui::Ui,
    label: &str,
    action: &str,
    listening: bool,
    assigned: bool,
) -> (egui::Response, egui::Response) {
    let width = (ui.available_width() - 22.).clamp(100., 218.);
    let (rect, allocated) = ui.allocate_exact_size(egui::vec2(width, 30.), egui::Sense::hover());
    let id = allocated.id;
    let clear_rect = egui::Rect::from_min_max(
        egui::pos2(rect.right() - 28., rect.top() + 2.),
        rect.max - egui::vec2(2., 2.),
    );
    let record_rect =
        egui::Rect::from_min_max(rect.min, egui::pos2(clear_rect.left(), rect.bottom()));
    let record = ui.interact(record_rect, id, egui::Sense::click());
    let clear = ui
        .add_enabled_ui(assigned, |ui| {
            ui.interact(clear_rect, id.with("clear"), egui::Sense::click())
        })
        .inner;
    let fill = if listening {
        crate::shell::ACCENT
    } else if record.hovered() {
        egui::Color32::from_rgb(79, 91, 97)
    } else {
        egui::Color32::from_rgb(65, 71, 74)
    };
    ui.painter().rect(
        rect,
        4,
        fill,
        egui::Stroke::new(
            1.,
            if record.has_focus() {
                crate::shell::ACCENT
            } else {
                egui::Color32::from_gray(115)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if clear.hovered() {
        ui.painter()
            .rect_filled(clear_rect, 3, egui::Color32::from_rgb(95, 103, 108));
    }
    ui.painter()
        .with_clip_rect(record_rect.shrink2(egui::vec2(7., 0.)))
        .text(
            record_rect.left_center() + egui::vec2(8., 0.),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.),
            egui::Color32::WHITE,
        );
    let c = clear_rect.center();
    let stroke = egui::Stroke::new(
        1.3,
        if assigned {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_gray(115)
        },
    );
    for sign in [-1., 1.] {
        ui.painter().line_segment(
            [
                c + egui::vec2(-3.5, sign * 3.5),
                c + egui::vec2(3.5, -sign * 3.5),
            ],
            stroke,
        );
    }
    record.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            true,
            format!("Set {action} shortcut: {label}"),
        )
    });
    clear.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            assigned,
            format!("Clear {action} shortcut"),
        )
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("shortcut-field", action)),
            (rect, record_rect, clear_rect),
        )
    });
    (
        record.on_hover_text(format!("Set {action} shortcut")),
        clear.on_hover_text(format!("Clear {action} shortcut")),
    )
}
