use super::{Action, Tray};
use std::{
    ptr::{null, null_mut},
    sync::mpsc,
    thread::JoinHandle,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
    UI::{Shell::*, WindowsAndMessaging::*},
};

const CALLBACK: u32 = WM_APP + 43;
const ACTIVATE: u32 = WM_APP + 44;
const ACTIVATE_ACK: usize = 0x43434150;
const ICON_ID: u32 = 1;
// Windows SDK macro: NIN_SELECT | NINF_KEY (not emitted by windows-sys).
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub(super) fn activate(window: capture_core::quick_preferences::StartupWindow) -> bool {
    let id = if window == capture_core::quick_preferences::StartupWindow::Editor {
        1
    } else {
        2
    };
    unsafe {
        let hwnd = FindWindowW(wide("ClipCraft.NotificationArea").as_ptr(), null());
        let mut response = 0;
        !hwnd.is_null()
            && SendMessageTimeoutW(hwnd, ACTIVATE, id, 0, SMTO_ABORTIFHUNG, 250, &mut response) != 0
            && response == ACTIVATE_ACK
    }
}
fn menu_action(id: u32) -> Option<Action> {
    match id {
        1 => Some(Action::Editor),
        2 => Some(Action::Controller),
        3 => Some(Action::Capture),
        4 => Some(Action::Exit),
        _ => None,
    }
}
fn callback_action(lparam: LPARAM) -> Option<Action> {
    let value = lparam as u32;
    (value >> 16 == ICON_ID && matches!(value & 0xffff, NIN_SELECT | NIN_KEYSELECT))
        .then_some(Action::Editor)
}
struct State {
    ctx: crate::egui::Context,
    events: mpsc::Sender<Action>,
    icon: HICON,
    taskbar_created: u32,
}
impl State {
    fn send(&self, action: Action) {
        let _ = self.events.send(action);
        self.ctx.request_repaint_of(crate::egui::ViewportId::ROOT);
    }
    unsafe fn add(&self, hwnd: HWND) -> bool {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: self.icon,
            ..Default::default()
        };
        for (slot, value) in data
            .szTip
            .iter_mut()
            .zip(wide("ClipCraft · capture and editor"))
        {
            *slot = value;
        }
        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
            return false;
        }
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        if unsafe { Shell_NotifyIconW(NIM_SETVERSION, &data) } == 0 {
            unsafe {
                Shell_NotifyIconW(NIM_DELETE, &data);
            }
            return false;
        }
        true
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = unsafe { &*(l as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
    }
    let state = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() };
    if let Some(state) = state {
        if msg == ACTIVATE {
            if let Some(action @ (Action::Editor | Action::Controller)) = menu_action(w as u32) {
                state.send(action);
            }
            return ACTIVATE_ACK as LRESULT;
        }
        if msg == state.taskbar_created {
            // Explorer loses notification icons when it restarts. A hidden
            // top-level window receives its broadcast (HWND_MESSAGE does not).
            if !unsafe { state.add(hwnd) } {
                state.send(Action::Unavailable);
            }
            return 0;
        }
        if msg == CALLBACK {
            if let Some(action) = callback_action(l) {
                state.send(action);
            }
            if (l as u32) >> 16 == ICON_ID && (l as u32 & 0xffff) == WM_CONTEXTMENU {
                unsafe {
                    show_menu(hwnd, w, state);
                }
            }
            return 0;
        }
    }
    match msg {
        WM_CLOSE => {
            unsafe {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}
unsafe fn show_menu(hwnd: HWND, position: WPARAM, state: &State) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    let icons = [
        (1, MenuBitmap::new(&crate::brand::icon())),
        (2, MenuBitmap::new(&crate::brand::controller_icon())),
    ];
    for (id, label) in [
        (1, "Open ClipCraft Editor"),
        (2, "Open Capture Controller"),
        (3, "Capture"),
        (4, "Exit ClipCraft"),
    ] {
        if id == 4 {
            unsafe {
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
            }
        }
        unsafe {
            AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
        }
    }
    for (id, icon) in &icons {
        if !icon.0.is_null() {
            let info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_BITMAP,
                hbmpItem: icon.0,
                ..Default::default()
            };
            unsafe {
                SetMenuItemInfoW(menu, *id, 0, &info);
            }
        }
    }
    unsafe {
        SetMenuDefaultItem(menu, 1, 0);
        SetForegroundWindow(hwnd);
    }
    // Version-4 callback coordinates are signed screen coordinates, including
    // monitors left/above primary and keyboard invocation from the tray.
    let x = position as u16 as i16 as i32;
    let y = (position >> 16) as u16 as i16 as i32;
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            x,
            y,
            0,
            hwnd,
            null(),
        )
    };
    unsafe {
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
    if let Some(action) = menu_action(command as u32) {
        state.send(action);
    }
}

struct MenuBitmap(HBITMAP);
impl MenuBitmap {
    fn new(icon: &crate::egui::IconData) -> Self {
        let size = unsafe { GetSystemMetrics(SM_CXMENUCHECK) }.clamp(16, 64);
        let Some(image) = image::RgbaImage::from_raw(icon.width, icon.height, icon.rgba.clone())
        else {
            return Self(null_mut());
        };
        let pixels = image::imageops::resize(
            &image,
            size as u32,
            size as u32,
            image::imageops::FilterType::Lanczos3,
        );
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = null_mut();
        let bitmap = unsafe {
            CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0)
        };
        if !bitmap.is_null() && !bits.is_null() {
            let target = unsafe {
                std::slice::from_raw_parts_mut(bits.cast::<u8>(), (size * size * 4) as usize)
            };
            for (out, p) in target
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(pixels.pixels())
            {
                for (n, source) in [2, 1, 0].into_iter().enumerate() {
                    out[n] = (u16::from(p[source]) * u16::from(p[3]) / 255) as u8;
                }
                out[3] = p[3];
            }
        }
        Self(bitmap)
    }
}
impl Drop for MenuBitmap {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                DeleteObject(self.0);
            }
        }
    }
}
pub(super) struct Worker {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub(super) fn start(ctx: crate::egui::Context) -> Result<Tray, String> {
    let (ready_tx, ready) = mpsc::sync_channel(1);
    let (events_tx, events) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("clipcraft-tray".into())
        .spawn(move || unsafe {
            let instance = GetModuleHandleW(null());
            let class = wide("ClipCraft.NotificationArea");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance,
                lpszClassName: class.as_ptr(),
                ..Default::default()
            };
            if RegisterClassW(&wc) == 0 {
                let _ = ready_tx.send(Err("Cannot create the tray message window.".into()));
                return;
            }
            // LoadIconMetric requires ComCtl32 v6 activation even before main
            // runs. Use User32 directly so a missing activation manifest cannot
            // prevent startup. An explicit size retains a crisp small tray icon.
            let mut icon = LoadImageW(
                instance,
                101usize as *const u16,
                IMAGE_ICON,
                GetSystemMetrics(SM_CXSMICON),
                GetSystemMetrics(SM_CYSMICON),
                LR_DEFAULTCOLOR,
            );
            // LoadImage without LR_SHARED owns its icon; the fallback is shared.
            let owned_icon = !icon.is_null();
            if !owned_icon {
                icon = crate::brand::native_icon();
            }
            let state = Box::new(State {
                ctx,
                events: events_tx,
                icon,
                taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                wide("ClipCraft Tray").as_ptr(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                null_mut(),
                null_mut(),
                instance,
                (&*state as *const State).cast(),
            );
            let initialized =
                !hwnd.is_null() && !icon.is_null() && state.taskbar_created != 0 && state.add(hwnd);
            if initialized {
                let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                let mut message = MSG::default();
                loop {
                    let result = GetMessageW(&mut message, null_mut(), 0, 0);
                    if result <= 0 {
                        if result < 0 {
                            state.send(Action::Unavailable);
                        }
                        break;
                    }
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                Shell_NotifyIconW(
                    NIM_DELETE,
                    &NOTIFYICONDATAW {
                        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                        hWnd: hwnd,
                        uID: ICON_ID,
                        ..Default::default()
                    },
                );
            } else {
                let _ = ready_tx.send(Err(
                    "System tray is unavailable. ClipCraft will keep its editor accessible.".into(),
                ));
            }
            if !hwnd.is_null() {
                DestroyWindow(hwnd);
            }
            if owned_icon && !icon.is_null() {
                DestroyIcon(icon);
            }
            UnregisterClassW(class.as_ptr(), instance);
        })
        .map_err(|e| format!("Cannot start system tray: {e}"))?;
    match ready
        .recv()
        .unwrap_or_else(|_| Err("Tray worker stopped during startup.".into()))
    {
        Ok(thread_id) => Ok(Tray {
            events,
            _worker: Worker {
                thread_id,
                thread: Some(thread),
            },
        }),
        Err(error) => {
            let _ = thread.join();
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tray_menu_icons_are_small_owned_bitmaps() {
        for icon in [crate::brand::icon(), crate::brand::controller_icon()] {
            let bitmap = MenuBitmap::new(&icon);
            assert!(!bitmap.0.is_null());
            let mut info = BITMAP::default();
            assert_ne!(
                unsafe {
                    GetObjectW(
                        bitmap.0,
                        size_of::<BITMAP>() as i32,
                        (&mut info as *mut BITMAP).cast(),
                    )
                },
                0
            );
            assert!((16..=64).contains(&info.bmWidth));
            assert_eq!(info.bmHeight, info.bmWidth);
            assert_eq!(info.bmBitsPixel, 32);
        }
    }
    #[test]
    fn version_four_mouse_keyboard_and_menu_commands_route_only_valid_ids() {
        for event in [NIN_SELECT, NIN_KEYSELECT] {
            assert_eq!(
                callback_action(((ICON_ID << 16) | event) as isize),
                Some(Action::Editor)
            );
        }
        assert_eq!(callback_action(((2 << 16) | NIN_SELECT) as isize), None);
        assert_eq!(
            callback_action(((ICON_ID << 16) | WM_MOUSEMOVE) as isize),
            None
        );
        for (id, expected) in [
            (1, Action::Editor),
            (2, Action::Controller),
            (3, Action::Capture),
            (4, Action::Exit),
        ] {
            assert_eq!(menu_action(id), Some(expected));
        }
        assert_eq!(menu_action(0), None);
    }
}
