//! Bounded input sampling; accessible captions run on the existing async broker.
use super::*;
use capture_core::video::{
    instruction::Event,
    motion_path::{Node, Path, Purpose},
};

#[derive(Clone, Copy)]
struct MouseEvent {
    point: POINT,
    time: u32,
    button: usize,
    down: bool,
}
/// Clicks reported by the low-level hook. The hook runs on its own thread (see
/// [`Hook`]), so this queue is shared with the recording thread.
static EVENTS: std::sync::Mutex<std::collections::VecDeque<MouseEvent>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
fn take_events() -> Vec<MouseEvent> {
    EVENTS
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}
fn clear_events() {
    if let Ok(mut q) = EVENTS.lock() {
        q.clear();
    }
}
unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && matches!(
            w as u32,
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP
        )
    {
        let data = unsafe { &*(l as *const MSLLHOOKSTRUCT) };
        if data.flags & LLMHF_INJECTED == 0 {
            let message = w as u32;
            // The recording thread only holds this lock to drain a few events, so
            // the hook never waits meaningfully and no click is dropped.
            if let Ok(mut q) = EVENTS.lock()
                && q.len() < 1024
            {
                q.push_back(MouseEvent {
                    point: data.pt,
                    time: data.time,
                    button: usize::from(matches!(message, WM_RBUTTONDOWN | WM_RBUTTONUP)),
                    down: matches!(message, WM_LBUTTONDOWN | WM_RBUTTONDOWN),
                });
            }
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, w, l) }
}
/// A low-level mouse hook on a dedicated thread that only pumps messages.
/// Windows routes every pointer event through the hook's thread; installing it
/// on the busy capture/encode thread made the system cursor lag while recording.
struct Hook {
    thread: u32,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Hook {
    fn start() -> Result<Self, String> {
        let (ready, started) = std::sync::mpsc::channel();
        let join = std::thread::Builder::new()
            .name("clipdesigner-click-hook".into())
            .spawn(move || unsafe {
                let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), GetModuleHandleW(null()), 0);
                if hook.is_null() {
                    let _ = ready.send(Err(win_error("Cannot record instructional clicks")));
                    return;
                }
                let _ = ready.send(Ok(
                    windows_sys::Win32::System::Threading::GetCurrentThreadId(),
                ));
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                UnhookWindowsHookEx(hook);
            })
            .map_err(|e| e.to_string())?;
        match started.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(thread)) => Ok(Self {
                thread,
                join: Some(join),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                Err("Cannot record instructional clicks: the input hook did not start.".into())
            }
        }
    }
}
impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.thread, WM_QUIT, 0, 0);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
pub(super) struct Recorder {
    painter: cursor::Painter,
    pub path: Path,
    pub events: Vec<Event>,
    hook: Option<Hook>,
    active: [Option<usize>; 2],
    pending: Vec<(
        usize,
        std::sync::mpsc::Receiver<crate::capture::step_pipeline::Caption>,
    )>,
    last_sample: u64,
    enabled: bool,
    /// Step by Step describes what each click landed on; a plain recording
    /// only keeps when and where the clicks were.
    describe_clicks: bool,
    /// Consecutive samples where the cursor image could not be read; a short
    /// run holds the last cursor instead of blinking it out.
    missed: u32,
    /// Pointer position within the last visible cursor image.
    hotspot: [f32; 2],
}
/// About 100 ms of samples at the 16 ms sampling interval.
const HOLD_MISSED_SAMPLES: u32 = 6;
const MAX_FILLED_GAP_MS: u64 = 600;
fn fill_gaps(nodes: &mut [Node]) {
    let mut i = 0;
    while i < nodes.len() {
        if nodes[i].visible {
            i += 1;
            continue;
        }
        let mut end = i;
        while end < nodes.len() && !nodes[end].visible {
            end += 1;
        }
        if i > 0 && end < nodes.len() && nodes[end].at - nodes[i - 1].at <= MAX_FILLED_GAP_MS {
            let (a, b) = (nodes[i - 1].clone(), nodes[end].clone());
            let span = (b.at - a.at).max(1) as f32;
            for n in &mut nodes[i..end] {
                let t = (n.at - a.at) as f32 / span;
                n.visible = true;
                n.sprite = a.sprite;
                n.rect = a.rect;
                n.rect[0] += (b.rect[0] - a.rect[0]) * t;
                n.rect[1] += (b.rect[1] - a.rect[1]) * t;
            }
        }
        i = end;
    }
}
impl Recorder {
    /// `steps` is Step by Step, which cannot work without the click hook. A plain
    /// recording with a cursor uses it for Direct paths and records without it
    /// if it cannot start.
    pub fn new(enabled: bool, steps: bool) -> Result<Self, String> {
        clear_events();
        let hook = if steps {
            Some(Hook::start()?)
        } else if enabled {
            Hook::start().ok()
        } else {
            None
        };
        Ok(Self {
            painter: Default::default(),
            path: Path {
                purpose: Purpose::Cursor,
                reference: [0., 0., 24., 24.],
                nodes: vec![],
                sprites: vec![],
                smooth: false,
                enabled: true,
                offset_ms: 0,
                reviewed: true,
                reduction: Default::default(),
                inputs: vec![],
                recorded: None,
            },
            events: vec![],
            hook,
            active: [None; 2],
            pending: vec![],
            last_sample: 0,
            enabled,
            describe_clicks: steps,
            missed: 0,
            hotspot: [0., 0.],
        })
    }
    pub fn pause(&mut self) {
        clear_events();
        self.active = [None; 2];
    }
    pub fn sample(
        &mut self,
        at: u64,
        origin: [i32; 2],
        source: [u32; 2],
        size: [u32; 2],
        excluded: HWND,
    ) {
        if !self.enabled {
            return;
        }
        let mut point = POINT::default();
        unsafe {
            GetCursorPos(&mut point);
        }
        let inside = point.x >= origin[0]
            && point.y >= origin[1]
            && point.x < origin[0] + source[0] as i32
            && point.y < origin[1] + source[1] as i32
            && unsafe { GetAncestor(WindowFromPoint(point), GA_ROOT) } != excluded;
        let scale = [
            size[0] as f32 / source[0] as f32,
            size[1] as f32 / source[1] as f32,
        ];
        let location = [
            (point.x - origin[0]) as f32 * scale[0],
            (point.y - origin[1]) as f32 * scale[1],
        ];
        let mut clicked = false;
        let first_event = self.events.len();
        let events = take_events();
        for event in events {
            let i = event.button;
            let p = event.point;
            let p_inside = p.x >= origin[0]
                && p.y >= origin[1]
                && p.x < origin[0] + source[0] as i32
                && p.y < origin[1] + source[1] as i32
                && unsafe { GetAncestor(WindowFromPoint(p), GA_ROOT) } != excluded;
            let pos = [
                (p.x - origin[0]) as f32 * scale[0],
                (p.y - origin[1]) as f32 * scale[1],
            ];
            let age = unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() }
                .wrapping_sub(event.time) as u64;
            if event.down && p_inside && self.events.len() < 1000 {
                let index = self.events.len();
                self.events.push(Event {
                    at: at.saturating_sub(age),
                    point: pos,
                    button: i as u8,
                    dragging: false,
                    release: None,
                    caption: String::new(),
                });
                self.active[i] = Some(index);
                clicked = true;
                if self.describe_clicks && self.pending.len() < 8 {
                    self.pending.push((
                        index,
                        step_context::describe([p.x, p.y], step_context::Activity::Click),
                    ));
                }
            }
            if !event.down {
                if let Some(index) = self.active[i] {
                    let e = &mut self.events[index];
                    if (e.point[0] - pos[0]).hypot(e.point[1] - pos[1]) > 8. {
                        e.dragging = true;
                    }
                    e.release = Some(at.saturating_sub(age).max(e.at));
                }
                self.active[i] = None;
            }
        }
        for index in self.active.iter().flatten() {
            let e = &mut self.events[*index];
            if (e.point[0] - location[0]).hypot(e.point[1] - location[1]) > 8. {
                e.dragging = true;
            }
        }
        self.poll();
        if !clicked && at.saturating_sub(self.last_sample) < 16 && !self.path.nodes.is_empty() {
            return;
        }
        self.last_sample = at;
        let mut node = Node {
            at,
            rect: self
                .path
                .nodes
                .last()
                .map_or(self.path.reference, |n| n.rect),
            visible: false,
            sprite: 0,
            confidence: 1.,
        };
        let snapshot = if inside {
            self.painter.snapshot()
        } else {
            None
        };
        if inside && snapshot.is_none() {
            self.missed += 1;
        } else {
            self.missed = 0;
        }
        // A brief failure to read the cursor image (shape change, busy desktop)
        // holds the last cursor at the pointer instead of blinking it out.
        // A longer run means the cursor really is hidden.
        if inside
            && snapshot.is_none()
            && self.missed <= HOLD_MISSED_SAMPLES
            && let Some(last) = self.path.nodes.last().filter(|n| n.visible).cloned()
        {
            node = last;
            node.at = at;
            node.rect[0] = location[0] - self.hotspot[0];
            node.rect[1] = location[1] - self.hotspot[1];
        }
        if let Some(c) = snapshot {
            let index = self.path.sprites.iter().position(|b| {
                std::sync::Arc::ptr_eq(b, &c.bitmap) || b.as_ref() == c.bitmap.as_ref()
            });
            let index = index.or_else(|| {
                if self.path.sprites.len() < 64 {
                    self.path.sprites.push(c.bitmap.clone());
                    Some(self.path.sprites.len() - 1)
                } else {
                    // Out of distinct shapes: reuse the closest-sized one rather
                    // than hiding the cursor.
                    self.path
                        .sprites
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, b)| {
                            let (s, t) = (b.size(), c.bitmap.size());
                            s[0].abs_diff(t[0]) + s[1].abs_diff(t[1])
                        })
                        .map(|(i, _)| i)
                }
            });
            if let Some(index) = index {
                node.sprite = index;
                node.visible = true;
                node.rect = [
                    (c.position[0] - origin[0]) as f32 * scale[0],
                    (c.position[1] - origin[1]) as f32 * scale[1],
                    c.bitmap.size()[0] as f32 * scale[0],
                    c.bitmap.size()[1] as f32 * scale[1],
                ];
                if self.path.sprites.len() == 1 && self.path.nodes.iter().all(|n| !n.visible) {
                    self.path.reference = node.rect;
                }
            }
        }
        if node.visible {
            let hotspot = [location[0] - node.rect[0], location[1] - node.rect[1]];
            self.hotspot = hotspot;
            for event in &self.events[first_event..] {
                let mut anchor = node.clone();
                anchor.at = event.at;
                anchor.rect[0] = event.point[0] - hotspot[0];
                anchor.rect[1] = event.point[1] - hotspot[1];
                let index = self.path.nodes.partition_point(|n| n.at < anchor.at);
                if self
                    .path
                    .nodes
                    .get(index)
                    .is_some_and(|n| n.at == anchor.at)
                {
                    self.path.nodes[index] = anchor;
                } else if self.path.nodes.len() < 219999 {
                    self.path.nodes.insert(index, anchor);
                }
            }
        }
        self.path.push(node);
    }
    /// Plain recordings keep every sample (no path reduction), so a short run of
    /// unreadable cursor samples showed as a blink (BA1). Gaps up to
    /// [`MAX_FILLED_GAP_MS`] between two visible samples glide instead.
    pub fn fill_cursor_gaps(&mut self) {
        fill_gaps(&mut self.path.nodes);
    }
    pub fn poll(&mut self) {
        self.pending.retain(|(index, rx)| match rx.try_recv() {
            Ok(c) => {
                self.events[*index].caption = c
                    .instruction
                    .unwrap_or_else(|| c.name.map(|n| format!("Click {n}")).unwrap_or_default());
                false
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
            _ => true,
        });
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.hook = None;
        clear_events();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_unreadable_cursor_runs_glide_but_real_absences_stay_hidden() {
        let node = |at: u64, x: f32, visible: bool| Node {
            at,
            rect: [x, 10., 24., 24.],
            visible,
            sprite: 0,
            confidence: 1.,
        };
        let mut nodes = vec![
            node(0, 0., true),
            node(16, 0., false),
            node(32, 0., false),
            node(48, 48., true),
            node(64, 48., false),
            node(2000, 48., false),
            node(2016, 100., true),
        ];
        fill_gaps(&mut nodes);
        assert!(nodes[..4].iter().all(|n| n.visible));
        assert!((nodes[1].rect[0] - 16.).abs() < 0.01);
        // Two seconds off the region or hidden is a real absence.
        assert!(!nodes[4].visible && !nodes[5].visible);
        assert!(nodes[6].visible);
    }
    #[test]
    fn click_hook_runs_on_its_own_thread_and_stops_cleanly() {
        let started = std::time::Instant::now();
        for _ in 0..3 {
            let hook = Hook::start().expect("hook installs");
            assert_ne!(hook.thread, unsafe {
                windows_sys::Win32::System::Threading::GetCurrentThreadId()
            });
            drop(hook);
        }
        assert!(started.elapsed() < Duration::from_secs(10));
        // Events queued by the hook reach the recording thread.
        EVENTS.lock().unwrap().push_back(MouseEvent {
            point: POINT { x: 5, y: 6 },
            time: 0,
            button: 0,
            down: true,
        });
        let events = take_events();
        assert_eq!(events.len(), 1);
        assert!(take_events().is_empty());
    }
}
