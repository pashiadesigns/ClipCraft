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
    Clip,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub audio_outputs: AudioOutputs,
    #[serde(default = "snapping_default")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub startup_window: StartupWindow,
    #[serde(default)]
    pub keybinds: crate::keybinds::Keybinds,
    #[serde(default)]
    pub capture: crate::capture_options::Settings,
    /// Capture Clip shows its settings window first; hotkeys and the tray never do.
    #[serde(default = "snapping_default")]
    pub ask_before_clip: bool,
    #[serde(default)]
    pub video: crate::video::Options,
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
    #[serde(default)]
    pub drawing_styles: Vec<crate::object_styles::ObjectStyle>,
    /// Saved export watermarks and the one used by default (FX-003).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub watermarks: Vec<crate::watermark::Watermark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_watermark: Option<String>,
    /// New Image and New Video remember their background (bk-proposal).
    #[serde(default)]
    pub blank: BlankDefaults,
    #[serde(default)]
    pub workspace: Workspace,
    /// Find Sensitive Info custom types (approved beta 5 tab 3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive_types: Vec<crate::privacy::custom::CustomType>,
    /// Preferences › Team (approved beta 6 tab 3).
    #[serde(default)]
    pub team: Team,
}
/// Preferences › Team: a shared folder of kits, and locks for projects
/// opened from shared folders.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Team {
    /// Kits here load automatically and update when the folder does.
    pub folder: Option<PathBuf>,
    /// Kits in the folder that are turned off, by file name.
    pub off: Vec<String>,
    /// Each kit file added, with the modified time it was added at, so a
    /// kit is added again only when it changes.
    pub added: std::collections::BTreeMap<String, u64>,
    pub lock_shared: bool,
    /// Shown to others on a lock; empty uses the Windows user name.
    pub display_name: String,
}
impl Default for Team {
    fn default() -> Self {
        Self {
            folder: None,
            off: vec![],
            added: Default::default(),
            lock_shared: true,
            display_name: String::new(),
        }
    }
}
impl Team {
    pub fn validate(&self) -> Result<()> {
        if self.display_name.chars().count() > 60
            || self.display_name.chars().any(char::is_control)
            || self.off.len() > 256
            || self.added.len() > 256
            || self
                .folder
                .as_ref()
                .is_some_and(|f| f.as_os_str().len() > 1024 || !f.is_absolute())
        {
            return Err(Error::Invalid(
                "Team settings need a name of up to 60 characters and a full folder path.".into(),
            ));
        }
        Ok(())
    }
}
/// The key that turns snapping off for one drag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BypassKey {
    #[default]
    Ctrl,
    Shift,
    Alt,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanvasBackground {
    #[default]
    Charcoal,
    Graphite,
    Black,
    Light,
}
/// Preferences > Editor and the settings other parts of the app read.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub snap_move: bool,
    pub snap_resize: bool,
    pub snap_draw: bool,
    pub show_grid: bool,
    pub grid_size: u32,
    pub bypass: BypassKey,
    pub background: CanvasBackground,
    pub remember_view: bool,
    /// None is fit to the window.
    pub zoom: Option<f32>,
    pub panel_width: Option<f32>,
    pub show_tooltips: bool,
    pub keep_versions: bool,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            snap_move: true,
            snap_resize: true,
            snap_draw: true,
            show_grid: false,
            grid_size: 20,
            bypass: BypassKey::Ctrl,
            background: CanvasBackground::Charcoal,
            remember_view: true,
            zoom: None,
            panel_width: None,
            show_tooltips: true,
            keep_versions: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlankDefaults {
    pub transparent: bool,
    pub image_color: [u8; 4],
    pub video_color: [u8; 4],
}
impl Default for BlankDefaults {
    fn default() -> Self {
        Self {
            transparent: false,
            image_color: [255; 4],
            video_color: [255; 4],
        }
    }
}
fn snapping_default() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            audio_outputs: AudioOutputs::default(),
            version: 12,
            close_to_tray: true,
            startup_window: StartupWindow::Editor,
            keybinds: Default::default(),
            capture: Default::default(),
            ask_before_clip: true,
            video: Default::default(),
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
            drawing_styles: vec![],
            watermarks: vec![],
            default_watermark: None,
            blank: BlankDefaults::default(),
            workspace: Workspace::default(),
            sensitive_types: Vec::new(),
            team: Team::default(),
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
            .unwrap_or_else(|| crate::capture_options::Settings::for_selection(selection))
    }

    pub fn choose(&mut self, choice: Choice) -> Result<()> {
        match choice {
            Choice::AudioOutputs(value) => {
                if [&value.speakers, &value.headphones]
                    .iter()
                    .any(|id| id.len() > 2048 || id.contains('\0'))
                {
                    return Err(Error::Invalid("Choose a valid audio output device.".into()));
                }
                self.audio_outputs = value;
            }
            Choice::ThumbnailDetails(value) => {
                self.thumbnail_details = value;
            }
            Choice::Folder(kind, path) => {
                validate_folder(&path)?;
                self.folders.insert(kind, path);
            }
            Choice::CloseToTray(value) => {
                self.close_to_tray = value;
            }
            Choice::StartupWindow(value) => {
                self.startup_window = value;
            }
            Choice::AskBeforeClip(value) => self.ask_before_clip = value,
            Choice::Blank(value) => self.blank = value,
            Choice::Workspace(value) => {
                if !(4..=512).contains(&value.grid_size) {
                    return Err(Error::Invalid(
                        "Grid size runs from 4 to 512 pixels.".into(),
                    ));
                }
                self.workspace = value;
            }
            Choice::RestoreToolbar { order, favorites } => {
                self.toolbar_order = order;
                self.favorite_tools = favorites;
            }
            Choice::RestoreFolders(folders) => self.folders = folders,
            Choice::SensitiveTypes(list) => {
                crate::privacy::custom::validate_all(&list)?;
                self.sensitive_types = list;
            }
            Choice::Team(team) => {
                team.validate()?;
                self.team = team;
            }
            Choice::Watermarks(list, default) => {
                if list.len() > 32
                    || default
                        .as_ref()
                        .is_some_and(|d| !list.iter().any(|w| &w.name == d))
                {
                    return Err(Error::Invalid("Keep up to 32 watermark presets.".into()));
                }
                for w in &list {
                    w.validate()?;
                }
                self.watermarks = list;
                self.default_watermark = default;
            }
            Choice::Keybinds(keys) => {
                keys.validate()?;
                self.keybinds = keys;
            }
            Choice::Video(options) => {
                options.validate()?;
                self.video = options;
            }
            Choice::Capture(settings) => {
                settings.validate()?;
                // Retain the current mode before switching to another.
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
            }
            Choice::Alignment(action) if action.minimum_objects() == 2 => self.alignment = action,
            Choice::Distribution(action) if action.minimum_objects() == 3 => {
                self.distribution = action
            }
            Choice::Justification(value) => self.justification = value,
            Choice::CanvasSnapping(value) => self.canvas_snapping = value,
            Choice::SnappingLimit(value) => {
                self.snapping_limit = value;
            }
            Choice::AutoSwitchTool(value) => self.auto_switch_tool = value,
            Choice::DrawingStyle(style) => {
                style.validate()?;
                self.drawing_styles.retain(|s| !s.same_variant(&style));
                self.drawing_styles.push(style);
            }
            Choice::MoveTool { tool, before } => {
                crate::toolbar::move_before(&mut self.toolbar_order, tool, before)?;
                // Older apps must not silently drop a customized order when
                // rewriting their known preference fields.
            }
            Choice::ResetToolbar => {
                self.toolbar_order = crate::toolbar::default_order();
                // Preserve membership; reset only each section's ordering.
                self.favorite_tools
                    .sort_by_key(|t| self.toolbar_order.iter().position(|id| id == t));
            }
            Choice::FavoriteTool { tool, favorite } => {
                let tool = crate::toolbar::slot(tool);

                if favorite && !self.favorite_tools.contains(&tool) {
                    self.favorite_tools.push(tool);
                }
                if !favorite {
                    self.favorite_tools.retain(|t| *t != tool);
                }
            }
            Choice::MoveSectionTool { tool, before } => {
                crate::toolbar::move_in_group(
                    &mut self.toolbar_order,
                    &mut self.favorite_tools,
                    tool,
                    before,
                )?;
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
        self.video.validate()?;
        if self.version != 12
            || self.folders.len() > 3
            || self.snapping_limit.is_some_and(|n| n == 0 || n > 10000)
            || self.capture_profiles.len() > crate::capture_options::Selection::ALL.len()
            || self.alignment.minimum_objects() != 2
            || self.distribution.minimum_objects() != 3
            || self.style_choices.len() > 24
            || self.drawing_styles.len() > 21
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
    AudioOutputs(AudioOutputs),
    Folder(Folder, PathBuf),
    ThumbnailDetails(bool),
    CloseToTray(bool),
    StartupWindow(StartupWindow),
    AskBeforeClip(bool),
    Blank(BlankDefaults),
    Workspace(Workspace),
    /// Cancel in Preferences puts these back exactly.
    RestoreToolbar {
        order: Vec<crate::toolbar::ToolId>,
        favorites: Vec<crate::toolbar::ToolId>,
    },
    RestoreFolders(std::collections::BTreeMap<Folder, PathBuf>),
    /// Watermark presets and the default preset's name.
    Watermarks(Vec<crate::watermark::Watermark>, Option<String>),
    SensitiveTypes(Vec<crate::privacy::custom::CustomType>),
    Team(Team),
    Keybinds(crate::keybinds::Keybinds),
    Capture(crate::capture_options::Settings),
    Video(crate::video::Options),
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
    Style {
        category: String,
        name: String,
    },
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioOutputs {
    pub speakers: String,
    pub headphones: String,
    pub use_headphones: bool,
}
impl AudioOutputs {
    pub fn selected(&self) -> &str {
        if self.use_headphones {
            &self.headphones
        } else {
            &self.speakers
        }
    }
}

/// Saved settings hold custom sensitive types and watermarks, a logo each
/// (BW6: a kit with those passed the old 16 KB cap, so adding it failed).
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

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
        let path = self.root.join("quick-controls-v2.json");
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Preferences::default()),
            Err(e) => return Err(e.into()),
        };
        if metadata.len() > MAX_FILE_BYTES {
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
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Invalid(
                "Your saved settings would pass 16 MB. Remove some saved watermarks or custom sensitive types, then try again.".into(),
            ));
        }
        atomic_write(&self.root.join("quick-controls-v2.json"), &bytes)?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_profiles_copy_images_by_default_but_preserve_explicit_opt_out() {
        use crate::capture_options::{Selection, Settings};
        let mut p = super::Preferences::default();
        for selection in Selection::ALL {
            let s = p.capture_for(selection);
            assert_eq!(s.clipboard, !matches!(selection, Selection::Steps));
            assert_eq!(s.cursor, selection == Selection::Steps);
        }
        p.choose(super::Choice::Capture(Settings {
            clipboard: false,
            ..Settings::default()
        }))
        .unwrap();
        let decoded: super::Preferences =
            serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert!(!decoded.capture_for(Selection::Region).clipboard);
        assert!(decoded.capture_for(Selection::Window).clipboard);
    }
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
        assert_eq!(loaded.version, 12);
        assert_eq!(loaded.keybinds, keys);
        assert_eq!(loaded.keybinds.bindings, capture);
        assert_eq!(loaded.favorite_tools, crate::toolbar::default_favorites());
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
        assert_eq!(loaded.version, 12);
        assert_eq!(loaded.drawing_styles, [rectangle.clone(), text]);
        let path = temp.path().join("quick-controls-v2.json");
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
        let path = temp.path().join("quick-controls-v2.json");
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
        let legacy=br#"{"version":12,"alignment":"left","distribution":"horizontal_gaps","justification":"center_middle"}"#;
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
        let bytes = fs::read(temp.path().join("quick-controls-v2.json")).unwrap();
        assert!(
            a.remember(Choice::Alignment(LayoutAction::HorizontalGaps))
                .is_err()
        );
        assert_eq!(
            fs::read(temp.path().join("quick-controls-v2.json")).unwrap(),
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
        let path = temp.path().join("quick-controls-v2.json");
        for bytes in [b"{broken".to_vec(),vec![b'x';16385],br#"{"version":99,"alignment":"left","distribution":"horizontal_gaps","justification":"center_middle"}"#.to_vec()] {
            fs::write(&path,&bytes).unwrap();assert!(Store::open(temp.path()).is_err());assert_eq!(fs::read(&path).unwrap(),bytes);
        }
    }
    /// BW6: settings past the old 16 KB cap (kits add custom types and
    /// watermarks here) are saved and read back.
    #[test]
    fn settings_past_sixteen_kilobytes_save_and_load() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let mut team = store.read().unwrap().team;
        for i in 0..200 {
            team.added
                .insert(format!("{i:03} {}", "kit name ".repeat(12)), i);
        }
        store.remember(Choice::Team(team.clone())).unwrap();
        assert!(
            fs::metadata(temp.path().join("quick-controls-v2.json"))
                .unwrap()
                .len()
                > 16384
        );
        assert_eq!(Store::open(temp.path()).unwrap().read().unwrap().team, team);
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
            .remember(Choice::StartupWindow(StartupWindow::Editor))
            .unwrap();
        let next = Store::open(root.path()).unwrap().read().unwrap();
        assert!(!next.close_to_tray);
        assert_eq!(next.startup_window, StartupWindow::Editor);
        assert_eq!(next.version, 12);
        assert_eq!(next.keybinds, original.keybinds);
        assert_eq!(next.favorite_tools, original.favorite_tools);
        assert_eq!(next.toolbar_order, original.toolbar_order);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Folder {
    ManagedProjects,
    Templates,
    Presets,
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
        let before = std::fs::read(dir.path().join("quick-controls-v2.json")).unwrap();
        assert!(
            store
                .remember(Choice::Folder(Folder::Open, "relative".into()))
                .is_err()
        );
        assert_eq!(
            before,
            std::fs::read(dir.path().join("quick-controls-v2.json")).unwrap()
        );
    }
}
