//! Event-driven capture registration. Changes reserve new chords before releasing
//! old ones, then commit only after preferences are saved. No keyboard hook.
use capture_core::keybinds::{Binding, Keybinds};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Receiver,
};
mod registrations;
#[cfg(windows)]
mod windows;

pub(super) trait Registration {
    fn stage(&mut self, keys: &Keybinds) -> Result<(), String>;
    fn finish(&mut self, commit: bool) -> Result<(), String>;
}
/// Retain the previous active registrations if registration OR persistence fails.
pub(super) fn apply(
    registry: &mut impl Registration,
    keys: &Keybinds,
    persist: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    keys.validate().map_err(|e| e.to_string())?;
    registry.stage(keys)?;
    if let Err(e) = persist() {
        let rollback = registry.finish(false);
        return Err(match rollback {
            Ok(()) => e,
            Err(r) => format!("{e}. Shortcut rollback failed: {r}"),
        });
    }
    registry.finish(true)
}

pub struct CaptureHotkey {
    pressed: Receiver<Binding>,
    accepting: Arc<AtomicBool>,
    recording: Arc<AtomicBool>,
    #[cfg(windows)]
    worker: Option<windows::Worker>,
}
impl CaptureHotkey {
    #[cfg(test)]
    pub(super) fn test_pressed(binding: Binding) -> Self {
        let (send, pressed) = std::sync::mpsc::channel();
        send.send(binding).unwrap();
        Self {
            pressed,
            accepting: Arc::new(AtomicBool::new(true)),
            recording: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            worker: None,
        }
    }
    pub fn start(
        ctx: eframe::egui::Context,
        owner: Option<isize>,
        keys: &Keybinds,
    ) -> Result<Self, String> {
        #[cfg(windows)]
        {
            // Thread registrations do not require an HWND. Some eframe startup
            // paths expose the window handle only after CreationContext.
            let mut worker = windows::start(ctx, owner)?;
            worker.stage(keys)?;
            worker.finish(true)?;
            Ok(worker)
        }
        #[cfg(not(windows))]
        {
            let _ = (ctx, owner, keys);
            Err("Global capture shortcuts are available on Windows.".into())
        }
    }
    /// One event per burst; busy editors discard events rather than replay later.
    pub fn take_pressed(&self) -> Option<Binding> {
        let first = self.pressed.try_recv().ok();
        self.pressed.try_iter().for_each(drop);
        first
    }
    pub fn set_accepting(&self, value: bool) {
        self.accepting.store(value, Ordering::Release);
        if !value && !self.recording.load(Ordering::Acquire) {
            self.take_pressed();
        }
    }
    pub fn set_recording(&self, value: bool) {
        self.recording.store(value, Ordering::Release);
    }
}
impl Registration for CaptureHotkey {
    fn stage(&mut self, keys: &Keybinds) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.worker
                .as_ref()
                .ok_or("Shortcut worker stopped")?
                .send(windows::Change::Stage(keys.clone()))
        }
        #[cfg(not(windows))]
        {
            let _ = keys;
            Err("Global capture shortcuts are available on Windows.".into())
        }
    }
    fn finish(&mut self, commit: bool) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.worker
                .as_ref()
                .ok_or("Shortcut worker stopped")?
                .send(windows::Change::Finish(commit))
        }
        #[cfg(not(windows))]
        {
            let _ = commit;
            Err("Global capture shortcuts are available on Windows.".into())
        }
    }
}
pub fn owner(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        cc.window_handle().ok().and_then(|h| match h.as_raw() {
            RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
            _ => None,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = cc;
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bursts_are_consumed_once_and_never_replayed_after_busy_input() {
        let (send, pressed) = std::sync::mpsc::channel();
        let h = CaptureHotkey {
            pressed,
            accepting: Arc::new(AtomicBool::new(true)),
            recording: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            worker: None,
        };
        let b = Keybinds::default().bindings[0];
        for _ in 0..100 {
            send.send(b).unwrap();
        }
        assert_eq!(h.take_pressed(), Some(b));
        assert!(h.take_pressed().is_none());
        send.send(b).unwrap();
        h.set_accepting(false);
        assert!(h.take_pressed().is_none());
        h.set_recording(true);
        send.send(b).unwrap();
        h.set_accepting(false);
        assert_eq!(h.take_pressed(), Some(b));
    }
}
