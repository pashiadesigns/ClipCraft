use crate::tips::Tip as _;
use crate::{Editor, FileAction, egui};
use capture_core::capture_options::{Selection, Settings};
use egui::{Color32, Stroke};

/// Cameras (id, name) or the reason they could not be listed.
type CameraList = Result<Vec<(String, String)>, String>;
#[derive(Default)]
pub(super) struct State {
    pub recording_target: u8,
    pub recording_project: Option<capture_core::video::editor::Project>,
    pub active: Option<Settings>,
    pub pending_video: Option<crate::capture::VideoRequest>,
    pub video_dialog: bool,
    pub video_options: Option<capture_core::video::Options>,
    pub video_error: String,
    pub scroll_mode: Option<std::sync::mpsc::Receiver<bool>>,
    pub restore_focus: Option<bool>,
    /// Capture Video dialog tab: false = General, true = Step by Step.
    pub video_steps_tab: bool,
    pub clip_dialog: bool,
    pub clip_options: Option<Settings>,
    /// Cameras for the Webcam option (id, name), listed on a worker (AS13).
    pub cameras: Option<CameraList>,
    pub camera_job: Option<std::sync::mpsc::Receiver<CameraList>>,
    /// The image frame the capture in progress goes into (From Capture).
    pub frame_target: Option<u64>,
}
const SECTION: Color32 = Color32::from_rgb(142, 154, 157);
const LINE: Color32 = Color32::from_rgb(52, 61, 64);
const SELECTED: Color32 = Color32::from_rgb(35, 65, 74);
const TILE: Color32 = Color32::from_rgb(36, 43, 46);
fn section(ui: &mut egui::Ui, text: &str) {
    crate::controls::dialog_section(ui, text, None);
}
/// A capture-area tile or an icon toggle: icon above a label, highlighted when on.
fn icon_tile(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    icon: &str,
    label: &str,
    on: bool,
    locked: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        size,
        if locked {
            egui::Sense::hover()
        } else {
            egui::Sense::click()
        },
    );
    let p = ui.painter();
    let hovered = response.hovered() && !locked;
    p.rect_filled(
        rect,
        8.,
        if on {
            SELECTED
        } else if hovered {
            Color32::from_rgb(44, 52, 56)
        } else {
            TILE
        },
    );
    p.rect_stroke(
        rect,
        8.,
        Stroke::new(1., if on { crate::shell::ACCENT } else { LINE }),
        egui::StrokeKind::Inside,
    );
    let color = if on {
        Color32::from_rgb(125, 214, 237)
    } else {
        Color32::from_gray(150)
    };
    crate::video_editor::ui::icon(
        p,
        egui::Rect::from_center_size(rect.center() - egui::vec2(0., 9.), egui::vec2(20., 20.)),
        icon,
        color,
    );
    p.text(
        rect.center() + egui::vec2(0., 13.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        if on {
            Color32::WHITE
        } else {
            Color32::from_gray(170)
        },
    );
    response
}
fn card(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .stroke(Stroke::new(1., LINE))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}
impl Editor {
    /// Camera, shape, size and corner for the webcam, with a placement preview
    /// (approved mockup rtifacts/bg-proposal, tab 2).
    fn webcam_card(&mut self, ui: &mut egui::Ui, webcam: &mut capture_core::video::webcam::Webcam) {
        use capture_core::video::webcam::{Corner, Shape, Size};
        if self.clip.cameras.is_none() && self.clip.camera_job.is_none() {
            self.clip.camera_job = Some(crate::video_editor::voiceover_capture::cameras());
        }
        if let Some(job) = &self.clip.camera_job {
            match job.try_recv() {
                Ok(result) => {
                    self.clip.cameras = Some(result);
                    self.clip.camera_job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint(),
                Err(_) => self.clip.camera_job = None,
            }
        }
        card(ui, |ui| {
            ui.horizontal(|ui| {
                crate::property_layout::field_label(ui, "Camera", 28.);
                match &self.clip.cameras {
                    Some(Ok(list)) if !list.is_empty() => {
                        let name = list
                            .iter()
                            .find(|(id, _)| *id == webcam.device)
                            .or(list.first())
                            .map_or(String::new(), |(_, n)| n.clone());
                        egui::ComboBox::from_id_salt("webcam-device")
                            .selected_text(name)
                            .width(ui.available_width() - 8.)
                            .show_ui(ui, |ui| {
                                for (id, name) in list {
                                    ui.selectable_value(&mut webcam.device, id.clone(), name);
                                }
                            });
                    }
                    Some(Ok(_)) => {
                        ui.colored_label(Color32::LIGHT_RED, "No camera found");
                    }
                    Some(Err(e)) => {
                        ui.colored_label(Color32::LIGHT_RED, e);
                    }
                    None => {
                        ui.weak("Looking for cameras…");
                    }
                }
            });
            ui.horizontal(|ui| {
                crate::property_layout::field_label(ui, "Shape", 28.);
                crate::controls::segments(ui, |ui| {
                    for s in Shape::ALL {
                        ui.selectable_value(&mut webcam.shape, s, s.label());
                    }
                });
            });
            ui.horizontal(|ui| {
                crate::property_layout::field_label(ui, "Size", 28.);
                crate::controls::segments(ui, |ui| {
                    for s in Size::ALL {
                        ui.selectable_value(&mut webcam.size, s, s.label());
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                crate::property_layout::field_label(ui, "Corner", 28.);
                crate::controls::segments(ui, |ui| {
                    for c in Corner::ALL {
                        ui.selectable_value(&mut webcam.corner, c, c.label());
                    }
                });
            });
            // Placement preview on a 16:9 screen.
            let width = ui.available_width();
            let (screen, _) = ui.allocate_exact_size(
                egui::vec2(width, width * 9. / 16. * 0.6),
                egui::Sense::hover(),
            );
            let screen = egui::Rect::from_center_size(
                screen.center(),
                egui::vec2(screen.height() * 16. / 9., screen.height()),
            );
            let p = ui.painter();
            p.rect_filled(screen, 6., Color32::from_rgb(29, 42, 51));
            p.rect_stroke(screen, 6., Stroke::new(1., LINE), egui::StrokeKind::Inside);
            let r = webcam.rect([1920, 1080]);
            let cam = egui::Rect::from_min_size(
                screen.min + egui::vec2(r[0] * screen.width(), r[1] * screen.height()),
                egui::vec2(r[2] * screen.width(), r[3] * screen.height()),
            );
            let radius = match webcam.shape {
                Shape::Circle => cam.width() * 0.5,
                Shape::Rounded => cam.width() * 0.18,
                Shape::Square => 1.,
            };
            p.rect_filled(cam, radius, Color32::from_rgb(90, 109, 120));
            p.circle_filled(
                cam.center() - egui::vec2(0., cam.height() * 0.1),
                cam.width() * 0.17,
                Color32::from_rgb(214, 183, 154),
            );
            p.rect_stroke(
                cam,
                radius,
                Stroke::new(2., Color32::WHITE),
                egui::StrokeKind::Inside,
            );
        });
        ui.weak("The webcam is recorded as its own video track. You can move, resize, crop or hide it afterwards.");
    }
    pub(super) fn open_video_capture(&mut self) {
        self.clip.recording_target = u8::from(self.is_video());
        self.clip.video_options = Some(self.quick_controls.video_settings());
        self.clip.video_dialog = true;
        self.clip.video_error.clear();
    }
    pub(super) fn video_capture_dialog(&mut self, ctx: &egui::Context) {
        if !self.clip.video_dialog {
            return;
        }
        let mut options = self
            .clip
            .video_options
            .take()
            .unwrap_or_else(|| self.quick_controls.video_settings());
        let mut start = false;
        let mut cancel = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("capture-video-options")).show(
            ctx,
            |ui| {
                // Approved mockup: a fixed-size dialog with General and Step by Step
                // tabs; only the tab content scrolls, and only if it overflows.
                ui.set_width(470.);
                ui.label(crate::panel_ui::title("Capture Video", 20.));
                ui.add_space(8.);
                // BA5: Properties-pane tab style with room between the tabs.
                ui.horizontal(|ui| {
                    if let Some(index) = dialog_tabs(ui, &["General", "Step by Step"], usize::from(self.clip.video_steps_tab)) {
                        self.clip.video_steps_tab = index == 1;
                    }
                    if options.instruction.enabled {
                        let badge = ui.label(egui::RichText::new(" Step by Step on ").size(10.).color(Color32::from_rgb(125, 214, 237)).background_color(SELECTED));
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("capture-steps-badge"), badge.rect));
                        let _ = badge;
                    }
                });
                ui.add_space(4.);
                let content_h = (ctx.content_rect().height() - 220.).clamp(200., 420.);
                egui::ScrollArea::vertical()
                    .id_salt(("capture-video-settings", self.clip.video_steps_tab))
                    .max_height(content_h)
                    .min_scrolled_height(content_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if !self.clip.video_steps_tab {
                            // General follows the recording sequence.
                            section(ui, "Capture area");
                            ui.horizontal(|ui| {
                                let w = (ui.available_width() - 2. * ui.spacing().item_spacing.x) / 3.;
                                for selection in [Selection::Region, Selection::Window, Selection::Fullscreen] {
                                    if icon_tile(ui, egui::vec2(w, 64.), selection.label(), selection.label(), options.selection == selection, false)
                                        .clicked()
                                    {
                                        options.selection = selection;
                                    }
                                }
                            });
                            if options.selection == Selection::Window {
                                crate::controls::toggle(ui, &mut options.pause_on_focus_loss, "Pause when the selected window loses focus");
                            }
                            section(ui, "Recording");
                            ui.horizontal(|ui| {
                                ui.label("Frame rate");
                                crate::controls::segments(ui, |ui| {
                                    for fps in [15, 30, 60] {
                                        ui.selectable_value(&mut options.fps, fps, format!("{fps} fps"));
                                    }
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.weak("MP4 · up to 1080p");
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Quality")
                                    .tip("Higher quality keeps fast motion and fine detail, and makes larger files.");
                                crate::controls::segments(ui, |ui| {
                                    for quality in capture_core::video::Quality::ALL {
                                        ui.selectable_value(&mut options.quality, quality, quality.label());
                                    }
                                });
                            });
                            ui.weak(format!(
                                "{} About {} per minute.",
                                options.quality.purpose(),
                                capture_core::video::size_text(options.quality.megabytes(options.fps, 60_000))
                            ));
                            if let Some(minutes) = options.quality.limit_minutes(options.fps) {
                                ui.weak(format!("Recordings stop at 4 GB, about {minutes} minutes at this setting."));
                            }
                            crate::controls::toggle(ui, &mut options.countdown, "3-second countdown");
                            section(ui, "Include");
                            ui.horizontal(|ui| {
                                let size = egui::vec2(88., 58.);
                                if icon_tile(ui, size, "Microphone", "Microphone", options.microphone, false)
                                    .tip("Record your default Windows microphone")
                                    .clicked()
                                {
                                    options.microphone = !options.microphone;
                                }
                                if icon_tile(ui, size, "System audio", "System audio", options.system_audio, false)
                                    .tip("Record sound playing on this PC (default playback device)")
                                    .clicked()
                                {
                                    options.system_audio = !options.system_audio;
                                }
                                let locked = options.instruction.enabled;
                                if icon_tile(ui, size, "Selection", "Cursor", options.cursor || locked, locked)
                                    .tip(if locked {
                                        "Always included while Step by Step video is on"
                                    } else {
                                        "Record the mouse pointer as an editable cursor object"
                                    })
                                    .clicked()
                                {
                                    options.cursor = !options.cursor;
                                }
                                let webcam = icon_tile(ui, size, "Webcam", "Webcam", options.webcam.enabled, false)
                                    .tip("Record your webcam as its own picture-in-picture track");
                                #[cfg(test)]
                                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("webcam-tile"), webcam.rect));
                                if webcam.clicked() {
                                    options.webcam.enabled = !options.webcam.enabled;
                                }
                            });
                            if options.webcam.enabled {
                                self.webcam_card(ui, &mut options.webcam);
                            }
                            section(ui, "After recording");
                            let destinations = ["Open as new project", "Append to this project", "Add on a new track at playhead"];
                            if self.is_video() {
                                egui::ComboBox::from_id_salt("recording-destination")
                                    .selected_text(destinations[self.clip.recording_target as usize])
                                    .width(ui.available_width() - 8.)
                                    .show_ui(ui, |ui| {
                                        for (i, label) in destinations.iter().enumerate() {
                                            ui.selectable_value(&mut self.clip.recording_target, i as u8, *label);
                                        }
                                    });
                            } else {
                                self.clip.recording_target = 0;
                                ui.weak("Opens as a new video project");
                            }
                            ui.horizontal(|ui| {
                                let size = egui::vec2(88., 58.);
                                if icon_tile(ui, size, "Open editor", "Open editor", options.preview, false)
                                    .tip("Open the recording in ClipDesigner, on the Insert tab")
                                    .clicked()
                                {
                                    options.preview = !options.preview;
                                }
                                if icon_tile(ui, size, "Folder", "Open folder", options.open_folder, false)
                                    .tip("Show the recording file in File Explorer")
                                    .clicked()
                                {
                                    options.open_folder = !options.open_folder;
                                }
                            });
                            crate::controls::toggle(ui, &mut options.find_sensitive, "Find sensitive information")
                                .tip("Open Find Sensitive Info when the recording arrives in the editor.");
                        } else {
                            ui.add_space(4.);
                            crate::controls::toggle(ui, &mut options.instruction.enabled, "Build an editable instructional video");
                            if options.instruction.enabled {
                                options.cursor = true;
                            }
                            ui.add_enabled_ui(options.instruction.enabled, |ui| {
                                section(ui, "Cursor");
                                card(ui, |ui| {
                                    ui.label(egui::RichText::new("Path reduction").weak());
                                    crate::video_editor::path_properties::reduction_picker(ui, "recording", &mut options.instruction.cursor_reduction, true);
                                });
                                section(ui, "Zooms");
                                card(ui, |ui| {
                                    crate::controls::toggle(ui, &mut options.instruction.zoom_clicks, "Automatic click zooms");
                                    ui.add_enabled_ui(options.instruction.zoom_clicks, |ui| {
                                        let mut seconds = options.instruction.zoom_cooldown_ms as f32 / 1000.;
                                        if crate::property_layout::slider(ui, "Cooldown", egui::Slider::new(&mut seconds, 0.0..=10.0).suffix(" s"))
                                            .tip("Clicks closer together than this keep the zoom in and pan to the next click instead of zooming out and back in.")
                                            .changed()
                                        {
                                            options.instruction.zoom_cooldown_ms = (seconds * 1000.).round() as u64;
                                        }
                                        crate::controls::toggle(ui, &mut options.instruction.zoom_follow_cursor, "Follow the cursor between clicks")
                                            .tip("While zoomed between nearby clicks, the view pans with the cursor so viewers keep their place.");
                                    });
                                    ui.add_enabled_ui(options.webcam.enabled, |ui| {
                                        crate::controls::toggle(ui, &mut options.webcam.steady, "Keep the webcam in place during zooms")
                                            .tip("The webcam sits above the zoom tracks, so click zooms enlarge the screen but not your face. Turn it off to zoom both together.")
                                            .tip_disabled("Turn on Webcam on the General tab first");
                                    });
                                });
                                section(ui, "Cues");
                                card(ui, |ui| {
                                    crate::controls::toggle(ui, &mut options.instruction.click_highlights, "Click highlights");
                                    crate::controls::toggle(ui, &mut options.instruction.captions, "Instruction captions");
                                    });
                                ui.weak("Everything stays editable on the timeline. Up to 1,000 click cues and 64 automatic zooms per recording.");
                            });
                        }
                        if !self.clip.video_error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, &self.clip.video_error);
                        }
                    });
                crate::panel_ui::divider(ui);
                let summary = [
                    Some(options.selection.label().to_string()),
                    Some(format!("{} fps", options.fps)),
                    options.microphone.then(|| "mic".to_string()),
                    options.system_audio.then(|| "system audio".to_string()),
                    options.webcam.enabled.then(|| "webcam".to_string()),
                    options.instruction.enabled.then(|| "Step by Step".to_string()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                ui.add(egui::Label::new(egui::RichText::new(summary).size(12.).color(SECTION)).truncate());
                (start, cancel) =
                    crate::controls::dialog_actions(ui, "Record", "Cancel", true, false);
            },
        );
        if options != self.quick_controls.video_settings() {
            self.quick_controls.remember_video(options.clone());
        }
        self.clip.video_options = Some(options.clone());
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            self.clip.video_dialog = false;
            return;
        }
        if start {
            self.clip.recording_project = self.session.document.video.clone();
            match self.managed_recording_path() {
                Ok(path) => {
                    let mut settings = self.quick_controls.capture_for(options.selection);
                    settings.cursor = options.cursor;
                    settings.scrolling = false;
                    settings.delay_seconds = 0;
                    settings.clipboard = false;
                    settings.preview = true;
                    self.clip.video_dialog = false;
                    self.request_action(
                        FileAction::StartVideo(
                            settings,
                            crate::capture::VideoRequest {
                                destination: path,
                                options,
                            },
                        ),
                        ctx,
                    );
                }
                Err(e) => self.clip.video_error = e,
            }
        }
    }
    pub(super) fn accept_capture_result(
        &mut self,
        captured: crate::capture::Captured,
        settings: Settings,
        copy: impl FnOnce(&capture_core::Document) -> Result<(), String>,
    ) {
        let crate::capture::Captured {
            document,
            warning,
            retained,
            video,
        } = captured;
        if let Some(video) = video {
            self.status = format!(
                "Video saved · {} seconds · {}. {}",
                video.seconds,
                video.path.display(),
                warning
            );
            if video.preview || video.open_folder {
                let after = crate::export_ui::AfterExport {
                    folder: video.open_folder,
                    image: false,
                };
                if let Err(e) = after.open(&video.path, true) {
                    self.status
                        .push_str(&format!(" Could not open the video player: {e}"));
                }
            }
            if self.clip.recording_target > 0
                && self
                    .clip
                    .recording_project
                    .as_ref()
                    .zip(self.session.document.video.as_ref())
                    .is_some_and(|(a, b)| a.same_edit(b))
            {
                self.append_recording(&document, self.clip.recording_target == 2);
            } else {
                match capture_core::Session::unsaved(document) {
                    Ok(session) => self.accept_video(session, video.path),
                    Err(e) => self.fail(e),
                }
            }
            self.shell.return_to_editor();
            self.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
            self.set_timeline_expanded(true);
            if self
                .clip
                .video_options
                .as_ref()
                .is_some_and(|o| o.find_sensitive)
            {
                self.open_video_privacy_review();
            }
            if !warning.is_empty() {
                self.status.push_str(&format!(" {warning}"));
            }
            return;
        }
        let find_sensitive = settings.find_sensitive && settings.preview;
        if let Some(entry) = retained {
            let copy_status = if settings.clipboard {
                copy(&document)
                    .err()
                    .map(|e| format!(" Clipboard transfer failed: {e}"))
                    .unwrap_or_else(|| " Copied to clipboard.".into())
            } else {
                String::new()
            };
            self.history.refresh();
            if settings.preview {
                match capture_core::Session::new(document) {
                    Ok(session) => {
                        self.switch_session(session, None, None);
                        self.reset_document_tools();
                        self.view = Default::default();
                        self.path = None;
                        self.select_only(None);
                        self.drag_start = None;
                        self.needs_render = true;
                        self.shell.return_to_editor();
                        // Every capture opens ready to annotate.
                        self.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
                        self.history.active = Some(entry.id.clone());
                        self.preferences.pending_source = crate::capture_source::take();
                        self.autosave.reset(
                            &self.session.document,
                            Some(entry),
                            "Step guide",
                            capture_core::history::Origin::Capture,
                        );
                    }
                    Err(e) => {
                        self.fail(e);
                        return;
                    }
                }
            }
            self.status = format!("Step guide saved to Library.{copy_status}");
        } else {
            self.accept_capture(document, settings, copy);
        }
        if find_sensitive {
            self.open_privacy_review();
        }
        if !warning.is_empty() {
            self.status.push_str(&format!(" {warning}"));
        }
    }
    pub(super) fn accept_capture(
        &mut self,
        document: capture_core::Document,
        settings: Settings,
        copy: impl FnOnce(&capture_core::Document) -> Result<(), String>,
    ) {
        if let Err(e) = document.validate() {
            self.fail(e);
            return;
        }
        let copy_status = if settings.clipboard {
            match copy(&document) {
                Ok(()) => " Copied to clipboard.".into(),
                Err(e) => format!(" Clipboard transfer failed: {e}"),
            }
        } else {
            String::new()
        };
        let mut retention_error = String::new();
        if !settings.preview {
            match self.history.retain_capture(&document) {
                Ok(()) => {
                    self.status = format!(
                        "{} captured and saved to Library.{copy_status}",
                        settings.selection.label()
                    );
                    return;
                }
                Err(e) => {
                    retention_error = format!(
                        " Library save failed: {e}. Opened in Editor to keep the image available."
                    )
                }
            }
        }
        match capture_core::Session::unsaved(document) {
            Ok(session) => {
                self.clip.restore_focus = Some(true);
                self.switch_session(session, None, None);
                self.reset_document_tools();
                self.view = Default::default();
                self.path = None;
                self.select_only(None);
                self.drag_start = None;
                self.needs_render = true;
                self.shell.return_to_editor();
                // Every capture opens ready to annotate.
                self.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
                let label = format!("{} capture", settings.selection.label());
                self.preferences.pending_source = crate::capture_source::take();
                self.record_history(&label, capture_core::history::Origin::Capture);
                self.status = format!(
                    "{} captured.{copy_status}{retention_error}",
                    settings.selection.label()
                );
            }
            Err(e) => self.fail(e),
        }
    }
    /// Image frame > From Capture: captures with the current settings and puts
    /// the picture in that frame. Cancelling leaves the frame as it was.
    pub(crate) fn capture_into_frame(&mut self, ctx: &egui::Context, id: u64) {
        use capture_core::capture_options::Selection;
        let mut settings = self.quick_controls.capture_settings();
        if settings.selection == Selection::Steps {
            settings = self.quick_controls.capture_for(Selection::Region);
        }
        settings.preview = true;
        settings.clipboard = false;
        match settings.request() {
            Ok(settings) => {
                self.clip.frame_target = Some(id);
                self.clip.pending_video = None;
                self.clip.active = Some(settings);
                self.capture_requested = crate::CAPTURE_ENABLED;
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
            Err(e) => self.fail(e),
        }
    }
    pub(super) fn place_capture_in_frame(&mut self, id: u64, document: capture_core::Document) {
        let result = document
            .render()
            .map_err(|e| e.to_string())
            .and_then(|pixels| {
                capture_core::clipboard_payload::ImagePayload::from_rgba(
                    pixels.width(),
                    pixels.height(),
                    pixels.into_raw(),
                )
                .map_err(|e| e.to_string())
            })
            .and_then(|payload| {
                capture_core::bitmap::Bitmap::new(payload.image).map_err(|e| e.to_string())
            })
            .and_then(|bitmap| self.replace_frame_bitmap(id, std::sync::Arc::new(bitmap)));
        self.status = match result {
            Ok(()) => "Capture placed in the image frame.".into(),
            Err(e) => e,
        };
    }
    /// Opens the Capture Clip settings window (approved mockup D); the Clip tab is gone.
    pub(super) fn open_clip(&mut self) {
        // The view behind the dialog stays as it is, as for Capture Video;
        // the editor opens only when a capture actually arrives.
        self.clip.clip_options = Some(self.quick_controls.capture_settings());
        self.clip.clip_dialog = true;
    }
    /// The Capture Clip buttons: the settings window first unless the user turned it off.
    pub(super) fn capture_clip_clicked(&mut self, ctx: &egui::Context) {
        if self.quick_controls.ask_before_clip() {
            self.open_clip();
        } else {
            self.request_action(
                FileAction::StartCapture(self.quick_controls.capture_settings()),
                ctx,
            );
        }
    }
    pub(super) fn request_keybind_capture(
        &mut self,
        action: capture_core::keybinds::Action,
        ctx: &egui::Context,
    ) {
        let selection = action
            .settings(self.quick_controls.capture_settings())
            .selection;
        match self.quick_controls.capture_for(selection).request() {
            Ok(settings) => self.request_action(FileAction::StartCapture(settings), ctx),
            Err(e) => {
                self.fail(e);
                self.open_clip();
            }
        }
    }
    fn capture_enabled(&self) -> bool {
        crate::CAPTURE_ENABLED && self.capture_result.is_none() && !self.capture_requested
    }
    /// Approved placement C: Capture Clip and Capture Video pills in the tab row,
    /// at the right end of the tab row; icon-only when the window is narrow.
    pub(super) fn capture_pills(&mut self, ui: &mut egui::Ui, tabs_right: f32) {
        let right = ui.max_rect().right() - 10.;
        let compact = ui.ctx().content_rect().width() < 1100.;
        let width = if compact { 34. } else { 126. };
        let y = ui.min_rect().center().y;
        let enabled = self.capture_enabled();
        for (index, label) in ["Capture Video", "Capture Clip"].into_iter().enumerate() {
            let x = right - (index as f32 + 1.) * width - index as f32 * 6.;
            if x < tabs_right + 8. {
                continue;
            }
            let rect = egui::Rect::from_min_size(egui::pos2(x, y - 14.), egui::vec2(width, 28.));
            let response = ui
                .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        ui.add_sized(
                            rect.size(),
                            egui::Button::new("")
                                .corner_radius(14)
                                .fill(Color32::from_rgb(36, 43, 46))
                                .stroke(Stroke::new(1., Color32::from_rgb(74, 86, 89))),
                        )
                    })
                    .inner
                })
                .inner;
            let p = ui.painter();
            let icon_rect = egui::Rect::from_center_size(
                if compact {
                    rect.center()
                } else {
                    rect.left_center() + egui::vec2(17., 0.)
                },
                egui::vec2(16., 16.),
            );
            // BB1: hover feedback, and the red record icon used by every capture command.
            if response.hovered() && enabled {
                p.rect_filled(rect, 14., Color32::from_white_alpha(10));
                p.rect_stroke(
                    rect,
                    14.,
                    Stroke::new(1.5, crate::shell::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            crate::ribbon::draw_icon(
                p,
                icon_rect.center(),
                label,
                if enabled {
                    Color32::from_rgb(235, 65, 76)
                } else {
                    Color32::from_gray(110)
                },
            );
            if !compact {
                p.text(
                    rect.left_center() + egui::vec2(31., 0.),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(12.5),
                    if enabled {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(120)
                    },
                );
            }
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(("capture-pill", label)), rect));
            let response = response.tip(if label == "Capture Clip" {
                "Capture Clip · choose the area and options"
            } else {
                "Capture Video · choose the area, audio and Step by Step"
            });
            if response.clicked() {
                if label == "Capture Clip" {
                    self.capture_clip_clicked(ui.ctx());
                } else {
                    self.open_video_capture();
                }
            }
        }
        // Help (HELP-001): a round "?" button left of the capture pills.
        let x = right - 2. * width - 6. - 8. - 28.;
        if x > tabs_right + 8. {
            let rect = egui::Rect::from_min_size(egui::pos2(x, y - 14.), egui::vec2(28., 28.));
            let response = ui
                .put(
                    rect,
                    egui::Button::new(egui::RichText::new("?").strong())
                        .corner_radius(14)
                        .fill(Color32::from_rgb(36, 43, 46))
                        .stroke(Stroke::new(1., Color32::from_rgb(74, 86, 89))),
                )
                .tip_title("Help")
                .tip("Search ClipDesigner Help. Shortcut: F1");
            if response.clicked() {
                crate::help::request(ui.ctx(), None, "");
            }
        }
    }
    /// Capture Clip settings: same frame as Capture Video (approved mockup D).
    pub(super) fn capture_clip_dialog(&mut self, ctx: &egui::Context) {
        if !self.clip.clip_dialog {
            return;
        }
        let mut settings = self
            .clip
            .clip_options
            .take()
            .unwrap_or_else(|| self.quick_controls.capture_settings());
        let mut ask = self.quick_controls.ask_before_clip();
        let mut start = false;
        let mut cancel = false;
        let modal = crate::controls::Modal::new(egui::Id::unique("capture-clip-options")).show(
            ctx,
            |ui| {
                ui.set_width(470.);
                ui.label(crate::panel_ui::title("Capture Clip", 20.));
                ui.add_space(8.);
                dialog_tabs(ui, &["General"], 0);
                ui.add_space(4.);
                let content_h = (ctx.content_rect().height() - 260.).clamp(200., 380.);
                egui::ScrollArea::vertical()
                    .id_salt("capture-clip-settings")
                    .max_height(content_h)
                    .min_scrolled_height(content_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        section(ui, "Capture area");
                        let w = (ui.available_width() - 2. * ui.spacing().item_spacing.x) / 3.;
                        let scrolling = settings.is_scrolling();
                        let areas = [
                            (Selection::Region, "Region", "Region"),
                            (Selection::Window, "Window", "Window"),
                            (Selection::Fullscreen, "Fullscreen", "Fullscreen"),
                            (Selection::Steps, "Steps", "Steps"),
                            (Selection::Scrolling, "Scrolling capture", "Scrolling"),
                        ];
                        for row in areas.chunks(3) {
                            ui.horizontal(|ui| {
                                for &(selection, icon, label) in row {
                                    let on = if selection == Selection::Scrolling {
                                        scrolling
                                    } else {
                                        !scrolling && settings.selection == selection
                                    };
                                    let r = icon_tile(ui, egui::vec2(w, 64.), icon, label, on, false)
                                        .tip(if selection == Selection::Scrolling {
                                            "Capture a region containing only scrolling content. Exclude fixed headers and footers."
                                        } else {
                                            selection.label()
                                        });
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("clip-area", label)), r.rect));
                                    if r.clicked() {
                                        if selection == Selection::Scrolling {
                                            settings = self.quick_controls.capture_for(Selection::Region);
                                            settings.scrolling = true;
                                        } else {
                                            settings = self.quick_controls.capture_for(selection);
                                            settings.scrolling = false;
                                        }
                                    }
                                }
                            });
                        }
                        section(ui, "Options");
                        ui.horizontal(|ui| {
                            let size = egui::vec2(88., 58.);
                            if settings.is_scrolling() {
                                if icon_tile(ui, size, "Auto-scroll", "Auto-scroll", settings.scroll_automatic, false)
                                    .tip("ClipDesigner scrolls downward until the end or an alignment problem. Turn off to scroll manually.")
                                    .clicked()
                                {
                                    settings.scroll_automatic = !settings.scroll_automatic;
                                }
                            } else if icon_tile(ui, size, "Selection", "Cursor", settings.cursor, false)
                                .tip("Include the mouse pointer in the capture")
                                .clicked()
                            {
                                settings.cursor = !settings.cursor;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Start delay");
                            crate::controls::segments(ui, |ui| {
                                for seconds in [0, 3, 5, 10] {
                                    ui.selectable_value(
                                        &mut settings.delay_seconds,
                                        seconds,
                                        if seconds == 0 { "Off".into() } else { format!("{seconds} s") },
                                    );
                                }
                            });
                        });
                        if settings.selection == Selection::Fullscreen {
                            ui.horizontal(|ui| {
                                ui.label("Monitor");
                                fullscreen_options(ui, &mut settings.fullscreen);
                            });
                        }
                        section(ui, "After capture");
                        ui.horizontal(|ui| {
                            let size = egui::vec2(88., 58.);
                            if icon_tile(ui, size, "Open editor", "Open editor", settings.preview, false)
                                .tip("Open the capture in ClipDesigner. Every clip is saved as an editable Library project either way.")
                                .clicked()
                            {
                                settings.preview = !settings.preview;
                            }
                            if icon_tile(ui, size, "Clipboard", "Clipboard", settings.clipboard, false)
                                .tip("Copy the capture to the clipboard")
                                .clicked()
                            {
                                settings.clipboard = !settings.clipboard;
                            }
                        });
                        ui.add_enabled_ui(settings.preview, |ui| {
                            crate::controls::toggle(ui, &mut settings.find_sensitive, "Find sensitive information")
                                .tip("Open Find Sensitive Info when the capture opens in the editor.")
                                .tip_disabled("Turn on Open editor first. There is nothing to scan until the capture opens.");
                        });
                    });
                crate::panel_ui::divider(ui);
                ui.add_space(4.);
                crate::controls::toggle(ui, &mut ask, "Show this window before each Capture Clip")
                    .tip("Turn off to capture straight away with these settings. Capture hotkeys and the tray always capture straight away.");
                let label = if settings.is_scrolling() { "Scrolling" } else { settings.selection.label() };
                let summary = [
                    Some(label.to_string()),
                    if settings.is_scrolling() {
                        Some(if settings.scroll_automatic { "auto-scroll" } else { "manual scroll" }.to_string())
                    } else {
                        settings.cursor.then(|| "cursor".to_string())
                    },
                    (settings.delay_seconds > 0).then(|| format!("{} s delay", settings.delay_seconds)),
                    settings.preview.then(|| "open editor".to_string()),
                    settings.clipboard.then(|| "clipboard".to_string()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                ui.add(egui::Label::new(egui::RichText::new(summary).size(12.).color(SECTION)).truncate());
                (start, cancel) = crate::controls::dialog_actions(ui, "Capture", "Cancel", true, false);
            },
        );
        if ask != self.quick_controls.ask_before_clip() {
            self.quick_controls.set_ask_before_clip(ask);
        }
        if settings != self.quick_controls.capture_settings() {
            self.quick_controls.remember_capture(settings.clone());
        }
        self.clip.clip_options = Some(settings.clone());
        if cancel || crate::text_edit::cancel_on_escape(&modal) {
            self.clip.clip_dialog = false;
            return;
        }
        if start {
            self.clip.clip_dialog = false;
            self.request_action(FileAction::StartCapture(settings), ctx);
        }
    }
}
/// Dialog tabs in the Properties pane style (rail of icon + label tabs).
pub(super) fn dialog_tabs(ui: &mut egui::Ui, labels: &[&str], selected: usize) -> Option<usize> {
    let mut clicked = None;
    crate::panel_ui::rail(ui, |ui| {
        ui.horizontal(|ui| {
            for (index, label) in labels.iter().enumerate() {
                let glyph = match *label {
                    "Step by Step" => "Steps tab",
                    _ => "General",
                };
                let r = crate::panel_ui::tab(ui, label, glyph, index == selected, 140., true);
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::unique(("capture-tab", *label)), r.rect));
                if r.clicked() {
                    clicked = Some(index);
                }
            }
        });
    });
    clicked
}
fn fullscreen_options(ui: &mut egui::Ui, target: &mut capture_core::capture_options::Fullscreen) {
    use capture_core::capture_options::{Display, Fullscreen};
    let id = egui::Id::unique("capture-display-list");
    let displays = ui.ctx().data_mut(|d| {
        d.get_temp::<Result<Vec<Display>, String>>(id)
            .unwrap_or_else(|| {
                let list = crate::capture::displays();
                d.insert_temp(id, list.clone());
                list
            })
    });
    let name = |target: &Fullscreen| match target {
        Fullscreen::Ask => "Select my monitor during capture".to_string(),
        Fullscreen::Pointer => "Monitor under cursor".to_string(),
        Fullscreen::All => "All monitors".to_string(),
        Fullscreen::Display(device) => displays
            .as_ref()
            .ok()
            .and_then(|list| list.iter().position(|d| &d.device == device))
            .map_or("Disconnected monitor".to_string(), |i| {
                format!("Monitor {}", i + 1)
            }),
    };
    egui::ComboBox::from_id_salt("clip-fullscreen-monitor")
        .width(ui.available_width() - 8.)
        .selected_text(name(target))
        .show_ui(ui, |ui| {
            for choice in [Fullscreen::Ask, Fullscreen::Pointer, Fullscreen::All] {
                let label = name(&choice);
                ui.selectable_value(target, choice, label);
            }
            match &displays {
                Ok(list) => {
                    for (index, display) in list.iter().enumerate() {
                        let [l, t, r, b] = display.bounds;
                        ui.selectable_value(
                            target,
                            Fullscreen::Display(display.device.clone()),
                            format!(
                                "Monitor {} · {} × {}{}",
                                index + 1,
                                r - l,
                                b - t,
                                if display.primary { " · Main" } else { "" }
                            ),
                        );
                    }
                }
                Err(e) => {
                    ui.label(e);
                }
            }
            if ui.button("Refresh displays").clicked() {
                ui.ctx()
                    .data_mut(|d| d.remove::<Result<Vec<Display>, String>>(id));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_settings_dialog_and_buttons_render_without_starting_capture() {
        use crate::property_layout::tests::{PaintedOutput, dump_meshes};
        for scale in [1., 1.5, 2.] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            ctx.set_pixels_per_point(scale);
            let mut editor = crate::canvas::tests::editor(&ctx);
            editor.clip.video_dialog = true;
            let mut time = 0.;
            let mut frame = || {
                time += 0.25;
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280., 720.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        editor.show_ribbon(ui);
                        editor.video_capture_dialog(&ctx);
                    },
                ))
            };
            let first = frame();
            let _ = frame();
            let output = frame();
            if let Some(root) = std::env::var_os("CLIPDESIGNER_VIDEO_EVIDENCE_DIR") {
                dump_meshes(
                    &ctx,
                    &root.to_string_lossy(),
                    &format!("video-ui-{scale}"),
                    first,
                    output,
                );
            }
            assert!(!editor.capture_requested && editor.clip.pending_video.is_none());
            // Approved placement C: both capture pills sit in the tab row.
            let a = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("capture-pill", "Capture Clip")))
                    .unwrap()
            });
            let b = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("capture-pill", "Capture Video")))
                    .unwrap()
            });
            let general = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("capture-tab", "General")))
                    .unwrap()
            });
            let steps = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("capture-tab", "Step by Step")))
                    .unwrap()
            });
            // BA5: Properties-pane style tabs with room between them.
            assert!(steps.left() - general.right() >= 4.);
            assert_eq!(general.size(), steps.size());
            assert_eq!(a.size(), b.size());
            assert!(a.right() < b.left());
            assert_eq!(a.center().y, b.center().y);
        }
    }
    #[test]
    fn cancelled_capture_keeps_last_scroll_mode_across_restart() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        let (mode_tx, mode_rx) = std::sync::mpsc::channel();
        editor.clip.active = Some(Settings::for_selection(Selection::Scrolling));
        editor.clip.scroll_mode = Some(mode_rx);
        editor.capture_result = Some(rx);
        mode_tx.send(false).unwrap();
        tx.send(Ok(None)).unwrap();
        editor.receive_capture();
        assert!(
            !editor
                .quick_controls
                .capture_for(Selection::Scrolling)
                .scroll_automatic
        );
        assert!(editor.clip.scroll_mode.is_none() && editor.clip.active.is_none());
        let dir = tempfile::tempdir().unwrap();
        let store = capture_core::quick_preferences::Store::open(dir.path()).unwrap();
        store
            .remember(capture_core::quick_preferences::Choice::Capture(
                editor.quick_controls.capture_for(Selection::Scrolling),
            ))
            .unwrap();
        assert!(
            !capture_core::quick_preferences::Store::open(dir.path())
                .unwrap()
                .read()
                .unwrap()
                .capture_for(Selection::Scrolling)
                .scroll_automatic
        );
    }
    #[test]
    fn capture_clip_popup_switches_saved_profiles_and_remembers_the_ask_switch() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.capture_clip_clicked(&ctx);
        assert!(e.clip.clip_dialog && !e.capture_requested);
        let frame = |e: &mut Editor, events| {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 720.),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| e.capture_clip_dialog(ui.ctx()),
            ))
        };
        for _ in 0..3 {
            let _ = frame(&mut e, vec![]);
        }
        let area = |name| {
            ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("clip-area", name))))
                .unwrap()
        };
        let size = area("Region").size();
        for name in ["Region", "Window", "Fullscreen", "Steps", "Scrolling"] {
            assert_eq!(area(name).size(), size, "{name}");
        }
        let pos = area("Steps").center();
        for pressed in [true, false] {
            let _ = frame(
                &mut e,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(
            e.quick_controls.capture_settings().selection,
            Selection::Steps
        );
        assert!(!e.capture_requested && e.clip.clip_dialog);
        // With the window turned off, Capture Clip captures straight away.
        e.clip.clip_dialog = false;
        e.quick_controls.set_ask_before_clip(false);
        e.capture_clip_clicked(&ctx);
        assert!(!e.clip.clip_dialog);
        assert!(e.capture_requested || e.pending.is_some());
    }
    #[test]
    fn clip_capture_honors_clipboard_and_editor_preview() {
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let doc = capture_core::Document::canvas(32, 24, [12, 34, 56, 255]).unwrap();
        let copied = std::cell::Cell::new(0);
        let settings = Settings::default();
        editor.accept_capture(doc.clone(), settings, |received| {
            assert_eq!(received.render().unwrap(), doc.render().unwrap());
            copied.set(copied.get() + 1);
            Ok(())
        });
        assert_eq!(copied.get(), 1);
        assert_eq!(
            editor.session.document.render().unwrap(),
            doc.render().unwrap()
        );
        editor.open_clip();
        assert!(editor.clip.clip_dialog);
    }
    #[test]
    fn shortcut_reads_the_corresponding_capture_profile() {
        use capture_core::keybinds::Action;
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let region = Settings {
            clipboard: true,
            cursor: true,
            delay_seconds: 5,
            ..Settings::default()
        };
        editor.quick_controls.remember_capture(region.clone());
        editor
            .quick_controls
            .remember_capture(Settings::for_selection(Selection::Steps));
        editor.request_keybind_capture(Action::Region, &ctx);
        assert_eq!(editor.clip.active, Some(region));
        editor.request_keybind_capture(Action::Saved, &ctx);
        assert_eq!(
            editor.clip.active,
            Some(Settings::for_selection(Selection::Steps))
        );
    }
}
