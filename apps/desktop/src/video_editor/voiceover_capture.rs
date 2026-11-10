//! Own the microphone on one worker, with bounded monitoring and disk streaming.
use super::*;
use std::sync::atomic::AtomicU32;
pub(super) enum Control {
    Record,
    Stop,
    Cancel,
}
pub(super) enum Event {
    Ready,
    Started,
    Take(Take),
    Error(String),
}
pub(crate) struct CameraTake {
    pub path: PathBuf,
    pub size: [u32; 2],
    pub duration: u64,
}
pub(super) struct Take {
    pub camera: Option<CameraTake>,
    pub path: PathBuf,
    pub duration: u64,
    pub keep: bool,
    pub wave: super::voiceover::waveform::Snapshot,
}
impl Drop for Take {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_file(&self.path);
            if let Some(c) = &self.camera {
                let _ = std::fs::remove_file(&c.path);
            }
        }
    }
}
pub(super) type CameraPreview = Arc<std::sync::Mutex<Option<Arc<image::RgbaImage>>>>;
pub(super) struct Capture {
    pub preview: CameraPreview,
    pub send: mpsc::Sender<Control>,
    pub events: mpsc::Receiver<Event>,
    pub level: Arc<AtomicU32>,
    pub monitor: Arc<AtomicBool>,
    pub mono: Arc<AtomicBool>,
    pub output_device: workers::Latest<String>,
    pub wave: Arc<std::sync::Mutex<super::voiceover::waveform::Overview>>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.send.send(Control::Cancel);
    }
}
impl Capture {
    pub fn start(
        device: String,
        camera: Option<(String, u32)>,
        folder: PathBuf,
        monitor: bool,
        mono: bool,
        monitor_device: String,
        ctx: egui::Context,
    ) -> Self {
        let preview: CameraPreview = Default::default();
        let preview_output = preview.clone();
        let (send, commands) = mpsc::channel();
        let (tx, events) = mpsc::channel();
        let level = Arc::new(AtomicU32::new(0));
        let meter = level.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let monitor = Arc::new(AtomicBool::new(monitor));
        let monitoring = monitor.clone();
        let mono = Arc::new(AtomicBool::new(mono));
        let input_mono = mono.clone();
        let output_device = workers::Latest::default();
        let _ = output_device.send(monitor_device);
        let output_route = output_device.clone();
        let wave = Arc::new(std::sync::Mutex::new(
            super::voiceover::waveform::Overview::default(),
        ));
        let waveform = wave.clone();
        std::thread::spawn(move || {
            #[cfg(windows)]
            let run = || -> Result<(), String> {
                let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let mut input=crate::capture::windows::video_audio::Input::open_device(false,Some(&device)).map_err(|e|format!("Cannot open microphone. Choose another input or check Windows microphone access: {e}"))?;
                let mut camera = camera
                    .map(|(id, height)| super::camera::Source::open(&id, height))
                    .transpose()?;
                let mut camera_record: Option<super::camera::Recording> = None;
                let mut camera_pixels: Option<Vec<u8>> = None;
                let mut announced = false;
                let mut output: Option<media::Output> = None;
                let mut output_started = false;
                let mut output_id = String::new();
                input.start().map_err(|e| e.to_string())?;
                if camera.is_none() {
                    let _ = tx.send(Event::Ready);
                    announced = true;
                    ctx.request_repaint();
                }
                let mut writer = None;
                let mut frames = 0u64;
                let mut anchor = 0;
                let mut meter_tick = Instant::now();
                let mut meter_peak = 0f32;
                let mut displayed_peak = 0f32;
                let mut alignment = capture_core::video::audio::Track::default();
                while !stop.load(Ordering::Relaxed) {
                    while let Ok(command) = commands.try_recv() {
                        match command {
                            Control::Record => {
                                input.pause().map_err(|e| e.to_string())?;
                                writer = Some(
                                    capture_core::video::wav::Writer::new(&folder)
                                        .map_err(|e| e.to_string())?,
                                );
                                frames = 0;
                                *waveform.lock().map_err(|_| "Waveform unavailable")? =
                                    Default::default();
                                anchor = super::camera::clock();
                                if let Some(c) = &camera {
                                    camera_record = Some(super::camera::Recording::new(
                                        &folder,
                                        c.size,
                                        anchor,
                                        camera_pixels.clone().ok_or(
                                            "Wait for the camera preview before recording.",
                                        )?,
                                    )?);
                                }
                                alignment.reset(0);
                                input.start().map_err(|e| e.to_string())?;
                                let _ = tx.send(Event::Started);
                                ctx.request_repaint();
                            }
                            Control::Stop => {
                                if let Some(w) = writer.take() {
                                    if frames < 4800 {
                                        return Err("Record at least a tenth of a second before keeping a take.".into());
                                    }
                                    let path = w.finish().map_err(|e| e.to_string())?;
                                    let mut take = Take {
                                        path,
                                        duration: frames / 48,
                                        keep: false,
                                        camera: None,
                                        wave: waveform
                                            .lock()
                                            .map_err(|_| "Waveform unavailable")?
                                            .snapshot(),
                                    };
                                    if let Some(c) = camera_record.take() {
                                        take.camera = Some(c.finish(take.duration)?);
                                    }
                                    let _ = tx.send(Event::Take(take));
                                    ctx.request_repaint();
                                    return Ok(());
                                }
                            }
                            Control::Cancel => return Ok(()),
                        }
                    }
                    if let Some(c) = &mut camera
                        && let Some((stamp, bytes)) = c.poll()?
                    {
                        let pixels = super::camera::preview_pixels(&bytes, c.size);
                        *preview_output
                            .lock()
                            .map_err(|_| "Camera preview unavailable")? = Some(Arc::new(pixels));
                        if let Some(record) = &mut camera_record {
                            record.push(stamp, bytes)?;
                        } else {
                            camera_pixels = Some(bytes);
                        }
                        if !announced {
                            let _ = tx.send(Event::Ready);
                            announced = true;
                        }
                    }
                    if let Some(id) = output_route.take() {
                        if output_id != id {
                            output = None;
                            output_started = false;
                        }
                        output_id = id;
                    }
                    if monitoring.load(Ordering::Relaxed) && output.is_none() {
                        output = Some(
                            media::Output::open(&output_id)
                                .map_err(|e| format!("Headphone monitoring unavailable: {e}"))?,
                        );
                        output_started = false;
                    }
                    if !monitoring.load(Ordering::Relaxed) {
                        output = None;
                        output_started = false;
                    }
                    let mut peak = 0f32;
                    for mut packet in input.packets().map_err(|e| {
                        format!(
                            "Microphone interrupted. Reconnect it and retake this recording: {e}"
                        )
                    })? {
                        if input_mono.load(Ordering::Relaxed) {
                            mix_mono(&mut packet.samples);
                        }
                        peak = peak.max(
                            packet
                                .samples
                                .iter()
                                .map(|s| (*s as f32 / 32768.).abs())
                                .fold(0., f32::max),
                        );
                        if let Some(o) = &output {
                            let n = (o.room()? as usize * 2).min(packet.samples.len());
                            if n > 0 {
                                o.write(&packet.samples[..n])?;
                                if !output_started {
                                    o.start()?;
                                    output_started = true;
                                }
                            }
                        }
                        if let Some(w) = &mut writer {
                            let skip =
                                (anchor.saturating_sub(packet.qpc) * 48_000 / 10_000_000) as usize;
                            let samples = &packet.samples[(skip * 2).min(packet.samples.len())..];
                            if samples.is_empty() {
                                continue;
                            }
                            let at = (packet.qpc.saturating_sub(anchor) * 48_000 / 10_000_000)
                                .max(frames);
                            if at > 48_000 * 3600 {
                                return Err("Voiceover reached the one-hour limit. Stop and keep shorter takes.".into());
                            }
                            let at = alignment
                                .push_positioned(packet.position + skip as u64, at, samples)
                                .map_err(|e| e.to_string())?;
                            let count =
                                (at + samples.len() as u64 / 2).saturating_sub(frames) as usize;
                            let samples = alignment.drain(count);
                            w.append(&samples).map_err(|e| e.to_string())?;
                            waveform
                                .lock()
                                .map_err(|_| "Waveform unavailable")?
                                .push(&samples);
                            frames += count as u64;
                        }
                    }
                    meter_peak = meter_peak.max(peak);
                    if meter_tick.elapsed() >= Duration::from_millis(33) {
                        let seconds = meter_tick.elapsed().as_secs_f32();
                        displayed_peak = meter_peak.max(displayed_peak * (-seconds / 0.35).exp());
                        meter.store(displayed_peak.to_bits(), Ordering::Relaxed);
                        meter_peak = 0.;
                        meter_tick = Instant::now();
                        ctx.request_repaint();
                    }
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(())
            };
            #[cfg(windows)]
            if let Err(e) = run() {
                let _ = tx.send(Event::Error(e));
                ctx.request_repaint();
            }
            #[cfg(not(windows))]
            {
                let _ = (
                    commands,
                    meter,
                    stop,
                    monitoring,
                    input_mono,
                    waveform,
                    device,
                    camera,
                    preview_output,
                    folder,
                );
                let _ = tx.send(Event::Error("Voiceover recording requires Windows.".into()));
                ctx.request_repaint();
            }
        });
        Self {
            preview,
            send,
            events,
            level,
            monitor,
            mono,
            output_device,
            wave,
            cancel,
        }
    }
}
fn mix_mono(samples: &mut [i16]) {
    for frame in samples.as_chunks_mut::<2>().0 {
        let mono = ((i32::from(frame[0]) + i32::from(frame[1])) / 2) as i16;
        frame.fill(mono);
    }
}
#[test]
fn mono_recording_routes_both_sides_without_overflow() {
    let mut samples = [30000, 0, 30000, 30000, -30000, -30000];
    mix_mono(&mut samples);
    assert_eq!(samples, [15000, 15000, 30000, 30000, -30000, -30000]);
}
pub(super) fn devices() -> mpsc::Receiver<Result<Vec<(String, String)>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let result = media::Runtime::new()
            .map_err(|e| e.to_string())
            .and_then(|_r| {
                crate::capture::windows::video_audio::devices().map_err(|e| e.to_string())
            });
        #[cfg(not(windows))]
        let result = Err("Voiceover input requires Windows.".into());
        let _ = tx.send(result);
    });
    rx
}

pub(crate) fn cameras() -> mpsc::Receiver<Result<Vec<(String, String)>, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let result = media::Runtime::new()
            .map_err(|e| e.to_string())
            .and_then(|_r| super::camera::devices());
        #[cfg(not(windows))]
        let result = Err("Camera recording requires Windows.".into());
        let _ = tx.send(result);
    });
    rx
}
