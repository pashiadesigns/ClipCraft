//! Best-effort action labels on one bounded process-wide worker. Never called
//! from the input hook; a slow provider cannot block screenshots or create an
//! unbounded collection of abandoned threads.
use super::*;
use capture_core::step_capture::{Context, Gesture, Guide};
use std::{
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

struct Request {
    point: [i32; 2],
    window: usize,
    at: Instant,
    pixels: image::RgbaImage,
    reply: mpsc::Sender<String>,
}
#[derive(Default)]
pub(super) struct Collector {
    pending: Vec<(usize, Receiver<String>)>,
}
impl Collector {
    pub fn offer(
        &mut self,
        index: usize,
        origin: [i32; 2],
        gesture: Gesture,
        pixels: &image::RgbaImage,
    ) -> Context {
        if index == 0 {
            self.pending.clear();
        }
        let point = [
            origin[0] + gesture.end[0] as i32,
            origin[1] + gesture.end[1] as i32,
        ];
        let window = unsafe {
            GetAncestor(
                WindowFromPoint(POINT {
                    x: point[0],
                    y: point[1],
                }),
                GA_ROOT,
            )
        };
        let bounds = unsafe { window_bounds(window) }.map(|b| {
            [
                b[0] - origin[0],
                b[1] - origin[1],
                b[2] - origin[0],
                b[3] - origin[1],
            ]
        });
        let (tx, rx) = mpsc::channel();
        let x = gesture.end[0].saturating_sub(200);
        let y = gesture.end[1].saturating_sub(32);
        let crop = image::imageops::crop_imm(
            pixels,
            x,
            y,
            400.min(pixels.width() - x),
            64.min(pixels.height() - y),
        )
        .to_image();
        if let Some(sender) = broker()
            && sender
                .try_send(Request {
                    point,
                    window: window as usize,
                    at: Instant::now(),
                    pixels: crop,
                    reply: tx,
                })
                .is_ok()
        {
            self.pending.push((index, rx));
        }
        Context { bounds, name: None }
    }
    pub fn finish(&mut self, guide: &mut Guide) {
        // Whole finish budget, not one wait per step. Unavailable labels retain
        // their editable Click/Drag caption; source screenshots are unaffected.
        let deadline = Instant::now() + Duration::from_secs(1);
        for (index, rx) in self.pending.drain(..) {
            if let Ok(name) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                guide.set_name(index, &name);
            }
        }
    }
}
fn broker() -> Option<&'static SyncSender<Request>> {
    static BROKER: OnceLock<Option<SyncSender<Request>>> = OnceLock::new();
    BROKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel::<Request>(4);
            std::thread::Builder::new()
                .name("clipcraft-step-labels".into())
                .spawn(move || {
                    use windows::Win32::{
                        System::Com::{
                            CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance,
                            CoInitializeEx, CoUninitialize,
                        },
                        UI::Accessibility::{CUIAutomation8, IUIAutomation2},
                    };
                    let prior = unsafe {
                        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
                    };
                    let _dpi = (!prior.is_null()).then_some(DpiContext(prior));
                    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                    let automation = if initialized {
                        unsafe {
                            CoCreateInstance::<_, IUIAutomation2>(
                                &CUIAutomation8,
                                None,
                                CLSCTX_INPROC_SERVER,
                            )
                        }
                        .ok()
                    } else {
                        None
                    };
                    if let Some(a) = &automation {
                        unsafe {
                            let _ = a.SetConnectionTimeout(80);
                            let _ = a.SetTransactionTimeout(80);
                            let _ = a.SetAutoSetFocus(false);
                        }
                    }
                    for request in rx {
                        let mut password = false;
                        let current = unsafe {
                            GetAncestor(
                                WindowFromPoint(POINT {
                                    x: request.point[0],
                                    y: request.point[1],
                                }),
                                GA_ROOT,
                            )
                        } as usize;
                        let name = if request.at.elapsed() < Duration::from_millis(200)
                            && current == request.window
                        {
                            automation.as_ref().and_then(|a| unsafe {
                                let e = a
                                    .ElementFromPoint(windows::Win32::Foundation::POINT {
                                        x: request.point[0],
                                        y: request.point[1],
                                    })
                                    .ok()?;
                                password = e.CurrentIsPassword().ok()?.as_bool();
                                if password {
                                    None
                                } else {
                                    e.CurrentName()
                                        .ok()
                                        .map(|s| s.to_string())
                                        .filter(|s| !s.trim().is_empty())
                                }
                            })
                        } else {
                            None
                        };
                        let name = name.or_else(|| {
                            if password {
                                return None;
                            }
                            crate::ocr::recognize_bounded(
                                &request.pixels,
                                Duration::from_millis(250),
                            )
                            .ok()
                            .and_then(|r| {
                                r.text
                                    .lines()
                                    .find(|s| !s.trim().is_empty())
                                    .map(str::to_owned)
                            })
                        });
                        if let Some(name) = name {
                            let _ = request.reply.send(name);
                        }
                    }
                    drop(automation);
                    if initialized {
                        unsafe {
                            CoUninitialize();
                        }
                    }
                })
                .ok()?;
            Some(tx)
        })
        .as_ref()
}
pub(super) fn date() -> String {
    let mut time = windows_sys::Win32::Foundation::SYSTEMTIME::default();
    unsafe {
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time);
    }
    format!("{:04}-{:02}-{:02}", time.wYear, time.wMonth, time.wDay)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_caption_replies_stay_with_their_steps_and_missing_names_are_safe() {
        let mut guide = Guide::new([80, 60]).unwrap();
        for _ in 0..2 {
            guide
                .push(
                    image::RgbaImage::new(80, 60),
                    Gesture {
                        start: [20, 20],
                        end: [20, 20],
                        button: capture_core::step_capture::Button::Left,
                    },
                )
                .unwrap();
        }
        let (a, ar) = mpsc::channel();
        let (b, br) = mpsc::channel();
        b.send("Second button".into()).unwrap();
        a.send("First button".into()).unwrap();
        let mut c = Collector {
            pending: vec![(0, ar), (1, br)],
        };
        c.finish(&mut guide);
        let d = guide.document().unwrap();
        let labels: Vec<_> = d
            .annotations
            .iter()
            .filter(|a| a.kind == capture_core::Kind::Text)
            .map(|a| a.text.as_ref().unwrap().content.as_str())
            .collect();
        assert_eq!(
            labels,
            ["Click \"First button\"", "Click \"Second button\""]
        );
        let (tx, rx) = mpsc::channel();
        drop(tx);
        c.pending.push((1, rx));
        c.finish(&mut guide);
        assert_eq!(guide.document().unwrap().annotations, d.annotations);
    }
}
