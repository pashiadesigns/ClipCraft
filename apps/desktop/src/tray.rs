//! Event-driven Windows notification icon. The worker owns all native resources;
//! editor commands run on the application's logic callback, even while hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Editor,
    Controller,
    Capture,
    Exit,
    Unavailable,
}

pub(super) struct Tray {
    events: std::sync::mpsc::Receiver<Action>,
    #[cfg(windows)]
    _worker: windows::Worker,
}
impl Tray {
    pub fn start(ctx: crate::egui::Context) -> Result<Self, String> {
        #[cfg(windows)]
        {
            windows::start(ctx)
        }
        #[cfg(not(windows))]
        {
            let _ = ctx;
            Err("System tray is available on Windows.".into())
        }
    }
    pub fn take(&self) -> Vec<Action> {
        self.events.try_iter().collect()
    }
}
#[cfg(windows)]
mod windows;
