//! A temporary mouse-only hook feeds a bounded background capture writer.
//! The hook never captures pixels, blocks on a queue, records keys or saves data.
use super::*;
use crate::capture::step_pipeline::{self, Area, Input};
use windows_sys::Win32::UI::Controls::*;
mod session;
use capture_core::{history::History, step_capture::Button};
use session::{Session, controller_position};
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, SyncSender},
    },
    time::Instant,
};

struct HookState {
    tracker: Session,
    sender: SyncSender<Input>,
    failure: Arc<Mutex<Option<String>>>,
}
thread_local! {static INPUT:RefCell<Option<HookState>>=const {RefCell::new(None)};}
struct Hook(HHOOK);
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.0);
        }
        INPUT.with(|v| *v.borrow_mut() = None);
    }
}
unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let message = wparam as u32;
        if matches!(
            message,
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP
        ) {
            // Data is valid only for this callback. Copy coordinates immediately.
            let data = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            if data.flags & LLMHF_INJECTED == 0 {
                INPUT.with(|v| {
                    let mut state = v.borrow_mut();
                    let Some(state) = state.as_mut() else { return };
                    let mut pid = 0;
                    unsafe { GetWindowThreadProcessId(WindowFromPoint(data.pt), &mut pid); }
                    if pid == std::process::id() { state.tracker.cancel_gesture(); return; }
                    let button = if matches!(message, WM_LBUTTONDOWN | WM_LBUTTONUP) { Button::Left } else { Button::Right };
                    let point = [data.pt.x, data.pt.y];
                    let result = (|| -> Result<(), String> {
                        if matches!(message, WM_LBUTTONDOWN | WM_RBUTTONDOWN) {
                            let area = monitor_at(point)?.0;
                            if state.tracker.down(point, button, area)? {
                                state.sender.try_send(Input::Screen(area)).map_err(|_| "Cannot start the recording monitor".to_string())?;
                            }
                        } else if let Some(gesture) = state.tracker.up(point, button) {
                            state.sender.try_send(Input::Capture {gesture, at: Instant::now()}).map_err(|_| "Recording stopped: clicks arrived faster than capture could save them. Earlier steps are preserved.".to_string())?;
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        *state.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                        state.tracker.finish();
                    }
                });
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

struct Dialog {
    count: usize,
    hover: u32,
    tooltip: HWND,
    pause: HWND,
    paused: bool,
    finish: bool,
    confirm_action: Option<bool>,
    pending: Option<Input>,
    stop: HWND,
    restart: HWND,
    cancel: HWND,
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Dialog;
        if !ptr.is_null() {
            let state = &mut *ptr;
            match message {
                WM_ERASEBKGND => return 1,
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    let dc = BeginPaint(hwnd, &mut ps);
                    if !dc.is_null() {
                        paint_bar(dc, state.count, state.paused, state.finish);
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                WM_DRAWITEM => {
                    let item =
                        &*(lparam as *const windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT);
                    paint_button(
                        item.hDC,
                        item.rcItem,
                        item.CtlID,
                        state.paused,
                        item.itemState & 0x10 != 0 || state.hover == item.CtlID,
                    );
                    return 1;
                }
                WM_NCHITTEST => {
                    let mut point = POINT {
                        x: lparam as u16 as i16 as i32,
                        y: (lparam >> 16) as u16 as i16 as i32,
                    };
                    ScreenToClient(hwnd, &mut point);
                    if point.x >= 98 && point.x < 230 && point.y >= 0 && point.y < 52 {
                        return HTCAPTION as LRESULT;
                    }
                }
                WM_CLOSE => {
                    state.confirm_action = Some(false);
                    pause_tracker(true);
                    return 0;
                }
                WM_COMMAND if wparam & 0xffff == 3 => {
                    state.confirm_action = Some(true);
                    pause_tracker(true);
                    return 0;
                }
                WM_COMMAND if wparam & 0xffff == 4 => {
                    state.confirm_action = Some(false);
                    pause_tracker(true);
                    return 0;
                }
                WM_COMMAND if wparam & 0xffff == 1 => {
                    state.paused = !state.paused;
                    INPUT.with(|v| {
                        if let Some(h) = v.borrow_mut().as_mut() {
                            h.tracker.pause(state.paused);
                        }
                    });
                    SetWindowTextW(
                        state.pause,
                        if state.paused {
                            w!("Resume")
                        } else {
                            w!("Pause")
                        },
                    );
                    tooltip_label(
                        state.tooltip,
                        hwnd,
                        state.pause,
                        if state.paused {
                            w!("Resume recording")
                        } else {
                            w!("Pause recording")
                        },
                        false,
                    );
                    InvalidateRect(state.pause, null(), 0);
                    InvalidateRect(hwnd, null(), 0);
                    return 0;
                }
                WM_COMMAND if wparam & 0xffff == 2 => {
                    state.finish = true;
                    return 0;
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.0, GWLP_USERDATA, 0);
            KillTimer(self.0, 1);
            DestroyWindow(self.0);
        }
    }
}

pub(super) fn record(owner: HWND, cursor: bool, root: Option<PathBuf>) -> CaptureResult {
    let root = root.ok_or("Steps capture requires available local Library storage")?;
    // Check storage access before accepting any input; no personal data is read.
    History::open(&root).map_err(|e| e.to_string())?;
    let tracker = Session::default();
    let mut point = POINT::default();
    unsafe {
        GetCursorPos(&mut point);
    }
    let (initial_area, work) = monitor_at([point.x, point.y])?;
    let position = controller_position(work);
    let mut intro = Some(super::steps_intro::show(owner, initial_area)?);
    unsafe {
        static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
        CLASS
            .get_or_init(|| {
                let class = WNDCLASSW {
                    hIcon: crate::brand::native_icon(),
                    lpfnWndProc: Some(procedure),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                    hbrBackground: null_mut(),
                    lpszClassName: w!("ClipCraftStepRecorder"),
                    ..Default::default()
                };
                let atom = RegisterClassW(&class);
                if atom == 0 {
                    Err(win_error("Cannot register Steps controls"))
                } else {
                    Ok(atom)
                }
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let mut state = Box::new(Dialog {
            count: 0,
            hover: 0,
            tooltip: null_mut(),
            pause: null_mut(),
            paused: false,
            finish: false,
            confirm_action: None,
            pending: None,
            stop: null_mut(),
            restart: null_mut(),
            cancel: null_mut(),
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("ClipCraftStepRecorder"),
            w!("ClipCraft Steps"),
            WS_POPUP | WS_CLIPCHILDREN,
            position[0],
            position[1],
            340,
            52,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            (&mut *state as *mut Dialog).cast(),
        );
        if hwnd.is_null() {
            return Err(win_error("Cannot open Steps controls"));
        }
        let window = Window(hwnd);
        // Hide only our recorder from supported Windows capture APIs. If the OS
        // cannot exclude it, stop before recording rather than contaminate frames.
        if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
            return Err(win_error(
                "Windows cannot exclude Steps controls from capture",
            ));
        }
        state.pause = CreateWindowExW(
            0,
            w!("BUTTON"),
            w!("Pause recording"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            4,
            4,
            44,
            44,
            hwnd,
            1usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        let stop = CreateWindowExW(
            0,
            w!("BUTTON"),
            w!("Finish guide"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            56,
            4,
            36,
            44,
            hwnd,
            2usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        state.stop = stop;
        state.restart = CreateWindowExW(
            0,
            w!("BUTTON"),
            w!("Restart capture"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            236,
            4,
            44,
            44,
            hwnd,
            3usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        state.cancel = CreateWindowExW(
            0,
            w!("BUTTON"),
            w!("Cancel capture"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            288,
            4,
            44,
            44,
            hwnd,
            4usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        if state.pause.is_null()
            || stop.is_null()
            || state.restart.is_null()
            || state.cancel.is_null()
        {
            return Err(win_error("Cannot create Steps controls"));
        }
        state.tooltip = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            TOOLTIPS_CLASSW,
            null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            hwnd,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if state.tooltip.is_null() {
            return Err(win_error("Cannot create Steps tooltips"));
        }
        SendMessageW(state.tooltip, TTM_SETDELAYTIME, TTDT_INITIAL as usize, 350);
        for (button, label) in [
            (state.pause, w!("Pause recording")),
            (stop, w!("Finish guide")),
            (state.restart, w!("Restart capture")),
            (state.cancel, w!("Cancel capture")),
        ] {
            tooltip_label(state.tooltip, hwnd, button, label, true);
        }
        if SetTimer(hwnd, 1, 16, None) == 0 {
            return Err(win_error("Cannot start Steps refresh timer"));
        }
        let (sender, inputs) = mpsc::sync_channel(4);
        let count = Arc::new(AtomicUsize::new(0));
        let failure = Arc::new(Mutex::new(None));
        INPUT.with(|v| {
            *v.borrow_mut() = Some(HookState {
                tracker,
                sender,
                failure: failure.clone(),
            })
        });
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), GetModuleHandleW(null()), 0);
        if hook.is_null() {
            INPUT.with(|v| *v.borrow_mut() = None);
            return Err(win_error("Cannot start Steps mouse capture"));
        }
        let mut hook = Some(Hook(hook));
        let c = count.clone();
        let f = failure.clone();
        let worker = std::thread::Builder::new()
            .name("clipcraft-step-writer".into())
            .spawn(move || {
                let prior =
                    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                if prior.is_null() {
                    return Err(win_error("Cannot set Steps capture DPI"));
                }
                let _dpi = DpiContext(prior);
                let labels = std::cell::RefCell::new(super::step_context::Collector::default());
                step_pipeline::run_context(
                    inputs,
                    |Area { origin, size }, gesture, index| {
                        let shot = Snapshot::capture_rect(origin, size, cursor, false)?;
                        let mut frame = shot.frame(Region {
                            x: 0,
                            y: 0,
                            width: size[0],
                            height: size[1],
                        })?;
                        frame.context =
                            labels
                                .borrow_mut()
                                .offer(index, origin, gesture, &frame.image);
                        Ok(frame)
                    },
                    root,
                    c,
                    f,
                    step_pipeline::Enrichment {
                        date: super::step_context::date(),
                        finish: Box::new(|g| labels.borrow_mut().finish(g)),
                    },
                )
            })
            .map_err(|e| e.to_string())?;
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let mut message = MSG::default();
        let mut positioned = None;
        loop {
            if intro.as_ref().is_some_and(|cue| !cue.tick()) {
                intro.take();
            }
            let area = INPUT.with(|v| v.borrow().as_ref().and_then(|h| h.tracker.area));
            if area != positioned {
                positioned = area;
                if let Some(area) = area {
                    let work = monitor_at(area.origin)?.1;
                    let [x, y] = controller_position(work);
                    SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
            let mut pointer = POINT::default();
            GetCursorPos(&mut pointer);
            let under = WindowFromPoint(pointer);
            let buttons = [state.pause, stop, state.restart, state.cancel];
            let hovered = buttons
                .iter()
                .position(|b| *b == under)
                .map_or(0, |i| i as u32 + 1);
            if hovered != state.hover {
                state.hover = hovered;
                for button in buttons {
                    InvalidateRect(button, null(), 0);
                }
            }
            if let Some(restart) = state.confirm_action.take() {
                match super::restart::confirm(hwnd, restart) {
                    Ok(true) => {
                        state.pending = Some(if restart {
                            Input::Restart
                        } else {
                            Input::Cancel
                        });
                    }
                    Ok(false) => pause_tracker(state.paused),
                    Err(e) => {
                        *failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                    }
                }
            }
            if let Some(command) = state.pending.take() {
                let cancel = matches!(command, Input::Cancel);
                let sent = INPUT.with(|v| v.borrow().as_ref().map(|h| h.sender.try_send(command)));
                match sent {
                    Some(Ok(())) => {
                        if cancel {
                            state.finish = true;
                        } else {
                            state.paused = false;
                            INPUT.with(|v| {
                                if let Some(h) = v.borrow_mut().as_mut() {
                                    h.tracker = Session::default();
                                }
                            });
                            tooltip_label(
                                state.tooltip,
                                hwnd,
                                state.pause,
                                w!("Pause recording"),
                                false,
                            );
                            SetWindowTextW(state.pause, w!("Pause recording"));
                        }
                    }
                    Some(Err(mpsc::TrySendError::Full(command))) => state.pending = Some(command),
                    _ => {
                        state.finish = true;
                    }
                }
            }
            for button in [state.pause, stop, state.restart, state.cancel] {
                EnableWindow(button, (!state.finish && state.pending.is_none()) as i32);
            }
            if !state.finish && failure.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                state.finish = true;
            }
            if state.finish {
                hook.take();
                EnableWindow(state.pause, 0);
                EnableWindow(stop, 0);
            }
            let next = count.load(Ordering::Acquire);
            if next != state.count || state.finish {
                state.count = next;
                InvalidateRect(hwnd, null(), 0);
            }
            if worker.is_finished() {
                break;
            }
            let code = GetMessageW(&mut message, null_mut(), 0, 0);
            if code <= 0 {
                hook.take();
                break;
            }
            if IsDialogMessageW(hwnd, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        hook.take();
        drop(window);
        worker.join().map_err(|_| {
            "Steps writer stopped unexpectedly; completed steps remain in Library".to_string()
        })?
    }
}

fn paint_bar(dc: HDC, count: usize, paused: bool, finish: bool) {
    unsafe {
        let background = Bitmap(CreateSolidBrush(0x0023211d));
        let pen = Bitmap(CreatePen(PS_SOLID, 2, 0x00b98b00));
        let oldb = SelectObject(dc, background.0);
        let oldp = SelectObject(dc, pen.0);
        RoundRect(dc, 0, 0, 340, 52, 12, 12);
        let font = Bitmap(CreateFontW(
            -14,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            w!("Segoe UI"),
        ));
        let oldf = SelectObject(dc, font.0);
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, 0x00ffffff);
        let mut text: Vec<u16> = if finish {
            "Finishing…".into()
        } else if paused {
            format!("{count} Steps · Paused")
        } else {
            format!("{count} Steps")
        }
        .encode_utf16()
        .collect();
        DrawTextW(
            dc,
            text.as_mut_ptr(),
            text.len() as i32,
            &mut RECT {
                left: 100,
                top: 4,
                right: 228,
                bottom: 48,
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SelectObject(dc, oldf);
        SelectObject(dc, oldp);
        SelectObject(dc, oldb);
    }
}
fn paint_button(dc: HDC, rect: RECT, id: u32, paused: bool, focus: bool) {
    unsafe {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let buffer = MemoryDc(CreateCompatibleDC(dc));
        let bitmap = Bitmap(CreateCompatibleBitmap(dc, width * 4, height * 4));
        if buffer.0.is_null() || bitmap.0.is_null() {
            paint_button_raw(dc, rect, id, paused, focus);
            return;
        }
        let old = SelectObject(buffer.0, bitmap.0);
        SetMapMode(buffer.0, MM_ANISOTROPIC);
        SetWindowExtEx(buffer.0, 1, 1, null_mut());
        SetViewportExtEx(buffer.0, 4, 4, null_mut());
        paint_button_raw(
            buffer.0,
            RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
            id,
            paused,
            focus,
        );
        SetMapMode(buffer.0, MM_TEXT);
        let mode = SetStretchBltMode(dc, HALFTONE);
        StretchBlt(
            dc,
            rect.left,
            rect.top,
            width,
            height,
            buffer.0,
            0,
            0,
            width * 4,
            height * 4,
            SRCCOPY,
        );
        SetStretchBltMode(dc, mode);
        SelectObject(buffer.0, old);
    }
}

fn paint_button_raw(dc: HDC, rect: RECT, id: u32, paused: bool, focus: bool) {
    unsafe {
        let background = Bitmap(CreateSolidBrush(0x0023211d));
        FillRect(dc, &rect, background.0);
        let brush = Bitmap(CreateSolidBrush(if id == 1 {
            if focus { 0x004c48de } else { 0x00302ec1 }
        } else if focus {
            0x00554d42
        } else {
            0x0023211d
        }));
        let border = Bitmap(CreatePen(
            PS_SOLID,
            1,
            if focus {
                0x00ffffff
            } else if id == 1 {
                0x00302ec1
            } else {
                0x0023211d
            },
        ));
        let oldb = SelectObject(dc, brush.0);
        let oldp = SelectObject(dc, border.0);
        RoundRect(dc, rect.left, rect.top, rect.right, rect.bottom, 9, 9);
        let ink = Bitmap(CreateSolidBrush(0x00ffffff));
        SelectObject(dc, ink.0);
        let pen = Bitmap(CreatePen(PS_SOLID, 1, 0x00ffffff));
        SelectObject(dc, pen.0);
        let x = (rect.left + rect.right) / 2;
        let y = (rect.top + rect.bottom) / 2;
        if id == 2 {
            RoundRect(dc, x - 7, y - 7, x + 7, y + 7, 4, 4);
        } else if id == 3 {
            let stroke = Bitmap(CreatePen(PS_SOLID, 2, 0x00ffffff));
            SelectObject(dc, stroke.0);
            let hollow = SelectObject(dc, GetStockObject(HOLLOW_BRUSH));
            let direction = SetArcDirection(dc, AD_CLOCKWISE);
            Arc(dc, x - 8, y - 8, x + 8, y + 8, x - 6, y - 6, x - 8, y + 1);
            SetArcDirection(dc, direction);
            SelectObject(dc, hollow);
            let points = [
                POINT { x: x - 8, y: y - 9 },
                POINT { x: x - 8, y: y - 1 },
                POINT { x, y: y - 5 },
            ];
            Polygon(dc, points.as_ptr(), 3);
            SelectObject(dc, pen.0);
        } else if id == 4 {
            RoundRect(dc, x - 5, y - 5, x + 5, y + 8, 2, 2);
            Rectangle(dc, x - 7, y - 8, x + 7, y - 6);
            Rectangle(dc, x - 2, y - 10, x + 2, y - 8);
        } else if paused {
            let points = [
                POINT { x: x - 5, y: y - 9 },
                POINT { x: x + 8, y },
                POINT { x: x - 5, y: y + 9 },
            ];
            Polygon(dc, points.as_ptr(), 3);
        } else {
            RoundRect(dc, x - 7, y - 8, x - 3, y + 8, 2, 2);
            RoundRect(dc, x + 3, y - 8, x + 7, y + 8, 2, 2);
        }
        SelectObject(dc, oldp);
        SelectObject(dc, oldb);
    }
}

fn monitor_at(point: [i32; 2]) -> Result<(Area, [i32; 4]), String> {
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: point[0],
                y: point[1],
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return Err(win_error("Cannot read recording monitor"));
        }
        let r = info.rcMonitor;
        let w = info.rcWork;
        Ok((
            Area {
                origin: [r.left, r.top],
                size: [(r.right - r.left) as u32, (r.bottom - r.top) as u32],
            },
            [w.left, w.top, w.right, w.bottom],
        ))
    }
}

unsafe fn tooltip_label(
    tooltip: HWND,
    owner: HWND,
    button: HWND,
    label: windows_sys::core::PCWSTR,
    add: bool,
) {
    let info = TTTOOLINFOW {
        cbSize: size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_IDISHWND | TTF_SUBCLASS,
        hwnd: owner,
        uId: button as usize,
        lpszText: label.cast_mut(),
        ..Default::default()
    };
    unsafe {
        SendMessageW(
            tooltip,
            if add {
                TTM_ADDTOOLW
            } else {
                TTM_UPDATETIPTEXTW
            },
            0,
            (&info as *const TTTOOLINFOW) as LPARAM,
        );
    }
}

fn pause_tracker(paused: bool) {
    INPUT.with(|v| {
        if let Some(h) = v.borrow_mut().as_mut() {
            h.tracker.pause(paused);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_steps_toolbar_renders_offscreen() {
        unsafe {
            let dc = MemoryDc(CreateCompatibleDC(null_mut()));
            let mut bits = null_mut();
            let bitmap = Bitmap(CreateDIBSection(
                dc.0,
                &bitmap_info([340, 52]),
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            ));
            assert!(!bitmap.0.is_null());
            let old = SelectObject(dc.0, bitmap.0);
            paint_bar(dc.0, 4, false, false);
            paint_button(
                dc.0,
                RECT {
                    left: 4,
                    top: 4,
                    right: 48,
                    bottom: 48,
                },
                1,
                false,
                false,
            );
            paint_button(
                dc.0,
                RECT {
                    left: 56,
                    top: 4,
                    right: 92,
                    bottom: 48,
                },
                2,
                false,
                false,
            );
            paint_button(
                dc.0,
                RECT {
                    left: 236,
                    top: 4,
                    right: 280,
                    bottom: 48,
                },
                3,
                false,
                false,
            );
            paint_button(
                dc.0,
                RECT {
                    left: 288,
                    top: 4,
                    right: 332,
                    bottom: 48,
                },
                4,
                false,
                false,
            );
            GdiFlush();
            SelectObject(dc.0, old);
            let bytes = std::slice::from_raw_parts(bits.cast::<u8>(), 340 * 52 * 4);
            let image = crop_bgra(
                bytes,
                [340, 52],
                Region {
                    x: 0,
                    y: 0,
                    width: 340,
                    height: 52,
                },
            )
            .unwrap();
            assert_eq!(image.get_pixel(10, 26).0, [193, 46, 48, 255]);
            if let Some(dir) = std::env::var_os("CLIPCRAFT_CURSOR_EVIDENCE_DIR") {
                image
                    .save(std::path::PathBuf::from(dir).join("steps-toolbar.png"))
                    .unwrap();
            }
        }
    }
}
