//! Screen pixels, opt-in WASAPI audio and MP4 encoding share a worker-owned clock.
use super::*;
use super::{
    step_recorder::visual::Caption,
    transport::{Action, Transport},
    video_audio,
    video_encoder::{Encoder, Runtime},
};
use capture_core::video::{self, Phase, Session, audio::Track};
use std::time::Instant;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
#[path = "window_frames.rs"]
mod window_frames;
fn now() -> Result<u64, String> {
    unsafe {
        let mut counter = 0;
        let mut freq = 0;
        QueryPerformanceCounter(&mut counter).map_err(|e| e.to_string())?;
        QueryPerformanceFrequency(&mut freq).map_err(|e| e.to_string())?;
        Ok((counter as u128 * 10_000_000 / freq as u128) as u64)
    }
}
struct Frames {
    screen: ScreenDc,
    memory: MemoryDc,
    _bitmap: Bitmap,
    old: HGDIOBJ,
    bits: *mut u8,
    size: [u32; 2],
    cursor: cursor::Painter,
}
impl Drop for Frames {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory.0, self.old);
        }
    }
}
impl Frames {
    fn new(size: [u32; 2]) -> Result<Self, String> {
        unsafe {
            let screen = ScreenDc(GetDC(null_mut()));
            let memory = MemoryDc(CreateCompatibleDC(screen.0));
            let mut bits = null_mut();
            let bitmap = Bitmap(CreateDIBSection(
                memory.0,
                &bitmap_info(size),
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            ));
            if screen.0.is_null() || memory.0.is_null() || bitmap.0.is_null() || bits.is_null() {
                return Err(win_error("Cannot allocate video capture surface"));
            }
            let old = SelectObject(memory.0, bitmap.0);
            SetStretchBltMode(memory.0, HALFTONE);
            Ok(Self {
                screen,
                memory,
                _bitmap: bitmap,
                old,
                bits: bits.cast(),
                size,
                cursor: Default::default(),
            })
        }
    }
    fn capture(
        &mut self,
        origin: [i32; 2],
        source: [u32; 2],
        cursor: bool,
    ) -> Result<&[u8], String> {
        unsafe {
            if StretchBlt(
                self.memory.0,
                0,
                0,
                self.size[0] as i32,
                self.size[1] as i32,
                self.screen.0,
                origin[0],
                origin[1],
                source[0] as i32,
                source[1] as i32,
                // BA1: CAPTUREBLT makes Windows hide and redraw the live pointer on
                // every copy (a visible blink at the recording frame rate). With DWM
                // composition, layered windows are captured without it.
                SRCCOPY,
            ) == 0
            {
                return Err(win_error("Cannot capture video frame"));
            }
            GdiFlush();
            if cursor {
                self.cursor.draw(
                    std::slice::from_raw_parts_mut(
                        self.bits,
                        self.size[0] as usize * self.size[1] as usize * 4,
                    ),
                    origin,
                    source,
                    self.size,
                );
            }
            Ok(std::slice::from_raw_parts(
                self.bits,
                self.size[0] as usize * self.size[1] as usize * 4,
            ))
        }
    }
}
pub(super) fn record(
    owner: HWND,
    origin: [i32; 2],
    source: [u32; 2],
    window: Option<usize>,
    request: super::super::VideoRequest,
) -> CaptureResult {
    let options = &request.options;
    options.validate().map_err(|e| e.to_string())?;
    let size = video::Options::output_size(source).map_err(|e| e.to_string())?;
    let _runtime =
        Runtime::new().map_err(|e| format!("Windows video encoding is unavailable: {e}"))?;
    let parent = request
        .destination
        .parent()
        .ok_or("Choose a video folder.")?;
    if free_bytes(parent)? < 256 * 1024 * 1024 {
        return Err("Free at least 256 MB in the selected video folder before recording.".into());
    }
    let temp = tempfile::Builder::new()
        .prefix(".clipcraft-recording-")
        .suffix(".mp4")
        .tempfile_in(parent)
        .map_err(|e| format!("Cannot create video in this folder: {e}"))?
        .into_temp_path();
    let mut encoder = Encoder::with_bitrate(
        &temp,
        size,
        options.fps,
        options.microphone || options.system_audio,
        options.quality.bitrate(options.fps),
    )
    .map_err(|e| format!("Cannot start the Windows MP4 encoder: {e}"))?;
    let mic = if options.microphone {
        Some(video_audio::Input::open(false).map_err(|e|format!("Cannot open the default microphone. Check Windows microphone privacy permissions and the device: {e}"))?)
    } else {
        None
    };
    let system = if options.system_audio {
        Some(video_audio::Input::open(true).map_err(|e| {
            format!("Cannot open system audio. Check the default playback device: {e}")
        })?)
    } else {
        None
    };
    // AS13: the webcam opens before the countdown so its first frames are ready
    // when the screen recording starts. A busy or missing camera never stops the
    // screen recording; the warning says it was left out.
    let mut webcam_warning = String::new();
    let mut webcam = if options.webcam.enabled {
        match Webcam::open(&options.webcam.device, parent.to_path_buf()) {
            Ok(w) => Some(w),
            Err(e) => {
                webcam_warning = format!("Recorded without the webcam: {e}");
                None
            }
        }
    } else {
        None
    };
    unsafe {
        let center = POINT {
            x: origin[0] + source[0] as i32 / 2,
            y: origin[1] + source[1] as i32 / 2,
        };
        let target = window.map_or_else(
            || GetAncestor(WindowFromPoint(center), GA_ROOT),
            |h| h as HWND,
        );
        let mut original = RECT::default();
        GetWindowRect(target, &mut original);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(
            MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST),
            &mut info,
        ) == 0
        {
            return Err(win_error("Cannot locate video monitor"));
        }
        let countdown_seconds = if options.countdown { 3 } else { 0 };
        let mut transport = Transport::new(
            owner,
            info.rcWork,
            GetDpiForWindow(owner).max(96) as i32,
            Caption::Video {
                seconds: 0,
                countdown: countdown_seconds,
                steps: options.instruction.enabled.then_some(0),
                webcam: webcam.is_some(),
            },
        )?;
        let mut window_frames = window.map(|_| {
            window_frames::WindowFrames::start(
                target as usize,
                original,
                origin,
                source,
                size,
                false,
                options.fps,
            )
        });
        let countdown = Instant::now();
        while countdown.elapsed() < Duration::from_secs(countdown_seconds as u64) {
            if !transport::pump() || GetAsyncKeyState(VK_ESCAPE as i32) < 0 {
                return Ok(None);
            }
            while let Some(action) = transport.action() {
                if action == Action::Cancel {
                    return Ok(None);
                }
            }
            transport.update(
                Caption::Video {
                    seconds: 0,
                    countdown: countdown_seconds
                        .saturating_sub(countdown.elapsed().as_secs() as u32),
                    steps: options.instruction.enabled.then_some(0),
                    webcam: webcam.is_some(),
                },
                false,
                false,
                false,
            )?;
            std::thread::sleep(Duration::from_millis(10));
        }
        DwmFlush();
        // Wait for the selected window before starting the audio/video clock.
        // With countdown disabled the PrintWindow worker often has not run yet.
        if let Some(window) = &mut window_frames {
            while window.frame()?.is_none() {
                if !transport::pump() || GetAsyncKeyState(VK_ESCAPE as i32) < 0 {
                    return Ok(None);
                }
                while let Some(action) = transport.action() {
                    if matches!(action, Action::Cancel | Action::Finish) {
                        return Ok(None);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let config = video::Settings {
            source: video::Source::Region {
                origin,
                size: source,
            },
            size,
            fps: options.fps,
            bitrate: options.quality.bitrate(options.fps),
            microphone: options.microphone,
            system_audio: options.system_audio,
            cursor: options.cursor,
        };
        let mut session = Session::new(config).map_err(|e| e.to_string())?;
        let mut frames = Frames::new(size)?;
        let mut input = super::video_input::Recorder::new(
            options.cursor || options.instruction.enabled,
            options.instruction.enabled,
        )?;
        let mut audio = Audio::new(mic, system, parent)?;
        let mut poster = None;
        let mut warning = webcam_warning;
        let mut finish = false;
        let origin_clock = now()?;
        session.start(origin_clock).map_err(|e| e.to_string())?;
        audio.start()?;
        let mut escape_down = false;
        let mut disk_check = Instant::now();
        loop {
            if !transport::pump() {
                return Ok(None);
            }
            let escape = GetAsyncKeyState(VK_ESCAPE as i32) < 0;
            if escape && !escape_down {
                if session.phase() == Phase::Recording {
                    audio.pause(&mut session, &mut encoder)?;
                }
                if restart::confirm_capture(transport.hwnd(), false)? {
                    return Ok(None);
                }
            }
            escape_down = escape;
            while let Some(action) = transport.action() {
                match action {
                    Action::Record => {
                        if session.phase() == Phase::Recording {
                            audio.pause(&mut session, &mut encoder)?;
                        } else if session.phase() == Phase::Paused {
                            if window.is_some() && options.pause_on_focus_loss {
                                SetForegroundWindow(target);
                            }
                            warning.clear();
                            session.resume(now()?).map_err(|e| e.to_string())?;
                            audio.start()?;
                        }
                    }
                    Action::Cancel => {
                        if session.phase() == Phase::Recording {
                            audio.pause(&mut session, &mut encoder)?;
                        }
                        if restart::confirm_capture(transport.hwnd(), false)? {
                            return Ok(None);
                        }
                    }
                    Action::Finish => finish = poster.is_some(),
                    _ => (),
                }
            }
            let clock = now()?;
            let elapsed = session.elapsed(clock);
            if let Some(w) = &webcam {
                w.update(&session, origin_clock, clock);
            }
            if elapsed >= 36_000_000_000 {
                warning = "One-hour recording limit reached.".into();
                finish = true;
            }
            if window.is_some() && session.phase() == Phase::Recording {
                let mut bounds = RECT::default();
                if IsWindow(target) == 0
                    || IsIconic(target) != 0
                    || GetWindowRect(target, &mut bounds) == 0
                    || [bounds.left, bounds.top, bounds.right, bounds.bottom]
                        != [original.left, original.top, original.right, original.bottom]
                    || options.pause_on_focus_loss
                        && GetForegroundWindow() != target
                        && GetForegroundWindow() != transport.hwnd()
                {
                    audio.pause(&mut session, &mut encoder)?;
                    warning="Recording paused because the selected window moved, resized, minimized or lost focus. Restore it before resuming.".into();
                }
            }
            if session.phase() == Phase::Recording {
                input.sample(elapsed / 10_000, origin, source, size, transport.hwnd());
                let audio_result = audio.collect(&session);
                if let Err(e) = audio_result {
                    audio.broken = true;
                    warning = format!(
                        "Audio capture stopped: {e}. Recording ended at the last complete frames."
                    );
                    finish = poster.is_some();
                    if !finish {
                        return Err(warning);
                    }
                }
                audio.flush(
                    elapsed.saturating_sub(1_000_000) * 48_000 / 10_000_000,
                    &mut encoder,
                )?;
                if !finish
                    && let Some(pts) = session.frame_time(clock).map_err(|e| e.to_string())?
                {
                    let pixels = if let Some(window) = &mut window_frames {
                        match window.frame() {
                            Ok(Some(pixels)) => pixels,
                            Ok(None) => continue,
                            Err(e) if poster.is_some() => {
                                warning = e;
                                break;
                            }
                            Err(e) => return Err(e),
                        }
                    } else {
                        frames.capture(origin, source, false)?
                    };
                    encoder
                        .video(pts, pixels)
                        .map_err(|e| format!("Cannot encode video: {e}"))?;
                    if poster.is_none() {
                        poster = Some(crop_bgra(
                            pixels,
                            size,
                            Region {
                                x: 0,
                                y: 0,
                                width: size[0],
                                height: size[1],
                            },
                        )?);
                    }
                }
            }
            if session.phase() != Phase::Recording {
                input.pause();
            }
            if disk_check.elapsed() > Duration::from_secs(2) {
                disk_check = Instant::now();
                if free_bytes(parent)? < 128 * 1024 * 1024 {
                    warning = "Recording stopped because storage is running low.".into();
                    finish = true;
                }
                if std::fs::metadata(&temp).map_err(|e| e.to_string())?.len()
                    > video::RECORDING_LIMIT_BYTES
                {
                    warning = "The 4 GB recording limit was reached.".into();
                    finish = true;
                }
            }
            if finish {
                break;
            }
            transport.update(
                Caption::Video {
                    seconds: elapsed / 10_000_000,
                    countdown: 0,
                    steps: options.instruction.enabled.then_some(input.events.len()),
                    webcam: webcam.is_some(),
                },
                false,
                poster.is_some(),
                session.phase() == Phase::Paused,
            )?;
            std::thread::sleep(Duration::from_millis(3));
        }
        if session.phase() == Phase::Recording {
            audio.pause(&mut session, &mut encoder)?;
        }
        let elapsed = session.elapsed(now()?);
        let webcam_take = match webcam.take().map(|w| w.finish((elapsed / 10_000).max(1))) {
            Some(Ok(Some(take))) => Some(take),
            Some(Ok(None)) => {
                warning = "The webcam sent no frames, so the recording has no webcam clip.".into();
                None
            }
            Some(Err(e)) => {
                warning = format!(
                    "The webcam stopped ({e}); the screen recording is complete without it."
                );
                None
            }
            None => None,
        };
        if warning.is_empty()
            && let Some(advice) = session.frame_advice(options.quality)
        {
            warning = advice;
        }
        session.finish().map_err(|e| e.to_string())?;
        let result = encoder.finish();
        session
            .finalized(result.is_ok())
            .map_err(|e| e.to_string())?;
        result.map_err(|e| {
            format!(
                "MP4 could not be finalized: {e}. The selected destination was not overwritten."
            )
        })?;
        drop(encoder);
        if let Err(error) = temp.persist(&request.destination) {
            let reason = error.error.to_string();
            let fallback = error
                .path
                .keep()
                .map_err(|e| format!("Cannot retain finalized recording: {e}"))?;
            return Err(format!(
                "Recording finished but could not replace the selected file ({reason}). Your completed MP4 is at {}.",
                fallback.display()
            ));
        }
        let mut document = Document::from_image(poster.ok_or("No video frame was captured.")?)
            .map_err(|e| e.to_string())?;
        let mut project = video::editor::Project::new(
            request.destination.to_string_lossy().into_owned(),
            size,
            (elapsed / 10_000).max(1),
            options.fps,
        );
        project.recorded_quality = Some(options.quality);
        for (index, writer) in audio.retained.iter_mut().enumerate() {
            if let Some(writer) = writer.take() {
                let path = writer.finish().map_err(|e| {
                    format!("MP4 saved, but editable audio could not be retained: {e}")
                })?;
                project.segments[0].audio.push(video::editor::Audio {
                    path: path.to_string_lossy().into_owned(),
                    label: if index == 0 {
                        "Microphone"
                    } else {
                        "System audio"
                    }
                    .into(),
                    volume: 1.,
                    muted: false,
                    fade_in: 0,
                    fade_out: 0,
                });
            }
        }
        document.video = Some(project);
        for path in document.video.as_ref().unwrap().media_paths() {
            if let Err(e) =
                capture_core::history::media_cleanup::mark_created(std::path::Path::new(path))
            {
                warning = format!(
                    "Recording saved; media cleanup ownership could not be registered: {e}"
                );
            }
        }
        document.canvas_behavior.dynamic_expansion = false;
        input.poll();
        input.fill_cursor_gaps();
        video::instruction::apply(
            &mut document,
            Some(input.path.clone()),
            &input.events,
            &options.instruction,
        )
        .map_err(|e| e.to_string())?;
        if let Some(take) = webcam_take {
            let path = keep_webcam_clip(take.path, &request.destination);
            let before = document.video.clone();
            let project = document.video.as_mut().unwrap();
            options.webcam.place(
                project,
                path.to_string_lossy().into_owned(),
                take.size,
                take.duration,
            );
            // The screen recording always opens; a webcam clip the project
            // rejects is left out with the reason and its file kept (BH5).
            if let Err(e) = document.validate() {
                document.video = before;
                warning = format!(
                    "The webcam clip was left out ({e}). It is saved at {}; add it from Insert.",
                    path.display()
                );
            }
            if let Err(e) = capture_core::history::media_cleanup::mark_created(&path) {
                warning = format!(
                    "Recording saved; media cleanup ownership could not be registered: {e}"
                );
            }
        }
        Ok(Some(super::super::Captured {
            document,
            warning,
            retained: None,
            video: Some(super::super::VideoOutput {
                path: request.destination,
                preview: options.preview,
                open_folder: options.open_folder,
                seconds: elapsed / 10_000_000,
            }),
        }))
    }
}
/// Webcam picture-in-picture during screen recording (AS13). The camera and its
/// encoder run on their own thread so the recording loop, which also feeds the
/// audio devices, never waits for them (BG8). Camera frames are placed on the
/// screen recording's own timeline from a clock snapshot the loop publishes:
/// frames from before the first screen frame are dropped and paused time is
/// skipped, so the webcam clip starts with the screen video however long the
/// area selection and countdown took.
struct Webcam {
    clock: std::sync::Arc<std::sync::Mutex<WebcamClock>>,
    /// 0 while recording; otherwise the final duration in ms plus one, or
    /// `u64::MAX` to discard the clip.
    stop: std::sync::Arc<std::sync::atomic::AtomicU64>,
    worker: Option<std::thread::JoinHandle<WebcamResult>>,
}
type WebcamResult = Result<Option<crate::video_editor::voiceover_capture::CameraTake>, String>;
#[derive(Clone, Copy, Default)]
struct WebcamClock {
    recording: bool,
    /// Recording clock (100 ns) of the first screen frame.
    start: u64,
    /// Recorded time (100 ns) at `clock`.
    elapsed: u64,
    clock: u64,
}
impl WebcamClock {
    /// Where a frame captured at recording-clock time `at` falls in the recording.
    fn position(&self, at: i128) -> Option<u64> {
        (self.recording && at >= self.start as i128)
            .then(|| (self.elapsed as i128 + at - self.clock as i128).max(0) as u64)
    }
}
impl Webcam {
    fn open(device: &str, folder: std::path::PathBuf) -> Result<Self, String> {
        use crate::video_editor::camera;
        use std::sync::atomic::Ordering;
        let clock = std::sync::Arc::new(std::sync::Mutex::new(WebcamClock::default()));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (ready_tx, ready) = std::sync::mpsc::channel();
        let (shared, stopping, device) = (clock.clone(), stop.clone(), device.to_owned());
        let worker = std::thread::spawn(move || -> WebcamResult {
            let _runtime = Runtime::new().map_err(|e| e.to_string())?;
            let opened = (|| -> Result<_, String> {
                let devices = camera::devices()?;
                let id = devices
                    .iter()
                    .find(|(id, _)| *id == device)
                    .or_else(|| devices.first())
                    .map(|(id, _)| id.clone())
                    .ok_or("no camera is connected")?;
                let source = camera::Source::open(&id, 720)?;
                let offset = camera::clock() as i128 - now()? as i128;
                Ok((source, offset))
            })();
            let (mut source, offset) = match opened {
                Ok(v) => {
                    let _ = ready_tx.send(Ok(()));
                    v
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e.clone()));
                    return Err(e);
                }
            };
            let mut recording: Option<camera::Recording> = None;
            loop {
                let stopped = stopping.load(Ordering::Acquire);
                if stopped == u64::MAX {
                    return Ok(None);
                }
                while let Some((at, pixels)) = source.poll()? {
                    let snapshot = *shared.lock().map_err(|_| "webcam clock poisoned")?;
                    let Some(position) = snapshot.position(at as i128 - offset) else {
                        continue;
                    };
                    match &mut recording {
                        Some(r) => r.push(position, pixels)?,
                        None => {
                            let mut r =
                                camera::Recording::new(&folder, source.size, 0, pixels.clone())?;
                            r.push(position, pixels)?;
                            recording = Some(r);
                        }
                    }
                }
                if stopped != 0 {
                    return recording.map(|r| r.finish(stopped - 1)).transpose();
                }
                std::thread::sleep(Duration::from_millis(4));
            }
        });
        match ready.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(Self {
                clock,
                stop,
                worker: Some(worker),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                stop.store(u64::MAX, Ordering::Release);
                Err("the camera did not respond".into())
            }
        }
    }
    /// Publishes the recording clock for the webcam thread.
    fn update(&self, session: &Session, start: u64, clock: u64) {
        if let Ok(mut c) = self.clock.lock() {
            *c = WebcamClock {
                recording: session.phase() == Phase::Recording,
                start,
                elapsed: session.elapsed(clock),
                clock,
            };
        }
    }
    /// Stops the webcam thread and returns its clip, `duration` ms long.
    fn finish(mut self, duration: u64) -> WebcamResult {
        self.stop
            .store(duration.max(1) + 1, std::sync::atomic::Ordering::Release);
        self.worker.take().map_or(Ok(None), |w| {
            w.join()
                .map_err(|_| "the webcam thread stopped".to_string())?
        })
    }
}
impl Drop for Webcam {
    fn drop(&mut self) {
        // A cancelled recording discards the webcam clip.
        if self.worker.is_some() {
            self.stop
                .store(u64::MAX, std::sync::atomic::Ordering::Release);
        }
    }
}
#[test]
fn webcam_frames_land_on_the_recording_timeline_and_skip_pauses() {
    let c = WebcamClock {
        recording: true,
        start: 1_000,
        elapsed: 50_000,
        clock: 60_000,
    };
    assert_eq!(c.position(500), None, "before the first screen frame");
    assert_eq!(c.position(70_000), Some(60_000));
    assert_eq!(c.position(55_000), Some(45_000));
    let paused = WebcamClock {
        recording: false,
        ..c
    };
    assert_eq!(paused.position(70_000), None, "paused time is skipped");
}
/// Moves the finished webcam clip next to the recording as `<name>-webcam.mp4`.
fn keep_webcam_clip(path: std::path::PathBuf, destination: &std::path::Path) -> std::path::PathBuf {
    let stem = destination
        .file_stem()
        .map_or_else(|| "recording".into(), |s| s.to_string_lossy().into_owned());
    let folder = destination.parent().unwrap_or(std::path::Path::new("."));
    for n in 0..100 {
        let name = if n == 0 {
            format!("{stem}-webcam.mp4")
        } else {
            format!("{stem}-webcam-{n}.mp4")
        };
        let target = folder.join(name);
        if !target.exists() && std::fs::rename(&path, &target).is_ok() {
            return target;
        }
    }
    path
}
/// Audio is flushed through the pause boundary before device queues are reset.
/// Both devices always advance against one output sample cursor, including silence.
fn free_bytes(path: &std::path::Path) -> Result<u64, String> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free = 0;
    unsafe {
        if windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            null_mut(),
            null_mut(),
        ) == 0
        {
            return Err(win_error("Cannot check free space for recording"));
        }
    }
    Ok(free)
}
struct Audio {
    mic: Option<video_audio::Input>,
    system: Option<video_audio::Input>,
    mic_track: Track,
    system_track: Track,
    frame: u64,
    broken: bool,
    retained: [Option<video::wav::Writer>; 2],
}
impl Audio {
    fn new(
        mic: Option<video_audio::Input>,
        system: Option<video_audio::Input>,
        folder: &std::path::Path,
    ) -> Result<Self, String> {
        let retained = [mic.is_some(), system.is_some()].map(|on| {
            if on {
                video::wav::Writer::new(folder).map(Some)
            } else {
                Ok(None)
            }
        });
        let [mic_file, system_file] = retained;
        Ok(Self {
            retained: [
                mic_file.map_err(|e| e.to_string())?,
                system_file.map_err(|e| e.to_string())?,
            ],
            mic,
            system,
            mic_track: Track::default(),
            system_track: Track::default(),
            frame: 0,
            broken: false,
        })
    }
    fn start(&self) -> Result<(), String> {
        for input in [&self.mic, &self.system].into_iter().flatten() {
            input
                .start()
                .map_err(|e| format!("Cannot start selected audio device: {e}"))?;
        }
        Ok(())
    }
    fn collect(&mut self, session: &Session) -> Result<(), String> {
        for (input, track) in [
            (&mut self.mic, &mut self.mic_track),
            (&mut self.system, &mut self.system_track),
        ] {
            if let Some(input) = input {
                for packet in input.packets().map_err(|e| e.to_string())? {
                    let at = session.elapsed(packet.qpc) * 48_000 / 10_000_000;
                    track
                        .push_positioned(packet.position, at, &packet.samples)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
    fn flush(&mut self, end: u64, encoder: &mut Encoder) -> Result<(), String> {
        if self.mic.is_none() && self.system.is_none() {
            return Ok(());
        }
        while self.frame < end {
            let count = (end - self.frame).min(480) as usize;
            let mic = self.mic_track.drain(count);
            let system = self.system_track.drain(count);
            for (writer, values) in self.retained.iter_mut().zip([&mic, &system]) {
                if let Some(writer) = writer {
                    writer
                        .append(values)
                        .map_err(|e| format!("Cannot retain editable audio: {e}"))?;
                }
            }
            let mixed = video::audio::mix(&mic, &system).map_err(|e| e.to_string())?;
            encoder
                .audio(self.frame, &mixed)
                .map_err(|e| format!("Cannot encode audio: {e}"))?;
            self.frame += count as u64;
        }
        Ok(())
    }
    fn pause(&mut self, session: &mut Session, encoder: &mut Encoder) -> Result<(), String> {
        if !self.broken {
            self.collect(session)?;
        }
        session.pause(now()?).map_err(|e| e.to_string())?;
        for input in [&mut self.mic, &mut self.system].into_iter().flatten() {
            let result = input.pause();
            if !self.broken {
                result.map_err(|e| e.to_string())?;
            }
        }
        self.flush(session.elapsed(now()?) * 48_000 / 10_000_000, encoder)?;
        self.mic_track.reset(self.frame);
        self.system_track.reset(self.frame);
        Ok(())
    }
}
