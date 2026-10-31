//! Recording writer shared by native capture and injected synthetic tests.
use super::CaptureResult;
use capture_core::{
    history::{Entry, History, Origin},
    step_capture::{Gesture, Guide},
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Area {
    pub origin: [i32; 2],
    pub size: [u32; 2],
}

pub(super) enum Input {
    Screen(Area),
    Capture { gesture: Gesture, at: Instant },
    Restart,
    Cancel,
}

#[cfg(test)]
pub(super) fn run<F: Into<capture_core::step_capture::Frame>>(
    inputs: mpsc::Receiver<Input>,
    size: [u32; 2],
    mut capture: impl FnMut() -> Result<F, String>,
    root: PathBuf,
    count: Arc<AtomicUsize>,
    failure: Arc<Mutex<Option<String>>>,
) -> CaptureResult {
    run_writer(
        inputs,
        Some(Area {
            origin: [0, 0],
            size,
        }),
        |_, _, _| capture(),
        root,
        count,
        failure,
        None,
    )
}

#[cfg(test)]
pub(super) fn run_dynamic<F: Into<capture_core::step_capture::Frame>>(
    inputs: mpsc::Receiver<Input>,
    mut capture: impl FnMut(Area) -> Result<F, String>,
    root: PathBuf,
    count: Arc<AtomicUsize>,
    failure: Arc<Mutex<Option<String>>>,
) -> CaptureResult {
    run_writer(
        inputs,
        None,
        |a, _, _| capture(a),
        root,
        count,
        failure,
        None,
    )
}

pub(super) struct Enrichment<'a> {
    pub date: String,
    pub finish: Box<dyn FnMut(&mut Guide) + 'a>,
}
pub(super) fn run_context<F: Into<capture_core::step_capture::Frame>>(
    inputs: mpsc::Receiver<Input>,
    capture: impl FnMut(Area, Gesture, usize) -> Result<F, String>,
    root: PathBuf,
    count: Arc<AtomicUsize>,
    failure: Arc<Mutex<Option<String>>>,
    enrichment: Enrichment<'_>,
) -> CaptureResult {
    run_writer(
        inputs,
        None,
        capture,
        root,
        count,
        failure,
        Some(enrichment),
    )
}

fn run_writer<F: Into<capture_core::step_capture::Frame>>(
    inputs: mpsc::Receiver<Input>,
    initial: Option<Area>,
    mut capture: impl FnMut(Area, Gesture, usize) -> Result<F, String>,
    root: PathBuf,
    count: Arc<AtomicUsize>,
    failure: Arc<Mutex<Option<String>>>,
    mut enrichment: Option<Enrichment<'_>>,
) -> CaptureResult {
    let mut started = None;
    let mut area = initial;
    let mut guide = initial
        .map(|a| Guide::new(a.size))
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut history = History::open(&root).map_err(|e| e.to_string())?;
    let mut entry: Option<Entry> = None;
    let mut latest_saved = false;
    for input in inputs {
        if let Input::Screen(selected) = input {
            if guide.as_ref().is_some_and(|g| !g.is_empty()) {
                *failure.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some("Recording monitor cannot change until Restart".into());
                break;
            }
            guide = Some(Guide::new(selected.size).map_err(|e| e.to_string())?);
            area = Some(selected);
            continue;
        }
        if matches!(input, Input::Restart | Input::Cancel) {
            if let Some(current) = &entry
                && let Err(e) = history.discard(current)
            {
                *failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
                break;
            }
            if let Some(warning) = history.cleanup_warning() {
                *failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(warning.to_owned());
            }
            area = initial;
            guide = initial
                .map(|a| Guide::new(a.size))
                .transpose()
                .map_err(|e| e.to_string())?;
            entry = None;
            latest_saved = false;
            started = None;
            count.store(0, Ordering::Release);
            if matches!(input, Input::Cancel) {
                return if let Some(warning) =
                    failure.lock().unwrap_or_else(|e| e.into_inner()).clone()
                {
                    Err(warning)
                } else {
                    Ok(None)
                };
            }
            continue;
        }
        let Input::Capture { gesture, at } = input else {
            unreachable!()
        };
        let result = (|| -> Result<(), String> {
            if at.elapsed() > Duration::from_millis(300) {
                return Err("Recording stopped: the PC was too busy to capture the next click promptly. Saved steps are preserved; repeat the last action in a new guide.".into());
            }
            let guide = guide.as_mut().ok_or("Select a recording monitor first")?;
            let pixels = capture(
                area.ok_or("Select a recording monitor first")?,
                gesture,
                guide.len(),
            )?;
            let began = *started.get_or_insert(at);
            if let Some(options) = &enrichment {
                guide
                    .set_header(capture_core::step_capture::Header {
                        date: options.date.clone(),
                        seconds: at.saturating_duration_since(began).as_secs(),
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
            }
            guide.push(pixels, gesture).map_err(|e| e.to_string())?;
            latest_saved = false;
            let document = guide.document().map_err(|e| e.to_string())?;
            let saved = if let Some(old) = &entry {
                history.update(old, &document)
            } else {
                history.create(&document, "Step guide", Origin::Capture)
            }
            .map_err(|e| e.to_string())?;
            entry = Some(saved);
            latest_saved = true;
            count.store(guide.len(), Ordering::Release);
            Ok(())
        })();
        if let Err(e) = result {
            *failure.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
            break;
        }
    }
    let warning = failure
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    if guide.as_ref().is_none_or(|g| g.is_empty()) {
        return if warning.is_empty() {
            Ok(None)
        } else {
            Err(warning)
        };
    }
    // Enrichment uses already captured data. It cannot delay the low-level hook
    // or rewrite an earlier session after Restart. Save the enriched guide too.
    if let Some(options) = &mut enrichment {
        let guide = guide.as_mut().unwrap();
        (options.finish)(guide);
        let document = guide.document().map_err(|e| e.to_string())?;
        if let Some(old) = &entry {
            match history.update(old, &document) {
                Ok(saved) => {
                    entry = Some(saved);
                    latest_saved = true;
                }
                Err(e) => {
                    return Ok(Some(crate::capture::Captured {
                        document,
                        warning: format!("{warning} Guide captions could not be saved: {e}"),
                        recognized: None,
                        retained: None,
                    }));
                }
            }
        }
    }
    Ok(Some(crate::capture::Captured {
        document: guide.unwrap().document().map_err(|e| e.to_string())?,
        warning,
        recognized: None,
        retained: if latest_saved { entry } else { None },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contextual_writer_enriches_current_session_and_reopens_exact_final_guide() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let area = Area {
            origin: [-800, 50],
            size: [320, 180],
        };
        for message in [
            Input::Screen(area),
            input(Instant::now()),
            Input::Restart,
            Input::Screen(area),
            input(Instant::now()),
        ] {
            tx.send(message).unwrap();
        }
        drop(tx);
        let mut captured = 0;
        let result = run_context(
            rx,
            |a, g, index| {
                assert_eq!(a, area);
                assert_eq!(index, 0);
                captured += 1;
                Ok(capture_core::step_capture::Frame {
                    image: image::RgbaImage::new(a.size[0], a.size[1]),
                    cursor: None,
                    context: capture_core::step_capture::Context {
                        bounds: Some([0, 0, 180, 140]),
                        name: Some(format!("Action {captured} at {}", g.end[0])),
                    },
                })
            },
            dir.path().to_owned(),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(None)),
            Enrichment {
                date: "2026-10-31".into(),
                finish: Box::new(|g| {
                    assert_eq!(g.len(), 1);
                    assert!(g.set_name(0, "Current session only"));
                }),
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(captured, 2);
        let history = History::open(dir.path()).unwrap();
        assert_eq!(history.entries().len(), 1);
        let saved = history.load(&result.retained.unwrap().id).unwrap();
        assert_eq!(saved.annotations, result.document.annotations);
        assert_eq!(saved.guide.as_ref().unwrap().sections.len(), 2);
        let content: Vec<_> = saved
            .annotations
            .iter()
            .filter_map(|a| a.text.as_ref().map(|t| t.content.as_str()))
            .collect();
        assert!(content.contains(&"Click \"Current session only\""));
        assert!(content.contains(&"1 step | 0:00 | 2026-10-31"));
    }
    #[test]
    fn first_monitor_routes_capture_and_restart_accepts_a_different_monitor() {
        let dir = tempfile::tempdir().unwrap();
        let left = Area {
            origin: [-1200, -50],
            size: [80, 60],
        };
        let right = Area {
            origin: [0, 0],
            size: [100, 75],
        };
        let (tx, rx) = mpsc::channel();
        tx.send(Input::Screen(left)).unwrap();
        tx.send(input(Instant::now())).unwrap();
        tx.send(Input::Restart).unwrap();
        tx.send(Input::Screen(right)).unwrap();
        tx.send(input(Instant::now())).unwrap();
        drop(tx);
        let mut captured = Vec::new();
        let result = run_dynamic(
            rx,
            |area| {
                captured.push(area);
                Ok(image::RgbaImage::from_pixel(
                    area.size[0],
                    area.size[1],
                    image::Rgba([20, 40, 60, 255]),
                ))
            },
            dir.path().to_owned(),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(None)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(captured, [left, right]);
        assert_eq!(result.document.annotations.len(), 4);
        assert_eq!(History::open(dir.path()).unwrap().entries().len(), 1);
        let (tx, rx) = mpsc::channel();
        drop(tx);
        assert!(
            run_dynamic(
                rx,
                |_| -> Result<image::RgbaImage, String> { panic!("No click: no screenshot") },
                dir.path().to_owned(),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(Mutex::new(None))
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn restart_and_cancel_discard_only_this_recording_and_reset_the_count() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::open(dir.path()).unwrap();
        let other = h
            .create(
                &capture_core::Document::blank(40, 30).unwrap(),
                "Other image",
                Origin::Canvas,
            )
            .unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(input(Instant::now())).unwrap();
        tx.send(Input::Restart).unwrap();
        tx.send(input(Instant::now())).unwrap();
        drop(tx);
        let count = Arc::new(AtomicUsize::new(0));
        let mut calls = 0;
        let capture = run(
            rx,
            [40, 30],
            || {
                if calls == 1 {
                    assert_eq!(History::open(dir.path()).unwrap().entries().len(), 1);
                    assert_eq!(count.load(Ordering::Acquire), 0);
                }
                calls += 1;
                Ok(image::RgbaImage::from_pixel(
                    40,
                    30,
                    image::Rgba([calls, 80, 120, 255]),
                ))
            },
            dir.path().to_owned(),
            count.clone(),
            Arc::new(Mutex::new(None)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(count.load(Ordering::Acquire), 1);
        assert_eq!(capture.document.annotations.len(), 4);
        let retained = capture.retained.unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(input(Instant::now())).unwrap();
        tx.send(Input::Cancel).unwrap();
        drop(tx);
        assert!(
            run(
                rx,
                [40, 30],
                || Ok(image::RgbaImage::from_pixel(
                    40,
                    30,
                    image::Rgba([70, 80, 120, 255])
                )),
                dir.path().to_owned(),
                count.clone(),
                Arc::new(Mutex::new(None))
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(count.load(Ordering::Acquire), 0);
        let h = History::open(dir.path()).unwrap();
        assert_eq!(h.entries().len(), 2);
        assert!(h.load(&other.id).is_ok());
        assert!(h.load(&retained.id).is_ok());
    }
    fn input(at: Instant) -> Input {
        Input::Capture {
            gesture: Gesture {
                start: [20, 20],
                end: [20, 20],
                button: capture_core::step_capture::Button::Left,
            },
            at,
        }
    }
    #[test]
    fn writer_saves_each_step_into_one_reopenable_library_item_and_retains_partial_success() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        let count = Arc::new(AtomicUsize::new(0));
        let failure = Arc::new(Mutex::new(None));
        for _ in 0..3 {
            tx.send(input(Instant::now())).unwrap();
        }
        drop(tx);
        let mut calls = 0;
        let result = run(
            rx,
            [80, 60],
            || {
                // The previous step must already be durable before another is read.
                if calls > 0 {
                    let h = History::open(dir.path()).unwrap();
                    assert_eq!(h.entries().len(), 1);
                    assert_eq!(
                        h.load(&h.entries()[0].id).unwrap().annotations.len(),
                        calls * 4
                    );
                }
                calls += 1;
                if calls == 3 {
                    return Err("injected display disconnected".into());
                }
                Ok(image::RgbaImage::from_pixel(
                    80,
                    60,
                    image::Rgba([calls as u8, 30, 40, 255]),
                ))
            },
            dir.path().to_owned(),
            count.clone(),
            failure,
        )
        .unwrap()
        .unwrap();
        assert_eq!(count.load(Ordering::Acquire), 2);
        assert!(result.warning.contains("display disconnected"));
        let history = History::open(dir.path()).unwrap();
        let entry = result.retained.unwrap();
        assert_eq!(history.entries().len(), 1);
        assert_eq!(
            history.load(&entry.id).unwrap().annotations,
            result.document.annotations
        );
        assert_eq!(result.document.annotations.len(), 8);
    }
    #[test]
    fn stale_events_never_capture_an_inaccurate_screenshot_and_empty_finish_is_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::channel();
        tx.send(input(Instant::now() - Duration::from_secs(2)))
            .unwrap();
        drop(tx);
        let result = run(
            rx,
            [80, 60],
            || -> Result<image::RgbaImage, String> {
                panic!("Late events must not read the screen")
            },
            dir.path().to_owned(),
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(None)),
        );
        assert!(result.err().unwrap().contains("too busy"));
        assert!(History::open(dir.path()).unwrap().entries().is_empty());
        let (tx, rx) = mpsc::channel();
        drop(tx);
        assert!(
            run(
                rx,
                [80, 60],
                || -> Result<image::RgbaImage, String> { panic!("Empty capture") },
                dir.path().to_owned(),
                Arc::new(AtomicUsize::new(0)),
                Arc::new(Mutex::new(None))
            )
            .unwrap()
            .is_none()
        );
    }
}
