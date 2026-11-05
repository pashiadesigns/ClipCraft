use crate::{Editor, FileAction, egui, tray};
use capture_core::quick_preferences::StartupWindow;

#[derive(Default)]
pub(super) struct Lifecycle {
    tray: Option<tray::Tray>,
    pub available: bool,
    pub hidden: bool,
    startup_pending: bool,
    pub error: String,
    incoming: std::collections::VecDeque<std::path::PathBuf>,
}
impl Lifecycle {
    pub fn new(ctx: egui::Context) -> Self {
        match tray::Tray::start(ctx) {
            Ok(tray) => Self {
                tray: Some(tray),
                available: true,
                startup_pending: true,
                ..Default::default()
            },
            Err(error) => Self {
                error,
                startup_pending: true,
                ..Default::default()
            },
        }
    }
}
impl Editor {
    pub(super) fn show_editor_window(&mut self, ctx: &egui::Context) {
        self.lifecycle.hidden = false;
        for command in [
            egui::ViewportCommand::Visible(true),
            egui::ViewportCommand::Focus,
        ] {
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, command);
        }
        // Windows SW_RESTORE also unmaximizes a window. Only request it when
        // actually minimized, never on an ordinary post-capture show/focus.
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::Minimized(false),
            );
        }
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
    pub(super) fn show_clip(&mut self, ctx: &egui::Context) {
        self.open_clip();
        self.show_editor_window(ctx);
    }
    pub(super) fn close_editor_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::CancelClose);
        if self.capture_result.is_some()
            || self.capture_requested
            || self.pending.is_some()
            || self.history_dialog_open()
            || self.autosave.action.is_some()
        {
            return;
        }
        if !self.quick_controls.close_to_tray() || !self.lifecycle.available {
            self.request_action(FileAction::Close, ctx);
            return;
        }
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        self.cancel_gesture();
        self.options.recording = None;
        self.new_image = None;
        self.resize_image = None;
        self.resize_canvas = None;
        self.image_transform = None;
        self.shell.help = false;
        egui::Popup::close_all(ctx);
        self.autosave_tick(ctx, true);
        self.lifecycle.hidden = true;
        ctx.send_viewport_cmd_to(
            egui::ViewportId::ROOT,
            egui::ViewportCommand::Visible(false),
        );
        self.status =
            "ClipCraft is running in the system tray. Capture shortcuts remain active.".into();
    }
    pub(super) fn tray_action(&mut self, action: tray::Action, ctx: &egui::Context) {
        if let tray::Action::Open(path) = &action {
            self.lifecycle.incoming.push_back(path.clone());
            self.show_editor_window(ctx);
            return;
        }
        if action == tray::Action::Unavailable {
            self.lifecycle.available = false;
            self.lifecycle.error =
                "System tray recovery failed. The editor remains open so you can access ClipCraft."
                    .into();
            self.show_editor_window(ctx);
            return;
        }
        if self.capture_result.is_some() || self.capture_requested {
            return;
        }
        match action {
            tray::Action::Editor => self.show_editor_window(ctx),
            tray::Action::Clip => self.show_clip(ctx),
            tray::Action::CaptureClip if self.accepts_global_capture(ctx) => {
                self.request_keybind_capture(capture_core::keybinds::Action::Saved, ctx)
            }
            tray::Action::Capture(selection) if self.accepts_global_capture(ctx) => {
                let mut settings = self.quick_controls.capture_for(selection);
                settings.scrolling = selection == capture_core::capture_options::Selection::Region
                    && self.quick_controls.capture_settings().is_scrolling();
                self.request_action(FileAction::StartCapture(settings), ctx);
            }
            tray::Action::ToggleScrolling => {
                let enabled = !self.quick_controls.capture_settings().is_scrolling();
                let mut settings = self.quick_controls.capture_settings();
                if enabled {
                    settings = self
                        .quick_controls
                        .capture_for(capture_core::capture_options::Selection::Region);
                }
                settings.scrolling = enabled;
                self.quick_controls.remember_capture(settings);
            }
            tray::Action::ToggleOpenAfter => {
                let mut settings = self.quick_controls.capture_settings();
                settings.preview = !settings.preview;
                self.quick_controls.remember_capture(settings);
            }
            tray::Action::CaptureVideo if self.accepts_global_capture(ctx) => {
                self.show_editor_window(ctx);
                self.open_video_capture();
            }
            tray::Action::Exit => self.request_action(FileAction::Close, ctx),
            _ => (),
        }
    }
    pub(super) fn apply_startup_window(
        &mut self,
        ctx: &egui::Context,
        explicit_file: bool,
        window: StartupWindow,
    ) {
        if !explicit_file && window == StartupWindow::Clip {
            self.open_clip();
        }
        self.show_editor_window(ctx);
    }
    pub(super) fn background_logic(&mut self, ctx: &egui::Context) {
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("tray-scrolling"),
                self.quick_controls.capture_settings().is_scrolling(),
            );
            d.insert_temp(
                egui::Id::unique("tray-open-after"),
                self.quick_controls.capture_settings().preview,
            );
        });
        if self.lifecycle.startup_pending {
            self.lifecycle.startup_pending = false;
            let launch = crate::launch::Request::read();
            self.apply_startup_window(
                ctx,
                launch.file.is_some(),
                launch.startup(self.quick_controls.startup_window()),
            );
        }
        let actions = self
            .lifecycle
            .tray
            .as_ref()
            .map(|t| t.take())
            .unwrap_or_default();
        for action in actions {
            self.tray_action(action, ctx);
        }
        let was_capturing = self.capture_result.is_some();
        self.receive_capture();
        if was_capturing
            && self.capture_result.is_none()
            && (self.clip.restore_focus == Some(true) || self.ocr.is_open())
        {
            self.show_editor_window(ctx);
        }
        if let Some(binding) = self.global_capture.as_ref().and_then(|h| h.take_pressed()) {
            if self.options.recording.is_some() {
                self.options.record(binding.chord);
            } else if self.accepts_global_capture(ctx) {
                self.request_keybind_capture(binding.action, ctx);
            }
        }
        self.autosave_tick(ctx, false);
        self.finish_autosave_action(ctx);
        if self.pending.is_none()
            && self.autosave.action.is_none()
            && self.accepts_global_capture(ctx)
            && let Some(path) = self.lifecycle.incoming.pop_front()
        {
            self.show_editor_window(ctx);
            self.request_action(FileAction::OpenPath(path), ctx);
        }
        // An autosave failure must never leave a confirmation in a hidden window.
        if self.pending.is_some() && self.lifecycle.hidden {
            self.show_editor_window(ctx);
        }
        if let Some(hotkey) = &self.global_capture {
            hotkey.set_recording(self.options.recording.is_some());
            hotkey.set_accepting(!self.allow_close && self.accepts_global_capture(ctx));
        }
    }
}

#[cfg(test)]
mod tests;
