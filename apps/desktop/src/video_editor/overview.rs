//! Small, bounded media summaries are decoded once, away from the UI thread.
use super::*;
type MediaSummary = (String, Vec<(u64, image::RgbaImage)>, Vec<f32>);
pub(crate) struct Overview {
    pub frames: Vec<(u64, image::RgbaImage)>,
    pub waves: Vec<Vec<f32>>,
    pub media: Vec<MediaSummary>,
}
pub(crate) fn start(project: Project, ctx: egui::Context) -> mpsc::Receiver<Overview> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        #[cfg(windows)]
        {
            let run = || -> Result<Overview, String> {
                let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
                let mut frames = vec![];
                if let Ok(mut reader) = media::Video::open(Path::new(&project.source)) {
                    for i in 0..16 {
                        let time = i * project.source_duration / 16;
                        if let Ok(p) = reader.frame(time) {
                            frames.push((time, image::imageops::thumbnail(&p, 160, 90)));
                        }
                    }
                }
                let mut waves = vec![];
                for track in project
                    .segments
                    .iter()
                    .filter(|c| c.source.is_none())
                    .take(1)
                    .flat_map(|s| &s.audio)
                {
                    let Ok(mut reader) = media::Sound::open(Path::new(&track.path)) else {
                        continue;
                    };
                    let mut wave = vec![];
                    for i in 0..256u64 {
                        let start = i * project.source_duration * 48 / 256;
                        let mut peak = 0f32;
                        for j in 0..480 {
                            let values = reader.value(start + j).unwrap_or([0, 0]);
                            peak = peak
                                .max(values[0].unsigned_abs() as f32 / 32768.)
                                .max(values[1].unsigned_abs() as f32 / 32768.);
                        }
                        wave.push(peak);
                    }
                    waves.push(wave);
                }
                let mut media = vec![];
                let mut seen = std::collections::BTreeSet::new();
                for clip in project.segments.iter().take(128) {
                    if let Some(source) = &clip.source
                        && seen.insert(source.path.clone())
                    {
                        let mut thumbnails = vec![];
                        let mut waveform = vec![];
                        match source.kind {
                            capture_core::video::editor::MediaKind::Image => {
                                let d = capture_core::Document::open_image(Path::new(&source.path))
                                    .map_err(|e| e.to_string())?;
                                thumbnails.push((
                                    0,
                                    image::imageops::thumbnail(
                                        &d.render().map_err(|e| e.to_string())?,
                                        160,
                                        90,
                                    ),
                                ));
                            }
                            capture_core::video::editor::MediaKind::Video => {
                                let Ok(mut reader) = media::Video::open(Path::new(&source.path))
                                else {
                                    continue;
                                };
                                for i in 0..8 {
                                    let t = i * source.duration / 8;
                                    thumbnails.push((
                                        t,
                                        image::imageops::thumbnail(
                                            &match reader.frame(t) {
                                                Ok(p) => p,
                                                Err(_) => continue,
                                            },
                                            160,
                                            90,
                                        ),
                                    ));
                                }
                            }
                            capture_core::video::editor::MediaKind::Audio => {}
                        }
                        if let Some(sound) = clip.audio.first() {
                            let reader = media::Sound::open(Path::new(&sound.path));
                            if let Ok(mut reader) = reader {
                                for i in 0..128 {
                                    let start = i * source.duration * 48 / 128;
                                    let mut peak = 0f32;
                                    for j in 0..240 {
                                        let values = reader.value(start + j).unwrap_or([0, 0]);
                                        peak = peak
                                            .max(values[0].unsigned_abs() as f32 / 32768.)
                                            .max(values[1].unsigned_abs() as f32 / 32768.);
                                    }
                                    waveform.push(peak);
                                }
                            }
                        }
                        media.push((source.path.clone(), thumbnails, waveform));
                    }
                }
                Ok(Overview {
                    frames,
                    waves,
                    media,
                })
            };
            if let Ok(result) = run() {
                let _ = tx.send(result);
                ctx.request_repaint();
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (project, ctx, tx);
        }
    });
    rx
}
