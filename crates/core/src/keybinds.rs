//! Validated, portable capture shortcut preferences. No platform registration here.
use crate::{
    Error, Result,
    capture_options::{Selection, Settings},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Saved,
    Region,
    Window,
    Fullscreen,
    Steps,
    GrabText,
}
impl Action {
    pub const ALL: [Self; 6] = [
        Self::Saved,
        Self::Region,
        Self::Window,
        Self::Fullscreen,
        Self::Steps,
        Self::GrabText,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Saved => "Capture with saved settings",
            Self::Region => "Region",
            Self::Window => "Window",
            Self::Fullscreen => "Fullscreen",
            Self::Steps => "Steps",
            Self::GrabText => "Grab Text",
        }
    }
    pub fn settings(self, mut saved: Settings) -> Settings {
        saved.selection = match self {
            Self::Saved => saved.selection,
            Self::Region => Selection::Region,
            Self::Window => Selection::Window,
            Self::Fullscreen => Selection::Fullscreen,
            Self::Steps => Selection::Steps,
            Self::GrabText => Selection::GrabText,
        };
        saved
    }
}

/// Windows virtual key and MOD_ALT/CONTROL/SHIFT/WIN flags; never MOD_NOREPEAT.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    pub key: u16,
    pub modifiers: u8,
}
impl Chord {
    pub fn key_label(self) -> Option<String> {
        Some(match self.key {
            0x30..=0x39 | 0x41..=0x5a => char::from_u32(self.key.into())?.to_string(),
            0x70..=0x87 => format!("F{}", self.key - 0x6f),
            0x2c => "Print Screen".into(),
            0x20 => "Space".into(),
            0x08 => "Backspace".into(),
            0x21 => "Page Up".into(),
            0x22 => "Page Down".into(),
            0x23 => "End".into(),
            0x24 => "Home".into(),
            0x25 => "Left".into(),
            0x26 => "Up".into(),
            0x27 => "Right".into(),
            0x28 => "Down".into(),
            0x2d => "Insert".into(),
            0x2e => "Delete".into(),
            0xbb => "=".into(),
            0xbd => "-".into(),
            _ => return None,
        })
    }
    pub fn label(self) -> String {
        let mut parts = Vec::new();
        for (mask, label) in [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")] {
            if self.modifiers & mask != 0 {
                parts.push(label.to_owned());
            }
        }
        parts.push(self.key_label().unwrap_or_else(|| "Unsupported key".into()));
        parts.join("+")
    }
    pub fn validate(self) -> Result<()> {
        let message = if self.key == 0x7b {
            Some("Windows reserves F12 for debugging. Try Alt+F11 or another combination.")
        } else if self.modifiers & !15 != 0 || self.key_label().is_none() {
            Some("Unsupported shortcut key or modifiers")
        } else if self.modifiers & (1 | 2 | 8) == 0
            && !(0x70..=0x87).contains(&self.key)
            && self.key != 0x2c
        {
            Some("Use Ctrl, Alt or Win with this key to avoid capturing while typing.")
        } else {
            None
        };
        message.map_or(Ok(()), |m| Err(Error::Invalid(m.into())))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Binding {
    pub action: Action,
    pub chord: Chord,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Keybinds {
    pub enabled: bool,
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub editor: crate::editor_keybinds::Keybinds,
}
impl Default for Keybinds {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            enabled: true,
            bindings: vec![
                Binding {
                    action: Action::Saved,
                    chord: Chord {
                        key: 0x2c,
                        modifiers: 0,
                    },
                },
                Binding {
                    action: Action::Region,
                    chord: Chord {
                        key: 0x7a,
                        modifiers: 1,
                    },
                },
            ],
        }
    }
}
impl Keybinds {
    pub(crate) fn is_legacy_default(&self) -> bool {
        self.enabled
            && self.editor == crate::editor_keybinds::Keybinds::default()
            && self.bindings
                == [Binding {
                    action: Action::Saved,
                    chord: Chord {
                        key: 0x43,
                        modifiers: 3,
                    },
                }]
    }
    pub fn get(&self, action: Action) -> Option<Chord> {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.chord)
    }
    pub fn set(&mut self, action: Action, chord: Option<Chord>) {
        self.bindings.retain(|b| b.action != action);
        if let Some(chord) = chord {
            self.bindings.push(Binding { action, chord });
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.editor.validate()?;
        if self.bindings.len() > Action::ALL.len() {
            return Err(Error::Invalid("Too many capture shortcuts".into()));
        }
        for (i, b) in self.bindings.iter().enumerate() {
            b.chord.validate()?;
            if self.enabled && self.editor.action(b.chord).is_some() {
                return Err(Error::Invalid(format!(
                    "{} is already assigned to an editor action",
                    b.chord.label()
                )));
            }
            for other in &self.bindings[..i] {
                if other.action == b.action {
                    return Err(Error::Invalid("Duplicate capture action".into()));
                }
                if other.chord == b.chord {
                    return Err(Error::Invalid(format!(
                        "{} is already assigned to {}",
                        b.chord.label(),
                        other.action.label()
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn new_capture_defaults_and_saved_legacy_customizations_are_distinct() {
        let defaults = Keybinds::default();
        defaults.validate().unwrap();
        assert_eq!(defaults.get(Action::Saved).unwrap().label(), "Print Screen");
        assert_eq!(defaults.get(Action::Region).unwrap().label(), "Alt+F11");
        let legacy = serde_json::json!({"version":4,"alignment":"left","distribution":"horizontal_gaps","justification":"center_middle", "keybinds":{"enabled":true,"bindings":[{"action":"saved","chord":{"key":67,"modifiers":3}}]}});
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("quick-controls.json");
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let store = crate::quick_preferences::Store::open(root.path()).unwrap();
        let preferences = store.read().unwrap();
        assert!(preferences.keybinds.is_legacy_default());
        assert_eq!(
            preferences.startup_window,
            crate::quick_preferences::StartupWindow::Editor
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap(),
            legacy
        );
    }
    #[test]
    fn mode_shortcuts_override_only_selection_and_keep_saved_capture_options() {
        let saved = Settings {
            cursor: true,
            clipboard: true,
            delay_seconds: 5,
            preview: false,
            fullscreen: crate::capture_options::Fullscreen::All,
            ..Default::default()
        };
        for action in Action::ALL {
            let settings = action.settings(saved.clone());
            let mut restored = settings.clone();
            restored.selection = saved.selection;
            assert_eq!(restored, saved);
            assert!(
                settings
                    .request(crate::capture_options::Mode::Image)
                    .is_ok()
            );
        }
        assert_eq!(Action::Steps.settings(saved).selection, Selection::Steps);
    }
    #[test]
    fn shortcuts_migrate_persist_and_reject_conflicts_without_changing_preferences() {
        use crate::quick_preferences::{Choice, Preferences, Store};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quick-controls.json");
        let mut legacy = serde_json::to_value(Preferences::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("keybinds");
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.read().unwrap().keybinds, Keybinds::default());
        let mut keys = Keybinds::default();
        keys.set(Action::Saved, None);
        keys.set(
            Action::Region,
            Some(Chord {
                key: 0x7a,
                modifiers: 1,
            }),
        );
        keys.set(
            Action::Fullscreen,
            Some(Chord {
                key: 0x2c,
                modifiers: 0,
            }),
        );
        let saved = store.remember(Choice::Keybinds(keys.clone())).unwrap();
        assert_eq!(saved.version, 8);
        assert_eq!(
            Store::open(dir.path()).unwrap().read().unwrap().keybinds,
            keys
        );
        let bytes = std::fs::read(&path).unwrap();
        keys.set(Action::Window, keys.get(Action::Region));
        assert!(store.remember(Choice::Keybinds(keys)).is_err());
        assert_eq!(bytes, std::fs::read(&path).unwrap());
        for chord in [
            Chord {
                key: 0x7b,
                modifiers: 1,
            },
            Chord {
                key: 0x41,
                modifiers: 0,
            },
            Chord {
                key: 0x41,
                modifiers: 4,
            },
            Chord {
                key: 0xffff,
                modifiers: 3,
            },
        ] {
            assert!(chord.validate().is_err());
        }
        let mut bad = saved;
        bad.version = 5;
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(store.read().is_err());
    }
}
