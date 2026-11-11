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
pub(crate) mod camera;
mod creation;
#[cfg(test)]
mod flow_tests;
mod footer;
#[cfg(test)]
mod footer_tests;
mod frames;
mod hide_tracks;
pub(crate) use hide_tracks::{TrackType, track_kind};
mod gif_audition;
mod gif_studio;
pub(crate) mod imports;
mod jump;
#[cfg(test)]
mod keyframe_tests;
mod keyframes;
#[cfg(windows)]
mod media;
#[cfg(test)]
mod media_tests;
mod media_timeline;
#[cfg(test)]
mod narration_tests;
mod object_menu;
mod overview;
pub(crate) mod path_properties;
mod placement_motion;
#[cfg(test)]
mod precision_tests;
mod privacy;
mod properties;
#[cfg(test)]
mod public_beta_tests;
#[cfg(test)]
mod recording_repair_tests;
#[cfg(test)]
pub(crate) mod repair_tests;
mod selection;
mod still_frame;
#[cfg(test)]
mod tests;
mod timeline;
#[cfg(test)]
mod tip_tests;
mod tracks;
pub(crate) mod ui;
mod voiceover;
pub(crate) mod voiceover_capture;
mod workers;
mod zoom;
mod zoom_focus;
pub(super) use workers::library_picture;
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
/// [`still`] on a thread of its own: the decoder needs a multithreaded COM
/// apartment, which the window thread can never join.
fn still_off_thread(doc: &Document) -> Result<Document, String> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| still(doc))
            .join()
            .unwrap_or_else(|_| Err("The video frame could not be read.".into()))
    })
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
    /// Font, paragraph and fitting for objects that hold text (bh-proposal).
    Text,
    Timing,
    Motion,
    Audio,
}
impl PropertyTab {
    const ALL: [Self; 5] = [
        Self::Style,
        Self::Text,
        Self::Timing,
        Self::Motion,
        Self::Audio,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Text => "Text",
            Self::Timing => "Timing",
            Self::Motion => "Motion",
            Self::Audio => "Audio",
        }
    }
}
type MediaOverview = (Vec<(u64, egui::TextureHandle)>, Vec<f32>);
/// A preview no larger than 1000 pixels, with the shape of the frame.
fn fitted_preview(pixels: &image::RgbaImage) -> image::RgbaImage {
    let (width, height) = pixels.dimensions();
    let scale = (1000. / width.max(height) as f32).min(1.);
    image::imageops::thumbnail(
        pixels,
        (width as f32 * scale).round().max(1.) as u32,
        (height as f32 * scale).round().max(1.) as u32,
    )
}
/// Export video's preview of the watermark on the frame at the playhead.
pub(crate) struct ExportPreview {
    key: (u64, u64),
    base: image::RgbaImage,
    shown: Option<(capture_core::watermark::Watermark, egui::TextureHandle)>,
}
#[cfg(test)]
impl ExportPreview {
    fn showing(&self, mark: &capture_core::watermark::Watermark) -> bool {
        self.shown.as_ref().is_some_and(|(m, _)| m == mark)
    }
}

pub(super) struct State {
    pub(crate) gif: gif_studio::State,
    pub(crate) keyframes: keyframes::State,
    pub(crate) jump: jump::State,
    /// A frame preset chosen this frame, for every selected clip.
    pub(crate) clip_preset: Option<usize>,
    pub(crate) creation: Option<creation::Draft>,
    pub(crate) template_catalog: creation::Catalog,
    pub(crate) privacy: privacy::State,
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
    /// The tab to return to when inline typing ends (typing opens Text).
    text_return: Option<PropertyTab>,
    /// Inline typing was already followed to the Text tab (BH1: open it once).
    typing: bool,
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
    /// Track kinds hidden from the timeline view (view only; never edits the project).
    pub hidden_tracks: std::collections::BTreeSet<TrackType>,
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
    pub export_preview: Option<ExportPreview>,
    /// The timeline keeps the moving playhead in view.
    pub follow_playhead: bool,
    follow_seen: u64,
    follow_held: Option<Instant>,
    /// Where an Alt+drag on the timeline would put its copies.
    pub copy_preview: Option<Project>,
    pub export_after: [bool; 2],
    /// Save the recording as captured instead of rendering the edits.
    pub export_original: bool,
    /// Put the exported MP4 on a PowerPoint slide (approved beta 6 tab 2).
    pub export_slide: bool,
    /// None follows the quality the project was recorded at.
    pub export_quality: Option<capture_core::video::Quality>,
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
            gif: Default::default(),
            keyframes: Default::default(),
            jump: Default::default(),
            clip_preset: None,
            creation: None,
            template_catalog: Default::default(),
            privacy: Default::default(),
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
            text_return: None,
            typing: false,
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
            hidden_tracks: Default::default(),
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
            export_preview: None,
            follow_playhead: true,
            follow_seen: 0,
            follow_held: None,
            copy_preview: None,
            export_after: [false, false],
            export_original: false,
            export_slide: false,
            export_quality: None,
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
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
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
        self.sync_timeline_with_canvas();
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
    pub(super) fn commit_video(&mut self, mut project: Project) {
        // Following zooms track the cursor as it now is (path reduction, trims).
        project.refollow_zooms([self.session.document.width, self.session.document.height]);
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
            return still_off_thread(&self.session.document);
        }
        if self.video.frame_layers.is_empty()
            && self.video.frame.as_ref().is_some_and(|(_, p)| {
                p.dimensions() != (self.session.document.width, self.session.document.height)
            })
        {
            return still_off_thread(&self.session.document);
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
    /// The video frame on screen with its objects, at document size, for color
    /// sampling and palettes (BI6). `None` until a frame has loaded.
    pub(crate) fn video_frame_pixels(&self) -> Option<image::RgbaImage> {
        let frame = self.video.frame.as_ref()?;
        let size = (self.session.document.width, self.session.document.height);
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
        .ok()?;
        (pixels.dimensions() == size).then_some(pixels)
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
        // Labels are optional presentation. A labelling failure must never swap
        // the frame layers for the raw project document, which hides the footage.
        if self.shell.redaction_labels
            && let Ok(labelled) = capture_core::redaction_labels::decorate(doc.clone(), true)
        {
            doc = labelled;
        }
        // Transform only the source frame here; retained vector overlays are sampled independently.
        self.display.scene.interacting = self.edit_drag.is_some()
            || self.group_drag.is_some()
            || self.rotation_drag.is_some()
            || self.video.timing_preview.is_some();
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
        self.paint_keyframe_path(ui, t);
        true
    }
    pub(super) fn save_video(&mut self, save_as: bool) -> bool {
        if !self.commit_text() {
            return false;
        }
        self.video.pause();
        if !save_as && self.refuse_locked_save() {
            return false;
        }
        let mut pack = None;
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
                    .set_file_name(
                        self.shell
                            .untitled_name
                            .as_ref()
                            .map_or_else(|| "Video clip.vclip".into(), |n| format!("{n}.vclip")),
                    )
                    .add_filter("ClipDesigner video clip", &["vclip"])
                    .add_filter(
                        "Pack project (one file with all media)",
                        &[capture_core::pack::EXTENSION],
                    )
                    .save_file()
                    .and_then(|mut p| {
                        if crate::sharing::is_pack(&p) {
                            pack = Some(p);
                            return None;
                        }
                        p.set_extension("vclip");
                        Some(p)
                    })
            });
        if let Some(out) = pack {
            self.pack_project(out);
            return false;
        }
        if let Some(path) = path {
            if self.locked_by_other().is_some() && self.path.as_ref() == Some(&path) {
                self.refuse_locked_save();
                return false;
            }
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
    /// The frame at the playhead with the watermark, built only when the frame,
    /// the edits or the watermark change. It uses the frame already decoded for
    /// the canvas, so nothing is decoded on the UI thread.
    fn export_watermark_preview(&mut self, ui: &mut egui::Ui) {
        let Some(mark) = self.export_preferences.options.watermark.clone() else {
            return;
        };
        crate::watermark_ui::heading(
            ui,
            "Preview",
            "The frame at the playhead with this watermark.",
        );
        let cursor = self.session.document.video.as_ref().map_or(0, |v| v.cursor);
        let key = (cursor, self.video.generation);
        if self
            .video
            .export_preview
            .as_ref()
            .is_none_or(|p| p.key != key)
        {
            let Some(pixels) = self.video_frame_pixels() else {
                ui.weak("The preview appears once the frame at the playhead has loaded.");
                return;
            };
            self.video.export_preview = Some(ExportPreview {
                key,
                base: fitted_preview(&pixels),
                shown: None,
            });
        }
        let preview = self.video.export_preview.as_mut().unwrap();
        if preview.shown.as_ref().is_none_or(|(m, _)| *m != mark) {
            let mut pixels = preview.base.clone();
            // The watermark scales with the output, so the preview matches.
            let _ = capture_core::watermark::apply(&mut pixels, Some(&mark));
            let texture = ui.ctx().load_texture(
                "video-export-preview",
                egui::ColorImage::from_rgba_unmultiplied(
                    [pixels.width() as usize, pixels.height() as usize],
                    pixels.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            preview.shown = Some((mark, texture));
        }
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 170.), egui::Sense::hover());
        let size = egui::vec2(preview.base.width() as f32, preview.base.height() as f32);
        let image = egui::Rect::from_center_size(
            rect.center(),
            size * (rect.width() / size.x).min(rect.height() / size.y),
        );
        if let Some((_, texture)) = &preview.shown {
            ui.painter().image(
                texture.id(),
                image,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        }
    }
    pub(super) fn export_video(&mut self) {
        self.video.pause();
        self.video.export_dialog = true;
    }
    pub(super) fn show_video_export(&mut self, ctx: &egui::Context) {
        use crate::tips::Tip as _;
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
        let mut mark_all = false;
        let mut review_first: Option<capture_core::privacy::tracking::Stop> = None;
        let confirm_id = egui::Id::unique("video-export-mark-all");
        let modal = crate::controls::Modal::new(egui::Id::unique("video-export")).show(ctx, |ui| {
            // Approved bj-proposal layout: summary card, Privacy review,
            // Watermark, After export, then Cancel / Export.
            ui.set_width(crate::watermark_ui::DIALOG_WIDTH);
            // A scrollbar that appears must not widen the dialog.
            ui.spacing_mut().scroll.floating_allocated_width = 0.;
            crate::controls::dialog_title(ui, "Export video");
            if let Some(v) = &self.session.document.video {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(25, 32, 34))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(egui::vec2(52., 30.), egui::Sense::hover());
                            ui.painter()
                                .rect_filled(r, 4., egui::Color32::from_rgb(43, 75, 87));
                            let c = r.center();
                            ui.painter().add(egui::Shape::convex_polygon(
                                vec![
                                    c + egui::vec2(-4., -6.),
                                    c + egui::vec2(6., 0.),
                                    c + egui::vec2(-4., 6.),
                                ],
                                egui::Color32::from_rgb(207, 230, 238),
                                egui::Stroke::NONE,
                            ));
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}.mp4", self.project_name()))
                                        .strong(),
                                );
                                let ms = v.duration();
                                ui.label(
                                    egui::RichText::new(format!(
                                        "MP4 · {} × {} · {} fps · {}:{:04.1}",
                                        self.session.document.width,
                                        self.session.document.height,
                                        v.fps,
                                        ms / 60_000,
                                        (ms % 60_000) as f64 / 1000.
                                    ))
                                    .monospace()
                                    .size(11.5)
                                    .color(egui::Color32::from_rgb(142, 154, 157)),
                                );
                            });
                        });
                    });
            }
            let body = (ui.ctx().content_rect().height() - 190.).max(240.);
            let original = egui::ScrollArea::vertical()
                .id_salt("export-video-body")
                .max_height(body)
                .auto_shrink([false, true])
                .show(ui, |ui| {
            // Room for the scrollbar, whether or not it is showing.
            ui.set_max_width(ui.available_width() - 13.);
            let recording = self.recording_file();
            if recording.is_none() {
                self.video.export_original = false;
            }
            if recording.is_some()
                && let Some(edited) = crate::backstage::segmented(
                    ui,
                    ["Original capture", "Edited Capture"],
                    !self.video.export_original,
                )
            {
                self.video.export_original = !edited;
            }
            let original = self.video.export_original;
            if original {
                ui.label(
                    "Saves a copy of the recording as it was captured, straight away and at its full quality. Edits are not included.",
                );
            } else if let Some(v) = &self.session.document.video {
                use capture_core::video::{Quality, size_text};
                crate::watermark_ui::heading(
                    ui,
                    "Quality",
                    "Match recording keeps the quality you recorded at. A lower choice makes a smaller file.",
                );
                let choice = self
                    .video
                    .export_quality
                    .map_or(0, |q| 1 + Quality::ALL.iter().position(|a| *a == q).unwrap_or(0));
                if let Some(index) = crate::backstage::segmented_choice(
                    ui,
                    &["Match recording", "Standard", "High", "Maximum"],
                    choice,
                ) {
                    self.video.export_quality = index.checked_sub(1).map(|i| Quality::ALL[i]);
                }
                let size = size_text(
                    self.video
                        .export_quality
                        .or(v.recorded_quality)
                        .unwrap_or_default()
                        .megabytes(v.fps, v.duration()),
                );
                let estimate = match (self.video.export_quality, v.recorded_quality) {
                    (None, Some(quality)) => {
                        format!("Recorded at {}. About {size} for this video.", quality.label())
                    }
                    _ => format!("About {size} for this video."),
                };
                // The slide switch shares the estimate's line, so the dialog
                // keeps its height on small screens.
                let slide = &mut self.video.export_slide;
                ui.horizontal(|ui| {
                    ui.weak(estimate);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        crate::controls::toggle(ui, slide, "On a PowerPoint slide")
                            .tip("On a PowerPoint slide · Saves a .pptx with the video on a slide, its first frame as the poster. Click the video on the slide to play it.");
                    });
                });
                if self.can_copy_for_export() && !self.video.export_slide {
                    ui.weak("No edits to render, so this saves straight away.");
                }
            }
            let pending = capture_core::privacy::tracking::pending_reviews(&self.session.document);
            let stops = capture_core::privacy::tracking::review_stops(&self.session.document);
            let obscured = self.session.document.annotations.iter().any(|a| {
                matches!(
                    a.kind,
                    crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate
                )
            });
            if original {
                let warning = egui::Color32::from_rgb(242, 201, 76);
                if obscured || !pending.is_empty() {
                    ui.colored_label(warning, "This copy shows everything in the recording. Your redactions are not applied.");
                }
                if self
                    .session
                    .document
                    .video
                    .as_ref()
                    .is_some_and(|v| v.has_recorded_cursor())
                {
                    ui.colored_label(warning, "The mouse pointer was recorded separately, so it isn't in this copy.");
                }
            } else if obscured || !pending.is_empty() {
                crate::watermark_ui::heading(
                    ui,
                    "Privacy review",
                    crate::privacy_review::VIDEO_LIMITATION,
                );
                let warn = !pending.is_empty();
                let (fill, stroke, dot) = if warn {
                    (
                        egui::Color32::from_rgb(42, 38, 24),
                        egui::Color32::from_rgb(92, 79, 34),
                        egui::Color32::from_rgb(242, 201, 76),
                    )
                } else {
                    (
                        egui::Color32::from_rgb(24, 42, 32),
                        egui::Color32::from_rgb(45, 90, 62),
                        egui::Color32::from_rgb(99, 197, 143),
                    )
                };
                let time = |ms: u64| format!("{}:{:04.1}", ms / 60_000, (ms % 60_000) as f64 / 1000.);
                let mut confirming = ui.ctx().data(|d| d.get_temp::<bool>(confirm_id)).unwrap_or(false) && warn;
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1., stroke))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(8., 8.), egui::Sense::hover());
                            ui.painter().circle_filled(r.center(), 4., dot);
                            ui.label(egui::RichText::new(if warn {
                                format!(
                                    "{} tracked redaction{} need{} review",
                                    pending.len(),
                                    if pending.len() == 1 { "" } else { "s" },
                                    if pending.len() == 1 { "s" } else { "" }
                                )
                            } else {
                                "Redactions reviewed".into()
                            })
                            .strong());
                        });
                        if !warn {
                            ui.weak("Editing a redaction clears its approval.");
                            return;
                        }
                        if !confirming {
                            egui::ScrollArea::vertical()
                                .id_salt("export-pending")
                                .max_height(REVIEW_LIST_HEIGHT)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    for p in &pending {
                                        ui.horizontal_top(|ui| {
                                            let room = (ui.available_width() - 56.).max(40.);
                                            ui.allocate_ui(egui::vec2(room, 20.), |ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "{} · {}–{}",
                                                            p.kind,
                                                            time(p.start),
                                                            time(p.end)
                                                        ))
                                                        .monospace()
                                                        .size(11.),
                                                    );
                                                    for (a, b) in &p.uncertain {
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "Needs review {}–{}",
                                                                time(*a),
                                                                time(*b)
                                                            ))
                                                            .size(10.5)
                                                            .color(egui::Color32::from_rgb(242, 201, 76)),
                                                        );
                                                    }
                                                });
                                            });
                                            if ui
                                                .button("Show")
                                                .tip("Close this and go to this redaction's first flagged moment.")
                                                .clicked()
                                            {
                                                review_first =
                                                    stops.iter().find(|s| s.id == p.id).copied();
                                            }
                                        });
                                    }
                                });
                        }
                        let uncertain: Vec<String> = pending
                            .iter()
                            .flat_map(|p| {
                                p.uncertain
                                    .iter()
                                    .map(move |(a, b)| format!("{} {}–{}", p.kind, time(*a), time(*b)))
                            })
                            .collect();
                        if !confirming {
                            ui.horizontal(|ui| {
                                if ui
                                    .button(
                                        egui::RichText::new("Mark all as reviewed…")
                                            .color(egui::Color32::from_rgb(242, 201, 76)),
                                    )
                                    .tip("Approve every tracked redaction for export after checking them.")
                                    .clicked()
                                {
                                    confirming = true;
                                }
                                if ui
                                    .button("Review in the editor")
                                    .tip("Close this and go to the earliest flagged moment. Previous and Next in Properties step through the rest.")
                                    .clicked()
                                {
                                    review_first = stops.first().copied();
                                }
                            });
                        } else {
                            ui.separator();
                            let mut text = format!(
                                "Mark all {} as reviewed? Automatic tracking can miss text.",
                                pending.len()
                            );
                            if !uncertain.is_empty() {
                                text.push_str(&format!(
                                    " {} flagged range{} will be approved too.",
                                    uncertain.len(),
                                    if uncertain.len() == 1 { "" } else { "s" },
                                ));
                            }
                            text.push_str(" You can undo this.");
                            ui.label(egui::RichText::new(text).size(12.));
                            if !uncertain.is_empty() {
                                egui::ScrollArea::vertical()
                                    .id_salt("export-flagged")
                                    .max_height(REVIEW_LIST_HEIGHT)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        for range in &uncertain {
                                            ui.label(egui::RichText::new(range).monospace().size(11.));
                                        }
                                    });
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add(crate::controls::dialog_button("Mark all as reviewed", true))
                                    .clicked()
                                {
                                    mark_all = true;
                                    confirming = false;
                                }
                                if ui.add(crate::controls::dialog_button("Cancel", false)).clicked() {
                                    confirming = false;
                                }
                            });
                        }
                    });
                ui.ctx().data_mut(|d| d.insert_temp(confirm_id, confirming));
            }
            self.export_watermark_preview(ui);
            if !self.export_preferences.watermark_ready {
                self.export_preferences.watermark_ready = true;
                self.export_preferences.options.watermark = self.quick_controls.default_watermark();
            }
            if !original {
                let presets = self.quick_controls.watermarks();
                if let crate::watermark_ui::Outcome::Presets(list, default) =
                    crate::watermark_ui::section(
                        ui,
                        &mut self.export_preferences.options.watermark,
                        &presets.0,
                        &presets.1,
                    )
                {
                    self.quick_controls.remember_watermarks(list, default);
                }
            }
            crate::watermark_ui::heading(ui, "After export", "What ClipDesigner does when the video is saved.");
            crate::controls::toggle(ui, &mut self.video.export_after[0], "Open export folder");
            crate::controls::toggle(ui, &mut self.video.export_after[1], "Open video after export");
            original
                })
                .inner;
            ui.add_space(4.);
            ui.separator();
            let review = capture_core::privacy::tracking::require_review(&self.session.document);
            if review.is_err() && !original {
                ui.weak("Review the redactions to export");
            }
            let action = crate::controls::dialog_actions(
                ui,
                if original { "Save copy" } else { "Export" },
                "Cancel",
                original || review.is_ok(),
                false,
            );
            confirm = action.0;
            if action.1 {
                self.video.export_dialog = false;
            }
        });
        #[cfg(test)]
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("video-export-rect"), modal.response.rect));
        if mark_all && let Some(mut project) = self.session.document.video.clone() {
            let n = capture_core::privacy::tracking::mark_all_reviewed(&mut project);
            if n > 0 {
                self.execute(crate::Command::Video { project });
                self.status = format!(
                    "Marked {n} tracked redaction{} as reviewed. Ctrl+Z undoes it.",
                    if n == 1 { "" } else { "s" }
                );
            }
        }
        if let Some(stop) = review_first {
            self.video.export_dialog = false;
            ctx.data_mut(|d| d.remove::<bool>(confirm_id));
            self.show_review_stop(stop);
        }
        if crate::text_edit::cancel_on_escape(&modal) {
            self.video.export_dialog = false;
        }
        if confirm {
            self.video.export_dialog = false;
            self.suspend_global_capture();
            let slide = self.video.export_slide && !self.video.export_original;
            if slide {
                if let Some(mut path) = rfd::FileDialog::new()
                    .set_directory(
                        self.dialog_folder(capture_core::quick_preferences::Folder::Export),
                    )
                    .set_file_name("Video.pptx")
                    .add_filter("PowerPoint", &["pptx"])
                    .save_file()
                {
                    path.set_extension("pptx");
                    self.remember_dialog_folder(
                        capture_core::quick_preferences::Folder::Export,
                        &path,
                    );
                    self.video.export_progress = 0.;
                    self.video.export = Some(workers::export_slide(
                        self.session.document.clone(),
                        path,
                        self.export_preferences.options.watermark.clone(),
                        self.export_bitrate(),
                        self.export_preferences.office.slides.size,
                    ));
                }
                return;
            }
            let copy = (self.video.export_original || self.can_copy_for_export())
                .then(|| self.recording_file())
                .flatten();
            let extension = copy
                .as_ref()
                .and_then(|p| p.extension())
                .map_or("mp4".to_owned(), |e| {
                    e.to_string_lossy().to_ascii_lowercase()
                });
            if let Some(mut path) = rfd::FileDialog::new()
                .set_directory(self.dialog_folder(capture_core::quick_preferences::Folder::Export))
                .set_file_name(format!("Video.{extension}"))
                .add_filter("Video", &[extension.as_str()])
                .save_file()
            {
                path.set_extension(&extension);
                self.remember_dialog_folder(capture_core::quick_preferences::Folder::Export, &path);
                self.video.export_progress = 0.;
                self.video.export = Some(match copy {
                    Some(source) => workers::save_copy(source, path),
                    None => workers::export(
                        self.session.document.clone(),
                        path,
                        self.export_preferences.options.watermark.clone(),
                        self.export_bitrate(),
                    ),
                });
            }
        }
    }
    /// The recording file behind this project, when it is there to copy.
    pub(super) fn recording_file(&self) -> Option<std::path::PathBuf> {
        let path = self.session.document.video.as_ref()?.recording_path()?;
        let path = std::path::PathBuf::from(path);
        path.is_file().then_some(path)
    }
    /// Edited Capture with nothing to render saves the recording itself.
    fn can_copy_for_export(&self) -> bool {
        let d = &self.session.document;
        self.video.export_quality.is_none()
            && self.export_preferences.options.watermark.is_none()
            && d.video
                .as_ref()
                .is_some_and(|v| v.is_unedited([d.width, d.height], d.annotations.len()))
            && self
                .recording_file()
                .is_some_and(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")))
    }
    fn export_bitrate(&self) -> u32 {
        let project = self.session.document.video.as_ref();
        self.video
            .export_quality
            .or(project.and_then(|v| v.recorded_quality))
            .unwrap_or_default()
            .bitrate(project.map_or(30, |v| v.fps))
    }
}

/// The flagged lists in Export video scroll at this height, so its buttons
/// stay on screen however many redactions a project has.
const REVIEW_LIST_HEIGHT: f32 = 104.;

#[cfg(test)]
mod dialog_gallery_tests;
#[cfg(test)]
mod help_gallery_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod roundup_tests;

pub(crate) mod property_design;

#[cfg(test)]
mod capture_refinement_tests;
