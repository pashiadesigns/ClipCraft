//! Capture intent, independent of platform access or editor document state.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selection {
    #[default]
    Region,
    Window,
    Fullscreen,
    Steps,
    Scrolling,
    AutomaticScrolling,
    GrabText,
}
impl Selection {
    pub const ALL: [Self; 7] = [
        Self::Region,
        Self::Window,
        Self::Fullscreen,
        Self::Steps,
        Self::Scrolling,
        Self::AutomaticScrolling,
        Self::GrabText,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Region => "Region",
            Self::Window => "Window",
            Self::Fullscreen => "Fullscreen",
            Self::Steps => "Steps",
            Self::Scrolling => "Scrolling",
            Self::AutomaticScrolling => "Automatic Scrolling",
            Self::GrabText => "Grab Text",
        }
    }
    pub fn available(self) -> bool {
        matches!(
            self,
            Self::Region | Self::Window | Self::Fullscreen | Self::Steps | Self::GrabText
        )
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    AllInOne,
    #[default]
    Image,
    Video,
}
impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::AllInOne => "All-in-One",
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub selection: Selection,
    pub preview: bool,
    pub clipboard: bool,
    pub cursor: bool,
    pub delay_seconds: u32,
    pub fullscreen: Fullscreen,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fullscreen {
    #[default]
    Pointer,
    All,
    Display(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub device: String,
    pub bounds: [i32; 4],
    pub primary: bool,
}
pub fn fullscreen_bounds(
    target: &Fullscreen,
    displays: &[Display],
    point: [i32; 2],
) -> Result<[i32; 4]> {
    if displays.is_empty() {
        return Err(Error::Invalid("No display is available".into()));
    }
    match target {
        Fullscreen::All => Ok([
            displays.iter().map(|d| d.bounds[0]).min().unwrap(),
            displays.iter().map(|d| d.bounds[1]).min().unwrap(),
            displays.iter().map(|d| d.bounds[2]).max().unwrap(),
            displays.iter().map(|d| d.bounds[3]).max().unwrap(),
        ]),
        Fullscreen::Display(device) => displays
            .iter()
            .find(|d| &d.device == device)
            .map(|d| d.bounds)
            .ok_or_else(|| {
                Error::Invalid(
                    "The selected display is disconnected. Choose another in Fullscreen settings."
                        .into(),
                )
            }),
        Fullscreen::Pointer => Ok(displays
            .iter()
            .find(|d| {
                point[0] >= d.bounds[0]
                    && point[0] < d.bounds[2]
                    && point[1] >= d.bounds[1]
                    && point[1] < d.bounds[3]
            })
            .or_else(|| displays.iter().find(|d| d.primary))
            .unwrap_or(&displays[0])
            .bounds),
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            selection: Selection::Region,
            preview: true,
            clipboard: false,
            cursor: false,
            delay_seconds: 0,
            fullscreen: Fullscreen::Pointer,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if let Fullscreen::Display(device) = &self.fullscreen
            && (device.is_empty() || device.len() > 128 || device.contains('\0'))
        {
            return Err(Error::Invalid("Invalid display selection".into()));
        }
        if ![0, 3, 5, 10].contains(&self.delay_seconds) {
            return Err(Error::Invalid(
                "Capture delay must be 0, 3, 5 or 10 seconds".into(),
            ));
        }
        Ok(())
    }
    pub fn request(&self, mode: Mode) -> Result<Self> {
        self.validate()?;
        if mode != Mode::Image {
            return Err(Error::Invalid(format!(
                "{} capture is planned for a later version",
                mode.label()
            )));
        }
        if !self.selection.available() {
            return Err(Error::Invalid(format!(
                "{} capture is not implemented yet",
                self.selection.label()
            )));
        }
        Ok(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fullscreen_targets_preserve_signed_bounds_and_reject_missing_displays() {
        let displays = vec![
            Display {
                device: "left".into(),
                bounds: [-1920, -200, 0, 880],
                primary: false,
            },
            Display {
                device: "main".into(),
                bounds: [0, 0, 2560, 1440],
                primary: true,
            },
        ];
        assert_eq!(
            fullscreen_bounds(&Fullscreen::All, &displays, [100, 100]).unwrap(),
            [-1920, -200, 2560, 1440]
        );
        assert_eq!(
            fullscreen_bounds(&Fullscreen::Display("left".into()), &displays, [100, 100]).unwrap(),
            displays[0].bounds
        );
        assert_eq!(
            fullscreen_bounds(&Fullscreen::Pointer, &displays, [-1500, 0]).unwrap(),
            displays[0].bounds
        );
        assert!(
            fullscreen_bounds(&Fullscreen::Display("missing".into()), &displays, [0, 0]).is_err()
        );
        let old: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.fullscreen, Fullscreen::Pointer);
    }
    #[test]
    fn capture_preferences_migrate_legacy_and_preserve_last_valid_settings() {
        use crate::quick_preferences::{Choice, Preferences, Store};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quick-controls.json");
        let mut old = serde_json::to_value(Preferences::default()).unwrap();
        old.as_object_mut().unwrap().remove("capture");
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.read().unwrap().capture, Settings::default());
        let settings = Settings {
            selection: Selection::Steps,
            preview: false,
            clipboard: true,
            cursor: true,
            delay_seconds: 5,
            fullscreen: Fullscreen::All,
        };
        let saved = store.remember(Choice::Capture(settings.clone())).unwrap();
        assert_eq!(saved.version, 5);
        assert_eq!(
            Store::open(dir.path()).unwrap().read().unwrap().capture,
            settings
        );
        let before = std::fs::read(&path).unwrap();
        let bad = Settings {
            delay_seconds: 999,
            ..settings
        };
        assert!(store.remember(Choice::Capture(bad)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    #[test]
    fn future_modes_never_dispatch_and_requests_freeze_preferences() {
        let mut settings = Settings::default();
        let request = settings.request(Mode::Image).unwrap();
        settings.cursor = true;
        settings.clipboard = true;
        settings.delay_seconds = 5;
        assert_eq!(request, Settings::default());
        for mode in [Mode::AllInOne, Mode::Video] {
            assert!(settings.request(mode).is_err());
        }
        for selection in Selection::ALL {
            settings.selection = selection;
            assert_eq!(
                settings.request(Mode::Image).is_ok(),
                matches!(
                    selection,
                    Selection::Region
                        | Selection::Window
                        | Selection::Fullscreen
                        | Selection::Steps
                        | Selection::GrabText
                )
            );
        }
        settings.delay_seconds = 9;
        assert!(settings.validate().is_err());
    }
}
