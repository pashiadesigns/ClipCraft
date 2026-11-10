//! Bounded audition and estimate workers. Neither decodes on the UI thread.
use super::*;
use capture_core::gif_studio::{Estimate, Range};
#[derive(Clone, PartialEq)]
pub(super) struct Request {
    pub ms: u64,
    pub range: Range,
}
/// What the audition and estimates depend on. The name only labels a GIF, so
/// typing one never re-renders previews or re-estimates sizes. The key keeps a
/// fixed valid name: workers validate every range they receive.
pub(super) fn audition_key(r: &Range) -> Range {
    Range {
        name: "GIF".into(),
        ..r.clone()
    }
}
#[derive(Clone)]
pub(super) struct Pair {
    pub request: Request,
    pub output: image::RgbaImage,
}
pub(super) struct Preview {
    pub request: workers::Latest<Request>,
    pub result: workers::Latest<Result<Pair, String>>,
    pub stop: Arc<AtomicBool>,
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
/// Recently rendered preview frames, bounded by memory (oldest dropped first).
struct FrameCache<K> {
    frames: std::collections::VecDeque<(K, image::RgbaImage)>,
    bytes: usize,
}
impl<K> Default for FrameCache<K> {
    fn default() -> Self {
        Self {
            frames: Default::default(),
            bytes: 0,
        }
    }
}
impl<K: PartialEq> FrameCache<K> {
    const BUDGET: usize = 96 << 20;
    fn get(&self, key: &K) -> Option<image::RgbaImage> {
        self.frames
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, f)| f.clone())
    }
    fn put(&mut self, key: K, frame: image::RgbaImage) {
        self.bytes += frame.as_raw().len();
        self.frames.push_back((key, frame));
        while self.bytes > Self::BUDGET && self.frames.len() > 1 {
            if let Some((_, old)) = self.frames.pop_front() {
                self.bytes -= old.as_raw().len();
            }
        }
    }
}
impl Preview {
    pub fn start(doc: Document, ctx: egui::Context) -> Self {
        let request: workers::Latest<Request> = Default::default();
        let input = request.clone();
        let result = workers::Latest::default();
        let output = result.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        std::thread::spawn(move || {
            #[cfg(windows)]
            {
                let runtime = media::Runtime::new();
                if let Err(e) = runtime {
                    let _ = output.send(Err(e.to_string()));
                    ctx.request_repaint();
                    return;
                }
                let mut gif_reader = media::Visuals::new();
                let mut gif_cache = capture_core::video::frame::CompositeCache::default();
                // BA12: looped playback reuses decoded and quantized frames instead
                // of decoding twice and re-quantizing every frame on every pass.
                let mut originals = FrameCache::default();
                let mut outputs: FrameCache<(u64, capture_core::gif_studio::Settings)> =
                    FrameCache::default();
                while !cancel.load(Ordering::Relaxed) {
                    if let Some(r) = input.take() {
                        let mut run = || -> Result<Pair, String> {
                            r.range.validate().map_err(|e| e.to_string())?;
                            let p = doc.video.as_ref().unwrap();
                            let ms = (r.ms * p.fps as u64 / 1000) * 1000 / p.fps as u64;
                            // Inside the range, show the GIF's own frame timing; outside
                            // it, follow the playhead so both previews stay together.
                            let gif_ms = if (r.range.start..r.range.end).contains(&r.ms) {
                                r.range.preview_ms(r.ms)
                            } else {
                                ms
                            };
                            let key = (gif_ms, r.range.settings.clone());
                            let output = match outputs.get(&key) {
                                Some(frame) => frame,
                                None => {
                                    // BB14: only the GIF output is previewed, so only its
                                    // own frame is decoded and composited.
                                    let frame = match originals.get(&gif_ms) {
                                        Some(frame) => frame,
                                        None => {
                                            let layers = gif_reader.layers(
                                                p,
                                                [doc.width, doc.height],
                                                gif_ms,
                                            )?;
                                            let frame = gif_cache
                                                .composite(&doc, &layers, gif_ms)
                                                .map_err(|e| e.to_string())?;
                                            originals.put(gif_ms, frame.clone());
                                            frame
                                        }
                                    };
                                    let out = capture_core::gif_studio::preview(
                                        &frame,
                                        &r.range.settings,
                                    )
                                    .map_err(|e| e.to_string())?;
                                    outputs.put(key, out.clone());
                                    out
                                }
                            };
                            Ok(Pair {
                                request: r.clone(),
                                output,
                            })
                        };
                        let _ = output.send(run());
                        ctx.request_repaint();
                    } else {
                        std::thread::sleep(Duration::from_millis(8));
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (doc, input, cancel);
                let _ = output.send(Err("Video preview requires Windows.".into()));
                ctx.request_repaint();
            }
        });
        Self {
            request,
            result,
            stop,
        }
    }
}
pub(super) struct Estimates {
    pub request: workers::Latest<(u64, Vec<Range>)>,
    pub result: mpsc::Receiver<(Range, Result<Estimate, String>)>,
    pub generation: Arc<std::sync::atomic::AtomicU64>,
    pub paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}
impl Drop for Estimates {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Estimates {
    pub fn start(doc: Document, ctx: egui::Context) -> Self {
        let request: workers::Latest<(u64, Vec<Range>)> = Default::default();
        let input = request.clone();
        let (tx, result) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let generation = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let revision = generation.clone();
        let paused = Arc::new(AtomicBool::new(false));
        let pause = paused.clone();
        std::thread::spawn(move || {
            #[cfg(windows)]
            {
                let runtime = media::Runtime::new();
                if runtime.is_err() {
                    return;
                }
                let mut reader = media::Visuals::new();
                let mut cache = capture_core::video::frame::CompositeCache::default();
                while !cancel.load(Ordering::Relaxed) {
                    if pause.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(30));
                        continue;
                    }
                    if let Some((version, ranges)) = input.take() {
                        for r in ranges {
                            if revision.load(Ordering::Relaxed) != version
                                || cancel.load(Ordering::Relaxed)
                            {
                                break;
                            }
                            let estimate = capture_core::gif_studio::estimate(
                                &r,
                                [doc.width, doc.height],
                                |ms| {
                                    while pause.load(Ordering::Relaxed)
                                        && !cancel.load(Ordering::Relaxed)
                                        && revision.load(Ordering::Relaxed) == version
                                    {
                                        std::thread::sleep(Duration::from_millis(30));
                                    }
                                    if cancel.load(Ordering::Relaxed)
                                        || revision.load(Ordering::Relaxed) != version
                                    {
                                        return Err("Estimate cancelled".into());
                                    }
                                    let layers = reader.layers(
                                        doc.video.as_ref().unwrap(),
                                        [doc.width, doc.height],
                                        ms,
                                    )?;
                                    cache
                                        .composite(&doc, &layers, ms)
                                        .map_err(|e| e.to_string())
                                },
                                || {
                                    cancel.load(Ordering::Relaxed)
                                        || revision.load(Ordering::Relaxed) != version
                                },
                            );
                            if revision.load(Ordering::Relaxed) == version {
                                let _ = tx.send((r, estimate));
                                ctx.request_repaint();
                            }
                        }
                    } else {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (doc, ctx, input, tx, cancel, revision, pause);
            }
        });
        Self {
            request,
            result,
            generation,
            paused,
            stop,
        }
    }
}
