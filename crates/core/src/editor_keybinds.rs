//! Local editor bindings. These are never registered as global capture hotkeys.
use crate::{Error, Result, keybinds::Chord, toolbar::ToolId};
use serde::{Deserialize, Serialize};
pub mod gestures;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Function {
    Undo,
    Redo,
    Save,
    SaveAs,
    New,
    Open,
    Copy,
    Cut,
    Paste,
    CopyAll,
    NewFromClipboard,
    PasteStyle,
    Delete,
    SelectAll,
    Group,
    Ungroup,
    PixelGrid,
    FlattenAll,
    BringForward,
    SendBackward,
    ZoomIn,
    ZoomOut,
    ActualSize,
    VideoPlayback,
    ToggleUi,
    ToggleRecent,
    ToggleAnnotations,
    FitCanvas,
    JumpToObject,
}
impl Function {
    pub const ALL: [Self; 29] = [
        Self::Undo,
        Self::Redo,
        Self::Save,
        Self::SaveAs,
        Self::New,
        Self::Open,
        Self::Copy,
        Self::Cut,
        Self::Paste,
        Self::CopyAll,
        Self::NewFromClipboard,
        Self::PasteStyle,
        Self::Delete,
        Self::SelectAll,
        Self::Group,
        Self::Ungroup,
        Self::PixelGrid,
        Self::FlattenAll,
        Self::BringForward,
        Self::SendBackward,
        Self::ZoomIn,
        Self::ZoomOut,
        Self::ActualSize,
        Self::VideoPlayback,
        Self::ToggleUi,
        Self::ToggleRecent,
        Self::ToggleAnnotations,
        Self::FitCanvas,
        Self::JumpToObject,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleUi => "Show / hide UI",
            Self::ToggleRecent => "Show / hide Recent Clips",
            Self::ToggleAnnotations => "Show / hide annotations",
            Self::FitCanvas => "Fit canvas to view",
            Self::JumpToObject => "Jump to Object in the timeline",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Save => "Save",
            Self::SaveAs => "Save As",
            Self::New => "New image",
            Self::Open => "Open",
            Self::Copy => "Copy",
            Self::Cut => "Cut",
            Self::Paste => "Paste",
            Self::CopyAll => "Copy All",
            Self::NewFromClipboard => "New from Clipboard",
            Self::PasteStyle => "Paste formatting",
            Self::Delete => "Delete objects",
            Self::SelectAll => "Select all",
            Self::Group => "Group",
            Self::Ungroup => "Ungroup",
            Self::PixelGrid => "Pixel grid",
            Self::FlattenAll => "Flatten all",
            Self::BringForward => "Bring forward",
            Self::SendBackward => "Send backward",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::ActualSize => "Actual size",
            Self::VideoPlayback => {
                "Video play / pause · twice: clip start · three times: project start"
            }
        }
    }
    pub fn default_chord(self) -> Chord {
        let (key, modifiers) = match self {
            Self::ToggleUi => (0x09, 0),
            Self::ToggleRecent => (0x52, 2),
            Self::ToggleAnnotations => (0x48, 2),
            Self::FitCanvas => (0x39, 2),
            Self::Undo => (0x5a, 2),
            Self::Redo => (0x59, 2),
            Self::Save => (0x53, 2),
            Self::SaveAs => (0x53, 6),
            Self::New => (0x4e, 2),
            Self::Open => (0x4f, 2),
            Self::Copy => (0x43, 2),
            Self::Cut => (0x58, 2),
            Self::Paste => (0x56, 2),
            Self::CopyAll => (0x43, 6),
            Self::NewFromClipboard => (0x4e, 6),
            Self::PasteStyle => (0x56, 6),
            Self::Delete => (0x2e, 0),
            Self::SelectAll => (0x41, 2),
            Self::Group => (0x47, 2),
            Self::Ungroup => (0x47, 6),
            Self::PixelGrid => (0x47, 3),
            Self::FlattenAll => (0x54, 6),
            Self::BringForward => (0x46, 6),
            Self::SendBackward => (0x42, 6),
            Self::ZoomIn => (0xbb, 2),
            Self::ZoomOut => (0xbd, 2),
            Self::ActualSize => (0x30, 2),
            Self::VideoPlayback => (0x20, 0),
            Self::JumpToObject => (0x46, 0),
        };
        Chord { key, modifiers }
    }
}
pub const TOOL_DEFAULTS: [(ToolId, u16); 11] = [
    (ToolId::Move, 0x56),
    (ToolId::Lines, 0x41),
    (ToolId::Text, 0x54),
    (ToolId::Shape, 0x53),
    (ToolId::Icons, 0x32),
    (ToolId::Steps, 0x31),
    (ToolId::Penlighter, 0x50),
    (ToolId::Obfuscate, 0x42),
    (ToolId::Crop, 0x5a),
    (ToolId::CutOut, 0x58),
    (ToolId::Eraser, 0x08),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Tool(ToolId),
    Function(Function),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Binding {
    pub action: Action,
    pub chord: Chord,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Keybinds {
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub gestures: gestures::Gestures,
    #[serde(default = "default_video_playback")]
    pub video_playback: Option<Chord>,
}
impl<'de> Deserialize<'de> for Keybinds {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Stored {
            bindings: Vec<Binding>,
            gestures: Option<gestures::Gestures>,
            #[serde(default = "default_video_playback")]
            video_playback: Option<Chord>,
        }
        let s = Stored::deserialize(d)?;
        let mut keys = Self {
            bindings: s.bindings,
            gestures: s.gestures.clone().unwrap_or_default(),
            video_playback: s.video_playback,
        };
        // Existing preferences gain the newly configurable view actions once.
        // A saved unassignment stays unassigned; user overrides win conflicts.
        if s.gestures.is_none() {
            for f in [
                Function::ToggleUi,
                Function::ToggleRecent,
                Function::ToggleAnnotations,
                Function::FitCanvas,
            ] {
                if keys.get(Action::Function(f)).is_none()
                    && keys.action(f.default_chord()).is_none()
                {
                    keys.set(Action::Function(f), Some(f.default_chord()));
                }
            }
        }
        // Jump to Object (bg-proposal) joins existing preferences when F is free.
        let jump = Function::JumpToObject;
        if keys.get(Action::Function(jump)).is_none() && keys.action(jump.default_chord()).is_none()
        {
            keys.set(Action::Function(jump), Some(jump.default_chord()));
        }
        Ok(keys)
    }
}
fn default_video_playback() -> Option<Chord> {
    Some(Function::VideoPlayback.default_chord())
}
impl Default for Keybinds {
    fn default() -> Self {
        Self {
            video_playback: default_video_playback(),
            gestures: Default::default(),
            bindings: TOOL_DEFAULTS
                .into_iter()
                .map(|(tool, key)| Binding {
                    action: Action::Tool(tool),
                    chord: Chord { key, modifiers: 0 },
                })
                .chain(
                    Function::ALL
                        .into_iter()
                        .filter(|f| *f != Function::VideoPlayback)
                        .map(|f| Binding {
                            action: Action::Function(f),
                            chord: f.default_chord(),
                        }),
                )
                .collect(),
        }
    }
}
impl Keybinds {
    pub fn get(&self, action: Action) -> Option<Chord> {
        if action == Action::Function(Function::VideoPlayback) {
            return self.video_playback;
        }
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.chord)
    }
    pub fn action(&self, chord: Chord) -> Option<Action> {
        if self.video_playback == Some(chord) {
            return Some(Action::Function(Function::VideoPlayback));
        }
        self.bindings
            .iter()
            .find(|b| b.chord == chord)
            .map(|b| b.action)
    }
    pub fn set(&mut self, action: Action, chord: Option<Chord>) {
        if action == Action::Function(Function::VideoPlayback) {
            self.video_playback = chord;
            return;
        }
        self.bindings.retain(|b| b.action != action);
        if let Some(chord) = chord {
            self.bindings.push(Binding { action, chord });
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.gestures.validate()?;
        if let Some(c) = self.video_playback
            && (c.key_label().is_none()
                || c.modifiers & !7 != 0
                || self.bindings.iter().any(|b| b.chord == c))
        {
            return Err(Error::Invalid(
                "Video playback shortcut conflicts with another shortcut.".into(),
            ));
        }
        if self.bindings.len() > 64 {
            return Err(Error::Invalid("Too many editor shortcuts".into()));
        }
        for (i, b) in self.bindings.iter().enumerate() {
            if b.chord.key_label().is_none()
                || b.chord.modifiers & !7 != 0
                || (b.chord.modifiers & 3 == 0 && matches!(b.chord.key, 0x20..=0x28 | 0x2c))
            {
                return Err(Error::Invalid(
                    "This combination is reserved for navigation or Windows. Choose another key."
                        .into(),
                ));
            }
            if self.bindings[..i]
                .iter()
                .any(|o| o.action == b.action || o.chord == b.chord)
            {
                return Err(Error::Invalid(format!(
                    "{} is already assigned in editor shortcuts",
                    b.chord.label()
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_preferences_gain_view_actions_but_cleared_actions_stay_cleared() {
        let mut raw = serde_json::to_value(Keybinds::default()).unwrap();
        raw.as_object_mut().unwrap().remove("gestures");
        raw["bindings"].as_array_mut().unwrap().retain(|b| {
            ![
                "toggle_ui",
                "toggle_recent",
                "toggle_annotations",
                "fit_canvas",
            ]
            .iter()
            .any(|s| b["action"]["function"] == *s)
        });
        let mut keys: Keybinds = serde_json::from_value(raw).unwrap();
        keys.validate().unwrap();
        assert_eq!(
            keys.get(Action::Function(Function::ToggleUi)),
            Some(Function::ToggleUi.default_chord())
        );
        keys.set(Action::Function(Function::ToggleUi), None);
        let saved: Keybinds = serde_json::from_slice(&serde_json::to_vec(&keys).unwrap()).unwrap();
        assert_eq!(saved.get(Action::Function(Function::ToggleUi)), None);
    }
    #[test]
    fn defaults_remapping_clearing_and_persistence() {
        let mut keys = Keybinds::default();
        keys.validate().unwrap();
        for (tool, key) in TOOL_DEFAULTS {
            assert_eq!(
                keys.action(Chord { key, modifiers: 0 }),
                Some(Action::Tool(tool))
            );
        }
        let v = keys.get(Action::Tool(ToolId::Move)).unwrap();
        keys.set(Action::Tool(ToolId::Text), Some(v));
        assert!(keys.validate().is_err());
        keys.set(Action::Tool(ToolId::Text), None);
        keys.validate().unwrap();
        assert_eq!(
            keys,
            serde_json::from_slice::<Keybinds>(&serde_json::to_vec(&keys).unwrap()).unwrap()
        );
        assert_eq!(Function::Group.default_chord().label(), "Ctrl+G");
        assert_eq!(Function::PixelGrid.default_chord().label(), "Ctrl+Alt+G");
    }
}
