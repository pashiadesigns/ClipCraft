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
}
impl Selection {
    pub const ALL: [Self; 5] = [
        Self::Region,
        Self::Window,
        Self::Fullscreen,
        Self::Steps,
        Self::Scrolling,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Region => "Region",
            Self::Window => "Window",
            Self::Fullscreen => "Fullscreen",
            Self::Steps => "Step by Step",
            Self::Scrolling => "Scrolling",
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
    pub scroll_automatic: bool,
    pub scrolling: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fullscreen {
    #[default]
    Ask,
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
        Fullscreen::Ask => Err(Error::Invalid(
            "Select a monitor in the capture preview.".into(),
        )),
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
        Self::for_selection(Selection::Region)
    }
}
impl Settings {
    pub fn for_selection(selection: Selection) -> Self {
        Self {
            selection,
            preview: true,
            clipboard: !matches!(selection, Selection::Steps),
            cursor: selection == Selection::Steps,
            delay_seconds: 0,
            fullscreen: Fullscreen::Ask,
            scroll_automatic: true,
            scrolling: selection == Selection::Scrolling,
        }
    }
}
impl Settings {
    pub fn is_scrolling(&self) -> bool {
        self.selection == Selection::Scrolling
            || self.scrolling && self.selection != Selection::Steps
    }
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
    pub fn request(&self) -> Result<Self> {
        self.validate()?;
        let mut request = self.clone();
        // Until fixed header/footer exclusion is available, scrolling always
        // asks for an explicit content region, including saved older choices.
        if request.is_scrolling() {
            request.selection = Selection::Region;
            request.scrolling = true;
        }
        Ok(request)
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
        assert_eq!(old.fullscreen, Fullscreen::Ask);
    }
    #[test]
    fn capture_preferences_migrate_legacy_and_preserve_last_valid_settings() {
        use crate::quick_preferences::{Choice, Preferences, Store};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("quick-controls-v2.json");
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
            scroll_automatic: false,
            scrolling: false,
        };
        let saved = store.remember(Choice::Capture(settings.clone())).unwrap();
        assert_eq!(saved.version, 12);
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
    fn requests_freeze_preferences_and_validate_supported_modes() {
        let mut settings = Settings::default();
        let request = settings.request().unwrap();
        settings.cursor = true;
        settings.clipboard = true;
        settings.delay_seconds = 5;
        assert_eq!(request, Settings::default());
        for selection in Selection::ALL {
            settings.selection = selection;
            assert!(settings.request().is_ok());
        }
        settings.delay_seconds = 9;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn mode_profiles_persist_independently_and_steps_default_to_cursor() {
        use crate::quick_preferences::{Choice, Preferences, Store};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let defaults = Preferences::default();
        assert!(defaults.capture_for(Selection::Steps).cursor);
        assert!(!defaults.capture_for(Selection::Window).cursor);
        let region = Settings {
            clipboard: true,
            delay_seconds: 3,
            ..Default::default()
        };
        store.remember(Choice::Capture(region.clone())).unwrap();
        let window = Settings {
            selection: Selection::Window,
            preview: false,
            cursor: true,
            ..Default::default()
        };
        store.remember(Choice::Capture(window.clone())).unwrap();
        let steps = Settings {
            selection: Selection::Steps,
            cursor: false,
            ..Default::default()
        };
        store.remember(Choice::Capture(steps.clone())).unwrap();
        let reopened = Store::open(dir.path()).unwrap().read().unwrap();
        assert_eq!(reopened.capture_for(Selection::Region), region);
        assert_eq!(reopened.capture_for(Selection::Window), window);
        assert_eq!(
            reopened.capture_for(Selection::Steps),
            steps,
            "explicit cursor-off must survive restart"
        );
        let mut bad = reopened.clone();
        bad.capture_profiles.push(region.clone());
        std::fs::write(
            dir.path().join("quick-controls-v2.json"),
            serde_json::to_vec(&bad).unwrap(),
        )
        .unwrap();
        assert!(
            store.read().is_err(),
            "ambiguous duplicate profiles must fail"
        );
        let mut legacy = serde_json::to_value(&reopened).unwrap();
        legacy.as_object_mut().unwrap().remove("capture_profiles");
        legacy["version"] = 9.into();
        legacy["capture"] = serde_json::to_value(&region).unwrap();
        let legacy: Preferences = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.capture_for(Selection::Region), region);
        assert!(legacy.capture_for(Selection::Steps).cursor);
    }
}
