use crate::{Editor, FileAction, egui};
use capture_core::capture_options::{Selection, Settings};
use egui::{Color32, Stroke};

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
}
impl Editor {
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
                ui.set_width(390.);
                ui.label(crate::panel_ui::title("Capture Video", 20.));
                ui.add_space(12.);
                crate::panel_ui::divider(ui);
                ui.label(crate::panel_ui::title("Capture area", 14.));
                ui.add_space(6.);
                ui.horizontal(|ui| {
                    for selection in [Selection::Region, Selection::Window, Selection::Fullscreen] {
                        if crate::video_editor::ui::button(
                            ui,
                            selection.label(),
                            selection.label(),
                            112.,
                            options.selection == selection,
                        )
                        .clicked()
                        {
                            options.selection = selection;
                        }
                    }
                });
                ui.add_space(8.);
                if options.selection == Selection::Window {
                    crate::controls::toggle(
                        ui,
                        &mut options.pause_on_focus_loss,
                        "Pause when the selected window loses focus",
                    );
                }
                crate::panel_ui::divider(ui);
                ui.label(crate::panel_ui::title("Recording", 14.));
                ui.add_space(6.);
                ui.weak("MP4 · Up to 1920 × 1080");
                crate::controls::toggle(ui, &mut options.countdown, "3-second countdown");
                ui.horizontal(|ui| {
                    ui.label("Frame rate");
                    for fps in [15, 30, 60] {
                        ui.selectable_value(&mut options.fps, fps, format!("{fps} fps"));
                    }
                });
                crate::panel_ui::divider(ui);
                ui.label(crate::panel_ui::title("Audio & cursor", 14.));
                ui.add_space(6.);
                crate::controls::toggle(ui, &mut options.microphone, "Microphone")
                    .on_hover_text("Record the default Windows microphone");
                crate::controls::toggle(ui, &mut options.system_audio, "System audio")
                    .on_hover_text("Record the default Windows playback device");
                crate::controls::toggle(ui, &mut options.cursor, "Include cursor");
                crate::panel_ui::divider(ui);
                ui.label(crate::panel_ui::title("After recording", 14.));
                ui.add_space(6.);
                if self.is_video() {
                    egui::ComboBox::from_id_salt("recording-destination")
                        .selected_text(
                            [
                                "Open as new project",
                                "Append to this project",
                                "Add on a new track at playhead",
                            ][self.clip.recording_target as usize],
                        )
                        .width(ui.available_width() - 8.)
                        .show_ui(ui, |ui| {
                            for (i, label) in [
                                "Open as new project",
                                "Append to this project",
                                "Add on a new track at playhead",
                            ]
                            .iter()
                            .enumerate()
                            {
                                ui.selectable_value(
                                    &mut self.clip.recording_target,
                                    i as u8,
                                    *label,
                                );
                            }
                        });
                } else {
                    self.clip.recording_target = 0;
                }
                crate::controls::toggle(ui, &mut options.open_folder, "Open recording folder");
                crate::controls::toggle(
                    ui,
                    &mut options.preview,
                    "Open in ClipCraft after recording",
                );
                if !self.clip.video_error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, &self.clip.video_error);
                }
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
                    Err(e) => self.status = e.to_string(),
                }
            }
            self.shell.return_to_editor();
            self.shell.ribbon_tab = crate::ribbon::RibbonTab::Insert;
            self.set_timeline_expanded(true);
            if !warning.is_empty() {
                self.status.push_str(&format!(" {warning}"));
            }
            return;
        }
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
                        self.history.active = Some(entry.id.clone());
                        self.autosave.reset(
                            &self.session.document,
                            Some(entry),
                            "Step guide",
                            capture_core::history::Origin::Capture,
                        );
                    }
                    Err(e) => {
                        self.status = e.to_string();
                        return;
                    }
                }
            }
            self.status = format!("Step guide saved to Library.{copy_status}");
        } else {
            self.accept_capture(document, settings, copy);
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
            self.status = e.to_string();
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
                let label = format!("{} capture", settings.selection.label());
                self.record_history(&label, capture_core::history::Origin::Capture);
                self.status = format!(
                    "{} captured.{copy_status}{retention_error}",
                    settings.selection.label()
                );
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub(super) fn open_clip(&mut self) {
        self.shell.return_to_editor();
        self.shell.ribbon_tab = crate::ribbon::RibbonTab::Clip;
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
                self.status = e.to_string();
                self.open_clip();
            }
        }
    }
    pub(super) fn clip_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        use crate::ribbon::{divider, group};
        let mut settings = self.quick_controls.capture_settings();
        for video in [false, true] {
            let label = if video {
                "Capture Video"
            } else {
                "Capture Clip"
            };
            ui.vertical(|ui| {
                let enabled = crate::CAPTURE_ENABLED
                    && self.capture_result.is_none()
                    && !self.capture_requested;
                if crate::ribbon::command_tile(
                    ui,
                    label,
                    label,
                    if video {
                        "Record a video with optional microphone and system audio"
                    } else {
                        "Capture using these settings"
                    },
                    enabled,
                    false,
                    124.,
                    height + 14.,
                )
                .clicked()
                {
                    if video {
                        self.open_video_capture();
                    } else {
                        self.request_action(FileAction::StartCapture(settings.clone()), ui.ctx());
                    }
                }
            });
        }
        divider(ui, height);
        group(ui, "Capture method", 388., height, |ui| {
            for selection in Selection::ALL
                .into_iter()
                .filter(|s| *s != Selection::Scrolling)
            {
                let icon = match selection {
                    Selection::Region => "Region",
                    Selection::Window => "Window",
                    Selection::Fullscreen => "Fullscreen",
                    Selection::Steps => "Steps",
                    Selection::Scrolling => "Scrolling capture",
                };
                let response = crate::shell::ribbon_tile(
                    ui,
                    icon,
                    selection.label(),
                    settings.selection == selection,
                    Color32::WHITE,
                );
                if response.clicked() {
                    let scrolling = settings.is_scrolling();
                    settings = self.quick_controls.capture_for(selection);
                    settings.scrolling = scrolling && selection == Selection::Region;
                }
                response.on_hover_text(selection.label());
            }
            let response = crate::shell::ribbon_tile(
                ui,
                "Scrolling capture",
                "Scrolling",
                settings.is_scrolling(),
                Color32::WHITE,
            );
            if response.clicked() {
                let enabled = !settings.is_scrolling();
                if enabled {
                    settings = self.quick_controls.capture_for(Selection::Region);
                }
                settings.scrolling = enabled;
            }
            response.on_hover_text(
                "Capture a region containing only scrolling content. Exclude fixed headers and footers.",
            );
        });
        divider(ui, height);
        group(ui, "Output", 260., height, |ui| {
            ui.vertical(|ui| {
                crate::controls::toggle(ui, &mut settings.preview, "Open in ClipCraft after clipping")
                    .on_hover_text("Every clip is saved as an editable Library project, including clipboard-only captures.");

                crate::controls::toggle(ui, &mut settings.clipboard, "Copy to clipboard");
            });
        });
        divider(ui, height);
        group(ui, "Options", 215., height, |ui| {
            ui.vertical(|ui| {
                if settings.is_scrolling() {
                    crate::controls::toggle(ui,&mut settings.scroll_automatic,"Scroll automatically")
                        .on_hover_text("Select only scrolling content. ClipCraft scrolls downward until the end or an alignment problem. Turn off to scroll manually.");
                } else {
                    crate::controls::toggle(ui, &mut settings.cursor, "Include cursor");
                }

                ui.horizontal(|ui| {
                    ui.label("Start Delay");
                    egui::ComboBox::from_id_salt("clip-delay")
                        .width(65.)
                        .selected_text(if settings.delay_seconds == 0 {
                            "Off".into()
                        } else {
                            format!("{} sec", settings.delay_seconds)
                        })
                        .show_ui(ui, |ui| {
                            for seconds in [0, 3, 5, 10] {
                                ui.selectable_value(
                                    &mut settings.delay_seconds,
                                    seconds,
                                    if seconds == 0 {
                                        "Off".into()
                                    } else {
                                        format!("{seconds} sec")
                                    },
                                );
                            }
                        });
                    if settings.selection == Selection::Fullscreen {
                        fullscreen_options(ui, &mut settings.fullscreen);
                    }
                });
            });
        });
        if settings != self.quick_controls.capture_settings() {
            self.quick_controls.remember_capture(settings.clone());
        }
    }
}
fn fullscreen_options(ui: &mut egui::Ui, target: &mut capture_core::capture_options::Fullscreen) {
    use capture_core::capture_options::{Display, Fullscreen};
    let id = egui::Id::unique("capture-display-list");
    let response = ui
        .menu_button("   ", |ui| {
            ui.set_min_width(220.);
            ui.heading("Fullscreen displays");
            let mut ask = matches!(target, Fullscreen::Ask);
            if crate::controls::toggle(ui, &mut ask, "Select my monitor during capture").changed() {
                *target = if ask {
                    Fullscreen::Ask
                } else {
                    crate::capture::displays()
                        .ok()
                        .and_then(|d| d.into_iter().find(|d| d.primary))
                        .map_or(Fullscreen::Pointer, |d| Fullscreen::Display(d.device))
                };
            }
            ui.selectable_value(target, Fullscreen::Pointer, "Monitor under cursor");
            let all = ui.selectable_value(target, Fullscreen::All, "All monitors");
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("all-monitors"), all.rect));
            #[cfg(not(test))]
            let _ = all;
            ui.separator();
            let displays = ui.ctx().data_mut(|d| {
                d.get_temp::<Result<Vec<Display>, String>>(id)
                    .unwrap_or_else(|| {
                        let list = crate::capture::displays();
                        d.insert_temp(id, list.clone());
                        list
                    })
            });
            match displays {
                Ok(displays) => {
                    for (index, display) in displays.iter().enumerate() {
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
        })
        .response
        .on_hover_text("Fullscreen settings");
    let center = response.rect.center();
    let painter = ui.painter();
    painter.circle_stroke(center, 4.5, Stroke::new(1.5, Color32::WHITE));
    painter.circle_stroke(center, 1.5, Stroke::new(1.2, Color32::WHITE));
    for i in 0..8 {
        let angle = i as f32 * std::f32::consts::FRAC_PI_4;
        let v = egui::vec2(angle.cos(), angle.sin());
        painter.line_segment(
            [center + v * 4.5, center + v * 7.],
            Stroke::new(2., Color32::WHITE),
        );
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("fullscreen-settings"), response.rect));
    #[cfg(not(test))]
    let _ = response;
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
            editor.shell.ribbon_tab = crate::ribbon::RibbonTab::Clip;
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
            if let Some(root) = std::env::var_os("CLIPCRAFT_VIDEO_EVIDENCE_DIR") {
                dump_meshes(
                    &ctx,
                    &root.to_string_lossy(),
                    &format!("video-ui-{scale}"),
                    first,
                    output,
                );
            }
            assert!(!editor.capture_requested && editor.clip.pending_video.is_none());
            let a = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("command-tile", "Capture Clip")))
                    .unwrap()
            });
            let b = ctx.data(|d| {
                d.get_temp::<egui::Rect>(egui::Id::unique(("command-tile", "Capture Video")))
                    .unwrap()
            });
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
    fn capture_tiles_align_and_switch_saved_profiles_without_capturing() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.ribbon_tab = crate::ribbon::RibbonTab::Clip;
        let mut frame = |events| {
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
                |ui| {
                    e.show_ribbon(ui);
                },
            ))
        };
        for _ in 0..3 {
            let _ = frame(vec![]);
        }
        let tile = |name| {
            ctx.data(|d| {
                d.get_temp::<(egui::Rect, bool, bool)>(egui::Id::unique(("ribbon-tile", name)))
                    .unwrap()
            })
        };
        let start = ctx
            .data(|d| d.get_temp::<egui::Rect>(egui::Id::unique(("command-tile", "Capture Video"))))
            .unwrap();
        let method_size = tile("Region").0.size();
        for name in [
            "Region",
            "Window",
            "Fullscreen",
            "Step by Step",
            "Scrolling",
        ] {
            let r = tile(name).0;
            assert_eq!(r.size(), method_size);
            assert!(
                r.top() >= start.top() && r.bottom() <= start.bottom(),
                "{name}: {r:?} {start:?}"
            );
            assert!(r.left() > start.right());
        }
        assert!(start.height() > method_size.y);
        assert!(tile("Scrolling").2);
        let pos = tile("Step by Step").0.center();
        for pressed in [true, false] {
            let _ = frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert_eq!(
            e.quick_controls.capture_settings().selection,
            Selection::Steps
        );
        assert!(!e.capture_requested);
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
        assert_eq!(editor.shell.ribbon_tab, crate::ribbon::RibbonTab::Clip);
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
