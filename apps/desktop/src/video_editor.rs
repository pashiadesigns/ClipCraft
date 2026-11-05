//! Video workspace uses the image editor's objects, with a separate media clock.
use crate::{Command, Document, Editor, FileAction, Session, egui};
use capture_core::video::editor::Project;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
mod audio_effects;
mod audio_output;
#[cfg(test)]
mod beta_finish_tests;
#[cfg(windows)]
mod camera;
#[cfg(test)]
mod flow_tests;
mod footer;
#[cfg(test)]
mod footer_tests;
mod frames;
mod imports;
#[cfg(windows)]
mod media;
#[cfg(test)]
mod media_tests;
mod media_timeline;
#[cfg(test)]
mod narration_tests;
mod object_menu;
mod overview;
mod placement_motion;
#[cfg(test)]
mod precision_tests;
mod properties;
#[cfg(test)]
mod public_beta_tests;
#[cfg(test)]
mod recording_repair_tests;
#[cfg(test)]
mod repair_tests;
mod selection;
mod still_frame;
#[cfg(test)]
mod tests;
mod timeline;
mod tracks;
pub(crate) mod ui;
mod voiceover;
mod voiceover_capture;
mod workers;
mod zoom;
mod zoom_focus;
pub(super) use workers::load;
pub(super) fn still(doc: &Document) -> Result<Document, String> {
    #[cfg(windows)]
    {
        let v = doc.video.as_ref().ok_or("Open a video first.")?;
        let _runtime = media::Runtime::new().map_err(|e| e.to_string())?;
        let mut reader = media::Visuals::new();
        let layers = reader.layers(v, [doc.width, doc.height], v.cursor)?;
        let pixels = capture_core::video::frame::composite(doc, &layers, v.cursor)
            .map_err(|e| e.to_string())?;
        Document::from_image(pixels).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = doc;
        Err("Video decoding requires Windows.".into())
    }
}
pub(super) fn supported(path: &Path) -> bool {
    path.extension()
        .is_some_and(|x| x.eq_ignore_ascii_case("mp4") || x.eq_ignore_ascii_case("vclip"))
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Selection {
    #[default]
    Objects,
    Recording(usize),
    Audio(usize),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PropertyTab {
    #[default]
    Style,
    Timing,
    Motion,
    Audio,
}
impl PropertyTab {
    const ALL: [Self; 4] = [Self::Style, Self::Timing, Self::Motion, Self::Audio];
    fn label(self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Timing => "Timing",
            Self::Motion => "Motion",
            Self::Audio => "Audio",
        }
    }
}
type MediaOverview = (Vec<(u64, egui::TextureHandle)>, Vec<f32>);
pub(super) struct State {
    placement_motion: [Option<capture_core::reusable_presets::Motion>; 2],
    pub(crate) still_frame: Option<still_frame::Dialog>,
    selection: std::collections::BTreeSet<capture_core::video::editing::Item>,
    multi_drag: Option<selection::Move>,
    marquee: Option<selection::Marquee>,
    timeline_offset: egui::Vec2,
    timeline_panning: bool,
    drop_paths: Vec<PathBuf>,
    drop_probe: Option<mpsc::Receiver<Result<Vec<capture_core::video::editor::Segment>, String>>>,
    drop_duration: Option<u64>,
    drop_clips: Vec<capture_core::video::editor::Segment>,
    pub(crate) zoom_draw: bool,
    zoom_edit: bool,
    zoom_focus_drag: Option<zoom_focus::FocusDrag>,
    pub(crate) zoom_from: Option<[f32; 2]>,
    pub(crate) zoom_selected: Option<u64>,
    zoom_drag: Option<zoom::Drag>,
    fx: audio_effects::State,
    voiceover: voiceover::State,
    pub tab: PropertyTab,
    pub object_tab: PropertyTab,
    pub audio_index: usize,
    pub rename: Option<(u64, String, bool)>,
    property_selection: Option<(Selection, Option<u64>)>,
    animation_preview: Option<properties::Preview>,
    animation_clock: Option<Instant>,
    overview: Option<mpsc::Receiver<overview::Overview>>,
    thumbnails: Vec<(u64, egui::TextureHandle)>,
    waves: Vec<Vec<f32>>,
    media_overviews: std::collections::BTreeMap<String, MediaOverview>,
    overview_key: Vec<String>,
    pub playing: bool,
    pub clock: Option<(Instant, u64)>,
    pub selected: Selection,
    pub collapsed: bool,
    pub properties_priority: bool,
    pub snap: bool,
    pub zoom: f32,
    timeline_base: u64,
    timeline_extent: u64,
    pub importing:
        Option<mpsc::Receiver<Result<Vec<capture_core::video::editor::Segment>, String>>>,
    pub import_at: u64,
    pub import_track: u64,
    pub loading: Option<mpsc::Receiver<Result<(Document, PathBuf), String>>>,
    pub worker: Option<workers::Frames>,
    pub source: String,
    pub frame: Option<(u64, Arc<image::RgbaImage>)>,
    pub frame_revision: u64,
    frame_layers: workers::MediaLayers,
    frame_generation: u64,
    preview_density: f32,
    render_project: Option<(Project, [u32; 2])>,
    generation: u64,
    pub last_request: Option<u64>,
    pub audio_stop: Option<Arc<AtomicBool>>,
    pub audio_error: Option<mpsc::Receiver<String>>,
    pub audio_spectrum:
        Option<workers::Latest<Option<(capture_core::video::effects::Target, u64)>>>,
    pub audio_updates: Option<workers::Latest<Project>>,
    pub audio_meters: Option<mpsc::Receiver<workers::Meters>>,
    pub meters: workers::Meters,
    pub export: Option<workers::Export>,
    pub export_progress: f32,
    pub export_dialog: bool,
    pub export_after: [bool; 2],
    pub drag: Option<timeline::Drag>,
    pub active_track: Option<capture_core::video::links::Layer>,
    pub track_rename: Option<(capture_core::video::links::Layer, String, bool)>,
    pub track_drag: Option<capture_core::video::links::Layer>,
    pub delete_track: Option<capture_core::video::links::Layer>,
    pub volume_drag: Option<media_timeline::VolumeDrag>,
    pub record_drag: Option<media_timeline::MediaDrag>,
    frame_drag: Option<frames::Drag>,
    frame_crop: bool,
    pub timing_preview: Option<Project>,
    pub replay_end: Option<u64>,
    pub space_last: Option<Instant>,
    pub space_count: u8,
    pub settings: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            placement_motion: Default::default(),
            still_frame: None,
            selection: Default::default(),
            multi_drag: None,
            marquee: None,
            timeline_offset: egui::Vec2::ZERO,
            timeline_panning: false,
            drop_paths: vec![],
            drop_probe: None,
            drop_duration: None,
            drop_clips: vec![],
            zoom_draw: false,
            zoom_edit: false,
            zoom_focus_drag: None,
            zoom_from: None,
            zoom_selected: None,
            zoom_drag: None,
            fx: Default::default(),
            voiceover: Default::default(),
            tab: PropertyTab::Style,
            object_tab: PropertyTab::Style,
            audio_index: 0,
            rename: None,
            property_selection: None,
            animation_preview: None,
            animation_clock: None,
            overview: None,
            thumbnails: vec![],
            waves: vec![],
            media_overviews: Default::default(),
            overview_key: vec![],
            playing: false,
            clock: None,
            selected: Selection::Objects,
            collapsed: false,
            properties_priority: false,
            snap: true,
            zoom: 1.,
            timeline_base: 0,
            timeline_extent: 0,
            importing: None,
            import_at: 0,
            import_track: 1,
            loading: None,
            worker: None,
            source: String::new(),
            frame: None,
            frame_revision: 0,
            frame_layers: vec![],
            frame_generation: 0,
            preview_density: 1.,
            render_project: None,
            generation: 0,
            last_request: None,
            audio_stop: None,
            audio_error: None,
            audio_updates: None,
            audio_spectrum: None,
            audio_meters: None,
            meters: Default::default(),
            export: None,
            export_progress: 0.,
            export_dialog: false,
            export_after: [false, false],
            drag: None,
            active_track: None,
            track_rename: None,
            track_drag: None,
            delete_track: None,
            volume_drag: None,
            record_drag: None,
            frame_drag: None,
            frame_crop: false,
            timing_preview: None,
            replay_end: None,
            space_last: None,
            space_count: 0,
            settings: false,
        }
    }
}
impl State {
    pub(crate) fn leave_project(&mut self) {
        self.pause();
        self.voiceover = voiceover::State::default();
        self.fx = audio_effects::State::default();
    }
    pub fn pause(&mut self) {
        self.playing = false;
        self.clock = None;
        self.replay_end = None;
        self.audio_updates = None;
        self.audio_spectrum = None;
        self.audio_meters = None;
        if let Some(stop) = self.audio_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.pause();
        if let Some(job) = &self.export {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}
impl Editor {
    pub(crate) fn clear_timeline_selection(&mut self) {
        self.video.selection.clear();
        self.video.selected = Selection::Objects;
    }
    pub(crate) fn sync_video_object_selection(&mut self, id: u64) {
        self.video.selected = Selection::Objects;
        if !self
            .video
            .selection
            .contains(&capture_core::video::editing::Item::Annotation(id))
        {
            self.video.selection.clear();
        }
    }
    pub(crate) fn has_video_selection(&self) -> bool {
        self.is_video()
            && (self.video.selected != Selection::Objects
                || !self.video.selection.is_empty()
                || self.video.zoom_selected.is_some())
    }
    pub(super) fn is_video(&self) -> bool {
        self.session.document.video.is_some()
    }
    pub(super) fn video_time(&self) -> u64 {
        self.session.document.video.as_ref().map_or(0, |v| v.cursor)
    }
    pub(super) fn video_seek(&mut self, time: u64) {
        self.video.pause();
        if let Some(v) = self.session.document.video.as_mut() {
            let time = time.min(14_400_000 - 4000);
            if v.cursor == time {
                return;
            }
            v.cursor = time;
        }
        self.video.last_request = None;
        self.needs_render = true;
    }
    pub(super) fn video_play(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        let Some(v) = self.session.document.video.as_mut() else {
            return;
        };
        if self.video.playing {
            self.video.pause();
            return;
        }
        if v.cursor >= v.duration().saturating_sub(1) {
            v.cursor = 0;
        }
        self.video.playing = true;
        self.video.clock = Some((Instant::now(), v.cursor));
        if v.has_audio() {
            let job = workers::audio(
                v.clone(),
                v.cursor,
                self.quick_controls.audio_outputs().selected().to_owned(),
            );
            self.video.audio_stop = Some(job.stop);
            self.video.audio_error = Some(job.errors);
            self.video.audio_updates = Some(job.updates);
            self.video.audio_spectrum = Some(job.spectrum);
            self.video.audio_meters = Some(job.meters);
        }
    }
    pub(super) fn video_tick(&mut self, ctx: &egui::Context) {
        self.poll_voiceover(ctx);
        self.poll_audio_analysis(ctx);
        if let Some(tx) = &self.video.audio_spectrum {
            let _ = tx.send(if self.video.fx.open && !self.video.fx.analysis_open {
                Some((
                    self.video
                        .fx
                        .target(self.session.document.video.as_ref().unwrap()),
                    self.video.fx.selected,
                ))
            } else {
                None
            });
        }
        if let Some(m) = self
            .video
            .audio_meters
            .as_ref()
            .and_then(|rx| rx.try_iter().last())
        {
            self.video.meters = m;
        }
        self.poll_media_import(ctx);
        if let Some(result) = self
            .video
            .overview
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        {
            self.video.overview = None;
            self.video.thumbnails = result
                .frames
                .into_iter()
                .map(|(at, p)| {
                    (
                        at,
                        ctx.load_texture(
                            "video-filmstrip",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [p.width() as usize, p.height() as usize],
                                p.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        ),
                    )
                })
                .collect();
            self.video.waves = result.waves;
            self.video.media_overviews = result
                .media
                .into_iter()
                .map(|(path, frames, wave)| {
                    let frames = frames
                        .into_iter()
                        .map(|(t, p)| {
                            (
                                t,
                                ctx.load_texture(
                                    "media-thumbnail",
                                    egui::ColorImage::from_rgba_unmultiplied(
                                        [p.width() as usize, p.height() as usize],
                                        p.as_raw(),
                                    ),
                                    egui::TextureOptions::LINEAR,
                                ),
                            )
                        })
                        .collect();
                    (path, (frames, wave))
                })
                .collect();
        }
        if let Some(result) = self.video.loading.as_ref().and_then(|r| r.try_recv().ok()) {
            self.video.loading = None;
            match result {
                Ok((doc, path)) => match Session::new(doc) {
                    Ok(session) => {
                        self.request_action(FileAction::VideoLoaded { session, path }, ctx)
                    }
                    Err(e) => self.status = e.to_string(),
                },
                Err(e) => self.status = e,
            }
        }
        if self.video.loading.is_some() {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        if let Some(e) = self
            .video
            .audio_error
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        {
            self.video.pause();
            self.video.voiceover.review_clock = None;
            if self.video.voiceover.open {
                self.video.voiceover.error = format!("Playback audio: {e}");
            }
            self.status = format!("Playback audio: {e}");
        }
        if let Some(job) = &self.video.export {
            if let Some(p) = job.progress.try_iter().last() {
                self.video.export_progress = p;
            }
            if let Ok(result) = job.result.try_recv() {
                self.video.export = None;
                match result {
                    Ok(path) => {
                        self.status = format!("Video exported to {}", path.display());
                        let after = crate::export_ui::AfterExport {
                            folder: self.video.export_after[0],
                            image: self.video.export_after[1],
                        };
                        if let Err(e) = after.open(&path, true) {
                            self.status.push_str(&format!(" · {e}"));
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
        let Some(v) = self.session.document.video.as_mut() else {
            self.video.pause();
            return;
        };
        if self.video.playing {
            if let Some((start, at)) = self.video.clock {
                v.cursor =
                    (at + start.elapsed().as_millis() as u64).min(v.duration().saturating_sub(1));
                if v.cursor >= v.duration().saturating_sub(1)
                    || self.video.replay_end.is_some_and(|end| v.cursor >= end)
                {
                    self.video.pause();
                }
            }
            ctx.request_repaint_after(Duration::from_millis(1000 / v.fps as u64));
        }
        if self.video.source != v.source {
            self.video.thumbnails.clear();
            self.video.waves.clear();
            self.video.pause();
            self.video.source = v.source.clone();
            self.video.worker = Some(workers::Frames::start(v.source.clone(), ctx.clone()));
            self.video.frame = None;
            self.video.frame_layers.clear();
            self.video.last_request = None;
            self.video.frame_revision += 1;
        }
        let mut overview_key: Vec<_> = v.media_paths().cloned().collect();
        overview_key.sort();
        overview_key.dedup();
        if overview_key != self.video.overview_key {
            self.video.overview_key = overview_key;
            self.video.overview = Some(overview::start(v.clone(), ctx.clone()));
        }
        let render_v = self.video.timing_preview.as_ref().unwrap_or(v).clone();
        let size = [self.session.document.width, self.session.document.height];
        if self
            .video
            .render_project
            .as_ref()
            .is_none_or(|(old, old_size)| !old.same_visual(&render_v) || *old_size != size)
        {
            self.video.render_project = Some((render_v.clone(), size));
            self.video.generation += 1;
            self.video.last_request = None;
        }
        let at = v.cursor;
        if let Some(result) = self
            .video
            .worker
            .as_ref()
            .and_then(|worker| worker.result.try_recv().ok())
        {
            match result {
                Ok((generation, time, pixels, layers)) => {
                    // Styling remains live in the canvas. A decoded source frame
                    // may be older than a slider edit without becoming unusable.
                    self.video.frame = Some((time, Arc::new(pixels)));
                    self.video.frame_layers = layers;
                    self.video.frame_generation = generation;
                    self.video.frame_revision += 1;
                    self.needs_render = true;
                }
                Err(e) => {
                    self.video.pause();
                    self.status = e;
                }
            }
        }
        if self.video.last_request != Some(at)
            && self.video.worker.as_ref().is_some_and(|w| {
                w.request
                    .try_send((
                        render_v.clone(),
                        [self.session.document.width, self.session.document.height],
                        [self.session.document.width, self.session.document.height]
                            .map(|n| (n as f32 * self.video.preview_density).ceil().max(1.) as u32),
                        at,
                        self.video.generation,
                    ))
                    .is_ok()
            })
        {
            self.video.last_request = Some(at);
        }
    }
    pub(super) fn open_video(&mut self, path: PathBuf) {
        self.video.pause();
        self.status = "Opening video…".into();
        self.video.loading = Some(workers::open(path));
    }
    pub(super) fn accept_video(&mut self, session: Session, path: PathBuf) {
        let project = capture_core::project_file::is_video_project(&path);
        self.switch_session(session, None, Some(&path));
        self.reset_document_tools();
        self.shell.return_to_editor();
        self.path = project.then_some(path.clone());
        self.select_only(None);
        self.needs_render = true;
        self.status =
            "Video opened. Place markup at the playhead; drag object edges to set timing.".into();
        self.record_history(
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
            capture_core::history::Origin::Project,
        );
    }
    pub(super) fn commit_video(&mut self, project: Project) {
        let zoom = self
            .video
            .zoom_selected
            .filter(|id| project.zooms.iter().any(|z| z.id == *id));
        let selected = match self.video.selected {
            Selection::Recording(i) | Selection::Audio(i) => self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.segments.get(i))
                .map(|c| (c.id, matches!(self.video.selected, Selection::Audio(_)))),
            _ => None,
        };
        self.execute(Command::Video { project });
        self.video.zoom_selected = zoom;
        if let Some((id, audio)) = selected {
            self.video.selected = self
                .session
                .document
                .video
                .as_ref()
                .and_then(|p| p.segments.iter().position(|c| c.id == id))
                .map(|i| {
                    if audio {
                        Selection::Audio(i)
                    } else {
                        Selection::Recording(i)
                    }
                })
                .unwrap_or_default();
        }
        self.video.last_request = None;
    }
    pub(crate) fn delete_video_selection(&mut self) -> bool {
        use capture_core::video::editing::Item;
        let Some(p) = &self.session.document.video else {
            return false;
        };
        let mut items = self.video.selection.clone();
        if let Selection::Recording(i) | Selection::Audio(i) = self.video.selected
            && let Some(c) = p.segments.get(i)
        {
            items.insert(Item::Media(c.id));
        }
        if let Some(id) = self.video.zoom_selected {
            items.insert(Item::Zoom(id));
        }
        items.extend(
            self.selected_objects()
                .iter()
                .map(|a| Item::Annotation(a.id)),
        );
        if items.is_empty() {
            return false;
        }
        self.execute(Command::VideoDeleteItems { items });
        self.video.selection.clear();
        self.video.selected = Selection::Objects;
        self.video.zoom_selected = None;
        self.select_only(None);
        true
    }
    pub(super) fn video_object_active(&self, id: u64) -> bool {
        self.session.document.video.as_ref().is_none_or(|v| {
            v.objects.get(&id).is_some_and(|t| {
                t.active(v.cursor)
                    && !v.layer_hidden(capture_core::video::links::Layer::Annotation(t.track))
            })
        })
    }
    pub(super) fn video_frame_document(&self) -> Result<Document, String> {
        if self.session.document.video.as_ref().is_some_and(|v| {
            self.video
                .frame
                .as_ref()
                .is_none_or(|(at, _)| *at != v.cursor)
                || self.video.frame_generation != self.video.generation
                || self
                    .video
                    .render_project
                    .as_ref()
                    .is_none_or(|(old, size)| {
                        !old.same_visual(v)
                            || *size != [self.session.document.width, self.session.document.height]
                    })
        }) {
            return still(&self.session.document);
        }
        if self.video.frame.as_ref().is_some_and(|(_, p)| {
            p.dimensions() != (self.session.document.width, self.session.document.height)
        }) {
            return still(&self.session.document);
        }
        let frame = self
            .video
            .frame
            .as_ref()
            .ok_or("Wait for the video frame to load.")?;
        let pixels = if self.video.frame_layers.is_empty() {
            capture_core::video::editor::composite(
                &self.session.document,
                &frame.1,
                self.video_time(),
            )
        } else {
            capture_core::video::frame::composite(
                &self.session.document,
                &self.video.frame_layers,
                self.video_time(),
            )
        }
        .map_err(|e| e.to_string())?;
        Document::from_image(pixels).map_err(|e| e.to_string())
    }
    pub(super) fn paint_video(&mut self, ui: &egui::Ui, t: crate::canvas_view::Transform) -> bool {
        let density = (t.scale * ui.ctx().pixels_per_point()).clamp(0.1, 1.);
        if (density - self.video.preview_density).abs() > 0.01 {
            self.video.preview_density = density;
            self.video.last_request = None;
        }
        if !self.is_video() {
            return false;
        }
        let Some((_, pixels)) = &self.video.frame else {
            return true;
        };
        let mut doc = self.preview_document();
        if let Some(v) = &self.video.timing_preview {
            doc.video = Some(v.clone());
        }
        if self.video.zoom_edit
            && let Some(p) = &mut doc.video
        {
            p.zooms.clear();
            p.layer_order
                .retain(|l| !matches!(l, capture_core::video::links::Layer::Zoom(_)));
        }
        if self.video.zoom_draw
            && let Some(id) = self.video.zoom_selected
            && let Some(p) = &mut doc.video
        {
            p.zooms.retain(|z| z.id != id);
            p.layer_order
                .retain(|layer| *layer != capture_core::video::links::Layer::Zoom(id));
        }
        let project = doc.video.as_ref().unwrap().clone();
        doc.annotations = if pixels.dimensions() == (1, 1) {
            capture_core::video::frame::source_objects(
                &doc,
                &self.video.frame_layers,
                project.cursor,
                self.shell.annotations_hidden,
            )
        } else if self.video.frame_layers.is_empty() {
            if self.shell.annotations_hidden {
                vec![]
            } else {
                capture_core::video::editor::objects_at(&doc, project.cursor)
            }
        } else {
            capture_core::video::frame::ordered_objects(
                &doc,
                &self.video.frame_layers,
                project.cursor,
                self.shell.annotations_hidden,
            )
        };
        doc.video = None;
        doc.object_states.clear();
        doc = capture_core::redaction_labels::decorate(doc, self.shell.redaction_labels)
            .unwrap_or_else(|_| self.preview_document());
        // Transform only the source frame here; retained vector overlays are sampled independently.
        let result = self.display.scene.paint_video(
            ui,
            &doc,
            t,
            self.video.frame_layers.is_empty().then_some(pixels),
            self.video.frame_revision,
            &project,
        );
        if let Err(e) = result {
            self.status = e.to_string();
        }
        ui.painter().with_clip_rect(t.viewport).rect_stroke(
            t.image,
            0.,
            egui::Stroke::new(1., egui::Color32::from_rgb(190, 206, 213)),
            egui::StrokeKind::Inside,
        );
        self.paint_video_frame_handles(ui, t);
        true
    }
    pub(super) fn save_video(&mut self, save_as: bool) -> bool {
        if !self.commit_text() {
            return false;
        }
        self.video.pause();
        let path = self
            .path
            .clone()
            .filter(|p| !save_as && capture_core::project_file::is_video_project(p))
            .or_else(|| {
                self.suspend_global_capture();
                rfd::FileDialog::new()
                    .set_directory(
                        self.dialog_folder(capture_core::quick_preferences::Folder::Project),
                    )
                    .set_file_name("Video clip.vclip")
                    .add_filter("ClipCraft video clip", &["vclip"])
                    .save_file()
                    .map(|mut p| {
                        p.set_extension("vclip");
                        p
                    })
            });
        if let Some(path) = path {
            match self.session.save(&path) {
                Ok(()) => {
                    self.remember_dialog_folder(
                        capture_core::quick_preferences::Folder::Project,
                        &path,
                    );
                    self.path = Some(path.clone());
                    self.status = format!(
                        "Saved {} and its media folder. Keep them together when sharing.",
                        path.display()
                    );
                    return true;
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        false
    }
    pub(super) fn export_video(&mut self) {
        self.video.pause();
        self.video.export_dialog = true;
    }
    pub(super) fn show_video_export(&mut self, ctx: &egui::Context) {
        if self.video.export.is_some() {
            let mut open = true;
            egui::Window::new("Export video")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.add(egui::ProgressBar::new(self.video.export_progress).show_percentage());
                    if ui
                        .add(crate::controls::dialog_button("Cancel", false))
                        .clicked()
                        && let Some(job) = &self.video.export
                    {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                });
            if !open && let Some(job) = &self.video.export {
                job.cancel.store(true, Ordering::Relaxed);
            }
            return;
        }
        if !self.video.export_dialog {
            return;
        }
        let mut confirm = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("video-export")).show(ctx, |ui| {
            ui.set_width(360.);
            ui.heading("Export video");
            if self.session.document.annotations.iter().any(|a| {
                matches!(
                    a.kind,
                    crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate
                )
            }) {
                ui.label(crate::privacy_review::VIDEO_LIMITATION);
            }
            if let Some(v) = &self.session.document.video {
                ui.label(format!(
                    "MP4 · {} × {} · {} fps · {:.2} seconds",
                    self.session.document.width,
                    self.session.document.height,
                    v.fps,
                    v.duration() as f64 / 1000.
                ));
            }
            crate::controls::toggle(ui, &mut self.video.export_after[0], "Open export folder");
            crate::controls::toggle(
                ui,
                &mut self.video.export_after[1],
                "Open video after export",
            );
            let action = crate::controls::dialog_actions(ui, "Export", "Cancel", true, false);
            confirm = action.0;
            if action.1 {
                self.video.export_dialog = false;
            }
        });
        if crate::text_edit::cancel_on_escape(&modal) {
            self.video.export_dialog = false;
        }
        if confirm {
            self.video.export_dialog = false;
            self.suspend_global_capture();
            if let Some(mut path) = rfd::FileDialog::new()
                .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Export))
                .set_file_name("Video.mp4")
                .add_filter("MP4 video", &["mp4"])
                .save_file()
            {
                path.set_extension("mp4");
                self.remember_dialog_folder(capture_core::quick_preferences::Folder::Export, &path);
                self.video.export_progress = 0.;
                self.video.export = Some(workers::export(self.session.document.clone(), path));
            }
        }
    }
}

#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod roundup_tests;

pub(crate) mod property_design;

#[cfg(test)]
mod capture_refinement_tests;
