use super::voiceover_capture::{Capture, Control, Event, Take};
use super::*;
pub(super) mod waveform;
mod window;
type DeviceJob = mpsc::Receiver<Result<Vec<(String, String)>, String>>;
#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    pub position: Option<egui::Pos2>,
    pub wave: waveform::Snapshot,
    pub review_at: u64,
    pub review_with_project: bool,
    pub review_clock: Option<(Instant, u64)>,
    pub target: Option<u64>,
    pub confirm_discard: bool,
    pub device: String,
    pub with_camera: bool,
    pub camera_device: String,
    pub camera_height: u32,
    pub rearm_at: Option<Instant>,
    pub cameras: Vec<(String, String)>,
    pub cameras_job: Option<DeviceJob>,
    pub camera_texture: Option<(Arc<image::RgbaImage>, egui::TextureHandle)>,
    pub review_camera: Option<super::workers::Frames>,
    pub devices: Vec<(String, String)>,
    pub devices_job: Option<DeviceJob>,
    pub monitor: bool,
    pub mono: bool,
    pub from_start: bool,
    pub countdown: u32,
    pub deadline: Option<Instant>,
    pub recording: Option<Instant>,
    pub capture: Option<Capture>,
    pub ready: bool,
    pub waiting: bool,
    pub take: Option<Take>,
    pub at: u64,
    pub error: String,
    pub owner: Option<(String, Option<PathBuf>)>,
}
impl Editor {
    pub(crate) fn open_voiceover(&mut self) {
        if self.video.voiceover.open {
            self.video.tab = PropertyTab::Audio;
            self.shell.properties = true;
            return;
        }
        if !self.is_video() {
            return;
        }
        self.video.pause();
        self.video.voiceover.open = true;
        self.video.voiceover.owner = Some((
            self.session.document.video.as_ref().unwrap().source.clone(),
            self.path.clone(),
        ));
        self.video.voiceover.countdown = 0;
        self.video.voiceover.devices_job = Some(voiceover_capture::devices());
        self.video.voiceover.cameras_job = Some(voiceover_capture::cameras());
        self.video.voiceover.camera_height = 720;
        self.video.voiceover.error.clear();
        self.video.fx.open = false;
        self.video.tab = PropertyTab::Audio;
        self.shell.properties = true;
        self.shell.preview_ui = false;
    }
    fn arm_voiceover(&mut self, ctx: &egui::Context) {
        let state = &mut self.video.voiceover;
        if state.capture.is_some() || state.take.is_some() || !state.error.is_empty() {
            return;
        }
        if state.with_camera && state.camera_device.is_empty() {
            if state.cameras_job.is_none() {
                state.error =
                    "Connect a camera, then refresh the camera list or choose Voice only.".into();
            }
            return;
        }
        let Some(root) = self.history.root() else {
            state.error = "Open the Library storage folder before recording a voiceover.".into();
            return;
        };
        state.capture = Some(Capture::start(
            state.device.clone(),
            state
                .with_camera
                .then(|| (state.camera_device.clone(), state.camera_height)),
            root.join("voiceovers"),
            state.monitor,
            state.mono,
            self.quick_controls.audio_outputs().headphones,
            ctx.clone(),
        ));
        state.ready = false;
    }
    pub(super) fn poll_voiceover(&mut self, ctx: &egui::Context) {
        let owner = self
            .session
            .document
            .video
            .as_ref()
            .map(|v| (v.source.clone(), self.path.clone()));
        if self.video.voiceover.open && self.video.voiceover.owner != owner {
            self.video.voiceover = State::default();
            return;
        }
        if !self.video.voiceover.open {
            return;
        }
        if self
            .video
            .voiceover
            .rearm_at
            .is_some_and(|t| Instant::now() >= t)
        {
            self.video.voiceover.rearm_at = None;
            self.arm_voiceover(ctx);
        }

        if let Some(result) = self
            .video
            .voiceover
            .cameras_job
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        {
            self.video.voiceover.cameras_job = None;
            match result {
                Ok(cameras) => {
                    if !cameras
                        .iter()
                        .any(|c| c.0 == self.video.voiceover.camera_device)
                    {
                        self.video.voiceover.camera_device =
                            cameras.first().map(|c| c.0.clone()).unwrap_or_default();
                    }
                    self.video.voiceover.cameras = cameras;
                    if self.video.voiceover.with_camera && self.video.voiceover.capture.is_none() {
                        self.arm_voiceover(ctx);
                    }
                }
                Err(e) => {
                    if self.video.voiceover.with_camera {
                        self.video.voiceover.error = e;
                    }
                }
            }
        }
        if let Some(result) = self
            .video
            .voiceover
            .devices_job
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        {
            self.video.voiceover.devices_job = None;
            match result {
                Ok(devices) => {
                    self.video.voiceover.devices = devices;
                    self.arm_voiceover(ctx);
                }
                Err(e) => self.video.voiceover.error = e,
            }
        }
        let events: Vec<_> = self
            .video
            .voiceover
            .capture
            .as_ref()
            .map(|c| c.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Ready => self.video.voiceover.ready = true,
                Event::Started => {
                    self.video.voiceover.waiting = false;
                    self.video.voiceover.recording = Some(Instant::now());
                    self.video_seek(self.video.voiceover.at);
                    self.video_play();
                }
                Event::Take(take) => {
                    self.video.voiceover.capture = None;
                    self.video.voiceover.recording = None;
                    self.video.voiceover.waiting = false;
                    self.video.voiceover.wave = take.wave.clone();
                    self.video.voiceover.review_at = 0;
                    self.video.voiceover.take = Some(take);
                    self.video.voiceover.review_camera =
                        Some(super::workers::Frames::start(String::new(), ctx.clone()));
                    self.request_camera_review(0);
                    self.video.pause();
                }
                Event::Error(e) => {
                    self.video.voiceover.error = e;
                    self.video.voiceover.capture = None;
                    self.video.voiceover.recording = None;
                    self.video.voiceover.waiting = false;
                    self.video.voiceover.deadline = None;
                    self.video.pause();
                }
            }
        }
        if self
            .video
            .voiceover
            .deadline
            .is_some_and(|d| Instant::now() >= d)
        {
            self.video.voiceover.deadline = None;
            self.video.voiceover.waiting = true;
            if let Some(c) = &self.video.voiceover.capture {
                let _ = c.send.send(Control::Record);
            }
        }
        if let Some(c) = &self.video.voiceover.capture
            && self.video.voiceover.recording.is_some()
            && let Ok(w) = c.wave.try_lock()
        {
            self.video.voiceover.wave = w.snapshot();
        }
        self.poll_camera_preview(ctx);
        if let Some((clock, start)) = self.video.voiceover.review_clock {
            let end = self.video.voiceover.take.as_ref().map_or(0, |t| t.duration);
            self.video.voiceover.review_at = (start + clock.elapsed().as_millis() as u64).min(end);
            self.request_camera_review(self.video.voiceover.review_at);
            if self.video.voiceover.review_with_project
                && let Some(v) = &mut self.session.document.video
            {
                v.cursor = (self.video.voiceover.at + self.video.voiceover.review_at)
                    .min(v.duration().saturating_sub(1));
            }
            if self.video.voiceover.review_at >= end {
                self.video.pause();
                self.video.voiceover.review_clock = None;
            }
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        if self.video.voiceover.capture.is_some()
            || self.video.voiceover.devices_job.is_some()
            || self.video.voiceover.cameras_job.is_some()
            || self.video.voiceover.rearm_at.is_some()
            || self.video.voiceover.deadline.is_some()
        {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    pub(super) fn voiceover_properties(&mut self, ui: &mut egui::Ui) {
        super::ui::section(ui, "Narration");
        ui.label("Your recorder is open in a movable window.");
        ui.small("Record, review and scrub the waveform, then keep your take.");
        if super::ui::button(
            ui,
            "Voiceover",
            "Center recorder",
            ui.available_width(),
            false,
        )
        .clicked()
        {
            self.video.voiceover.position = None;
        }
    }
    pub(super) fn review_voiceover(&mut self, at: u64, play: bool) {
        self.video.pause();
        self.video.voiceover.review_clock = None;
        let Some(take) = &self.video.voiceover.take else {
            return;
        };
        let take_duration = take.duration;
        let at = at.min(take_duration);
        self.video.voiceover.review_at = at;
        let mut p = self.session.document.video.clone().unwrap();
        p.segments = vec![take_segment(take, 1, 1, 0)];
        p.media_tracks = vec![1];
        p.objects.clear();
        p.zooms.clear();
        p.clip_effects.clear();
        p.media_levels.clear();
        p.media_kinds.clear();
        p.layer_order.clear();
        if self.video.voiceover.review_with_project
            && let Some(v) = &mut self.session.document.video
        {
            v.cursor = (self.video.voiceover.at + at).min(v.duration().saturating_sub(1));
        }
        self.request_camera_review(at);
        self.video.last_request = None;
        self.needs_render = true;
        if play && at < take_duration {
            self.start_audio_preview(p, at);
            self.video.voiceover.review_clock = Some((Instant::now(), at));
        }
    }
    fn reset_voiceover_take(&mut self, ctx: &egui::Context) {
        self.video.pause();
        self.video_seek(self.video.voiceover.at);
        let s = &mut self.video.voiceover;
        s.capture = None;
        s.review_camera = None;
        s.camera_texture = None;
        s.take = None;
        s.deadline = None;
        s.recording = None;
        s.waiting = false;
        s.review_clock = None;
        s.review_at = 0;
        s.wave = Default::default();
        s.error.clear();
        s.confirm_discard = false;
        self.arm_voiceover(ctx);
    }
    fn keep_voiceover(&mut self) -> bool {
        let Some(mut take) = self.video.voiceover.take.take() else {
            return false;
        };
        let Some(mut p) = self.session.document.video.clone() else {
            return false;
        };
        let track = self
            .video
            .voiceover
            .target
            .filter(|t| p.media_tracks.contains(t))
            .unwrap_or_else(|| p.add_media_track());
        p.media_kinds
            .insert(track, capture_core::video::editor::TrackKind::Voiceover);
        let index = p.segments.len();
        let id = p.next_media_id();
        p.segments
            .push(take_segment(&take, id, track, self.video.voiceover.at));
        if let Some(camera) = &take.camera {
            let camera_track = p.add_typed_track(capture_core::video::editor::TrackKind::Video);
            let link = p.next_link();
            p.segments[index].link = Some(link);
            let mut clip = camera_segment(
                camera,
                id + 1,
                camera_track,
                self.video.voiceover.at,
                take.duration,
            );
            clip.link = Some(link);
            p.segments.push(clip);
            let mut order = p.layers();
            order.retain(|l| *l != capture_core::video::links::Layer::Media(camera_track));
            order.insert(0, capture_core::video::links::Layer::Media(camera_track));
            p.layer_order = order;
        }
        p.cursor = self.video.voiceover.at;
        self.commit_video(p);
        if self.session.document.video.as_ref().is_some_and(|p| {
            p.segments.iter().any(|c| {
                c.id == id
                    && c.source
                        .as_ref()
                        .is_some_and(|s| s.path == take.path.to_string_lossy())
            })
        }) {
            take.keep = true;
            self.video.voiceover = State::default();
            self.select_only(None);
            self.video.selected = Selection::Recording(index);
            self.video.tab = PropertyTab::Audio;
            self.set_timeline_expanded(true);
            self.status = if take.camera.is_some() {
                "Narration added. Camera and microphone clips stay linked for timing edits; originals are retained."
            } else {
                "Narration added. The original audio is retained and vocal effects remain editable."
            }.into();
            true
        } else {
            self.video.voiceover.take = Some(take);
            false
        }
    }
}
fn take_segment(take: &Take, id: u64, track: u64, at: u64) -> capture_core::video::editor::Segment {
    use capture_core::video::editor::*;
    let path = take.path.to_string_lossy().into_owned();
    let mut s = Segment::new(
        id,
        at,
        track,
        Some(Source {
            path: path.clone(),
            size: [1, 1],
            duration: take.duration,
            kind: MediaKind::Audio,
        }),
        take.duration,
    );
    s.label = "Narration".into();
    s.audio.push(Audio {
        path,
        label: "Narration".into(),
        volume: 1.,
        muted: false,
        fade_in: 0,
        fade_out: 0,
    });
    s
}

#[cfg(test)]
mod tests;

fn camera_segment(
    camera: &voiceover_capture::CameraTake,
    id: u64,
    track: u64,
    at: u64,
    duration: u64,
) -> capture_core::video::editor::Segment {
    use capture_core::video::editor::*;
    let mut s = Segment::new(
        id,
        at,
        track,
        Some(Source {
            path: camera.path.to_string_lossy().into_owned(),
            size: camera.size,
            duration: camera.duration,
            kind: MediaKind::Video,
        }),
        duration.min(camera.duration),
    );
    s.label = "Camera".into();
    s.rect = [0.69, 0.62, 0.28, 0.32];
    s.frame = Some(Default::default());
    s
}
impl Editor {
    fn request_camera_review(&mut self, at: u64) {
        let Some(camera) = self
            .video
            .voiceover
            .take
            .as_ref()
            .and_then(|t| t.camera.as_ref())
        else {
            return;
        };
        let mut p = Project::new(
            camera.path.to_string_lossy().into_owned(),
            camera.size,
            camera.duration,
            30,
        );
        p.segments[0].audio.clear();
        if let Some(worker) = &self.video.voiceover.review_camera {
            let _ = worker.request.try_send((p, [640, 360], [640, 360], at, 0));
        }
    }
    fn poll_camera_preview(&mut self, ctx: &egui::Context) {
        let mut image = self
            .video
            .voiceover
            .capture
            .as_ref()
            .and_then(|c| c.preview.try_lock().ok().and_then(|p| p.clone()));
        if let Some(Ok((_, _, _, layers))) = self
            .video
            .voiceover
            .review_camera
            .as_ref()
            .and_then(|r| r.result.try_recv().ok())
        {
            image = layers.first().and_then(|(_, b)| b.pixels().ok()).cloned();
        }
        if let Some(pixels) = image
            && self
                .video
                .voiceover
                .camera_texture
                .as_ref()
                .is_none_or(|(old, _)| !Arc::ptr_eq(old, &pixels))
        {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [pixels.width() as usize, pixels.height() as usize],
                pixels.as_raw(),
            );
            let texture = if let Some((_, mut t)) = self.video.voiceover.camera_texture.take() {
                t.set(color, egui::TextureOptions::LINEAR);
                t
            } else {
                ctx.load_texture(
                    "narration-camera-preview",
                    color,
                    egui::TextureOptions::LINEAR,
                )
            };
            self.video.voiceover.camera_texture = Some((pixels, texture));
        }
    }
}
