use crate::tips::Tip as _;
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

#[derive(Default)]
pub(super) struct Options {
    pub category: usize,
    pub recording: Option<ShortcutAction>,
    pub draft: Option<Keybinds>,
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
            "{} capture keybind(s) active while ClipDesigner is running.",
            keys.bindings.len()
        )
    }
}
impl Editor {
    pub(super) fn process_keybind_input(&mut self, ctx: &egui::Context) {
        if self.preferences.section() != Some(crate::preferences::Section::Shortcuts) {
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
    /// Apply in Preferences saves shortcut changes that weren't saved here.
    pub(super) fn apply_shortcut_draft(&mut self, ctx: &egui::Context) {
        if self.options.draft.as_ref() != Some(&self.quick_controls.keybinds()) {
            self.save_keybinds(ctx);
        }
        self.options.draft = None;
    }
    pub(super) fn discard_shortcut_draft(&mut self) {
        self.options.draft = None;
        self.options.recording = None;
        self.options.message.clear();
    }
    pub(super) fn shortcuts_section(&mut self, ui: &mut egui::Ui) {
        let saved = self.quick_controls.keybinds();
        let draft = self.options.draft.get_or_insert_with(|| saved.clone());
        crate::panel_ui::setting(
            ui,
            "Global capture shortcuts",
            "Capture while working in another application.",
            &mut draft.enabled,
        );
        ui.label(
            egui::RichText::new("Select a shortcut and press your combination. Save to apply.")
                .size(12.)
                .color(crate::panel_ui::MUTED),
        );
        ui.add_space(8.);
        crate::panel_ui::rail(ui, |ui| {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 8.) / 3.;
                for (i, label) in ["Capture", "Tools", "Functions"].into_iter().enumerate() {
                    if crate::panel_ui::tab(ui, label, label, self.options.category == i, w, true)
                        .clicked()
                    {
                        self.options.category = i;
                        self.options.recording = None;
                    }
                }
            });
        });
        ui.add_space(10.);
        crate::panel_ui::heading(
            ui,
            ["Capture", "Tools", "Functions"][self.options.category],
            ["Capture", "Tools", "Functions"][self.options.category],
        );
        ui.label(egui::RichText::new(match self.options.category {0=>"Uses the cursor, start delay, clipboard and preview settings saved for each mode.",1=>"Choose an Insert tool while editing. Text fields retain their normal typing shortcuts.",_=>"Editing and view commands apply inside ClipDesigner. Mouse gestures are below."}).size(12.).color(crate::panel_ui::MUTED));
        ui.add_space(8.);
        let actions: Vec<ShortcutAction> = match self.options.category {
            0 => Action::ALL
                .into_iter()
                .map(ShortcutAction::Capture)
                .collect(),
            1 => crate::shell::TOOLS
                .iter()
                .map(|t| ShortcutAction::Editor(capture_core::editor_keybinds::Action::Tool(t.id)))
                .collect(),
            _ => capture_core::editor_keybinds::Function::ALL
                .into_iter()
                .map(|f| ShortcutAction::Editor(capture_core::editor_keybinds::Action::Function(f)))
                .collect(),
        };
        let available = ui.available_rect_before_wrap();
        let footer_height = if available.width() < 450. { 156. } else { 128. };
        let _scroll=egui::ScrollArea::vertical().id_salt(("shortcut-category",self.options.category)).auto_shrink([false,false]).max_height((available.height()-footer_height).max(60.)).show(ui,|ui|{
            for action in actions {
                ui.push_id(action,|ui|{
                    ui.horizontal(|ui|{
                        let field=(ui.available_width()*0.43).clamp(120.,220.);
                        let left=(ui.available_width()-field-12.).max(80.);
                        ui.allocate_ui_with_layout(egui::vec2(left,38.),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_width(left);ui.add(egui::Label::new(crate::panel_ui::title(action.label(),13.)).wrap());});
                        ui.allocate_ui_with_layout(egui::vec2(field,38.),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                            let listening=self.options.recording==Some(action);
                            let label=if listening {"Press keys…".into()}else{action.get(draft).map(Chord::label).unwrap_or_else(||"Not assigned".into())};
                            let (record,clear)=shortcut_field(ui,&label,action.label(),listening,action.get(draft).is_some());
                            if record.clicked(){self.options.recording=Some(action);self.options.message.clear();self.options.error=false;}
                            if clear.clicked(){action.set(draft,None);self.options.recording=None;}
                        });
                    });
                    crate::panel_ui::divider(ui);
                });
            }
            if self.options.category==2 {
                crate::panel_ui::heading(ui,"Functions","Mouse gestures");
                for gesture in capture_core::editor_keybinds::gestures::Gesture::ALL {
                    ui.push_id(gesture,|ui| {ui.horizontal(|ui|{
                        let field=(ui.available_width()*0.43).clamp(120.,220.);
                        let left=(ui.available_width()-field-12.).max(80.);
                        ui.allocate_ui_with_layout(egui::vec2(left,38.),egui::Layout::left_to_right(egui::Align::Center),|ui|{ui.set_min_width(left);ui.add(egui::Label::new(crate::panel_ui::title(gesture.label(),13.)).wrap());});
                        let mut selected=draft.editor.gestures.get(gesture);
                        let label=gesture_label(selected);
                        ui.allocate_ui_with_layout(egui::vec2(field,38.),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                            ui.spacing_mut().interact_size.y=30.;
                            ui.visuals_mut().widgets.inactive.bg_fill=egui::Color32::from_rgb(65,71,74);
                            ui.visuals_mut().widgets.inactive.bg_stroke=egui::Stroke::new(1.,egui::Color32::from_gray(115));
                            egui::ComboBox::from_id_salt("gesture-modifiers").width((field-22.).clamp(100.,218.)).selected_text(label).show_ui(ui,|ui|{for v in [Some(2),Some(1),Some(4),Some(3),Some(6),Some(5),None]{ui.selectable_value(&mut selected,v,gesture_label(v));}});
                        });
                        draft.editor.gestures.set(gesture,selected);
                    });crate::panel_ui::divider(ui);});
                }
                ui.label(egui::RichText::new("Esc cancels the active tool. Shift range selection, constrained drawing, and text-entry navigation retain their standard behavior.").size(12.).color(crate::panel_ui::MUTED));
            }
        });
        let dirty = *draft != saved;
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("keybinds-scroll"),
                (_scroll.inner_rect, _scroll.state.clamped_offset()),
            )
        });
        let footer = egui::Rect::from_min_max(
            egui::pos2(available.left(), available.bottom() - footer_height + 8.),
            available.max,
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
            crate::panel_ui::divider(ui);
            if self.options.recording.is_some() {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Listening · Esc to cancel");
                    if ui.button("Use Print Screen").clicked() {
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
            let message = if self.options.message.is_empty() {
                if dirty {
                    "Unsaved changes"
                } else {
                    "Select a field to change its shortcut"
                }
            } else {
                &self.options.message
            };
            ui.colored_label(
                if self.options.error {
                    egui::Color32::LIGHT_RED
                } else {
                    crate::panel_ui::MUTED
                },
                message,
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.options.recording.is_none(),
                        crate::controls::dialog_button("Save shortcuts", true)
                            .min_size(egui::vec2(126., crate::controls::DIALOG_ACTION_HEIGHT)),
                    )
                    .clicked()
                {
                    self.save_keybinds(ui.ctx());
                }
                if ui
                    .add_enabled(
                        dirty,
                        crate::controls::dialog_button("Discard changes", false),
                    )
                    .clicked()
                {
                    self.options.draft = Some(saved);
                    self.options.recording = None;
                    self.options.message.clear();
                }
                if ui
                    .add(crate::controls::dialog_button("Restore defaults", false))
                    .clicked()
                {
                    self.options.draft = Some(Keybinds::default());
                    self.options.recording = None;
                    self.options.message = "Defaults loaded. Save shortcuts to apply.".into();
                    self.options.error = false;
                }
            });
            ui.label(
                egui::RichText::new(&self.global_capture_status)
                    .size(11.)
                    .color(crate::panel_ui::MUTED),
            );
        });
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
        Key::Tab => 0x09,
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
    if assigned && !listening {
        let text = ui.painter().layout_no_wrap(
            label.into(),
            egui::FontId::proportional(13.),
            egui::Color32::WHITE,
        );
        ui.painter()
            .with_clip_rect(record_rect.shrink(3.))
            .rect_filled(
                egui::Rect::from_center_size(
                    record_rect.center(),
                    text.size() + egui::vec2(12., 5.),
                ),
                3.,
                crate::tabs::BACKGROUND,
            );
    }
    ui.painter()
        .with_clip_rect(record_rect.shrink2(egui::vec2(7., 0.)))
        .text(
            record_rect.center(),
            egui::Align2::CENTER_CENTER,
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
        record.tip(format!("Set {action} shortcut")),
        clear.tip(format!("Clear {action} shortcut")),
    )
}

fn gesture_label(v: Option<u8>) -> &'static str {
    match v {
        Some(2) => "Ctrl",
        Some(1) => "Alt",
        Some(4) => "Shift",
        Some(3) => "Ctrl+Alt",
        Some(6) => "Ctrl+Shift",
        Some(5) => "Alt+Shift",
        _ => "Disabled",
    }
}
