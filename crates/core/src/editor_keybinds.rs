//! Local editor bindings. These are never registered as global capture hotkeys.
use crate::{Error, Result, keybinds::Chord, toolbar::ToolId};
use serde::{Deserialize, Serialize};

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
}
impl Function {
    pub const ALL: [Self; 23] = [
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
    ];
    pub fn label(self) -> &'static str {
        match self {
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
        }
    }
    pub fn default_chord(self) -> Chord {
        let (key, modifiers) = match self {
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
        };
        Chord { key, modifiers }
    }
}
pub const TOOL_DEFAULTS: [(ToolId, u16); 19] = [
    (ToolId::Move, 0x56),
    (ToolId::Text, 0x54),
    (ToolId::Shape, 0x53),
    (ToolId::Step, 0x31),
    (ToolId::Blur, 0x42),
    (ToolId::Selection, 0x4d),
    (ToolId::Pen, 0x50),
    (ToolId::Callout, 0x43),
    (ToolId::Highlighter, 0x48),
    (ToolId::Redact, 0x52),
    (ToolId::Arrow, 0x41),
    (ToolId::Line, 0x4c),
    (ToolId::Fill, 0x46),
    (ToolId::Simplify, 0x33),
    (ToolId::Stamp, 0x32),
    (ToolId::Crop, 0x5a),
    (ToolId::CutOut, 0x58),
    (ToolId::Eraser, 0x08),
    (ToolId::Magnify, 0xbb),
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Keybinds {
    pub bindings: Vec<Binding>,
}
impl Default for Keybinds {
    fn default() -> Self {
        Self {
            bindings: TOOL_DEFAULTS
                .into_iter()
                .map(|(tool, key)| Binding {
                    action: Action::Tool(tool),
                    chord: Chord { key, modifiers: 0 },
                })
                .chain(Function::ALL.into_iter().map(|f| Binding {
                    action: Action::Function(f),
                    chord: f.default_chord(),
                }))
                .collect(),
        }
    }
}
impl Keybinds {
    pub fn get(&self, action: Action) -> Option<Chord> {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.chord)
    }
    pub fn action(&self, chord: Chord) -> Option<Action> {
        self.bindings
            .iter()
            .find(|b| b.chord == chord)
            .map(|b| b.action)
    }
    pub fn set(&mut self, action: Action, chord: Option<Chord>) {
        self.bindings.retain(|b| b.action != action);
        if let Some(chord) = chord {
            self.bindings.push(Binding { action, chord });
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.bindings.len() > 64 {
            return Err(Error::Invalid("Too many editor shortcuts".into()));
        }
        for (i, b) in self.bindings.iter().enumerate() {
            if b.chord.key_label().is_none()
                || b.chord.modifiers & !7 != 0
                || (b.chord.modifiers & 3 == 0 && matches!(b.chord.key, 0x20..=0x28 | 0x2c))
                || b.action == Action::Tool(ToolId::Favorites)
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
