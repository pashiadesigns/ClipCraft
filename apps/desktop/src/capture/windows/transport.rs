//! Shared, cached recorder surface. All HWNDs and callbacks live on one worker.
use super::step_recorder::visual::{self, Caption};
use super::*;
use std::{collections::VecDeque, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Record,
    Finish,
    Reset,
    Cancel,
    Undo,
    Mode,
}
pub(super) struct Transport {
    state: Box<State>,
}
struct State {
    hwnd: HWND,
    buttons: [HWND; 6],
    events: VecDeque<Action>,
    dpi: i32,
    caption: Caption,
    undo: bool,
    has_data: bool,
    paused: bool,
    hover: u32,
    expanded: bool,
    expansion: f32,
    last_hover: Instant,
    last_tick: Instant,
    last_surface: Option<(u16, Caption, bool, bool, u32)>,
    base: Option<image::RgbaImage>,
    began: Instant,
    presented: Instant,
}
fn click_action(w: WPARAM, l: LPARAM) -> Option<Action> {
    if (w >> 16) & 0xffff != BN_CLICKED as usize || l == 0 {
        return None;
    }
    match w & 0xffff {
        1 => Some(Action::Record),
        2 => Some(Action::Finish),
        3 => Some(Action::Reset),
        4 => Some(Action::Cancel),
        5 => Some(Action::Undo),
        6 => Some(Action::Mode),
        _ => None,
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                (*(l as *const CREATESTRUCTW)).lpCreateParams as isize,
            );
        }
        let s = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if !s.is_null() {
            let s = &mut *s;
            match msg {
                WM_MOUSEACTIVATE => return MA_NOACTIVATE as LRESULT,
                WM_ERASEBKGND | WM_DRAWITEM => return 1,
                WM_CLOSE => {
                    s.events.push_back(Action::Cancel);
                    return 0;
                }
                WM_COMMAND => {
                    // Owner-drawn buttons also report focus/press-state changes.
                    // A transport action belongs exclusively to the completed click.
                    let action = click_action(w, l);
                    if let Some(action) = action
                        && s.events.len() < 8
                    {
                        s.events.push_back(action);
                    }
                    return 0;
                }
                _ => (),
            }
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.state.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.state.hwnd);
        }
    }
}
impl Transport {
    pub fn new(owner: HWND, work: RECT, dpi: i32, caption: Caption) -> Result<Self, String> {
        unsafe {
            static CLASS: OnceLock<Result<u16, String>> = OnceLock::new();
            CLASS
                .get_or_init(|| {
                    let c = WNDCLASSW {
                        hIcon: crate::brand::native_icon(),
                        lpfnWndProc: Some(procedure),
                        hInstance: GetModuleHandleW(null()),
                        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                        lpszClassName: w!("ClipCraftCaptureTransport"),
                        ..Default::default()
                    };
                    let atom = RegisterClassW(&c);
                    if atom == 0 {
                        Err(win_error("Cannot register recorder"))
                    } else {
                        Ok(atom)
                    }
                })
                .as_ref()
                .map_err(Clone::clone)?;
            let video = matches!(caption, Caption::Video { .. });
            let width = if video { 264 } else { 368 };
            let dpi = dpi.min(((work.right - work.left - 16) * 96 / width).max(48));
            let px = |n: i32| (n * dpi + 48) / 96;
            let mut state = Box::new(State {
                hwnd: null_mut(),
                buttons: [null_mut(); 6],
                events: VecDeque::new(),
                dpi,
                caption,
                undo: false,
                has_data: false,
                paused: false,
                hover: 0,
                expanded: true,
                expansion: 1.,
                last_hover: Instant::now(),
                last_tick: Instant::now(),
                last_surface: None,
                base: None,
                began: Instant::now(),
                presented: Instant::now(),
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
                w!("ClipCraftCaptureTransport"),
                w!("ClipCraft capture controls"),
                WS_POPUP | WS_CLIPCHILDREN,
                work.left + (work.right - work.left - px(width)) / 2,
                work.top + px(12),
                px(width),
                px(114),
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                (&mut *state as *mut State).cast(),
            );
            if hwnd.is_null() {
                return Err(win_error("Cannot create recorder"));
            }
            state.hwnd = hwnd;
            let mut result = Self { state };
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
                return Err(win_error("Cannot exclude recorder from capture"));
            }
            for (index, label, x, y, width, height) in [
                (0, w!("Record / stop scrolling"), 156, 14, 56, 56),
                (1, w!("Finish capture"), 293, 23, 52, 62),
                (2, w!("Reset capture"), 81, 23, 52, 62),
                (3, w!("Cancel capture"), 235, 23, 52, 62),
                (4, w!("Undo last scroll burst"), 23, 23, 52, 62),
                (5, w!("Switch Automatic / Manual"), 135, 78, 98, 29),
            ] {
                let x = if video {
                    match index {
                        0 => 104,
                        1 => 195,
                        3 => 17,
                        _ => x - 52,
                    }
                } else {
                    x
                };
                let b = CreateWindowExW(
                    WS_EX_NOACTIVATE,
                    w!("BUTTON"),
                    label,
                    WS_CHILD | BS_OWNERDRAW as u32,
                    px(x),
                    px(y),
                    px(width),
                    px(height),
                    hwnd,
                    (index + 1) as HMENU,
                    GetModuleHandleW(null()),
                    null(),
                );
                if b.is_null() {
                    return Err(win_error("Cannot create recorder action"));
                }
                result.state.buttons[index] = b;
            }
            result.update(caption, false, false, false)?;
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            Ok(result)
        }
    }
    pub fn hwnd(&self) -> HWND {
        self.state.hwnd
    }
    pub fn update(
        &mut self,
        caption: Caption,
        undo: bool,
        has_data: bool,
        paused: bool,
    ) -> Result<(), String> {
        let s = &mut *self.state;
        s.caption = caption;
        s.undo = undo;
        s.has_data = has_data;
        s.paused = paused;
        unsafe {
            let mut p = POINT::default();
            GetCursorPos(&mut p);
            let under = WindowFromPoint(p);
            let hover = s
                .buttons
                .iter()
                .position(|b| *b == under)
                .map_or(0, |i| i as u32 + 1);
            if under == s.hwnd || hover > 0 {
                s.last_hover = Instant::now();
            }
            s.expanded = s.last_hover.elapsed() < Duration::from_millis(280);
            let dt = s.last_tick.elapsed().as_secs_f32();
            s.last_tick = Instant::now();
            let goal = if s.expanded { 1. } else { 0. };
            let mut animate = 1i32;
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut animate as *mut i32).cast(),
                0,
            );
            s.expansion = if animate == 0 {
                goal
            } else if s.expansion < goal {
                (s.expansion + dt / 0.18).min(goal)
            } else {
                (s.expansion - dt / 0.18).max(goal)
            };
            s.hover = hover;
            for (i, b) in s.buttons.iter().enumerate() {
                let video = matches!(caption, Caption::Video { .. });
                let visible = if video {
                    i == 0 || s.expanded && [1, 3].contains(&i)
                } else {
                    [0, 5].contains(&i) || s.expanded
                };
                if (IsWindowVisible(*b) != 0) != visible {
                    ShowWindow(*b, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
                }
                let enabled = match i {
                    0 => !matches!(caption, Caption::Video { countdown, .. } if countdown > 0),
                    1 | 2 => has_data,
                    4 => undo,
                    _ => true,
                };
                if (IsWindowEnabled(*b) != 0) != enabled {
                    EnableWindow(*b, enabled as i32);
                }
            }
            let mode = match caption {
                Caption::Scrolling { automatic, .. } => {
                    if automatic {
                        w!("Automatic scrolling; switch to Manual")
                    } else {
                        w!("Manual scrolling; switch to Automatic")
                    }
                }
                _ => w!("Recording time"),
            };
            SetWindowTextW(s.buttons[5], mode);
            SetWindowTextW(
                s.buttons[0],
                match caption {
                    Caption::Video { .. } => {
                        if paused {
                            w!("Resume recording")
                        } else {
                            w!("Pause recording")
                        }
                    }
                    _ => w!("Record / stop scrolling"),
                },
            );
            let key = (
                (s.expansion * 32.).round() as u16,
                caption,
                undo,
                paused,
                hover,
            );
            if s.last_surface != Some(key) {
                let image = visual::render_capture(
                    s.dpi,
                    key.0 as f32 / 32.,
                    usize::from(undo),
                    paused,
                    hover,
                    0.,
                    caption,
                )?;
                s.base = Some(image);
                s.presented = Instant::now() - Duration::from_millis(50);
                s.last_surface = Some(key);
            }
            if s.presented.elapsed() >= Duration::from_millis(33) {
                if let Some(base) = &s.base {
                    let mut frame = base.clone();
                    let active = match caption {
                        Caption::Scrolling { running, .. } => running,
                        Caption::Video { countdown, .. } => !paused && countdown == 0,
                        _ => false,
                    };
                    if animate != 0 && active {
                        visual::pulse_ring(
                            &mut frame,
                            s.dpi,
                            s.began.elapsed().as_secs_f32() % 1.5 / 0.75,
                        );
                    }
                    visual::present_image(s.hwnd, &frame);
                }
                s.presented = Instant::now();
            }
        }
        Ok(())
    }
    pub fn action(&mut self) -> Option<Action> {
        self.state.events.pop_front()
    }
}
pub(super) fn pump() -> bool {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                return false;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_press_release_sequence_emits_one_latched_record_action() {
        let sequence = [
            BN_SETFOCUS,
            BN_HILITE,
            BN_UNHILITE,
            BN_CLICKED,
            BN_KILLFOCUS,
        ];
        let actions: Vec<_> = sequence
            .into_iter()
            .filter_map(|code| click_action(1 | ((code as usize) << 16), 1))
            .collect();
        assert_eq!(actions, vec![Action::Record]);
        assert_eq!(click_action(1, 0), None);
    }
}
