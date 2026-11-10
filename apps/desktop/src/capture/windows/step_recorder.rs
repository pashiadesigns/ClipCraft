//! Temporary input hooks enqueue bounded semantic activity. No raw typed text
//! is logged; a background worker reads only non-password accessibility fields.
use super::*;
use crate::capture::step_pipeline::{self, Area, Input};
use windows_sys::Win32::UI::Controls::*;
mod session;
#[path = "step_recorder/composition.rs"]
pub(super) mod visual;
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
    caption: Option<mpsc::Receiver<step_pipeline::Caption>>,
    last_point: Option<[i32; 2]>,
    typed: Option<Instant>,
    scroll: Option<([i32; 2], i32, Instant)>,
    held_keys: std::collections::BTreeSet<u32>,
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
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MOUSEWHEEL
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
                        if message == WM_MOUSEWHEEL {
                            let area = monitor_at(point)?.0;
                            if state.tracker.down(point, Button::Left, area)? { let _=state.sender.try_send(Input::Screen(area)); }
                            state.tracker.cancel_gesture();
                            if state.tracker.activity(point).is_some() {
                                let delta=(data.mouseData>>16) as i16 as i32;
                                let prior=state.scroll.map_or(0,|(_,d,_)|d);
                                state.scroll=Some((point, prior+delta, Instant::now()));
                            }
                        } else if matches!(message, WM_LBUTTONDOWN | WM_RBUTTONDOWN) {
                            flush_activity(state, true);
                            state.last_point=Some(point);
                            state.caption=Some(super::step_context::describe(point, super::step_context::Activity::Click));
                            let area = monitor_at(point)?.0;
                            if state.tracker.down(point, button, area)? {
                                state.sender.try_send(Input::Screen(area)).map_err(|_| "Cannot start the recording monitor".to_string())?;
                            }
                        } else if let Some(gesture) = state.tracker.up(point, button) {
                            let event = if let Some(caption)=state.caption.take() {Input::Described {gesture,at:Instant::now(),caption,optional:false}} else {Input::Capture {gesture, at:Instant::now()}};
                            state.sender.try_send(event).map_err(|_| "An action was missed while capture was busy. Repeat it before finishing.".to_string())?;
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        *state.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
                        state.tracker.cancel_gesture();
                    }
                });
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

fn send_activity(state: &mut HookState, point: [i32; 2], activity: super::step_context::Activity) {
    if let Some(gesture) = state.tracker.activity(point) {
        let caption = super::step_context::describe(point, activity);
        let input = Input::Described {
            gesture,
            at: Instant::now(),
            caption,
            optional: matches!(activity, super::step_context::Activity::Type),
        };
        if state.sender.try_send(input).is_err() {
            *state.failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(
                "An action was missed while capture was busy. Repeat it before finishing.".into(),
            );
            state.tracker.cancel_gesture();
        }
    }
}
fn flush_activity(state: &mut HookState, force: bool) {
    if state
        .typed
        .is_some_and(|at| force || at.elapsed() >= Duration::from_millis(700))
    {
        state.typed = None;
        if let Some(point) = state.last_point {
            send_activity(state, point, super::step_context::Activity::Type);
        }
    }
    if let Some((point, delta, at)) = state.scroll
        && (force || at.elapsed() >= Duration::from_millis(300))
    {
        state.scroll = None;
        if delta != 0 {
            send_activity(
                state,
                point,
                super::step_context::Activity::Scroll(delta < 0),
            );
        }
    }
}
unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP) {
        let key = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if key.flags & LLKHF_INJECTED == 0 {
            INPUT.with(|v| {
                let mut value = v.borrow_mut();
                let Some(state) = value.as_mut() else {
                    return;
                };
                let repeated = if wparam as u32 == WM_KEYUP {
                    state.held_keys.remove(&key.vkCode);
                    false
                } else {
                    !state.held_keys.insert(key.vkCode)
                };
                let Some(point) = state.last_point else {
                    return;
                };
                if state.tracker.activity(point).is_none() {
                    state.typed = None;
                    return;
                }
                let mut pid = 0;
                unsafe {
                    GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
                }
                if pid == std::process::id() {
                    return;
                }
                let name = match key.vkCode {
                    13 => Some("Enter"),
                    9 => Some("Tab"),
                    27 => Some("Escape"),
                    _ => None,
                };
                if wparam as u32 != WM_KEYUP
                    && !repeated
                    && let Some(name) = name
                {
                    flush_activity(state, true);
                    send_activity(state, point, super::step_context::Activity::Key(name));
                } else if wparam as u32 == WM_KEYUP
                    && ((0x30..=0x5a).contains(&key.vkCode)
                        || (0x60..=0x6f).contains(&key.vkCode)
                        || (0xba..=0xe2).contains(&key.vkCode)
                        || [8, 32, 46].contains(&key.vkCode))
                {
                    state.typed = Some(Instant::now());
                }
            });
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, wparam, lparam) }
}

struct Dialog {
    count: usize,
    undo: HWND,
    expanded: bool,
    expansion: crate::motion::Tween,
    expansion_value: f32,
    last_hover: Instant,
    began: Instant,
    animate: bool,
    dpi: i32,
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
                        visual::present(hwnd, state, state.expansion_value);
                    }
                    EndPaint(hwnd, &ps);
                    return 0;
                }
                // Layered composition owns all pixels; children retain semantics.
                WM_DRAWITEM => {
                    if !super::control_surface::layered(hwnd) {
                        visual::present(hwnd, state, state.expansion_value);
                    }
                    return 1;
                }
                WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
                WM_COMMAND if wparam & 0xffff == 5 => {
                    if state.count > 0 && state.pending.is_none() {
                        state.pending = Some(Input::Undo);
                        pause_tracker(true);
                    }
                    return 0;
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
    // The introductory dimming cue is optional. On unsupported DWM paths its
    // hidden window is destroyed before capture; never bypass its exclusion.
    let mut intro = super::steps_intro::show(owner, initial_area).ok();
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
            undo: null_mut(),
            expanded: false,
            expansion: crate::motion::Tween::new(0.),
            expansion_value: 0.,
            last_hover: Instant::now() - Duration::from_secs(1),
            began: Instant::now(),
            animate: {
                let mut enabled = 1i32;
                SystemParametersInfoW(
                    SPI_GETCLIENTAREAANIMATION,
                    0,
                    (&mut enabled as *mut i32).cast(),
                    0,
                );
                enabled != 0
            },
            dpi: 96,
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
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
            w!("ClipCraftStepRecorder"),
            w!("ClipCraft Step by Step"),
            WS_POPUP | WS_CLIPCHILDREN,
            position[0],
            position[1],
            368,
            114,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            (&mut *state as *mut Dialog).cast(),
        );
        if hwnd.is_null() {
            return Err(win_error("Cannot open Steps controls"));
        }
        let window = Window(hwnd);
        super::control_surface::exclude(hwnd, "Step by Step controls")?;
        for (id, label) in [
            (1, w!("Pause recording")),
            (2, w!("Finish guide")),
            (3, w!("Restart recording")),
            (4, w!("Cancel recording")),
            (5, w!("Undo step")),
        ] {
            let button = CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("BUTTON"),
                label,
                WS_CHILD | BS_OWNERDRAW as u32,
                0,
                0,
                40,
                40,
                hwnd,
                id as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            if button.is_null() {
                return Err(win_error("Cannot create recorder controls"));
            }
            match id {
                1 => state.pause = button,
                2 => state.stop = button,
                3 => state.restart = button,
                4 => state.cancel = button,
                _ => state.undo = button,
            }
        }
        let stop = state.stop;
        layout(hwnd, &mut state, false);
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
            (state.cancel, w!("Cancel recording")),
            (state.undo, w!("Undo last step")),
        ] {
            tooltip_label(state.tooltip, hwnd, button, label, true);
        }
        if SetTimer(hwnd, 1, 33, None) == 0 {
            return Err(win_error("Cannot start Steps refresh timer"));
        }
        let (sender, inputs) = mpsc::sync_channel(4);
        let count = Arc::new(AtomicUsize::new(0));
        let failure = Arc::new(Mutex::new(None));
        INPUT.with(|v| {
            *v.borrow_mut() = Some(HookState {
                tracker,
                caption: None,
                last_point: None,
                typed: None,
                scroll: None,
                held_keys: Default::default(),
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
        let key_hook =
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), GetModuleHandleW(null()), 0);
        if key_hook.is_null() {
            return Err(win_error("Cannot start Steps keyboard capture"));
        }
        let mut key_hook = Some(Hook(key_hook));
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

                step_pipeline::run_context(
                    inputs,
                    |Area { origin, size }, gesture, _index| {
                        let shot = Snapshot::capture_rect(origin, size, cursor, false)?;
                        let mut frame = shot.frame(Region {
                            x: 0,
                            y: 0,
                            width: size[0],
                            height: size[1],
                        })?;
                        let point = POINT {
                            x: origin[0] + gesture.end[0] as i32,
                            y: origin[1] + gesture.end[1] as i32,
                        };
                        frame.context.bounds =
                            window_bounds(GetAncestor(WindowFromPoint(point), GA_ROOT)).map(|b| {
                                [
                                    b[0] - origin[0],
                                    b[1] - origin[1],
                                    b[2] - origin[0],
                                    b[3] - origin[1],
                                ]
                            });
                        Ok(frame)
                    },
                    root,
                    c,
                    f,
                    step_pipeline::Enrichment {
                        date: super::step_context::date(),
                        finish: Box::new(|_| {}),
                    },
                )
            })
            .map_err(|e| e.to_string())?;
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let mut message = MSG::default();
        let mut positioned = None;
        loop {
            INPUT.with(|v| {
                if let Some(h) = v.borrow_mut().as_mut() {
                    flush_activity(h, false);
                }
            });
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
                    let expanded = state.expanded;
                    layout(hwnd, &mut state, expanded);
                }
            }
            let mut pointer = POINT::default();
            GetCursorPos(&mut pointer);
            let under = WindowFromPoint(pointer);
            let buttons = [state.pause, stop, state.restart, state.cancel, state.undo];
            let hovered = buttons
                .iter()
                .find(|b| **b == under)
                .map_or(0, |b| GetDlgCtrlID(*b) as u32);
            let inside = under == hwnd || buttons.contains(&under);
            if inside {
                state.last_hover = Instant::now();
            }
            let expanded = inside
                || state.last_hover.elapsed() < Duration::from_millis(280)
                || state.confirm_action.is_some();
            let value = state.expansion.advance(
                if expanded { 1. } else { 0. },
                state.began.elapsed().as_secs_f64(),
                if state.animate { 0.180 } else { 0. },
            );
            if state.expanded != expanded || state.expansion_value != value {
                state.expansion_value = value;
                layout(hwnd, &mut state, expanded);
            }
            if hovered != state.hover {
                state.hover = hovered;
                for button in buttons {
                    InvalidateRect(button, null(), 0);
                }
            }
            if state.animate && !state.paused {
                InvalidateRect(state.pause, null(), 0);
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
                let undo = matches!(command, Input::Undo);
                let sent = INPUT.with(|v| v.borrow().as_ref().map(|h| h.sender.try_send(command)));
                match sent {
                    Some(Ok(())) => {
                        if cancel {
                            state.finish = true;
                        } else if undo {
                            pause_tracker(state.paused);
                        } else {
                            state.paused = false;
                            INPUT.with(|v| {
                                if let Some(h) = v.borrow_mut().as_mut() {
                                    h.tracker = Session::default();
                                    h.caption = None;
                                    h.last_point = None;
                                    h.typed = None;
                                    h.scroll = None;
                                    h.held_keys.clear();
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
            for button in [state.pause, stop, state.restart, state.cancel, state.undo] {
                EnableWindow(
                    button,
                    (!state.finish
                        && state.pending.is_none()
                        && (button != state.undo || state.count > 0)) as i32,
                );
            }
            // A bounded queue warning is not a Finish command. Keep the
            // controller available; fatal worker errors still exit below.
            if !state.finish && failure.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                SetWindowTextW(
                    hwnd,
                    w!("Step by Step — an action was missed; repeat it before Finish"),
                );
                tooltip_label(
                    state.tooltip,
                    hwnd,
                    state.pause,
                    w!("An action was missed while capture was busy. Repeat it before Finish."),
                    false,
                );
            }
            if state.finish {
                INPUT.with(|v| {
                    if let Some(h) = v.borrow_mut().as_mut() {
                        flush_activity(h, true);
                        h.tracker.finish();
                    }
                });
                key_hook.take();
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
            visual::present(hwnd, &state, state.expansion_value);
            let code = GetMessageW(&mut message, null_mut(), 0, 0);
            if code <= 0 {
                INPUT.with(|v| {
                    if let Some(h) = v.borrow_mut().as_mut() {
                        flush_activity(h, true);
                    }
                });
                key_hook.take();
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

unsafe fn layout(hwnd: HWND, state: &mut Dialog, expanded: bool) {
    unsafe {
        state.expanded = expanded;
        state.dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd).max(96) as i32;
        let px = |n: i32| (n * state.dpi + 48) / 96;
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info);
        let x = info.rcWork.left + (info.rcWork.right - info.rcWork.left - px(368)) / 2;
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            info.rcWork.top + px(12),
            px(368),
            px(114),
            SWP_NOACTIVATE,
        );
        for (button, x, w) in [
            (state.undo, 23, 52),
            (state.restart, 81, 52),
            (state.pause, 156, 56),
            (state.cancel, 235, 52),
            (state.stop, 293, 52),
        ] {
            SetWindowPos(
                button,
                null_mut(),
                px(x),
                px(if button == state.pause { 14 } else { 23 }),
                px(w),
                px(if button == state.pause { 56 } else { 62 }),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            ShowWindow(
                button,
                if expanded || button == state.pause {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_HIDE
                },
            );
        }
        // Alpha edges replace the hard-edged Win32 region used by the old bar.
        SetWindowRgn(hwnd, null_mut(), 0);
        InvalidateRect(hwnd, null(), 0);
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
            if paused {
                flush_activity(h, true);
            }
            h.tracker.pause(paused);
        }
    });
}
