//! Small atomic local preferences for remembered contextual actions.
use crate::{Error, Result, atomic_write, layout_selection::LayoutAction};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Justification {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    CenterMiddle,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}
impl Justification {
    pub fn alignments(
        self,
    ) -> (
        crate::text::HorizontalAlignment,
        crate::text::VerticalAlignment,
    ) {
        use crate::text::{HorizontalAlignment as H, VerticalAlignment as V};
        match self {
            Self::TopLeft => (H::Left, V::Top),
            Self::TopCenter => (H::Center, V::Top),
            Self::TopRight => (H::Right, V::Top),
            Self::MiddleLeft => (H::Left, V::Middle),
            Self::CenterMiddle => (H::Center, V::Middle),
            Self::MiddleRight => (H::Right, V::Middle),
            Self::BottomLeft => (H::Left, V::Bottom),
            Self::BottomCenter => (H::Center, V::Bottom),
            Self::BottomRight => (H::Right, V::Bottom),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupWindow {
    #[default]
    Editor,
    CaptureController,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "snapping_default")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub startup_window: StartupWindow,
    #[serde(default)]
    pub keybinds: crate::keybinds::Keybinds,
    #[serde(default)]
    pub capture: crate::capture_options::Settings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capture_profiles: Vec<crate::capture_options::Settings>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub folders: std::collections::BTreeMap<Folder, PathBuf>,
    #[serde(default)]
    pub thumbnail_details: bool,
    pub version: u32,
    pub alignment: LayoutAction,
    pub distribution: LayoutAction,
    pub justification: Justification,
    #[serde(default = "snapping_default")]
    pub canvas_snapping: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapping_limit: Option<usize>,
    #[serde(default)]
    pub style_choices: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub auto_switch_tool: bool,
    #[serde(default = "crate::toolbar::default_order")]
    pub toolbar_order: Vec<crate::toolbar::ToolId>,
    #[serde(default)]
    pub favorite_tools: Vec<crate::toolbar::ToolId>,
    #[serde(default = "default_canvas_tool")]
    pub canvas_tool: crate::toolbar::ToolId,
    #[serde(default)]
    pub drawing_styles: Vec<crate::object_styles::ObjectStyle>,
}
fn snapping_default() -> bool {
    true
}
fn default_canvas_tool() -> crate::toolbar::ToolId {
    crate::toolbar::ToolId::Crop
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 4,
            close_to_tray: true,
            startup_window: StartupWindow::Editor,
            keybinds: Default::default(),
            capture: Default::default(),
            capture_profiles: Vec::new(),
            folders: Default::default(),
            thumbnail_details: false,
            alignment: LayoutAction::HorizontalCenter,
            distribution: LayoutAction::HorizontalGaps,
            justification: Justification::CenterMiddle,
            canvas_snapping: true,
            snapping_limit: None,
            style_choices: Default::default(),
            auto_switch_tool: false,
            toolbar_order: crate::toolbar::default_order(),
            favorite_tools: crate::toolbar::default_favorites(),
            canvas_tool: default_canvas_tool(),
            drawing_styles: vec![],
        }
    }
}
impl Preferences {
    pub fn capture_for(
        &self,
        selection: crate::capture_options::Selection,
    ) -> crate::capture_options::Settings {
        self.capture_profiles
            .iter()
            .find(|p| p.selection == selection)
            .cloned()
            .or_else(|| (self.capture.selection == selection).then(|| self.capture.clone()))
            .unwrap_or_else(|| crate::capture_options::Settings {
                selection,
                cursor: selection == crate::capture_options::Selection::Steps,
                ..Default::default()
            })
    }

    pub fn choose(&mut self, choice: Choice) -> Result<()> {
        match choice {
            Choice::ThumbnailDetails(value) => {
                self.thumbnail_details = value;
                self.version = self.version.max(11);
            }
            Choice::Folder(kind, path) => {
                validate_folder(&path)?;
                self.folders.insert(kind, path);
                self.version = self.version.max(11);
            }
            Choice::CloseToTray(value) => {
                self.close_to_tray = value;
                self.version = self.version.max(8);
            }
            Choice::StartupWindow(value) => {
                self.startup_window = value;
                self.version = self.version.max(8);
            }
            Choice::Keybinds(keys) => {
                keys.validate()?;
                self.keybinds = keys;
                self.version = self.version.max(
                    if self.keybinds.editor == crate::editor_keybinds::Keybinds::default() {
                        6
                    } else {
                        7
                    },
                );
            }
            Choice::Capture(settings) => {
                settings.validate()?;
                // Retain the legacy/current mode before switching to another.
                if !self
                    .capture_profiles
                    .iter()
                    .any(|p| p.selection == self.capture.selection)
                {
                    self.capture_profiles.push(self.capture.clone());
                }
                self.capture_profiles
                    .retain(|p| p.selection != settings.selection);
                self.capture_profiles.push(settings.clone());
                self.capture = settings;
                self.version = self.version.max(10);
            }
            Choice::Alignment(action) if action.minimum_objects() == 2 => self.alignment = action,
            Choice::Distribution(action) if action.minimum_objects() == 3 => {
                self.distribution = action
            }
            Choice::Justification(value) => self.justification = value,
            Choice::CanvasSnapping(value) => self.canvas_snapping = value,
            Choice::SnappingLimit(value) => {
                self.snapping_limit = value;
                self.version = self.version.max(9);
            }
            Choice::AutoSwitchTool(value) => self.auto_switch_tool = value,
            Choice::DrawingStyle(style) => {
                style.validate()?;
                self.drawing_styles.retain(|s| !s.same_variant(&style));
                self.drawing_styles.push(style);
                self.version = self.version.max(3);
            }
            Choice::MoveTool { tool, before } => {
                crate::toolbar::move_before(&mut self.toolbar_order, tool, before)?;
                // Older apps must not silently drop a customized order when
                // rewriting their known preference fields.
                self.version = self.version.max(2);
            }
            Choice::ResetToolbar => {
                self.toolbar_order = crate::toolbar::default_order();
                // Preserve membership; reset only each section's ordering.
                self.favorite_tools
                    .sort_by_key(|t| self.toolbar_order.iter().position(|id| id == t));
                self.version = self.version.max(2);
            }
            Choice::FavoriteTool { tool, favorite } => {
                let tool = crate::toolbar::slot(tool);
                if tool == crate::toolbar::ToolId::Favorites {
                    return Err(Error::Invalid("Favorites is a section, not a tool".into()));
                }
                if favorite && !self.favorite_tools.contains(&tool) {
                    self.favorite_tools.push(tool);
                }
                if !favorite {
                    self.favorite_tools.retain(|t| *t != tool);
                }
                self.version = self.version.max(4);
            }
            Choice::MoveSectionTool { tool, before } => {
                crate::toolbar::move_in_group(
                    &mut self.toolbar_order,
                    &mut self.favorite_tools,
                    tool,
                    before,
                )?;
                self.version = self.version.max(4);
            }
            Choice::CanvasTool(tool)
                if matches!(
                    tool,
                    crate::toolbar::ToolId::Crop | crate::toolbar::ToolId::CutOut
                ) =>
            {
                self.canvas_tool = tool;
                self.version = self.version.max(4);
            }
            Choice::Style { category, name } => {
                validate_style_choice(&category, &name)?;
                if self.style_choices.len() >= 24 && !self.style_choices.contains_key(&category) {
                    return Err(Error::Invalid(
                        "Too many remembered style categories".into(),
                    ));
                }
                self.style_choices.insert(category, name);
            }
            _ => return Err(Error::Invalid("Invalid quick control action".into())),
        }
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        if !matches!(self.version, 1..=11)
            || (self.version < 11 && (!self.folders.is_empty() || self.thumbnail_details))
            || self.folders.len() > 3
            || self.snapping_limit.is_some_and(|n| n == 0 || n > 10000)
            || (self.version < 10 && !self.capture_profiles.is_empty())
            || self.capture_profiles.len() > crate::capture_options::Selection::ALL.len()
            || (self.version < 9 && self.snapping_limit.is_some())
            || (self.version < 8
                && (!self.close_to_tray || self.startup_window != StartupWindow::Editor))
            || (self.version < 7
                && self.keybinds.editor != crate::editor_keybinds::Keybinds::default())
            || (self.version < 6 && self.keybinds != crate::keybinds::Keybinds::default())
            || (self.version < 5 && self.capture != crate::capture_options::Settings::default())
            || (self.version < 4
                && (!self.favorite_tools.is_empty() || self.canvas_tool != default_canvas_tool()))
            || !matches!(
                self.canvas_tool,
                crate::toolbar::ToolId::Crop | crate::toolbar::ToolId::CutOut
            )
            || self.alignment.minimum_objects() != 2
            || self.distribution.minimum_objects() != 3
            || self.style_choices.len() > 24
            || self.drawing_styles.len() > 21
            || (self.version < 3 && !self.drawing_styles.is_empty())
        {
            return Err(Error::Invalid(
                "Unsupported quick control preferences; file preserved".into(),
            ));
        }
        for path in self.folders.values() {
            validate_folder(path)?;
        }
        for (index, profile) in self.capture_profiles.iter().enumerate() {
            profile.validate()?;
            if self.capture_profiles[..index]
                .iter()
                .any(|p| p.selection == profile.selection)
            {
                return Err(Error::Invalid("Duplicate capture settings".into()));
            }
        }
        for (category, name) in &self.style_choices {
            validate_style_choice(category, name)?;
        }
        for (i, style) in self.drawing_styles.iter().enumerate() {
            style.validate()?;
            if self.drawing_styles[..i]
                .iter()
                .any(|s| s.same_variant(style))
            {
                return Err(Error::Invalid("Duplicate drawing defaults".into()));
            }
        }
        self.capture.validate()?;
        self.keybinds.validate()?;
        crate::toolbar::validate(&self.toolbar_order)?;
        crate::toolbar::validate_favorites(&self.favorite_tools)?;
        Ok(())
    }
}
fn validate_style_choice(category: &str, name: &str) -> Result<()> {
    if category.is_empty()
        || category.len() > 32
        || !category
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c == b'_')
        || name.trim() != name
        || name.is_empty()
        || name.chars().count() > 40
        || name.chars().any(char::is_control)
    {
        return Err(Error::Invalid("Invalid remembered style choice".into()));
    }
    Ok(())
}
#[derive(Clone)]
pub enum Choice {
    Folder(Folder, PathBuf),
    ThumbnailDetails(bool),
    CloseToTray(bool),
    StartupWindow(StartupWindow),
    Keybinds(crate::keybinds::Keybinds),
    Capture(crate::capture_options::Settings),
    Alignment(LayoutAction),
    Distribution(LayoutAction),
    Justification(Justification),
    CanvasSnapping(bool),
    SnappingLimit(Option<usize>),
    AutoSwitchTool(bool),
    DrawingStyle(crate::object_styles::ObjectStyle),
    MoveTool {
        tool: crate::toolbar::ToolId,
        before: Option<crate::toolbar::ToolId>,
    },
    ResetToolbar,
    FavoriteTool {
        tool: crate::toolbar::ToolId,
        favorite: bool,
    },
    MoveSectionTool {
        tool: crate::toolbar::ToolId,
        before: Option<crate::toolbar::ToolId>,
    },
    CanvasTool(crate::toolbar::ToolId),
    Style {
        category: String,
        name: String,
    },
}

pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let store = Self {
            root: fs::canonicalize(root)?,
        };
        store.read()?;
        Ok(store)
    }
    pub fn read(&self) -> Result<Preferences> {
        let path = self.root.join("quick-controls.json");
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
            Err(e) => return Err(e.into()),
        };
        if metadata.len() > 16384 {
            return Err(Error::Invalid(
                "Quick control preferences are too large; file preserved".into(),
            ));
        }
        let mut prefs: Preferences = serde_json::from_slice(&fs::read(path)?)?;
        prefs.validate()?;
        crate::toolbar::normalize(&mut prefs.toolbar_order)?;
        Ok(prefs)
    }
    pub fn remember(&self, choice: Choice) -> Result<Preferences> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("quick-controls.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Quick controls are busy in another window".into()))?;
        let mut next = self.read()?;
        next.choose(choice)?;
        next.validate()?;
        let bytes = serde_json::to_vec_pretty(&next)?;
        if bytes.len() > 16384 {
            return Err(Error::Invalid(
                "Quick control preferences exceed storage limit".into(),
            ));
        }
        atomic_write(&self.root.join("quick-controls.json"), &bytes)?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_shortcuts_persist_without_changing_capture_or_existing_favorites() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let mut keys = store.read().unwrap().keybinds;
        let capture = keys.bindings.clone();
        keys.editor.set(
            crate::editor_keybinds::Action::Tool(crate::toolbar::ToolId::Move),
            Some(crate::keybinds::Chord {
                key: 0x51,
                modifiers: 0,
            }),
        );
        store.remember(Choice::Keybinds(keys.clone())).unwrap();
        let loaded = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(loaded.version, 7);
        assert_eq!(loaded.keybinds, keys);
        assert_eq!(loaded.keybinds.bindings, capture);
        assert_eq!(loaded.favorite_tools, crate::toolbar::default_favorites());
        let mut legacy = serde_json::to_value(&loaded).unwrap();
        legacy["version"] = 6.into();
        legacy["keybinds"].as_object_mut().unwrap().remove("editor");
        legacy["favorite_tools"] = serde_json::json!([]);
        let restored: Preferences = serde_json::from_value(legacy).unwrap();
        restored.validate().unwrap();
        assert_eq!(
            restored.keybinds.editor,
            crate::editor_keybinds::Keybinds::default()
        );
        assert!(restored.favorite_tools.is_empty());
    }
    #[test]
    fn drawing_defaults_merge_by_kind_and_reject_invalid_data_without_losing_saved_styles() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let b = Store::open(temp.path()).unwrap();
        let rectangle = crate::object_styles::presets(crate::Kind::Rectangle).remove(0);
        let text = crate::object_styles::presets(crate::Kind::Text).remove(0);
        a.remember(Choice::DrawingStyle(rectangle.clone())).unwrap();
        b.remember(Choice::DrawingStyle(text.clone())).unwrap();
        a.remember(Choice::ResetToolbar).unwrap();
        let loaded = b.read().unwrap();
        assert_eq!(loaded.version, 4);
        assert_eq!(loaded.drawing_styles, [rectangle.clone(), text]);
        let path = temp.path().join("quick-controls.json");
        let bytes = fs::read(&path).unwrap();
        let mut invalid = rectangle.clone();
        invalid.width = 0;
        assert!(a.remember(Choice::DrawingStyle(invalid)).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let mut duplicate = loaded;
        duplicate.drawing_styles.push(rectangle);
        let bad_bytes = serde_json::to_vec(&duplicate).unwrap();
        fs::write(&path, &bad_bytes).unwrap();
        assert!(Store::open(temp.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), bad_bytes);
    }
    #[test]
    fn categories_remember_independently_merge_and_reject_invalid_changes() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let b = Store::open(temp.path()).unwrap();
        a.remember(Choice::Style {
            category: "shape_rectangle".into(),
            name: "Lime filled".into(),
        })
        .unwrap();
        b.remember(Choice::Style {
            category: "text_mono".into(),
            name: "Ocean".into(),
        })
        .unwrap();
        a.remember(Choice::Style {
            category: "shape_rectangle".into(),
            name: "Gold".into(),
        })
        .unwrap();
        b.remember(Choice::CanvasSnapping(false)).unwrap();
        b.remember(Choice::AutoSwitchTool(true)).unwrap();
        let p = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(p.style_choices["shape_rectangle"], "Gold");
        assert_eq!(p.style_choices["text_mono"], "Ocean");
        assert!(!p.canvas_snapping);
        assert!(p.auto_switch_tool);
        let path = temp.path().join("quick-controls.json");
        let bytes = fs::read(&path).unwrap();
        for (category, name) in [
            ("../../file", "Coral"),
            ("text_mono", "\nInvalid"),
            ("text_mono", ""),
        ] {
            assert!(
                a.remember(Choice::Style {
                    category: category.into(),
                    name: name.into()
                })
                .is_err()
            );
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        let legacy=br#"{"version":1,"alignment":"left","distribution":"horizontal_gaps","justification":"center_middle"}"#;
        fs::write(&path, legacy).unwrap();
        let p = a.read().unwrap();
        assert!(p.style_choices.is_empty());
        assert!(p.canvas_snapping);
        assert!(!p.auto_switch_tool);
    }
    #[test]
    fn independent_choices_persist_and_merge_across_windows() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let b = Store::open(temp.path()).unwrap();
        a.remember(Choice::Alignment(LayoutAction::Right)).unwrap();
        b.remember(Choice::Distribution(LayoutAction::VerticalGaps))
            .unwrap();
        a.remember(Choice::Justification(Justification::BottomCenter))
            .unwrap();
        b.remember(Choice::CanvasSnapping(false)).unwrap();
        let loaded = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(loaded.alignment, LayoutAction::Right);
        assert_eq!(loaded.distribution, LayoutAction::VerticalGaps);
        assert_eq!(loaded.justification, Justification::BottomCenter);
        assert!(!loaded.canvas_snapping);
        let bytes = fs::read(temp.path().join("quick-controls.json")).unwrap();
        assert!(
            a.remember(Choice::Alignment(LayoutAction::HorizontalGaps))
                .is_err()
        );
        assert_eq!(
            fs::read(temp.path().join("quick-controls.json")).unwrap(),
            bytes
        );
        a.remember(Choice::Alignment(LayoutAction::BottomRight))
            .unwrap();
        let loaded = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(loaded.alignment, LayoutAction::BottomRight);
        assert_eq!(loaded.distribution, LayoutAction::VerticalGaps);
        assert_eq!(loaded.justification, Justification::BottomCenter);
    }
    #[test]
    fn invalid_preferences_are_preserved_instead_of_silently_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("quick-controls.json");
        for bytes in [b"{broken".to_vec(),vec![b'x';16385],br#"{"version":99,"alignment":"left","distribution":"horizontal_gaps","justification":"center_middle"}"#.to_vec()] {
            fs::write(&path,&bytes).unwrap();assert!(Store::open(temp.path()).is_err());assert_eq!(fs::read(&path).unwrap(),bytes);
        }
    }
    #[test]
    fn automatic_tool_switching_defaults_off_but_explicit_on_survives_restart() {
        assert!(!Preferences::default().auto_switch_tool);
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        assert!(!store.read().unwrap().auto_switch_tool);
        store.remember(Choice::AutoSwitchTool(true)).unwrap();
        assert!(
            Store::open(temp.path())
                .unwrap()
                .read()
                .unwrap()
                .auto_switch_tool
        );
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn startup_and_tray_preferences_migrate_persist_and_preserve_other_settings() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open(root.path()).unwrap();
        let original = store.read().unwrap();
        assert!(original.close_to_tray);
        assert_eq!(original.startup_window, StartupWindow::Editor);
        let mut legacy = serde_json::to_value(&original).unwrap();
        legacy.as_object_mut().unwrap().remove("close_to_tray");
        legacy.as_object_mut().unwrap().remove("startup_window");
        let migrated: Preferences = serde_json::from_value(legacy).unwrap();
        migrated.validate().unwrap();
        assert!(migrated.close_to_tray);
        store.remember(Choice::CloseToTray(false)).unwrap();
        store
            .remember(Choice::StartupWindow(StartupWindow::CaptureController))
            .unwrap();
        let next = Store::open(root.path()).unwrap().read().unwrap();
        assert!(!next.close_to_tray);
        assert_eq!(next.startup_window, StartupWindow::CaptureController);
        assert_eq!(next.version, 8);
        assert_eq!(next.keybinds, original.keybinds);
        assert_eq!(next.favorite_tools, original.favorite_tools);
        assert_eq!(next.toolbar_order, original.toolbar_order);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Folder {
    Open,
    Project,
    Export,
}
fn validate_folder(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path.to_string_lossy().len() > 2048
        || path.to_string_lossy().chars().any(char::is_control)
    {
        return Err(Error::Invalid("Invalid remembered folder".into()));
    }
    Ok(())
}
#[cfg(test)]
mod folder_tests {
    use super::*;
    #[test]
    fn folders_survive_restart_without_overwriting_other_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        for kind in [Folder::Open, Folder::Project, Folder::Export] {
            store
                .remember(Choice::Folder(kind, dir.path().to_path_buf()))
                .unwrap();
        }
        store.remember(Choice::CanvasSnapping(false)).unwrap();
        store.remember(Choice::ThumbnailDetails(true)).unwrap();
        let reopened = Store::open(dir.path()).unwrap().read().unwrap();
        assert_eq!(reopened.folders.len(), 3);
        assert!(reopened.thumbnail_details);
        assert!(!Preferences::default().thumbnail_details);
        assert!(!reopened.canvas_snapping);
        let before = std::fs::read(dir.path().join("quick-controls.json")).unwrap();
        assert!(
            store
                .remember(Choice::Folder(Folder::Open, "relative".into()))
                .is_err()
        );
        assert_eq!(
            before,
            std::fs::read(dir.path().join("quick-controls.json")).unwrap()
        );
    }
}
