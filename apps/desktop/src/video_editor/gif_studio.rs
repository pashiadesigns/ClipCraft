//! Independent GIF audition and streaming exports from an immutable project snapshot.
use super::gif_audition::{Estimates, Preview, Request, audition_key};
use super::*;
use capture_core::gif_studio::{Range, Settings};
use std::collections::BTreeSet;
#[path = "gif_layout.rs"]
mod layout;

#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    draft: Option<Vec<Range>>,
    pub(super) selected: Option<u64>,
    checked: BTreeSet<u64>,
    cursor: u64,
    playing: Option<(Instant, u64)>,
    preview: Option<Preview>,
    texture: Option<egui::TextureHandle>,
    requested: Option<Request>,
    estimator: Option<Estimates>,
    estimates: Vec<(Range, Result<capture_core::gif_studio::Estimate, String>)>,
    estimate_ranges: Vec<Range>,
    estimate_due: Option<Instant>,
    job: Option<Job>,
    progress: f32,
    message: String,
    /// Files written by the last successful export, for Open GIF / Open folder.
    exported: Vec<PathBuf>,
    /// Range timeline gesture: (GIF id, edge -1/0/1, pointer ms at press, start, end).
    range_drag: Option<(u64, i8, u64, u64, u64)>,
    /// The Duplicate drag gesture is held, read fresh each frame.
    duplicate_held: bool,
}
/// Filmstrip frames along the project timeline: (project time, thumbnail).
type Strip = Vec<(u64, egui::TextureId)>;
struct Job {
    result: mpsc::Receiver<Result<Vec<PathBuf>, String>>,
    progress: workers::Latest<f32>,
    cancel: Arc<AtomicBool>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn export(
    doc: Document,
    ranges: Vec<Range>,
    folder: PathBuf,
    ctx: egui::Context,
    watermark: Option<capture_core::watermark::Watermark>,
) -> Job {
    let (tx, result) = mpsc::channel();
    let progress = workers::Latest::default();
    let report = progress.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    std::thread::spawn(move || {
        let run = || -> Result<Vec<PathBuf>, String> {
            doc.validate().map_err(|e| e.to_string())?;
            capture_core::privacy::tracking::require_review(&doc).map_err(|e| e.to_string())?;
            let p = doc.video.as_ref().ok_or("Open a video project first.")?;
            for r in &ranges {
                r.validate_export(p.duration()).map_err(|e| e.to_string())?;
            }
            #[cfg(windows)]
            {
                let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
                let mut reader = media::Visuals::new();
                let mut cache = capture_core::video::frame::CompositeCache::default();
                // The export watermark marks GIFs too (FX-003).
                let layer = watermark
                    .as_ref()
                    .map(|w| w.layer([doc.width, doc.height]))
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let mut files = vec![];
                for (index, r) in ranges.iter().enumerate() {
                    let mut temp = tempfile::Builder::new()
                        .prefix(".clipdesigner-gif-")
                        .suffix(".gif")
                        .tempfile_in(&folder)
                        .map_err(|e| e.to_string())?;
                    capture_core::gif_studio::encode(
                        r,
                        [doc.width, doc.height],
                        temp.as_file_mut(),
                        |ms| {
                            let layers = reader.layers(p, [doc.width, doc.height], ms)?;
                            let mut pixels = cache
                                .composite(&doc, &layers, ms)
                                .map_err(|e| e.to_string())?;
                            if let Some(layer) = &layer {
                                capture_core::watermark::blend(&mut pixels, layer);
                            }
                            Ok(pixels)
                        },
                        |value| {
                            if stop.load(Ordering::Relaxed) {
                                return Err(format!(
                                    "GIF export cancelled. {} completed file(s) remain in the chosen folder.",
                                    files.len()
                                ));
                            }
                            let _ = report.send((index as f32 + value) / ranges.len() as f32);
                            ctx.request_repaint();
                            Ok(())
                        },
                    )?;
                    temp.as_file().sync_all().map_err(|e| e.to_string())?;
                    let stem: String = r
                        .name
                        .chars()
                        .map(|c| {
                            if c.is_alphanumeric() || c == '-' || c == '_' {
                                c
                            } else {
                                '-'
                            }
                        })
                        .take(80)
                        .collect();
                    // Prefix avoids reserved Windows device names; never replace existing files.
                    let mut n = 1;
                    let mut path = folder.join(format!("ClipDesigner-{stem}.gif"));
                    while path.exists() {
                        n += 1;
                        path = folder.join(format!("ClipDesigner-{stem}-{n}.gif"));
                    }
                    temp.persist_noclobber(&path).map_err(|e| e.to_string())?;
                    files.push(path);
                }
                Ok(files)
            }
            #[cfg(not(windows))]
            {
                let _ = (folder, stop, report);
                Err("GIF video export requires Windows.".into())
            }
        };
        let _ = tx.send(run());
        ctx.request_repaint();
    });
    Job {
        result,
        progress,
        cancel,
    }
}
impl Editor {
    pub(crate) fn open_gif_studio(&mut self) {
        if !self.is_video() {
            return;
        }
        self.video.pause();
        self.commit_text();
        self.video.gif.draft = None;
        self.video.gif.open = true;
        self.video.gif.playing = None;
        self.video.gif.requested = None;
        self.video.gif.texture = None;
        self.video.gif.preview = None;
        self.video.gif.estimator = None;
        self.video.gif.estimates.clear();
        self.video.gif.estimate_ranges.clear();
        self.video.gif.message = String::new();
        self.video.gif.exported.clear();
        self.video.gif.range_drag = None;
    }
    /// Thumbnails for the GIF range timeline, sampled from the same frame
    /// overviews the main timeline uses for its video clips.
    fn gif_strip(&self, p: &Project, samples: usize) -> Strip {
        let duration = p.duration().max(1);
        (0..samples)
            .filter_map(|i| {
                let at = (i as u64 * 2 + 1) * duration / (samples as u64 * 2);
                let clip = p.segments.iter().rev().find(|c| {
                    c.kind() == capture_core::video::editor::MediaKind::Video
                        && !c.hidden
                        && c.active(at)
                })?;
                let thumbnails = clip
                    .source
                    .as_ref()
                    .and_then(|s| self.video.media_overviews.get(&s.path))
                    .map_or(&self.video.thumbnails, |o| &o.0);
                let time = clip.source_time(at);
                thumbnails
                    .iter()
                    .rev()
                    .find(|(t, _)| *t <= time)
                    .or_else(|| thumbnails.first())
                    .map(|(_, t)| (at, t.id()))
            })
            .collect()
    }
    pub(crate) fn show_gif_studio(&mut self, ctx: &egui::Context) {
        if !self.video.gif.open {
            return;
        }
        let Some(mut project) = self.session.document.video.clone() else {
            self.video.gif = Default::default();
            return;
        };
        let before = project.gif_ranges.clone();
        let mut s = std::mem::take(&mut self.video.gif);
        if let Some(draft) = &s.draft {
            project.gif_ranges = draft.clone();
        }
        let duration = project.duration();
        // Zero GIFs is a valid state: the studio shows an empty collection.
        if !project.gif_ranges.iter().any(|r| Some(r.id) == s.selected) {
            s.selected = project.gif_ranges.first().map(|r| r.id);
        }
        if s.preview.is_none() {
            s.preview = Some(Preview::start(self.session.document.clone(), ctx.clone()));
        }
        if let Some(job) = &s.job {
            if let Some(p) = job.progress.take() {
                s.progress = p;
            }
            if let Ok(result) = job.result.try_recv() {
                s.job = None;
                s.message = match result {
                    Ok(paths) => {
                        let message = format!(
                            "Exported {} GIF{} to {}",
                            paths.len(),
                            if paths.len() == 1 { "" } else { "s" },
                            paths
                                .first()
                                .and_then(|p| p.parent())
                                .unwrap_or(Path::new(""))
                                .display()
                        );
                        s.exported = paths;
                        message
                    }
                    Err(e) => {
                        s.exported.clear();
                        e
                    }
                };
            }
        }
        if let Some(result) = s.preview.as_ref().and_then(|p| p.result.take()) {
            match result {
                Ok(pair) => {
                    if s.requested.as_ref().is_some_and(|r| {
                        r.range == pair.request.range
                            && (s.playing.is_some() || r.ms == pair.request.ms)
                    }) {
                        let pixels = pair.output;
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [pixels.width() as usize, pixels.height() as usize],
                            pixels.as_raw(),
                        );
                        if let Some(t) = &mut s.texture {
                            t.set(image, egui::TextureOptions::LINEAR);
                        } else {
                            s.texture = Some(ctx.load_texture(
                                "gif-output",
                                image,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
                Err(e) => s.message = e,
            }
        }
        if s.estimator.is_none() {
            s.estimator = Some(Estimates::start(self.session.document.clone(), ctx.clone()));
        }
        let estimator = s.estimator.as_ref().unwrap();
        let keys: Vec<Range> = project.gif_ranges.iter().map(audition_key).collect();
        while let Ok((range, estimate)) = estimator.result.try_recv() {
            if keys.contains(&range) {
                s.estimates.retain(|(r, _)| r.id != range.id);
                s.estimates.push((range, estimate));
            }
        }
        estimator
            .paused
            .store(s.playing.is_some() || s.job.is_some(), Ordering::Relaxed);
        if s.estimate_ranges != keys {
            s.estimate_ranges = keys.clone();
            s.estimate_due = Some(Instant::now() + Duration::from_millis(400));
            estimator.generation.fetch_add(1, Ordering::Relaxed);
            s.estimates.retain(|(r, _)| keys.contains(r));
        }
        if let Some(due) = s.estimate_due {
            if Instant::now() >= due {
                let pending = keys
                    .iter()
                    .filter(|r| {
                        r.validate_export(duration).is_ok()
                            && !s.estimates.iter().any(|(cached, _)| cached == *r)
                    })
                    .cloned()
                    .collect();
                let _ = estimator
                    .request
                    .send((estimator.generation.load(Ordering::Relaxed), pending));
                s.estimate_due = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
        // With no GIF selected, the output preview uses default settings over
        // the whole project so both previews still follow the playhead.
        let selected = project
            .gif_ranges
            .iter()
            .find(|r| Some(r.id) == s.selected)
            .cloned()
            .unwrap_or_else(|| Range {
                id: u64::MAX,
                name: "GIF".into(),
                start: 0,
                end: duration.max(1),
                settings: Default::default(),
            });
        // BA12: Space plays and stops the GIF preview.
        if s.job.is_none()
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            s.playing = if s.playing.is_some() {
                None
            } else {
                Some((Instant::now(), s.cursor))
            };
        }
        if let Some((clock, start)) = s.playing {
            let length = selected.end.saturating_sub(selected.start).max(1);
            s.cursor = selected.start
                + (start.saturating_sub(selected.start) + clock.elapsed().as_millis() as u64)
                    % length;
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        s.cursor = s.cursor.min(duration.saturating_sub(1));
        let request = Request {
            ms: s.cursor,
            range: audition_key(&selected),
        };
        // Previous textures stay up until the new frames arrive, so edits and
        // renames never blank the previews.
        if s.requested.as_ref() != Some(&request) {
            let _ = s.preview.as_ref().unwrap().request.send(request.clone());
            s.requested = Some(request);
        }
        let strip = self.gif_strip(&project, 24);
        s.duplicate_held = ctx.input(|i| {
            self.gesture_matches(crate::gesture_input::Gesture::DuplicateDrag, i.modifiers)
        });
        let (close, export_ids) = layout::show(
            ctx,
            &mut project,
            &mut s,
            [self.session.document.width, self.session.document.height],
            &strip,
        );
        s.draft = Some(project.gif_ranges.clone());
        if project.gif_ranges != before && project.gif_ranges.iter().all(|r| r.validate().is_ok()) {
            self.commit_video(project.clone());
        }
        if let Some(ids) = export_ids
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("Choose a folder for your GIFs")
                .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Export))
                .pick_folder()
        {
            s.playing = None;
            s.progress = 0.;
            s.message = "Exporting from the current project snapshot…".into();
            let ranges = project
                .gif_ranges
                .iter()
                .filter(|r| ids.contains(&r.id))
                .cloned()
                .collect();
            s.job = Some(export(
                self.session.document.clone(),
                ranges,
                folder,
                ctx.clone(),
                self.export_preferences.options.watermark.clone(),
            ));
        }
        if close {
            if let Some(job) = &s.job {
                job.cancel.store(true, Ordering::Relaxed);
            }
            s.open = false;
            s.preview = None;
            s.estimator = None;
            s.playing = None;
        }
        self.video.gif = s;
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::property_layout::tests::PaintedOutput;
    pub(super) fn range(id: u64) -> Range {
        Range {
            id,
            name: format!("GIF {id}"),
            start: 1000 * id,
            end: 1000 * id + 3000,
            settings: Default::default(),
        }
    }
    struct Studio {
        ctx: egui::Context,
        e: Editor,
    }
    impl Studio {
        fn new() -> Self {
            let ctx = egui::Context::default();
            let mut e = super::super::repair_tests::editor(&ctx);
            e.open_gif_studio();
            e.video.gif.preview = Some(Preview {
                request: Default::default(),
                result: Default::default(),
                stop: Arc::new(AtomicBool::new(false)),
            });
            let mut s = Self { ctx, e };
            s.frame(vec![]);
            s.frame(vec![]);
            s
        }
        fn frame(&mut self, events: Vec<egui::Event>) {
            let mut out = self.ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 900.),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| self.e.show_gif_studio(ui.ctx()),
            );
            out.textures_delta.clear();
            let _ = PaintedOutput(out);
        }
        fn click(&mut self, at: egui::Pos2) {
            self.frame(super::super::repair_tests::press(at, true));
            self.frame(super::super::repair_tests::press(at, false));
            self.frame(vec![]);
        }
        fn drag(&mut self, from: egui::Pos2, to: egui::Pos2) {
            self.frame(super::super::repair_tests::press(from, true));
            for i in 1..=5 {
                self.frame(vec![egui::Event::PointerMoved(
                    from + (to - from) * i as f32 / 5.,
                )]);
            }
            self.frame(super::super::repair_tests::press(to, false));
            self.frame(vec![]);
        }
        fn rect(&self, key: impl std::hash::Hash + std::fmt::Debug) -> egui::Rect {
            self.ctx
                .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(key)))
                .expect("element drawn")
        }
        fn ranges(&self) -> Vec<Range> {
            self.e
                .session
                .document
                .video
                .as_ref()
                .unwrap()
                .gif_ranges
                .clone()
        }
    }
    #[test]
    fn empty_studio_adds_selects_and_removes_down_to_zero() {
        let mut s = Studio::new();
        assert!(s.ranges().is_empty());
        let add = s.rect("gif-add");
        s.click(add.center());
        s.click(s.rect("gif-add").center());
        let ranges = s.ranges();
        assert_eq!(ranges.len(), 2);
        assert_eq!(s.e.video.gif.selected, Some(ranges[1].id));
        // Clicking anywhere in a card selects it, not only its name.
        let card = s.rect(("gif-card", ranges[0].id));
        s.click(card.left_bottom() + egui::vec2(20., -6.));
        assert_eq!(s.e.video.gif.selected, Some(ranges[0].id));
        // The trash icon removes a GIF; removing every GIF leaves none.
        for r in &ranges {
            let trash = s.rect(("gif-trash", r.id));
            s.click(trash.center());
        }
        assert!(s.ranges().is_empty());
        assert_eq!(s.e.video.gif.selected, None);
        // Undo restores them in one step per change.
        s.e.execute(Command::Undo);
        assert_eq!(s.ranges().len(), 1);
    }
    #[test]
    fn range_moves_trims_and_follows_start_and_stop_here() {
        let mut s = Studio::new();
        s.e.video.gif.draft = Some(vec![range(1)]);
        s.frame(vec![]);
        s.frame(vec![]);
        let row = s.rect("gif-range-row");
        let duration = s.e.session.document.video.as_ref().unwrap().duration() as f32;
        let x = |ms: f32| row.left() + ms / duration * row.width();
        // Drag the body two seconds later: the length is kept.
        s.drag(
            egui::pos2(x(2500.), row.center().y),
            egui::pos2(x(4500.), row.center().y),
        );
        let r = &s.ranges()[0];
        assert!(
            (r.start as f32 - 3000.).abs() < 120. && r.end - r.start == 3000,
            "{:?}",
            (r.start, r.end)
        );
        // Trim the right edge.
        let end = r.end as f32;
        s.drag(
            egui::pos2(x(end), row.center().y),
            egui::pos2(x(end + 1500.), row.center().y),
        );
        let r = s.ranges()[0].clone();
        assert!(
            (r.end as f32 - end - 1500.).abs() < 120.,
            "{:?}",
            (r.start, r.end)
        );
        // Start here / Stop here set the range at the playhead.
        s.e.video.gif.cursor = r.start + 500;
        s.click(s.rect("gif-start-here").center());
        assert_eq!(s.ranges()[0].start, r.start + 500);
        s.e.video.gif.cursor = r.end - 700;
        s.click(s.rect("gif-stop-here").center());
        assert_eq!(s.ranges()[0].end, r.end - 699);
        // With no GIF, Start here adds one at the playhead.
        s.e.video.gif.draft = Some(vec![]);
        s.e.video.gif.selected = None;
        s.frame(vec![]);
        s.e.video.gif.cursor = 2000;
        s.click(s.rect("gif-start-here").center());
        assert_eq!(s.ranges().len(), 1);
        assert_eq!(s.ranges()[0].start, 2000);
    }
    /// Renders the populated studio for visual review (set CLIPDESIGNER_GIF_EVIDENCE).
    #[test]
    fn populated_studio_evidence() {
        let Some(folder) = std::env::var_os("CLIPDESIGNER_GIF_EVIDENCE") else {
            return;
        };
        for size in [[1280., 900.], [880., 680.]] {
            let ctx = egui::Context::default();
            let mut e = super::super::repair_tests::editor(&ctx);
            e.open_gif_studio();
            e.video.gif.preview = Some(Preview {
                request: Default::default(),
                result: Default::default(),
                stop: Arc::new(AtomicBool::new(false)),
            });
            let frame = |output: bool| {
                image::RgbaImage::from_fn(640, 360, |x, y| {
                    let band = |v: u32| if output { v / 24 * 24 } else { v };
                    image::Rgba(if x > 80 && x < 470 && y > 90 && y < 250 {
                        [244, 246, 248, 255]
                    } else {
                        [
                            band(40 + y / 6) as u8,
                            band(64 + x / 8) as u8,
                            band(110) as u8,
                            255,
                        ]
                    })
                })
            };
            let pixels = frame(true);
            e.video.gif.texture = Some(ctx.load_texture(
                "gif-evidence",
                egui::ColorImage::from_rgba_unmultiplied([640, 360], pixels.as_raw()),
                egui::TextureOptions::LINEAR,
            ));
            let mut a = range(1);
            a.name = "Open settings".into();
            let mut b = range(6);
            b.name = "Save the project".into();
            e.video.gif.draft = Some(vec![a, b]);
            e.video.gif.selected = Some(1);
            e.video.gif.checked.extend([1, 6]);
            e.video.gif.cursor = 2600;
            let mut draw = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(size[0], size[1]),
                        )),
                        ..Default::default()
                    },
                    |ui| e.show_gif_studio(ui.ctx()),
                ))
            };
            let mut first = draw();
            for _ in 0..6 {
                let out = draw();
                first.0.textures_delta.append(out.0.textures_delta.clone());
            }
            let last = draw();
            crate::property_layout::tests::dump_meshes(
                &ctx,
                &folder.to_string_lossy(),
                &format!("studio-populated-{}-{}", size[0], size[1]),
                first,
                last,
            );
        }
    }
    #[test]
    fn renaming_keeps_the_audition_request_and_estimates() {
        let mut s = Studio::new();
        s.e.video.gif.draft = Some(vec![range(1)]);
        s.frame(vec![]);
        let before = s.e.video.gif.requested.clone().map(|r| (r.ms, r.range));
        let keys = s.e.video.gif.estimate_ranges.clone();
        s.e.video.gif.draft.as_mut().unwrap()[0].name = "Intro loop".into();
        s.frame(vec![]);
        assert_eq!(s.ranges()[0].name, "Intro loop");
        assert_eq!(
            s.e.video.gif.requested.clone().map(|r| (r.ms, r.range)),
            before
        );
        assert_eq!(s.e.video.gif.estimate_ranges, keys);
    }
    /// The workers validate every range they receive (AZ14: previews never left
    /// "Preparing preview…" when the key failed validation).
    #[test]
    fn every_range_sent_to_workers_is_valid() {
        let mut s = Studio::new();
        let r = s.e.video.gif.requested.clone().expect("preview requested");
        r.range.validate().expect("placeholder range valid");
        s.e.video.gif.draft = Some(vec![range(1), range(4)]);
        s.frame(vec![]);
        let r = s.e.video.gif.requested.clone().expect("preview requested");
        r.range.validate().expect("selected range valid");
        for key in &s.e.video.gif.estimate_ranges {
            key.validate().expect("estimate key valid");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn magnified_callout_image_video_properties_and_atomic_lens_drag() {
        for video in [false, true] {
            for width in [290., 390.] {
                let ctx = egui::Context::default();
                let mut e = super::super::repair_tests::editor(&ctx);
                if !video {
                    e.session.document.video = None;
                }
                let index = crate::shell::TOOLS
                    .iter()
                    .position(|t| t.id == capture_core::toolbar::ToolId::MagnifiedCallout)
                    .unwrap();
                e.choose_tool(index);
                let a = e.new_annotation(crate::Kind::Rectangle, [100., 180.], [250., 280.]);
                let id = a.id;
                assert!(a.focus.unwrap().magnify.is_some());
                e.execute(Command::Add { annotation: a });
                e.select_only(Some(id));
                let mut draw = || {
                    PaintedOutput(ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 980.),
                            )),
                            ..Default::default()
                        },
                        |ui| {
                            if video {
                                e.video_properties_pane(ui);
                            } else {
                                e.image_properties_pane(ui);
                            }
                        },
                    ))
                };
                let mut first = draw();
                for _ in 0..3 {
                    let out = draw();
                    for (id, delta) in &out.textures_delta.set {
                        first
                            .0
                            .textures_delta
                            .set
                            .entry(*id)
                            .or_default()
                            .extend(delta.clone());
                    }
                }
                let last = draw();
                for shape in &last.shapes {
                    if let egui::Shape::Text(t) = &shape.shape {
                        assert!(
                            t.pos.x + t.galley.size().x <= width + 1.,
                            "Magnified pane overflow: {}",
                            t.galley.text()
                        );
                    }
                }
                if let Some(folder) = std::env::var_os("CLIPDESIGNER_GIF_EVIDENCE") {
                    dump_meshes(
                        &ctx,
                        &folder.to_string_lossy(),
                        &format!(
                            "magnified-{}-{width}",
                            if video { "video" } else { "image" }
                        ),
                        first,
                        last,
                    );
                }
                let original = e
                    .session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == id)
                    .unwrap()
                    .clone();
                let lens = capture_core::magnified::lens(&original).unwrap();
                let (lo, hi) = lens.body_bounds();
                let from = egui::pos2((lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5);
                let to = from + egui::vec2(35., 25.);
                let mut view = crate::canvas_view::CanvasView::default();
                view.apply(
                    crate::canvas_view::ZoomAction::Set(crate::canvas_view::ZoomMode::Manual(1.)),
                    None,
                );
                let t = view.transform(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960., 540.)),
                    egui::vec2(960., 540.),
                    1.,
                );
                e.session.begin_gesture();
                for events in [
                    super::super::repair_tests::press(from, true),
                    vec![egui::Event::PointerMoved(to)],
                    super::super::repair_tests::press(to, false),
                ] {
                    let _ = PaintedOutput(ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(960., 540.),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            e.magnified_input(ui, t, true);
                        },
                    ));
                }
                e.session.finish_gesture();
                let after = e
                    .session
                    .document
                    .annotations
                    .iter()
                    .find(|a| a.id == id)
                    .unwrap();
                assert_ne!(after.focus, original.focus);
                assert_eq!(after.start, original.start);
                e.execute(Command::Undo);
                assert_eq!(
                    e.session
                        .document
                        .annotations
                        .iter()
                        .find(|a| a.id == id)
                        .unwrap(),
                    &original
                );
            }
        }
    }
    #[test]
    fn studio_saves_ranges_and_renders_compact_and_wide_without_touching_playhead() {
        for size in [[880., 680.], [1280., 900.], [1920., 1080.]] {
            let ctx = egui::Context::default();
            let mut e = super::super::repair_tests::editor(&ctx);
            e.open_gif_studio();
            let cursor = e.video_time();
            e.video.gif.preview = Some(Preview {
                request: Default::default(),
                result: Default::default(),
                stop: Arc::new(AtomicBool::new(false)),
            });
            let pixels = image::RgbaImage::from_fn(640, 360, |x, y| {
                image::Rgba(if x > 80 && x < 470 && y > 90 && y < 250 {
                    [28, 145, 193, 255]
                } else {
                    [34, 45, 52, 255]
                })
            });
            e.video.gif.texture = Some(ctx.load_texture(
                "gif-fixture",
                egui::ColorImage::from_rgba_unmultiplied([640, 360], pixels.as_raw()),
                egui::TextureOptions::LINEAR,
            ));
            let mut draw = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(size[0], size[1]),
                        )),
                        ..Default::default()
                    },
                    |ui| e.show_gif_studio(ui.ctx()),
                ))
            };
            let mut first = draw();
            for _ in 0..8 {
                let out = draw();
                first.0.textures_delta.append(out.0.textures_delta.clone());
            }
            let last = draw();
            if let Some(folder) = std::env::var_os("CLIPDESIGNER_GIF_EVIDENCE") {
                dump_meshes(
                    &ctx,
                    &folder.to_string_lossy(),
                    &format!("studio-{}-{}", size[0], size[1]),
                    first,
                    last,
                );
            }
            assert_eq!(e.video_time(), cursor);
            // A project starts with no GIFs; adding one goes through the studio.
            assert!(
                e.session
                    .document
                    .video
                    .as_ref()
                    .unwrap()
                    .gif_ranges
                    .is_empty()
            );
            assert!(!e.editor_shortcuts_idle());
            let mut r = super::layout_tests::range(1);
            r.name = "GIF 1".into();
            e.video.gif.draft = Some(vec![r]);
            let _ = PaintedOutput(ctx.run_ui(Default::default(), |ui| e.show_gif_studio(ui.ctx())));
            e.video.gif.draft.as_mut().unwrap()[0].name = "Intro loop".into();
            let _ = PaintedOutput(ctx.run_ui(Default::default(), |ui| e.show_gif_studio(ui.ctx())));
            assert_eq!(
                e.session.document.video.as_ref().unwrap().gif_ranges[0].name,
                "Intro loop"
            );
            let saved = serde_json::to_string(&e.session.document).unwrap();
            let restored: Document = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored.video.unwrap().gif_ranges[0].name, "Intro loop");
        }
    }
    /// Runs the real audition worker on a real MP4: both previews must arrive
    /// for the exact request the studio sends (AZ14 regression).
    #[cfg(windows)]
    #[test]
    fn native_audition_worker_returns_both_previews() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mp4");
        let _runtime = media::Runtime::new().unwrap();
        let mut encoder = media::Encoder::new(&source, [320, 180], 30, false).unwrap();
        for i in 0..30 {
            let pixels: Vec<_> = (0..320 * 180)
                .flat_map(|_| [0, (i * 8) as u8, 200, 255])
                .collect();
            encoder.video(i * 10_000_000 / 30, &pixels).unwrap();
        }
        encoder.finish().unwrap();
        drop(encoder);
        let mut doc = Document::blank(320, 180).unwrap();
        doc.video = Some(Project::new(
            source.to_string_lossy().into_owned(),
            [320, 180],
            1000,
            30,
        ));
        doc.promote_features();
        let ctx = egui::Context::default();
        let preview = Preview::start(doc, ctx);
        let range = Range {
            id: 1,
            name: "Renamed while typing".into(),
            start: 100,
            end: 900,
            settings: Settings {
                width: 160,
                fps: 10,
                ..Default::default()
            },
        };
        // Inside the range, then outside it (the output follows the playhead).
        for ms in [400, 950] {
            let request = Request {
                ms,
                range: audition_key(&range),
            };
            let _ = preview.request.send(request.clone());
            let started = Instant::now();
            let pair = loop {
                if let Some(result) = preview.result.take() {
                    break result.expect("preview renders");
                }
                assert!(
                    started.elapsed() < Duration::from_secs(30),
                    "no preview for {ms} ms"
                );
                std::thread::sleep(Duration::from_millis(20));
            };
            assert!(pair.request == request);
            assert!(pair.output.height() > 0);
            assert!(pair.output.width() > 0);
        }
    }
    #[cfg(windows)]
    #[test]
    fn native_video_batch_gifs_trim_source_and_preserve_existing_outputs() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mp4");
        let _runtime = media::Runtime::new().unwrap();
        let mut encoder = media::Encoder::new(&source, [320, 180], 30, false).unwrap();
        for i in 0..30 {
            let pixels: Vec<_> = (0..320 * 180)
                .flat_map(|_| {
                    if i < 15 {
                        [0, 0, 230, 255]
                    } else {
                        [230, 0, 0, 255]
                    }
                })
                .collect();
            encoder.video(i * 10_000_000 / 30, &pixels).unwrap();
        }
        encoder.finish().unwrap();
        drop(encoder);
        let mut doc = Document::blank(320, 180).unwrap();
        doc.video = Some(Project::new(
            source.to_string_lossy().into_owned(),
            [320, 180],
            1000,
            30,
        ));
        doc.promote_features();
        let ranges: Vec<_> = (0..2)
            .map(|i| Range {
                id: i + 1,
                name: format!("Part {}", i + 1),
                start: i * 500,
                end: (i + 1) * 500,
                settings: Settings {
                    width: 160,
                    fps: 10,
                    ..Default::default()
                },
            })
            .collect();
        let folder = root.path().join("gifs");
        std::fs::create_dir_all(&folder).unwrap();
        for run in 0..2 {
            let job = export(
                doc.clone(),
                ranges.clone(),
                folder.clone(),
                egui::Context::default(),
                None,
            );
            let paths = job
                .result
                .recv_timeout(Duration::from_secs(60))
                .unwrap()
                .unwrap();
            assert_eq!(paths.len(), 2);
            for (i, path) in paths.iter().enumerate() {
                let bytes = std::fs::read(path).unwrap();
                assert_eq!(&bytes[..6], b"GIF89a");
                assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 160);
                if run == 0
                    && let Some(evidence) = std::env::var_os("CLIPDESIGNER_GIF_EVIDENCE")
                {
                    let folder = PathBuf::from(evidence);
                    std::fs::create_dir_all(&folder).unwrap();
                    std::fs::copy(path, folder.join(format!("native-part-{i}.gif"))).unwrap();
                }
            }
        }
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 4);
        assert!(source.exists());
    }
}
