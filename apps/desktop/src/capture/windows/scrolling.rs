//! Wheel timing only in the temporary hook; pixel work stays in the worker loop.
use super::*;
use super::{
    scrolling_preview::Preview,
    step_recorder::visual::Caption,
    transport::{Action, Transport},
};
use capture_core::scroll_capture::{Append, Stitcher};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
const BURST_GAP: Duration = Duration::from_millis(400);
struct Wheel {
    at: Instant,
    direction: i16,
}
struct WheelInput {
    region: RECT,
    events: mpsc::SyncSender<Wheel>,
}
thread_local! {static WHEELS:RefCell<Option<WheelInput>>=const{RefCell::new(None)};}
struct Hook(HHOOK);
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            UnhookWindowsHookEx(self.0);
        }
        WHEELS.with(|i| *i.borrow_mut() = None);
    }
}
struct WheelTracker {
    events: mpsc::Receiver<Wheel>,
    stop: Arc<AtomicBool>,
}
impl Drop for WheelTracker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl WheelTracker {
    fn start(region: RECT) -> Result<Self, String> {
        let (tx, events) = mpsc::sync_channel(256);
        let (ready, wait) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let ended = stop.clone();
        std::thread::spawn(move || unsafe {
            WHEELS.with(|i| *i.borrow_mut() = Some(WheelInput { region, events: tx }));
            let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), GetModuleHandleW(null()), 0);
            if hook.is_null() {
                let _ = ready.send(Err(win_error("Cannot track scrolling")));
                WHEELS.with(|i| *i.borrow_mut() = None);
                return;
            }
            let _hook = Hook(hook);
            let _ = ready.send(Ok(()));
            while !ended.load(Ordering::Relaxed) && transport::pump() {
                std::thread::sleep(Duration::from_millis(3));
            }
        });
        wait.recv()
            .map_err(|_| "Cannot start scrolling input".to_string())??;
        Ok(Self { events, stop })
    }
}
unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && w == WM_MOUSEWHEEL as usize {
        let data = unsafe { &*(l as *const MSLLHOOKSTRUCT) };
        if data.flags & LLMHF_INJECTED == 0 {
            WHEELS.with(|cell| {
                let mut cell = cell.borrow_mut();
                if let Some(i) = cell.as_mut() {
                    let r = i.region;
                    let p = data.pt;
                    let inside = p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom;
                    if inside {
                        let _ = i.events.try_send(Wheel {
                            at: Instant::now(),
                            direction: ((data.mouseData >> 16) as u16 as i16).signum(),
                        });
                    }
                }
            });
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, w, l) }
}
fn snapshot(origin: [i32; 2], size: [u32; 2]) -> Result<image::RgbaImage, String> {
    let s = Snapshot::capture_rect(origin, size, false, false)?;
    crop_bgra(
        &s.pixels,
        size,
        Region {
            x: 0,
            y: 0,
            width: size[0],
            height: size[1],
        },
    )
}
pub(super) fn record(
    owner: HWND,
    origin: [i32; 2],
    size: [u32; 2],
    mut automatic: bool,
    mode_changed: std::sync::mpsc::Sender<bool>,
) -> CaptureResult {
    unsafe {
        let center = POINT {
            x: origin[0] + size[0] as i32 / 2,
            y: origin[1] + size[1] as i32 / 2,
        };
        let target = GetAncestor(WindowFromPoint(center), GA_ROOT);
        let mut target_bounds = RECT::default();
        if target.is_null() || GetWindowRect(target, &mut target_bounds) == 0 {
            return Err("Choose scrolling content in an available window.".into());
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(
            MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST),
            &mut info,
        ) == 0
        {
            return Err(win_error("Cannot locate capture monitor"));
        }
        let dpi = GetDpiForWindow(target).max(96) as i32;
        let mut controls = Transport::new(
            owner,
            info.rcWork,
            dpi,
            Caption::Scrolling {
                automatic,
                running: false,
                countdown: 0,
            },
        )?;
        let mut preview = Preview::new(owner, info.rcWork, dpi)?;
        let wheel_tracker = WheelTracker::start(RECT {
            left: origin[0],
            top: origin[1],
            right: origin[0] + size[0] as i32,
            bottom: origin[1] + size[1] as i32,
        })?;
        // BB2: automatic scrolling wheels over the right edge of the selected
        // area, where page hover effects (links, expanding boxes) are least
        // likely, rather than over its center.
        let edge = POINT {
            x: origin[0] + size[0] as i32 - 6,
            y: center.y,
        };
        let park = if size[0] > 40 && GetAncestor(WindowFromPoint(edge), GA_ROOT) == target {
            edge
        } else {
            center
        };
        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor);
        let mut cursor_guard = AutoCursor {
            original: cursor,
            placed: park,
            enabled: false,
        };
        let mut stitch = None::<Stitcher>;
        let mut previous = None::<image::RgbaImage>;
        let mut running = false;
        let mut countdown = None::<Instant>;
        let mut last_wheel = None::<Wheel>;
        let mut auto_due = false;
        let mut end_probe = 0u8;
        let mut auto_scrolled = None::<Instant>;
        let mut await_input = false;
        let mut sampled = Instant::now();
        let began = Instant::now();
        let mut status = "Click Record to begin. Mode is remembered.".to_string();
        loop {
            if !transport::pump() || GetAsyncKeyState(VK_ESCAPE as i32) < 0 {
                return Ok(None);
            }
            while let Some(action) = controls.action() {
                end_probe = 0;
                match action {
                    Action::Mode => {
                        automatic = !automatic;
                        let _ = mode_changed.send(automatic);
                        running = !automatic && stitch.is_some();
                        countdown = None;
                        previous = None;
                        last_wheel = None;
                        auto_due = false;
                        auto_scrolled = None;
                        if let Some(s) = &mut stitch {
                            s.end_burst();
                        }
                        status = "Mode saved. Automatic starts with Record.".into();
                    }
                    Action::Record => {
                        if countdown.take().is_some() || running && automatic {
                            running = false;
                            status = "Automatic scrolling stopped. Preview retained.".into();
                        } else if !running {
                            if automatic {
                                countdown = Some(Instant::now());
                                status = "Click inside your scrolling window during 3–2–1.".into();
                            } else {
                                running = true;
                                await_input = false;
                                previous = None;
                                status = "Scroll the selected area. Finish when ready.".into();
                            }
                        }
                    }
                    Action::Undo => {
                        if automatic {
                            running = false;
                            countdown = None;
                        }
                        if let Some(s) = &mut stitch {
                            s.undo();
                            preview.set_image(Some(s.shared_image()));
                        }
                        previous = None;
                        await_input = true;
                        status = "Undone. Scroll back to overlap the remaining capture.".into();
                    }
                    Action::Reset | Action::Cancel => {
                        running = false;
                        countdown = None;
                        if restart::confirm_capture(controls.hwnd(), action == Action::Reset)? {
                            if action == Action::Cancel {
                                return Ok(None);
                            }
                            stitch = None;
                            previous = None;
                            last_wheel = None;
                            await_input = false;
                            preview.set_image(None);
                            status = "Capture reset. Click Record to begin again.".into();
                        } else {
                            status = "Capture retained. Click Record to continue.".into();
                        }
                    }
                    Action::Finish => {
                        if let Some(s) = stitch.take() {
                            let document = Document::from_capture_image(s.image().clone())
                                .map_err(|e| e.to_string())?;
                            return Ok(Some(super::super::Captured {
                                document,
                                warning: String::new(),
                                retained: None,
                                video: None,
                            }));
                        }
                    }
                }
            }
            if preview.expanded_requested() {
                if automatic {
                    running = false;
                    countdown = None;
                }
                status = "Preview enlarged. Scroll over Preview to inspect the image.".into();
            }
            let remaining =
                countdown.map_or(0, |t| 3u32.saturating_sub(t.elapsed().as_secs() as u32));
            if countdown.is_some() && remaining == 0 {
                countdown = None;
                if GetForegroundWindow() != target || IsIconic(target) != 0 {
                    status = "Activate the selected window, then click Record again.".into();
                } else if SetCursorPos(park.x, park.y) == 0 {
                    status = "Cannot position automatic scrolling. Switch to Manual.".into();
                } else {
                    cursor_guard.enabled = true;
                    running = true;
                    auto_due = false;
                    await_input = false;
                    previous = None;
                    auto_scrolled = None;
                }
            }
            let events = wheel_tracker.events.try_iter().collect::<Vec<_>>();
            for event in events {
                if !automatic && running {
                    if last_wheel.as_ref().is_some_and(|old| {
                        event.at.duration_since(old.at) >= BURST_GAP
                            || event.direction != old.direction
                    }) && let Some(s) = &mut stitch
                    {
                        s.end_burst();
                    }
                    last_wheel = Some(event);
                    await_input = false;
                }
            }
            if running {
                let mut bounds = RECT::default();
                let mut p = POINT::default();
                GetCursorPos(&mut p);
                if GetWindowRect(target, &mut bounds) == 0
                    || IsIconic(target) != 0
                    || [bounds.left, bounds.top, bounds.right, bounds.bottom]
                        != [
                            target_bounds.left,
                            target_bounds.top,
                            target_bounds.right,
                            target_bounds.bottom,
                        ]
                {
                    running = false;
                    status =
                        "The window moved or closed. Restore its position or finish this capture."
                            .into();
                } else if automatic
                    && (GetForegroundWindow() != target
                        || p.x.abs_diff(park.x) > 4
                        || p.y.abs_diff(park.y) > 4)
                {
                    running = false;
                    status="Automatic scrolling stopped: focus or pointer changed. Click Record to resume.".into();
                } else if began.elapsed() > Duration::from_secs(600) {
                    running = false;
                    status = "10-minute limit reached. Choose Finish to keep this capture.".into();
                }
            }
            if running
                && !await_input
                && sampled.elapsed() >= Duration::from_millis(80)
                && GetForegroundWindow() == target
            {
                sampled = Instant::now();
                match snapshot(origin, size) {
                    Err(e) => {
                        running = false;
                        status = e;
                    }
                    Ok(next) => {
                        let stable = previous
                            .as_ref()
                            .is_some_and(|p| capture_core::scroll_capture::settled(p, &next));
                        if stable {
                            if let Some(s) = &mut stitch {
                                match s.append(&next) {
                                    Ok(Append::Added(_)) => {
                                        status =
                                            "Preview updated. Undo removes the last scroll burst."
                                                .into();
                                        end_probe = 0;
                                        if automatic {
                                            s.end_burst();
                                            auto_due = true;
                                            auto_scrolled = None;
                                        }
                                    }
                                    Ok(Append::NoOverlap) => {
                                        if automatic {
                                            running = false;
                                        }
                                        status="Cannot align this view. Scroll back; keep some overlap.".into();
                                    }
                                    Ok(Append::Unchanged) => {
                                        if automatic && auto_scrolled.is_none() {
                                            auto_due = true;
                                        }
                                    }
                                    Err(e) => {
                                        running = false;
                                        status = e.to_string();
                                    }
                                }
                                if !automatic
                                    && last_wheel
                                        .as_ref()
                                        .is_none_or(|e| e.at.elapsed() >= BURST_GAP)
                                {
                                    s.end_burst();
                                }
                            } else {
                                stitch =
                                    Some(Stitcher::new(next.clone()).map_err(|e| e.to_string())?);
                                status = "Starting view captured. Scroll to extend it.".into();
                                if automatic {
                                    auto_due = true;
                                }
                            }
                            if let Some(s) = &stitch {
                                preview.set_image(Some(s.shared_image()));
                            }
                            if !automatic
                                && last_wheel
                                    .as_ref()
                                    .is_none_or(|e| e.at.elapsed() >= BURST_GAP)
                            {
                                await_input = true;
                            }
                        }
                        previous = Some(next);
                    }
                }
            }
            if automatic && running && auto_due && stitch.is_some() {
                match wheel_once() {
                    Ok(()) => {
                        auto_due = false;
                        auto_scrolled = Some(Instant::now());
                    }
                    Err(e) => {
                        running = false;
                        status = e;
                    }
                }
            }
            if automatic
                && running
                && auto_scrolled.is_some_and(|t| {
                    t.elapsed() > Duration::from_millis(if end_probe == 0 { 750 } else { 80 })
                })
            {
                if end_probe < 5 {
                    wheel_once()?;
                    end_probe += 1;
                    auto_scrolled = Some(Instant::now());
                    previous = None;
                    status = "Checking for more scrolling content…".into();
                } else if auto_scrolled.is_some_and(|t| t.elapsed() > Duration::from_millis(750)) {
                    running = false;
                    status = "End reached. Review Preview, then Finish.".into();
                }
            }
            controls.update(
                Caption::Scrolling {
                    automatic,
                    running,
                    countdown: remaining,
                },
                stitch.as_ref().is_some_and(Stitcher::can_undo),
                stitch.is_some(),
                false,
            )?;
            preview.status(&status);
            preview.refresh()?;
            std::thread::sleep(Duration::from_millis(15));
        }
    }
}
struct AutoCursor {
    original: POINT,
    placed: POINT,
    enabled: bool,
}
impl Drop for AutoCursor {
    fn drop(&mut self) {
        if self.enabled {
            unsafe {
                let mut p = POINT::default();
                if GetCursorPos(&mut p) != 0 && p.x == self.placed.x && p.y == self.placed.y {
                    SetCursorPos(self.original.x, self.original.y);
                }
            }
        }
    }
}
fn wheel_once() -> Result<(), String> {
    unsafe {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    mouseData: (-120i32) as u32,
                    dwFlags: MOUSEEVENTF_WHEEL,
                    ..Default::default()
                },
            },
        };
        if SendInput(1, &input, size_of::<INPUT>() as i32) != 1 {
            Err("Windows blocked automatic scrolling. Switch to Manual.".into())
        } else {
            Ok(())
        }
    }
}
