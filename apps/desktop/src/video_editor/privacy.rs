//! Local, cancellable review. Lost regions hold their last position briefly, then end,
//! and every held interval is flagged Needs review.
use super::*;
use crate::tips::Tip as _;
use capture_core::privacy::{
    Options,
    scan::{Scanner, Tracked},
};
static SCANNING: AtomicBool = AtomicBool::new(false);
struct ScanGuard;
impl Drop for ScanGuard {
    fn drop(&mut self) {
        SCANNING.store(false, Ordering::Release);
    }
}
#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    /// The range of the clip selected when the dialog opened.
    clip: Option<(u64, u64)>,
    scope_clip: bool,
    start: u64,
    end: u64,
    options: Options,
    phrases: String,
    job: Option<Job>,
    result: Option<Scan>,
    checked: Vec<bool>,
    selected: usize,
    cursor: u64,
    preview: Option<Preview>,
    texture: Option<egui::TextureHandle>,
    requested: Option<u64>,
    displayed: u64,
    error: String,
    progress: f32,
    reviewed: bool,
    cover: crate::sensitive_ui::Cover,
    /// The decoded frame under review, and what the picture currently shows.
    frame: Option<image::RgbaImage>,
    shown: Option<(u64, Vec<bool>, crate::sensitive_ui::Cover)>,
}
struct Scan {
    doc: Document,
    tracks: Vec<Tracked>,
    end: u64,
}
struct Job {
    result: mpsc::Receiver<Result<Scan, String>>,
    progress: workers::Latest<f32>,
    stop: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
struct Preview {
    request: workers::Latest<u64>,
    result: workers::Latest<Result<(u64, image::RgbaImage), String>>,
    stop: Arc<AtomicBool>,
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Preview {
    fn new(doc: Document, ctx: egui::Context) -> Self {
        let request = workers::Latest::default();
        let input = request.clone();
        let result = workers::Latest::default();
        let output = result.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        std::thread::spawn(move || {
            #[cfg(windows)]
            {
                let _runtime = match media::Runtime::new() {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = output.send(Err(e.to_string()));
                        return;
                    }
                };
                let mut reader = media::Visuals::new();
                let mut cache = capture_core::video::frame::CompositeCache::default();
                while !cancel.load(Ordering::Relaxed) {
                    if let Some(ms) = input.take() {
                        let r = reader
                            .layers(doc.video.as_ref().unwrap(), [doc.width, doc.height], ms)
                            .and_then(|layers| {
                                cache
                                    .composite(&doc, &layers, ms)
                                    .map_err(|e| e.to_string())
                            });
                        let _ = output.send(r.map(|pixels| (ms, pixels)));
                        ctx.request_repaint();
                    } else {
                        std::thread::sleep(Duration::from_millis(8));
                    }
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (doc, input, cancel);
                let _ = output.send(Err("Video review requires Windows.".into()));
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
fn scan(doc: Document, start: u64, end: u64, options: Options, ctx: egui::Context) -> Job {
    let (tx, result) = mpsc::channel();
    let progress = workers::Latest::default();
    let report = progress.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = stop.clone();
    std::thread::spawn(move || {
        let run = || -> Result<Scan, String> {
            if SCANNING.swap(true, Ordering::AcqRel) {
                return Err("The previous scan is finishing. Try again in a moment.".into());
            }
            let _guard = ScanGuard;
            if end <= start {
                return Err("Nothing to scan: the clip is empty.".into());
            }
            #[cfg(windows)]
            {
                let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
                let mut reader = media::Visuals::new();
                let mut cache = capture_core::video::frame::CompositeCache::default();
                let project = doc.video.as_ref().ok_or("Open a video first.")?;
                let size = [doc.width, doc.height];
                let density = (960. / doc.width as f32).min(1.);
                let small = [
                    (size[0] as f32 * density) as u32,
                    (size[1] as f32 * density) as u32,
                ];
                // The frames export writes, so a box is placed on the very
                // picture it will cover.
                let fps = project.fps as u64;
                let first = (start * fps).div_ceil(1000);
                let last = (end * fps).div_ceil(1000);
                let frames = last.saturating_sub(first).max(1);
                let mut scanner = Scanner::new(size, density, options, first * 1000 / fps);
                for index in first..last {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Scan cancelled. No objects were changed.".into());
                    }
                    let ms = index * 1000 / fps;
                    let layers = reader.layers(project, size, ms)?;
                    let pixels = cache
                        .composite(&doc, &layers, ms)
                        .map_err(|e| e.to_string())?;
                    let reduced = if density < 1. {
                        image::imageops::resize(
                            &pixels,
                            small[0],
                            small[1],
                            image::imageops::FilterType::Triangle,
                        )
                    } else {
                        pixels.clone()
                    };
                    scanner.frame(ms, reduced, || {
                        crate::ocr::recognize_bounded(&pixels, Duration::from_secs(12))
                            .map(|read| read.words)
                    })?;
                    let _ = report.send((index - first + 1) as f32 / frames as f32);
                    ctx.request_repaint();
                }
                let tracks = scanner.finish(end)?;
                Ok(Scan { doc, tracks, end })
            }
            #[cfg(not(windows))]
            {
                let _ = (doc, options, cancel, report);
                Err("Video scanning requires Windows.".into())
            }
        };
        let _ = tx.send(run());
        ctx.request_repaint();
    });
    Job {
        result,
        progress,
        stop,
    }
}
/// Runs the real scan on a saved project and writes what it produced, so a
/// change can be measured against actual footage. Set
/// CLIPDESIGNER_SCAN_PROJECT and CLIPDESIGNER_SCAN_AUDIT_DIR to run it.
#[cfg(all(test, windows))]
#[test]
fn scan_saved_project_for_audit() {
    use std::io::Write as _;
    let (Ok(project), Ok(dir)) = (
        std::env::var("CLIPDESIGNER_SCAN_PROJECT"),
        std::env::var("CLIPDESIGNER_SCAN_AUDIT_DIR"),
    ) else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut doc = capture_core::project_file::open(Path::new(&project)).unwrap();
    doc.annotations.clear();
    doc.video.as_mut().unwrap().objects.clear();
    let _runtime = media::Runtime::new().unwrap();
    let mut reader = media::Visuals::new();
    let mut cache = capture_core::video::frame::CompositeCache::default();
    let project = doc.video.as_ref().unwrap();
    let size = [doc.width, doc.height];
    let density = (960. / doc.width as f32).min(1.);
    let small = [
        (size[0] as f32 * density) as u32,
        (size[1] as f32 * density) as u32,
    ];
    let end = project.duration();
    let mut scanner = Scanner::new(size, density, Options::default(), 0);
    let mut gray = std::io::BufWriter::new(std::fs::File::create(dir.join("frames.gray")).unwrap());
    let mut times = vec![];
    for index in 0..(end * project.fps as u64).div_ceil(1000) {
        let ms = (index * 1000 / project.fps as u64).min(end - 1);
        let layers = reader.layers(project, size, ms).unwrap();
        let pixels = cache.composite(&doc, &layers, ms).unwrap();
        let luma: Vec<u8> = pixels
            .pixels()
            .map(|p| ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8)
            .collect();
        gray.write_all(&luma).unwrap();
        let reduced = image::imageops::resize(
            &pixels,
            small[0],
            small[1],
            image::imageops::FilterType::Triangle,
        );
        scanner
            .frame(ms, reduced, || {
                crate::ocr::recognize_bounded(&pixels, Duration::from_secs(12)).map(|r| r.words)
            })
            .unwrap();
        times.push(ms);
    }
    let log = scanner.scroll_log().to_vec();
    let tracks = scanner.finish(end).unwrap();
    let json = serde_json::json!({
        "log": log,
        "times": times,
        "end": end,
        "tracks": tracks.iter().map(|t| serde_json::json!({
            "start": t.follow.start,
            "end": t.end(end),
            // The shape of what was read, never the text itself.
            "category": t.label.split(" · ").next(),
            "shape": t.key.chars().map(|c| if c.is_ascii_digit() { '9' } else if c.is_alphabetic() { 'a' } else { c }).collect::<String>(),
            "nodes": t.path.nodes.iter().map(|n| serde_json::json!([n.at, n.rect, n.confidence, n.visible])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    std::fs::write(dir.join("tracks.json"), json.to_string()).unwrap();
}
/// The dialog with sample findings, for the dialog audit.
#[cfg(test)]
pub(super) fn seed_results(e: &mut Editor) {
    let doc = e.session.document.clone();
    let [w, h] = [doc.width, doc.height];
    let frame = image::RgbaImage::from_fn(w, h, |x, y| {
        let v = if y % 40 < 12 && x % 9 < 5 { 225 } else { 40 };
        image::Rgba([v, v, v, 255])
    });
    let options = Options {
        phrases: vec!["sam@mail.test".into()],
        ..Default::default()
    };
    let mut scanner = Scanner::new([w, h], 1., options, 0);
    for i in 0..4 {
        scanner
            .frame(i * 33, frame.clone(), || {
                Ok(vec![capture_core::privacy::Word {
                    text: "sam@mail.test".into(),
                    line: 0,
                    bounds: [80., 40., 180., 14.],
                }])
            })
            .unwrap();
    }
    let tracks = scanner.finish(132).unwrap();
    let d = &mut e.video.privacy;
    d.checked = vec![true; tracks.len()];
    d.result = Some(Scan {
        doc,
        tracks,
        end: 132,
    });
    d.frame = Some(frame);
    d.cover.kind = crate::Kind::Pixelate;
}
/// A time as minutes, seconds and tenths, such as 0:04.2.
fn clock(ms: u64) -> String {
    format!("{}:{:04.1}", ms / 60_000, (ms % 60_000) as f64 / 1000.)
}
fn same(a: &Document, b: &Document) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    for d in [&mut a, &mut b] {
        if let Some(v) = &mut d.video {
            v.cursor = 0;
        }
    }
    serde_json::to_vec(&a).ok() == serde_json::to_vec(&b).ok()
}
impl Editor {
    pub(crate) fn open_video_privacy_review(&mut self) {
        self.video.pause();
        self.commit_text();
        self.commit_property_preview();
        let duration = self
            .session
            .document
            .video
            .as_ref()
            .map_or(1, |v| v.duration());
        let clip = match self.video.selected {
            Selection::Recording(i) => self.session.document.video.as_ref().and_then(|p| {
                let item = capture_core::video::editing::Item::Media(p.segments.get(i)?.id);
                p.item_range(item).map(|(start, end, _)| (start, end))
            }),
            _ => None,
        };
        self.video.privacy = State {
            open: true,
            clip,
            end: duration,
            ..Default::default()
        };
    }
    pub(crate) fn show_video_privacy_review(&mut self, ctx: &egui::Context) {
        if !self.video.privacy.open {
            return;
        }
        let mut d = std::mem::take(&mut self.video.privacy);
        d.options.custom = self.quick_controls.sensitive_types();
        let mut types_changed = false;
        if let Some(job) = &d.job {
            if let Some(p) = job.progress.take() {
                d.progress = p;
            }
            if let Ok(result) = job.result.try_recv() {
                d.job = None;
                match result {
                    Ok(scan) => {
                        d.checked = vec![true; scan.tracks.len()];
                        d.frame = None;
                        d.shown = None;
                        d.cursor = d.start;
                        d.preview = Some(Preview::new(scan.doc.clone(), ctx.clone()));
                        d.result = Some(scan);
                        d.requested = None;
                    }
                    Err(e) => d.error = e,
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(80));
            }
        }
        if let Some(preview) = &d.preview {
            if d.requested != Some(d.cursor) {
                let _ = preview.request.send(d.cursor);
                d.requested = Some(d.cursor);
            }
            if let Some(result) = preview.result.take() {
                match result {
                    Ok((ms, p)) => {
                        d.displayed = ms;
                        d.frame = Some(p);
                        d.shown = None;
                    }
                    Err(e) => d.error = e,
                }
            }
        }
        if let (Some(scan), Some(frame)) = (&d.result, &d.frame)
            && d.shown
                .as_ref()
                .is_none_or(|(ms, c, k)| *ms != d.displayed || *c != d.checked || *k != d.cover)
        {
            let boxes: Vec<[f32; 4]> = scan
                .tracks
                .iter()
                .zip(&d.checked)
                .filter(|(t, c)| {
                    **c && d.displayed >= t.follow.start && d.displayed < t.end(scan.end)
                })
                .map(|(t, _)| t.path.sample(d.displayed - t.follow.start))
                .filter(|n| n.visible)
                .map(|n| n.rect)
                .collect();
            let shown = crate::sensitive_ui::preview(frame, &boxes, d.cover);
            d.texture = Some(ctx.load_texture(
                "video-privacy-review",
                egui::ColorImage::from_rgba_unmultiplied(
                    [shown.width() as usize, shown.height() as usize],
                    shown.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            ));
            d.shown = Some((d.displayed, d.checked.clone(), d.cover));
        }
        let mut close = false;
        let mut begin = false;
        let mut apply = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("video-privacy")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 70.).clamp(380., 900.));
            crate::controls::dialog_title(ui, "Find Sensitive Info · Video");
            egui::ScrollArea::vertical()
                .id_salt("video-privacy-content")
                .max_height((ctx.content_rect().height() - 170.).max(240.))
                .show(ui, |ui| {
                    ui.add_enabled_ui(d.job.is_none(), |ui| {
                        types_changed |= crate::sensitive_ui::look_for(ui, &mut d.options, &mut d.phrases).1;
                        ui.add_space(4.);
                        ui.horizontal(|ui| {
                            crate::controls::dialog_section_label(ui, "Scan", None);
                            let clip = d.clip.is_some();
                            crate::controls::segments(ui, |ui| {
                            ui.add_enabled_ui(clip, |ui| {
                                if ui
                                    .selectable_label(d.scope_clip && clip, "Active clip")
                                    .tip("Scan only the selected clip.")
                                    .tip_disabled("Select a clip on the timeline first.")
                                    .clicked()
                                {
                                    d.scope_clip = true;
                                }
                            });
                            if ui
                                .selectable_label(!d.scope_clip || !clip, "Full project")
                                .tip("Scan the whole timeline.")
                                .clicked()
                            {
                                d.scope_clip = false;
                            }
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if d.result.is_some() { "Scan again" } else { "Scan" };
                                begin = ui
                                    .add(crate::controls::dialog_button(label, d.result.is_none()))
                                    .clicked();
                            });
                        });
                    });
                    if d.job.is_some() {
                        ui.add(egui::ProgressBar::new(d.progress).show_percentage());
                        if ui.button("Cancel scan").clicked() {
                            d.job = None;
                        }
                    }
                    if let Some(scan) = &d.result {
                        crate::panel_ui::divider(ui);
                        let height = (ctx.content_rect().height() - 590.).clamp(110., 260.);
                        ui.columns(2, |columns| {
                            let ui = &mut columns[0];
                            if let Some(texture) = &d.texture {
                                let size = texture.size_vec2();
                                let scale = (ui.available_width() / size.x).min(height / size.y);
                                let response = ui.image((texture.id(), size * scale));
                                // Uncertain positions and the chosen finding stay marked.
                                let factor = response.rect.width() / scan.doc.width.max(1) as f32;
                                for (i, t) in scan.tracks.iter().enumerate().filter(|(i, t)| {
                                    d.checked[*i]
                                        && d.displayed >= t.follow.start
                                        && d.displayed < t.end(scan.end)
                                }) {
                                    let n = t.path.sample(d.displayed - t.follow.start);
                                    if !n.visible || (i != d.selected && n.confidence != 0.) {
                                        continue;
                                    }
                                    let r = egui::Rect::from_min_size(
                                        response.rect.min + egui::vec2(n.rect[0], n.rect[1]) * factor,
                                        egui::vec2(n.rect[2], n.rect[3]) * factor,
                                    );
                                    ui.painter().rect_stroke(
                                        r,
                                        2.,
                                        egui::Stroke::new(
                                            if i == d.selected { 2. } else { 1. },
                                            if n.confidence == 0. {
                                                egui::Color32::LIGHT_RED
                                            } else {
                                                crate::shell::ACCENT
                                            },
                                        ),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                            }
                            ui.horizontal(|ui| {
                                ui.label("Preview");
                                ui.spacing_mut().slider_width = (ui.available_width() - 60.).max(60.);
                                crate::controls::slider(
                                    ui,
                                    egui::Slider::new(&mut d.cursor, d.start..=scan.end - 1)
                                        .show_value(false),
                                )
                                .tip("Check how the covers sit at any moment.");
                                ui.label(clock(d.cursor));
                            });
                            let ui = &mut columns[1];
                            ui.horizontal(|ui| {
                                let n = scan.tracks.len();
                                ui.label(format!("{n} {}", if n == 1 { "finding" } else { "findings" }));
                                if ui.small_button("Select all").clicked() {
                                    d.checked.fill(true);
                                }
                                if ui.small_button("Clear").clicked() {
                                    d.checked.fill(false);
                                }
                            });
                            egui::ScrollArea::vertical()
                                .id_salt("video-privacy-findings")
                                .max_height(height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for (i, t) in scan.tracks.iter().enumerate() {
                                        ui.horizontal(|ui| {
                                            crate::controls::toggle(ui, &mut d.checked[i], "");
                                            if ui.selectable_label(i == d.selected, &t.label).clicked() {
                                                d.selected = i;
                                                d.cursor = t
                                                    .follow
                                                    .needs_review
                                                    .first()
                                                    .map_or(t.follow.start, |r| r.0);
                                            }
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    let span = format!(
                                                        "{}–{}",
                                                        clock(t.follow.start),
                                                        clock(t.end(scan.end))
                                                    );
                                                    match t.review_note() {
                                                        Some(note) => {
                                                            ui.colored_label(
                                                                egui::Color32::LIGHT_RED,
                                                                "Needs review",
                                                            )
                                                            .tip(format!("{span}. {note}"));
                                                        }
                                                        None => {
                                                            ui.weak(span);
                                                        }
                                                    }
                                                },
                                            );
                                        });
                                    }
                                });
                        });
                        crate::sensitive_ui::cover(ui, &mut d.cover);
                        ui.add_space(4.);
                        crate::controls::toggle(ui, &mut d.reviewed, "I reviewed the selected ranges and their coverage")
                            .tip("Where tracking lost a finding, its cover holds the last position briefly, then ends. Check every Needs review range. Originals stay in the editable project.");
                    }
                });
            if !d.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &d.error);
            }
            let count = d.checked.iter().filter(|v| **v).count();
            (apply, close) = crate::controls::dialog_actions(
                ui,
                &d.cover.action(count),
                "Close",
                d.result.is_some() && count > 0 && d.job.is_none(),
                false,
            );
        });
        if types_changed {
            let _ = self
                .quick_controls
                .set_sensitive_types(d.options.custom.clone());
        }
        if close || crate::text_edit::cancel_on_escape(&modal) {
            return;
        }
        if begin {
            d.error.clear();
            d.result = None;
            d.preview = None;
            d.texture = None;
            d.reviewed = false;
            let duration = self
                .session
                .document
                .video
                .as_ref()
                .map_or(1, |v| v.duration());
            (d.start, d.end) = d.clip.filter(|_| d.scope_clip).unwrap_or((0, duration));
            let mut options = d.options.clone();
            options.phrases = d
                .phrases
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            d.job = Some(scan(
                self.session.document.clone(),
                d.start,
                d.end,
                options,
                ctx.clone(),
            ));
        }
        if apply && let Some(scan) = &d.result {
            if !same(&scan.doc, &self.session.document) {
                d.error =
                    "The project changed. Scan it again before applying these results.".into();
            } else {
                let mut doc = self.session.document.clone();
                let mut p = doc.video.take().unwrap();
                let ids = self.session.next_id()..;
                let mut top = vec![];
                for (id, (i, t)) in ids.zip(
                    scan.tracks
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| d.checked[*i]),
                ) {
                    // AZ16: reuse an open privacy track; add one only when every
                    // existing privacy track overlaps this redaction.
                    let (start, end) = (t.follow.start, t.end(scan.end).max(t.follow.start + 1));
                    let privacy = |p: &Project, n: u64| {
                        p.custom_track_name(capture_core::video::links::Layer::Annotation(n))
                            .is_some_and(|s| s.starts_with("Privacy"))
                    };
                    let track = match p
                        .tracks
                        .iter()
                        .copied()
                        .find(|n| privacy(&p, *n) && p.free(*n, id, start, end))
                    {
                        Some(track) => track,
                        None => {
                            let count = p.tracks.iter().filter(|n| privacy(&p, **n)).count();
                            let track = p.add_track();
                            p.track_settings_mut(capture_core::video::links::Layer::Annotation(
                                track,
                            ))
                            .name = format!("Privacy {}", count + 1);
                            track
                        }
                    };
                    let layer = capture_core::video::links::Layer::Annotation(track);
                    if !top.contains(&layer) {
                        top.push(layer);
                    }
                    let mut path = t.path.clone();
                    path.reviewed = d.reviewed;
                    let r = path.reference;
                    let mut a = capture_core::steps::marker(
                        id,
                        [32., 32.],
                        32,
                        [0, 0, 0, 255],
                        (1, false),
                        [doc.width, doc.height],
                    )
                    .unwrap();
                    a.kind = d.cover.kind;
                    a.width = d.cover.width();
                    a.text = None;
                    a.step_outline = None;
                    a.start = [r[0], r[1]];
                    a.end = [r[0] + r[2], r[1] + r[3]];
                    a.fill = None;
                    let mut timing = capture_core::video::editor::Timing::new(
                        track,
                        t.follow.start,
                        t.end(scan.end).max(t.follow.start + 1),
                    );
                    timing.label = format!("Tracked {} {}", d.cover.noun(), i + 1);
                    timing.motion_path = Some(Arc::new(path));
                    p.place(id, timing);
                    doc.annotations.push(a);
                }
                // These coordinates describe the final canvas, above existing Zoom transforms.
                for z in &mut p.zooms {
                    z.pinned = false;
                }
                let order = p.layers();
                top.extend(
                    order
                        .into_iter()
                        .filter(|l| !top.contains(l))
                        .collect::<Vec<_>>(),
                );
                p.layer_order = top;
                self.execute(Command::VideoComposition {
                    project: p,
                    annotations: doc.annotations,
                });
                // Added, with a reminder that needs doing before export.
                self.warn(
                    format!(
                        "Tracked {}s added. Some stretches are uncertain: review them before export.",
                        d.cover.noun()
                    ),
                    Some(("Review them", crate::guided::Action::ReviewRedactions)),
                );
                return;
            }
        }
        self.video.privacy = d;
    }
}
