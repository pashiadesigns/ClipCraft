use super::{Action, Tray};
use std::{
    ptr::{null, null_mut},
    sync::mpsc,
    thread::JoinHandle,
};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
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
        let hwnd = FindWindowW(wide("ClipDesigner.NotificationArea").as_ptr(), null());
        if !hwnd.is_null() {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            AllowSetForegroundWindow(pid);
        }
        let mut response = 0;
        !hwnd.is_null()
            && SendMessageTimeoutW(hwnd, ACTIVATE, id, 0, SMTO_ABORTIFHUNG, 250, &mut response) != 0
            && response == ACTIVATE_ACK
    }
}
fn menu_action(id: u32) -> Option<Action> {
    match id {
        1 => Some(Action::Editor),
        2 => Some(Action::Clip),
        3 => Some(Action::CaptureClip),
        4 => Some(Action::Exit),
        5 => Some(Action::CaptureVideo),
        6 => Some(Action::Capture(
            capture_core::capture_options::Selection::Region,
        )),
        7 => Some(Action::Capture(
            capture_core::capture_options::Selection::Window,
        )),
        8 => Some(Action::Capture(
            capture_core::capture_options::Selection::Fullscreen,
        )),
        9 => Some(Action::Capture(
            capture_core::capture_options::Selection::Steps,
        )),
        10 => Some(Action::ToggleScrolling),
        11 => Some(Action::ToggleOpenAfter),
        _ => None,
    }
}
const OPEN_FILE: usize = 0x43434649;
pub(super) fn forward(request: &crate::launch::Request) -> bool {
    use std::os::windows::ffi::OsStrExt;
    let Some(file) = &request.file else {
        return super::activate(
            request
                .window
                .unwrap_or(capture_core::quick_preferences::StartupWindow::Editor),
        );
    };
    let absolute = if file.is_absolute() {
        file.clone()
    } else {
        std::env::current_dir().unwrap_or_default().join(file)
    };
    let payload: Vec<u16> = absolute.as_os_str().encode_wide().collect();
    if payload.is_empty() || payload.len() > 32768 || payload.contains(&0) {
        return false;
    }
    unsafe {
        let hwnd = FindWindowW(wide("ClipDesigner.NotificationArea").as_ptr(), null());
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        AllowSetForegroundWindow(pid);
        let data = COPYDATASTRUCT {
            dwData: OPEN_FILE,
            cbData: (payload.len() * 2) as u32,
            lpData: payload.as_ptr() as *mut _,
        };
        let mut result = 0;
        SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            0,
            &data as *const _ as isize,
            SMTO_ABORTIFHUNG,
            500,
            &mut result,
        ) != 0
            && result == ACTIVATE_ACK
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
            .zip(wide("ClipDesigner · capture and editor"))
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
    if msg == WM_MENUSELECT {
        let id = (w & 0xffff) as u32;
        let flags = (w >> 16) as u32;
        SELECTED_TOGGLE.with(|s| {
            s.set(
                (flags != 0xffff && flags & MF_POPUP == 0 && matches!(id, 10 | 11))
                    .then_some((l as HMENU, id)),
            )
        });
    }
    if let Some(state) = state {
        if msg == WM_COPYDATA {
            let Some(data) = (unsafe { (l as *const COPYDATASTRUCT).as_ref() }) else {
                return 0;
            };
            if data.dwData != OPEN_FILE
                || data.cbData == 0
                || data.cbData > 65536
                || data.cbData % 2 != 0
                || data.lpData.is_null()
            {
                return 0;
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize)
            };
            let wide: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();
            if wide.contains(&0) {
                return 0;
            }
            use std::os::windows::ffi::OsStringExt;
            let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&wide));
            if !path.is_absolute() {
                return 0;
            }
            state.send(Action::Open(path));
            return ACTIVATE_ACK as isize;
        }
        if msg == ACTIVATE {
            if let Some(action @ (Action::Editor | Action::Clip)) = menu_action(w as u32) {
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
    let capture = unsafe { CreatePopupMenu() };
    if capture.is_null() {
        unsafe {
            DestroyMenu(menu);
        }
        return;
    }
    let scrolling = state
        .ctx
        .data(|d| d.get_temp::<bool>(crate::egui::Id::unique("tray-scrolling")))
        .unwrap_or(false);
    for (id, label) in [
        (6, "Capture Region"),
        (7, "Capture Window"),
        (8, "Capture Fullscreen"),
        (9, "Capture Step by Step"),
        (10, "Scrolling (Region only)"),
    ] {
        unsafe {
            if id == 10 {
                AppendMenuW(capture, MF_SEPARATOR, 0, null());
            }
            AppendMenuW(
                capture,
                MF_STRING | if id == 10 && scrolling { MF_CHECKED } else { 0 },
                id,
                wide(label).as_ptr(),
            );
        }
    }
    let icons = [
        (1, MenuBitmap::new(&crate::brand::icon())),
        (2, MenuBitmap::new(&crate::brand::icon())),
        (3, MenuBitmap::new(&capture_icon(false))),
        (5, MenuBitmap::new(&capture_icon(true))),
        (6, MenuBitmap::new(&capture_icon(false))),
        (7, MenuBitmap::new(&capture_icon(false))),
        (8, MenuBitmap::new(&capture_icon(false))),
        (9, MenuBitmap::new(&capture_icon(false))),
    ];
    for (id, label) in [
        (1, "Open ClipDesigner Editor"),
        (2, "Capture Clip settings…"),
        (3, "Capture Clip"),
        (5, "Capture Video"),
        (11, "Open ClipDesigner after capture"),
        (4, "Exit ClipDesigner"),
    ] {
        if id == 4 {
            unsafe {
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
            }
        }
        unsafe {
            if id == 3 {
                AppendMenuW(menu, MF_POPUP, capture as usize, wide(label).as_ptr());
                let info = MENUITEMINFOW {
                    cbSize: size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_ID,
                    wID: 3,
                    ..Default::default()
                };
                SetMenuItemInfoW(menu, 2, 1, &info);
            } else {
                let checked = id == 11
                    && state
                        .ctx
                        .data(|d| d.get_temp::<bool>(crate::egui::Id::unique("tray-open-after")))
                        .unwrap_or(true);
                AppendMenuW(
                    menu,
                    MF_STRING | if checked { MF_CHECKED } else { 0 },
                    id,
                    wide(label).as_ptr(),
                );
            }
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
                SetMenuItemInfoW(if *id >= 6 { capture } else { menu }, *id, 0, &info);
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
    // Intercept only toggle activation inside the native menu loop. Windows
    // keeps the submenu, keyboard selection and pointer position intact.
    TOGGLE_MENUS.with(|s| s.set(Some((menu, capture, state as *const State))));
    let hook = unsafe {
        SetWindowsHookExW(
            WH_MSGFILTER,
            Some(toggle_filter),
            null_mut(),
            GetCurrentThreadId(),
        )
    };
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
    TOGGLE_MENUS.with(|s| s.set(None));
    SELECTED_TOGGLE.with(|s| s.set(None));
    unsafe {
        if !hook.is_null() {
            UnhookWindowsHookEx(hook);
        }
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
    if let Some(action) = menu_action(command as u32) {
        state.send(action);
    }
}

thread_local! {
    static TOGGLE_MENUS: std::cell::Cell<Option<(HMENU, HMENU, *const State)>> = const { std::cell::Cell::new(None) };
    static SELECTED_TOGGLE: std::cell::Cell<Option<(HMENU, u32)>> = const { std::cell::Cell::new(None) };
}
unsafe extern "system" fn toggle_filter(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == MSGF_MENU as i32 && l != 0 {
        let msg = unsafe { &*(l as *const MSG) };
        if msg.message == WM_LBUTTONUP
            || (msg.message == WM_KEYDOWN && matches!(msg.wParam, 13 | 32))
        {
            let handled = TOGGLE_MENUS.with(|s| {
                let Some((menu, capture, state)) = s.get() else {
                    return false;
                };
                for (parent, id) in [(capture, 10), (menu, 11)] {
                    if SELECTED_TOGGLE.with(|s| s.get()) != Some((parent, id)) {
                        continue;
                    }
                    let flags = unsafe { GetMenuState(parent, id, MF_BYCOMMAND) };
                    if flags != u32::MAX && flags & MF_HILITE != 0 {
                        unsafe {
                            CheckMenuItem(
                                parent,
                                id,
                                MF_BYCOMMAND
                                    | if flags & MF_CHECKED == 0 {
                                        MF_CHECKED
                                    } else {
                                        MF_UNCHECKED
                                    },
                            );
                        }
                        if let Some(action) = menu_action(id) {
                            unsafe { &*state }.send(action);
                        }
                        return true;
                    }
                }
                false
            });
            if handled {
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, w, l) }
}

struct MenuBitmap(HBITMAP);
// The same camera silhouettes and red surround as the Clip ribbon. Rasterize
// once at menu DPI; Windows owns no bitmap after the popup has been destroyed.
fn capture_icon(video: bool) -> crate::egui::IconData {
    use vello_cpu::{
        Pixmap, RenderContext,
        color::{AlphaColor, Srgb},
        kurbo::{Affine, BezPath, Cap, Circle, Join, Rect, Shape, Stroke},
    };
    let size = unsafe { GetSystemMetrics(SM_CXMENUCHECK) }.clamp(16, 64) as u16;
    let mut c = RenderContext::new(size, size);
    c.set_transform(Affine::scale(f64::from(size) / 28.) * Affine::translate((14., 14.)));
    c.set_paint(AlphaColor::<Srgb>::from_rgba8(235, 65, 76, 255));
    c.fill_path(&Circle::new((0., 0.), 11.).to_path(0.01));
    c.set_stroke(Stroke::new(1.));
    c.stroke_path(&Circle::new((0., 0.), 13.).to_path(0.01));
    c.set_paint(AlphaColor::<Srgb>::WHITE);
    c.set_stroke(
        Stroke::new(1.25)
            .with_caps(Cap::Round)
            .with_join(Join::Round),
    );
    if video {
        c.stroke_path(
            &Rect::new(-5.375, -3.375, 1.375, 3.375)
                .to_rounded_rect(0.5)
                .to_path(0.01),
        );
        c.stroke_path(&BezPath::from_svg("M2,-1.5 L6,-4 L6,4 L2,1.5 Z").unwrap());
    } else {
        c.stroke_path(
            &BezPath::from_svg("M-6,-3 L-3,-3 L-2,-5 L2,-5 L3,-3 L6,-3 L6,5 L-6,5 Z").unwrap(),
        );
        c.stroke_path(&Circle::new((0., 1.), 2.5).to_path(0.01));
    }
    let mut pixels = Pixmap::new(size, size);
    c.flush();
    c.render(&mut pixels, &mut vello_cpu::Resources::new());
    let rgba = pixels
        .data()
        .iter()
        .flat_map(|p| {
            let a = u16::from(p.a);
            let unpremultiply =
                |v: u8| (u16::from(v) * 255).checked_div(a).unwrap_or(0).min(255) as u8;
            [
                unpremultiply(p.r),
                unpremultiply(p.g),
                unpremultiply(p.b),
                p.a,
            ]
        })
        .collect();
    crate::egui::IconData {
        rgba,
        width: u32::from(size),
        height: u32::from(size),
    }
}
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
        .name("clipdesigner-tray".into())
        .spawn(move || unsafe {
            let instance = GetModuleHandleW(null());
            let class = wide("ClipDesigner.NotificationArea");
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
                wide("ClipDesigner Tray").as_ptr(),
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
            let initialized = !hwnd.is_null();
            let tray_available =
                initialized && !icon.is_null() && state.taskbar_created != 0 && state.add(hwnd);
            if initialized {
                let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                if !tray_available {
                    state.send(Action::Unavailable);
                }
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
                    "System tray is unavailable. ClipDesigner will keep its editor accessible."
                        .into(),
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
    fn menu_toggle_filter_changes_checks_without_ending_menu_loop() {
        unsafe {
            let menu = CreatePopupMenu();
            let capture = CreatePopupMenu();
            AppendMenuW(menu, MF_POPUP, capture as usize, wide("Capture").as_ptr());
            AppendMenuW(menu, MF_STRING, 11, wide("Open after capture").as_ptr());
            AppendMenuW(capture, MF_STRING, 10, wide("Scrolling").as_ptr());
            let (events, rx) = mpsc::channel();
            let state = State {
                ctx: crate::egui::Context::default(),
                events,
                icon: null_mut(),
                taskbar_created: 0,
            };
            TOGGLE_MENUS.with(|s| s.set(Some((menu, capture, &state))));
            for (parent, id, action) in [
                (capture, 10, Action::ToggleScrolling),
                (menu, 11, Action::ToggleOpenAfter),
            ] {
                let info = MENUITEMINFOW {
                    cbSize: size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_STATE,
                    fState: MFS_HILITE,
                    ..Default::default()
                };
                assert_ne!(SetMenuItemInfoW(parent, id, 0, &info), 0);
                for checked in [true, false] {
                    SELECTED_TOGGLE.with(|s| s.set(Some((parent, id))));
                    let msg = MSG {
                        message: WM_LBUTTONUP,
                        ..Default::default()
                    };
                    assert_eq!(
                        toggle_filter(MSGF_MENU as i32, 0, &msg as *const _ as isize),
                        1
                    );
                    assert_eq!(rx.try_recv().unwrap(), action);
                    assert_eq!(
                        GetMenuState(parent, id, MF_BYCOMMAND) & MF_CHECKED != 0,
                        checked
                    );
                }
                let info = MENUITEMINFOW { fState: 0, ..info };
                SetMenuItemInfoW(parent, id, 0, &info);
            }
            TOGGLE_MENUS.with(|s| s.set(None));
            SELECTED_TOGGLE.with(|s| s.set(None));
            DestroyMenu(menu);
        }
    }
    #[test]
    fn file_open_ipc_copies_unicode_and_rejects_malformed_payloads_in_hidden_window() {
        unsafe {
            let class = wide(&format!("ClipDesigner.IpcTest.{}", std::process::id()));
            let instance = GetModuleHandleW(null());
            assert_ne!(
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance,
                    lpszClassName: class.as_ptr(),
                    ..Default::default()
                }),
                0
            );
            let (tx, rx) = mpsc::channel();
            let state = State {
                ctx: crate::egui::Context::default(),
                events: tx,
                icon: null_mut(),
                taskbar_created: 0,
            };
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("IPC test").as_ptr(),
                WS_POPUP,
                0,
                0,
                20,
                20,
                null_mut(),
                null_mut(),
                instance,
                (&state as *const State).cast(),
            );
            assert!(!hwnd.is_null());
            assert_eq!(IsWindowVisible(hwnd), 0);
            let expected = std::path::PathBuf::from("C:/ClipDesigner test/日本語 📝.iclip");
            let mut payload: Vec<u16> = expected.to_string_lossy().encode_utf16().collect();
            let mut data = COPYDATASTRUCT {
                dwData: OPEN_FILE,
                cbData: (payload.len() * 2) as u32,
                lpData: payload.as_mut_ptr().cast(),
            };
            assert_eq!(
                SendMessageW(hwnd, WM_COPYDATA, 0, &data as *const _ as isize),
                ACTIVATE_ACK as isize
            );
            payload.fill(0);
            assert_eq!(rx.try_recv().unwrap(), Action::Open(expected));
            for size in [0, 1, 65537] {
                data.cbData = size;
                assert_eq!(
                    SendMessageW(hwnd, WM_COPYDATA, 0, &data as *const _ as isize),
                    0
                );
            }
            data.cbData = 4;
            assert_eq!(
                SendMessageW(hwnd, WM_COPYDATA, 0, &data as *const _ as isize),
                0
            );
            assert!(rx.try_recv().is_err());
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DestroyWindow(hwnd);
            UnregisterClassW(class.as_ptr(), instance);
        }
    }
    #[test]
    fn tray_menu_icons_are_small_owned_bitmaps() {
        {
            let icon = crate::brand::icon();
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
            (2, Action::Clip),
            (3, Action::CaptureClip),
            (4, Action::Exit),
            (5, Action::CaptureVideo),
        ] {
            assert_eq!(menu_action(id), Some(expected));
        }
        assert_eq!(menu_action(0), None);
    }
}
