use super::{
    CaptureHotkey,
    registrations::{Native, Registrations},
};
use capture_core::keybinds::{Chord, Keybinds};
use std::{
    cell::RefCell,
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};
use windows_sys::Win32::{
    System::Threading::GetCurrentThreadId,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub(super) enum Change {
    Stage(Keybinds),
    Finish(bool),
}
type Request = (Change, mpsc::SyncSender<Result<(), String>>);
pub(super) struct Worker {
    id: u32,
    thread: Option<JoinHandle<()>>,
    tx: mpsc::Sender<Request>,
}
const CHANGE: u32 = WM_APP + 29;
impl Worker {
    pub fn send(&self, change: Change) -> Result<(), String> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .send((change, tx))
            .map_err(|_| "Capture shortcut worker stopped")?;
        if unsafe { PostThreadMessageW(self.id, CHANGE, 0, 0) } == 0 {
            return Err("Cannot notify the capture shortcut worker".into());
        }
        rx.recv()
            .map_err(|_| "Capture shortcut worker stopped before applying changes".to_string())?
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take() {
            unsafe { PostThreadMessageW(self.id, WM_QUIT, 0, 0) };
            let _ = t.join();
        }
    }
}
struct HookState {
    keys: super::print_screen::PrintScreen,
    accepting: Arc<AtomicBool>,
    recording: Arc<AtomicBool>,
}
thread_local! { static PRINT_SCREEN: RefCell<Option<HookState>> = const { RefCell::new(None) }; }
unsafe extern "system" fn print_screen_hook(code: i32, message: usize, data: isize) -> isize {
    if code >= 0
        && matches!(
            message as u32,
            WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP
        )
    {
        let key = unsafe { &*(data as *const KBDLLHOOKSTRUCT) };
        if key.vkCode == u32::from(VK_SNAPSHOT) {
            let pressed = matches!(message as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            let mut modifiers = 0;
            for (vk, mask) in [
                (VK_MENU, 1),
                (VK_CONTROL, 2),
                (VK_SHIFT, 4),
                (VK_LWIN, 8),
                (VK_RWIN, 8),
            ] {
                if unsafe { GetAsyncKeyState(i32::from(vk)) } < 0 {
                    modifiers |= mask;
                }
            }
            let decision = PRINT_SCREEN.with(|slot| {
                slot.borrow_mut().as_mut().and_then(|s| {
                    s.keys.event(
                        pressed,
                        modifiers,
                        key.flags & LLKHF_INJECTED != 0,
                        s.accepting.load(Ordering::Acquire) || s.recording.load(Ordering::Acquire),
                    )
                })
            });
            if let Some(dispatch) = decision {
                if let Some(id) = dispatch {
                    unsafe {
                        PostThreadMessageW(GetCurrentThreadId(), WM_HOTKEY, id as usize, 0);
                    }
                }
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, message, data) }
}
struct Windows {
    hook: HHOOK,
}
impl Windows {
    fn register_print_screen(&mut self, id: i32, chord: Chord) -> Result<(), String> {
        if self.hook.is_null() {
            self.hook = unsafe {
                SetWindowsHookExW(
                    WH_KEYBOARD_LL,
                    Some(print_screen_hook),
                    windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()),
                    0,
                )
            };
            if self.hook.is_null() {
                return Err(format!(
                    "Print Screen is reserved and compatibility capture could not start ({}). Choose another key in File > Options > Keybinds.",
                    std::io::Error::last_os_error()
                ));
            }
        }
        PRINT_SCREEN.with(|slot| {
            slot.borrow_mut()
                .as_mut()
                .unwrap()
                .keys
                .bindings
                .push((id, chord))
        });
        Ok(())
    }
}
impl Native for Windows {
    fn register(&mut self, id: i32, chord: Chord) -> Result<(), String> {
        if unsafe {
            RegisterHotKey(
                null_mut(),
                id,
                u32::from(chord.modifiers) | MOD_NOREPEAT,
                chord.key.into(),
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            // Windows or another screenshot app may reserve Print Screen. Only
            // this explicitly configured key gets the narrow hook fallback.
            if chord.key == VK_SNAPSHOT && error.raw_os_error() == Some(1409) {
                return self.register_print_screen(id, chord);
            }
            Err(format!(
                "Windows could not register this combination ({error}). Choose another key in File > Options > Keybinds or release it in the other application."
            ))
        } else {
            Ok(())
        }
    }
    fn unregister(&mut self, id: i32) {
        let removed = PRINT_SCREEN.with(|slot| {
            let mut slot = slot.borrow_mut();
            let keys = &mut slot.as_mut().unwrap().keys;
            let old = keys.bindings.len();
            keys.bindings.retain(|(i, _)| *i != id);
            let removed = old != keys.bindings.len();
            if removed && keys.bindings.is_empty() {
                *keys = Default::default();
            }
            removed
        });
        if removed {
            if PRINT_SCREEN.with(|s| s.borrow().as_ref().unwrap().keys.bindings.is_empty()) {
                unsafe {
                    UnhookWindowsHookEx(self.hook);
                }
                self.hook = null_mut();
            }
            return;
        }
        unsafe { UnregisterHotKey(null_mut(), id) };
    }
}
pub(super) fn start(
    ctx: eframe::egui::Context,
    owner: Option<isize>,
) -> Result<CaptureHotkey, String> {
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (pressed_tx, pressed) = mpsc::sync_channel(1);
    let (tx, rx) = mpsc::channel::<Request>();
    let accepting = Arc::new(AtomicBool::new(false));
    let worker_accepting = accepting.clone();
    let recording = Arc::new(AtomicBool::new(false));
    let worker_recording = recording.clone();
    let thread = std::thread::Builder::new()
        .name("capture-hotkeys".into())
        .spawn(move || unsafe {
            let mut message = MSG::default();
            PeekMessageW(&mut message, null_mut(), 0, 0, PM_NOREMOVE);
            let mut state = Registrations::default();
            PRINT_SCREEN.with(|slot| {
                *slot.borrow_mut() = Some(HookState {
                    keys: Default::default(),
                    accepting: worker_accepting.clone(),
                    recording: worker_recording.clone(),
                })
            });
            let mut native = Windows { hook: null_mut() };
            if ready_tx.send(GetCurrentThreadId()).is_ok() {
                while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                    if message.message == CHANGE {
                        for (change, reply) in rx.try_iter() {
                            let result = match change {
                                Change::Stage(k) => state.stage(&k, &mut native),
                                Change::Finish(c) => {
                                    state.finish(c, &mut native);
                                    Ok(())
                                }
                            };
                            let _ = reply.send(result);
                        }
                    } else if message.message == WM_HOTKEY
                        && (worker_accepting.load(Ordering::Acquire)
                            || worker_recording.load(Ordering::Acquire))
                        && let Some(binding) = state.action(message.wParam as i32)
                    {
                        match pressed_tx.try_send(binding) {
                            Ok(()) => {
                                if !worker_recording.load(Ordering::Acquire) {
                                    ctx.send_viewport_cmd_to(
                                        eframe::egui::ViewportId::ROOT,
                                        eframe::egui::ViewportCommand::Minimized(false),
                                    );
                                }
                                if !worker_recording.load(Ordering::Acquire)
                                    && let Some(owner) = owner
                                    && IsIconic(owner as _) != 0
                                {
                                    ShowWindowAsync(owner as _, SW_RESTORE);
                                }
                                ctx.request_repaint();
                            }
                            Err(mpsc::TrySendError::Full(_)) => (),
                            Err(mpsc::TrySendError::Disconnected(_)) => break,
                        }
                    }
                }
            }
            state.clear(&mut native);
            PRINT_SCREEN.with(|slot| *slot.borrow_mut() = None);
        })
        .map_err(|e| format!("Cannot start capture shortcuts: {e}"))?;
    match ready_rx.recv() {
        Ok(id) => Ok(CaptureHotkey {
            pressed,
            accepting,
            recording,
            worker: Some(Worker {
                id,
                thread: Some(thread),
                tx,
            }),
        }),
        Err(_) => {
            let _ = thread.join();
            Err("Capture shortcut worker stopped during startup".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::Registration;
    #[test]
    #[ignore = "Opt-in isolated native message/registration test; no input injection or screenshots"]
    fn windows_thread_hotkey_dispatches_without_window_handle() {
        assert_eq!(
            std::env::var("CLIPDESIGNER_HOTKEY_TEST").as_deref(),
            Ok("1")
        );
        let mut keys = Keybinds::default();
        keys.bindings[0].chord = Chord {
            key: 0x87,
            modifiers: 3,
        };
        let h = CaptureHotkey::start(eframe::egui::Context::default(), None, &keys).unwrap();
        h.set_accepting(true);
        let thread = h.worker.as_ref().unwrap().id;
        assert_ne!(unsafe { PostThreadMessageW(thread, WM_HOTKEY, 1, 0) }, 0);
        assert_eq!(
            h.pressed
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            keys.bindings[0]
        );
        h.set_accepting(false);
        assert_ne!(unsafe { PostThreadMessageW(thread, WM_HOTKEY, 1, 0) }, 0);
        assert!(
            h.pressed
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );
    }
    #[test]
    #[ignore = "requires explicit desktop availability; temporarily registers Ctrl+Alt+F24"]
    fn windows_registration_conflict_and_drop_release_the_chord() {
        assert_eq!(
            std::env::var("CLIPDESIGNER_HOTKEY_TEST").as_deref(),
            Ok("1")
        );
        let ctx = eframe::egui::Context::default();
        let mut keys = Keybinds::default();
        keys.bindings[0].chord = Chord {
            key: 0x87,
            modifiers: 3,
        };
        let mut first = start(ctx.clone(), None).unwrap();
        first.stage(&keys).unwrap();
        first.finish(true).unwrap();
        let mut second = start(ctx, None).unwrap();
        assert!(second.stage(&keys).is_err());
        drop(first);
        second.stage(&keys).unwrap();
        second.finish(true).unwrap();
    }
}
