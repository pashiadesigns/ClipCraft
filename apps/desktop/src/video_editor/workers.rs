use super::*;
/// One pending value, replaced by newer UI edits instead of building a backlog.
#[derive(Clone)]
pub(crate) struct Latest<T>(Arc<std::sync::Mutex<Option<T>>>);
impl<T> Default for Latest<T> {
    fn default() -> Self {
        Self(Arc::new(std::sync::Mutex::new(None)))
    }
}
impl<T> Latest<T> {
    pub fn try_send(&self, value: T) -> Result<(), String> {
        self.send(value)
    }
    pub fn try_recv(&self) -> Result<T, ()> {
        self.take().ok_or(())
    }
    pub fn send(&self, value: T) -> Result<(), String> {
        *self
            .0
            .lock()
            .map_err(|_| "Playback update was interrupted.")? = Some(value);
        Ok(())
    }
    pub fn take(&self) -> Option<T> {
        self.0.lock().ok()?.take()
    }
}
pub(crate) type MediaLayers = Vec<(u64, Arc<capture_core::bitmap::Bitmap>)>;
type FrameRequest = (Project, [u32; 2], [u32; 2], u64, u64);
pub(super) fn preview_project(project: &Project, size: [u32; 2], preview: [u32; 2]) -> Project {
    let mut p = project.clone();
    let scale = (preview[0] as f32 / size[0] as f32).min(preview[1] as f32 / size[1] as f32);
    for clip in &mut p.segments {
        if let Some(frame) = &mut clip.frame {
            frame.radius *= scale;
            frame.width = (frame.width as f32 * scale).round() as u32;
            if let Some(shadow) = &mut frame.shadow {
                shadow.distance *= scale;
                shadow.blur *= scale;
            }
        }
    }
    p
}
pub(crate) struct Frames {
    pub request: Latest<FrameRequest>,
    pub result: Latest<Result<(u64, u64, image::RgbaImage, MediaLayers), String>>,
    stop: Arc<AtomicBool>,
}
impl Drop for Frames {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Frames {
    pub fn start(_path: String, ctx: egui::Context) -> Self {
        let tx = Latest::<FrameRequest>::default();
        let rx = tx.clone();
        let result = Latest::default();
        let out = result.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        std::thread::spawn(move || {
            #[cfg(windows)]
            {
                let run = || -> Result<(), String> {
                    let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
                    let mut reader = media::Visuals::new();
                    reader.preview = true;
                    while !cancel.load(Ordering::Relaxed) {
                        let Some(time) = rx.take() else {
                            std::thread::sleep(Duration::from_millis(3));
                            continue;
                        };
                        let (project, size, preview_size, time, generation) = time;
                        // Additive overlap compositing must match export. Ordinary
                        // sources stay raw so frame geometry is applied live on the GPU.
                        let active: Vec<_> = project
                            .segments
                            .iter()
                            .filter(|c| {
                                c.active(time)
                                    && !c.hidden
                                    && c.kind() != capture_core::video::editor::MediaKind::Audio
                            })
                            .collect();
                        let sources = reader.sources(&project, time);
                        let overlap = active.iter().enumerate().any(|(i, a)| {
                            active[i + 1..].iter().any(|b| a.track == b.track)
                                && sources.as_ref().is_ok_and(|sources| {
                                    !capture_core::video::frame::texture_dissolve_sources(
                                        &project, size, time, a.track, sources,
                                    )
                                })
                        });
                        let pixels = if overlap {
                            let preview = preview_project(&project, size, preview_size);
                            reader.layers(&preview, preview_size, time)
                        } else {
                            sources
                        }
                        .map(|layers| {
                            (
                                generation,
                                time,
                                image::RgbaImage::from_pixel(
                                    if overlap { preview_size[0] } else { 1 },
                                    if overlap { preview_size[1] } else { 1 },
                                    image::Rgba(project.background),
                                ),
                                layers,
                            )
                        });
                        if out.send(pixels).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    Ok(())
                };
                if let Err(e) = run() {
                    let _ = out.send(Err(e));
                    ctx.request_repaint();
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (_path, ctx, rx);
                let _ = out.send(Err("Video playback requires Windows.".into()));
            }
        });
        Self {
            request: tx,
            result,
            stop,
        }
    }
}
pub(crate) fn load(path: &Path) -> Result<Document, String> {
    if capture_core::project_file::is_video_project(path) {
        let d = capture_core::project_file::open(path).map_err(|e| e.to_string())?;
        let v = d.video.as_ref().ok_or("This is not a video project.")?;
        for media in v.media_paths() {
            if !Path::new(media).is_file() {
                return Err(format!(
                    "Missing media: {media}. Restore the matching .vclip-media folder beside this project and reopen it."
                ));
            }
        }
        return Ok(d);
    }
    #[cfg(windows)]
    {
        let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
        let mut reader = media::Video::open(path)?;
        let poster = reader.frame(0)?;
        let size =
            capture_core::video::Options::output_size(reader.size).map_err(|e| e.to_string())?;
        let mut project = Project::new(
            path.to_string_lossy().into_owned(),
            reader.size,
            reader.duration,
            reader.fps,
        );
        if reader.has_audio {
            project.segments[0]
                .audio
                .push(capture_core::video::editor::Audio {
                    path: project.source.clone(),
                    label: "Recording audio".into(),
                    volume: 1.,
                    muted: false,
                    fade_in: 0,
                    fade_out: 0,
                });
        }
        let poster = image::imageops::resize(
            &poster,
            size[0],
            size[1],
            image::imageops::FilterType::Triangle,
        );
        let mut d = Document::from_image(poster).map_err(|e| e.to_string())?;
        d.canvas_behavior.dynamic_expansion = false;
        d.video = Some(project);
        d.validate().map_err(|e| e.to_string())?;
        Ok(d)
    }
    #[cfg(not(windows))]
    Err("Video playback requires Windows.".into())
}
/// The Library's picture of a video project: the frame at the playhead, as it
/// exports, with every edit on it. `None` when the media can't be read now.
pub(crate) fn library_picture(doc: &Document) -> Option<image::RgbaImage> {
    let project = doc.video.as_ref()?;
    #[cfg(windows)]
    {
        let _runtime = media::Runtime::new().ok()?;
        let mut reader = media::Visuals::new();
        let mut cache = capture_core::video::frame::CompositeCache::default();
        let ms = project.cursor.min(project.duration().saturating_sub(1));
        let layers = reader.layers(project, [doc.width, doc.height], ms).ok()?;
        cache.composite(doc, &layers, ms).ok()
    }
    #[cfg(not(windows))]
    {
        let _ = project;
        None
    }
}
pub(crate) fn open(path: PathBuf) -> mpsc::Receiver<Result<(Document, PathBuf), String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = load(&path).map(|d| (d, path));
        let _ = tx.send(result);
    });
    rx
}
pub(crate) type Meters = std::collections::BTreeMap<
    capture_core::video::effects::Target,
    Vec<capture_core::video::effects::Meter>,
>;
pub(crate) struct Playback {
    pub stop: Arc<AtomicBool>,
    pub errors: mpsc::Receiver<String>,
    pub updates: Latest<Project>,
    pub spectrum: Latest<Option<(capture_core::video::effects::Target, u64)>>,
    pub meters: mpsc::Receiver<Meters>,
}
pub(crate) fn audio(mut project: Project, at: u64, device: String) -> Playback {
    let updates = Latest::<Project>::default();
    let changes = updates.clone();
    let spectrum = Latest::default();
    let spectrum_rx = spectrum.clone();
    let (meter_tx, meters) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = stop.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            let mut run = || -> Result<(), String> {
                let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
                let mut mix = media::Mixer::new(&project)?;
                let output = media::Output::open(&device)?;
                let mut cursor = at * 48;
                let end = project.duration() * 48;
                let mut started = false;
                while cursor < end && !cancel.load(Ordering::Relaxed) {
                    if let Some(next) = changes.take() {
                        project = next;
                    }
                    if let Some(next) = spectrum_rx.take() {
                        mix.spectrum = next;
                    }
                    let n = output.room()?.min(2400).min((end - cursor) as u32) as usize;
                    if n > 0 {
                        let values = mix.samples(&project, cursor, n)?;
                        let _ = meter_tx.try_send(mix.meters.clone());
                        output.write(&values)?;
                        cursor += n as u64;
                        if !started {
                            output.start()?;
                            started = true
                        }
                    } else {
                        std::thread::sleep(Duration::from_millis(4));
                    }
                }
                while !output.is_empty()? && !cancel.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            };
            if let Err(e) = run() {
                let _ = tx.send(e);
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (project, at, cancel, changes, spectrum_rx, meter_tx, device);
            let _ = tx.send("Audio playback requires Windows.".into());
        }
    });
    Playback {
        stop,
        errors: rx,
        updates,
        spectrum,
        meters,
    }
}
pub(crate) struct Export {
    pub cancel: Arc<AtomicBool>,
    pub progress: mpsc::Receiver<f32>,
    pub result: mpsc::Receiver<Result<PathBuf, String>>,
}
/// Saves the recording as it is, with no render.
pub(crate) fn save_copy(source: PathBuf, path: PathBuf) -> Export {
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let (progress_tx, progress) = mpsc::channel();
    let (tx, result) = mpsc::channel();
    std::thread::spawn(move || {
        let output = copy_recording(&source, &path, &stop, |p| {
            let _ = progress_tx.send(p);
        })
        .map(|_| path);
        let _ = tx.send(output);
    });
    Export {
        cancel,
        progress,
        result,
    }
}
pub(crate) fn copy_recording(
    source: &Path,
    path: &Path,
    stop: &AtomicBool,
    progress: impl Fn(f32),
) -> Result<(), String> {
    use std::io::{Read as _, Write as _};
    let same = |a: &Path, b: &Path| {
        let full = |p: &Path| p.canonicalize().or_else(|_| std::path::absolute(p));
        matches!((full(a), full(b)), (Ok(a), Ok(b))
            if a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()))
    };
    if same(source, path) {
        return Err(
            "Choose a different output filename to preserve the original recording.".into(),
        );
    }
    let mut from =
        std::fs::File::open(source).map_err(|e| format!("Cannot read the recording: {e}"))?;
    let total = from.metadata().map_err(|e| e.to_string())?.len().max(1);
    let mut temp = tempfile::Builder::new()
        .prefix(".clipdesigner-export-")
        .suffix(".tmp")
        .tempfile_in(path.parent().unwrap_or(Path::new(".")))
        .map_err(|e| e.to_string())?;
    let mut buffer = vec![0; 1 << 20];
    let mut done = 0u64;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("Video export cancelled. The destination was not replaced.".into());
        }
        let count = from
            .read(&mut buffer)
            .map_err(|e| format!("Cannot read the recording: {e}"))?;
        if count == 0 {
            break;
        }
        temp.write_all(&buffer[..count])
            .map_err(|e| e.to_string())?;
        done += count as u64;
        progress(done as f32 / total as f32);
    }
    temp.persist(path).map_err(|e| e.to_string())?;
    progress(1.);
    Ok(())
}
pub(crate) fn export(
    doc: Document,
    path: PathBuf,
    watermark: Option<capture_core::watermark::Watermark>,
    bitrate: u32,
) -> Export {
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    let (progress_tx, progress) = mpsc::channel();
    let (tx, result) = mpsc::channel();
    std::thread::spawn(move || {
        let output = export_marked(&doc, &path, &stop, watermark.as_ref(), bitrate, |p| {
            let _ = progress_tx.send(p);
        })
        .map(|_| path);
        let _ = tx.send(output);
    });
    Export {
        cancel,
        progress,
        result,
    }
}
#[cfg(test)]
pub(crate) fn export_sync(
    doc: &Document,
    path: &Path,
    stop: &AtomicBool,
    progress: impl Fn(f32),
) -> Result<(), String> {
    export_marked(doc, path, stop, None, 8_000_000, progress)
}
/// Video export with the export watermark laid over every frame (FX-003).
pub(crate) fn export_marked(
    doc: &Document,
    path: &Path,
    stop: &AtomicBool,
    watermark: Option<&capture_core::watermark::Watermark>,
    bitrate: u32,
    progress: impl Fn(f32),
) -> Result<(), String> {
    let layer = watermark
        .map(|w| w.layer([doc.width, doc.height]))
        .transpose()
        .map_err(|e| e.to_string())?;
    doc.validate().map_err(|e| e.to_string())?;
    capture_core::privacy::tracking::require_review(doc).map_err(|e| e.to_string())?;
    let project = doc.video.as_ref().ok_or("Open a video before exporting.")?;
    let absolute = path
        .canonicalize()
        .or_else(|_| std::path::absolute(path))
        .map_err(|e| e.to_string())?;
    for source in project.media_paths() {
        let original = Path::new(source)
            .canonicalize()
            .or_else(|_| std::path::absolute(source))
            .map_err(|e| e.to_string())?;
        if absolute
            .to_string_lossy()
            .eq_ignore_ascii_case(&original.to_string_lossy())
        {
            return Err(
                "Choose a different output filename to preserve the original recording.".into(),
            );
        }
    }
    #[cfg(windows)]
    {
        let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
        let mut reader = media::Visuals::new();
        let mut mix = media::Mixer::new(project)?;
        let folder = path.parent().unwrap_or(Path::new("."));
        let temp = tempfile::Builder::new()
            .prefix(".clipdesigner-export-")
            .suffix(".mp4")
            .tempfile_in(folder)
            .map_err(|e| e.to_string())?
            .into_temp_path();
        let mut encoder = media::Encoder::with_bitrate(
            &temp,
            [doc.width, doc.height],
            project.fps,
            project.has_audio(),
            bitrate,
        )
        .map_err(|e| e.to_string())?;
        let count = (project.duration() * project.fps as u64).div_ceil(1000);
        let mut audio_frame = 0;
        let mut compositor = capture_core::video::frame::CompositeCache::default();
        #[cfg(test)]
        let mut profile = [0.; 4];
        for i in 0..count {
            if stop.load(Ordering::Relaxed) {
                return Err("Video export cancelled. The destination was not replaced.".into());
            }
            let ms = i * 1000 / project.fps as u64;
            #[cfg(test)]
            let stage = Instant::now();
            let layers = reader.layers(project, [doc.width, doc.height], ms)?;
            #[cfg(test)]
            {
                profile[0] += stage.elapsed().as_secs_f64();
            }
            #[cfg(test)]
            let stage = Instant::now();
            let mut pixels = compositor
                .composite(doc, &layers, ms)
                .map_err(|e| e.to_string())?;
            if let Some(layer) = &layer {
                capture_core::watermark::blend(&mut pixels, layer);
            }
            #[cfg(test)]
            {
                profile[1] += stage.elapsed().as_secs_f64();
            }
            #[cfg(test)]
            let stage = Instant::now();
            let mut bgra = pixels.into_raw();
            for p in bgra.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
            }
            encoder
                .video(i * 10_000_000 / project.fps as u64, &bgra)
                .map_err(|e| e.to_string())?;
            #[cfg(test)]
            {
                profile[2] += stage.elapsed().as_secs_f64();
            }
            #[cfg(test)]
            let stage = Instant::now();
            let until = ((i + 1) * 48_000 / project.fps as u64).min(project.duration() * 48);
            while audio_frame < until && project.has_audio() {
                let n = (until - audio_frame).min(2400) as usize;
                let samples = mix.samples(project, audio_frame, n)?;
                encoder
                    .audio(audio_frame, &samples)
                    .map_err(|e| e.to_string())?;
                audio_frame += n as u64;
            }
            #[cfg(test)]
            {
                profile[3] += stage.elapsed().as_secs_f64();
            }
            if i % 10 == 0 {
                progress(i as f32 / count as f32)
            }
        }
        #[cfg(test)]
        if std::env::var_os("CLIPDESIGNER_PROFILE_PROJECT").is_some() {
            println!(
                "export stages seconds [decode/layers, composite, video encode, audio]: {profile:?}"
            );
        }
        encoder.finish().map_err(|e| {
            format!("MP4 finalization failed: {e}. The destination was not replaced.")
        })?;
        drop(encoder);
        if stop.load(Ordering::Relaxed) {
            return Err("Video export cancelled.".into());
        }
        temp.persist(path).map_err(|e| e.to_string())?;
        progress(1.);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (stop, progress);
        Err("Video export requires Windows.".into())
    }
}
