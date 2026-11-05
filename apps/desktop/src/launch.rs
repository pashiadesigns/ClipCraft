//! Explicit shortcuts override the saved startup preference without changing it.
use capture_core::quick_preferences::StartupWindow;
use std::{ffi::OsString, path::PathBuf};
#[derive(Default)]
pub(super) struct Request {
    pub file: Option<PathBuf>,
    pub window: Option<StartupWindow>,
}
impl Request {
    pub fn read() -> Self {
        Self::parse(std::env::args_os().nth(1))
    }
    fn parse(arg: Option<OsString>) -> Self {
        match arg.as_deref().and_then(|a| a.to_str()) {
            Some("--editor") => Self {
                window: Some(StartupWindow::Editor),
                ..Default::default()
            },
            Some("--clip") => Self {
                window: Some(StartupWindow::Clip),
                ..Default::default()
            },
            _ => Self {
                file: arg.map(PathBuf::from),
                ..Default::default()
            },
        }
    }
    pub fn startup(&self, saved: StartupWindow) -> StartupWindow {
        if self.file.is_some() {
            StartupWindow::Editor
        } else {
            self.window.unwrap_or(saved)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcuts_target_the_right_window_without_becoming_image_paths() {
        for (arg, expected) in [
            ("--editor", StartupWindow::Editor),
            ("--clip", StartupWindow::Clip),
        ] {
            let request = Request::parse(Some(arg.into()));
            assert!(request.file.is_none());
            for saved in [StartupWindow::Editor, StartupWindow::Clip] {
                assert_eq!(request.startup(saved), expected);
            }
        }
        let file = Request::parse(Some("Example with spaces.iclip".into()));
        assert!(file.file.is_some());
        assert_eq!(file.startup(StartupWindow::Clip), StartupWindow::Editor);
        assert_eq!(
            Request::default().startup(StartupWindow::Clip),
            StartupWindow::Clip
        );
    }
}
