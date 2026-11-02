use crate::{Editor, FileAction, egui};
use capture_core::capture_options::{Mode, Selection, Settings};
use egui::{Color32, Stroke};

#[derive(Default)]
pub(super) struct Controller {
    pub open: bool,
    mode: Mode,
    settings: Settings,
    pub active: Option<Settings>,
    pub suspended: bool,
    pub restore_focus: Option<bool>,
    pub focus_requested: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Capture,
    Editor,
    Close,
}

impl Editor {
    pub(super) fn accept_capture_result(
        &mut self,
        captured: crate::capture::Captured,
        mut settings: Settings,
        copy: impl FnOnce(&capture_core::Document) -> Result<(), String>,
    ) {
        let crate::capture::Captured {
            document,
            warning,
            recognized,
            retained,
        } = captured;
        let text_status = recognized
            .as_ref()
            .map(|r| match r {
                Ok(t) if t.text.is_empty() => " No text was found.".to_owned(),
                Ok(t) => format!(" Text recognized locally ({}).", t.language),
                Err(e) => format!(" Text recognition failed: {e}"),
            })
            .unwrap_or_default();
        if let Some(result) = recognized {
            self.ocr
                .present(result, settings.preview, settings.clipboard);
            settings.clipboard = false;
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
                        self.session = session;
                        self.quick_controls.snap_recent.clear();
                        self.stamps.reset_document();
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
        self.status.push_str(&text_status);
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
                self.capture_controller.restore_focus = Some(true);
                self.session = session;
                self.quick_controls.snap_recent.clear();
                self.stamps.reset_document();
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
    pub(super) fn open_capture_controller(&mut self) {
        self.capture_controller.settings = self.quick_controls.capture_settings();
        self.capture_controller.mode = Mode::Image;
        self.capture_controller.open = true;
    }
    pub(super) fn request_keybind_capture(
        &mut self,
        action: capture_core::keybinds::Action,
        ctx: &egui::Context,
    ) {
        let selection = action
            .settings(self.quick_controls.capture_settings())
            .selection;
        match self
            .quick_controls
            .capture_for(selection)
            .request(Mode::Image)
        {
            Ok(settings) => self.request_action(FileAction::StartCapture(settings), ctx),
            Err(e) => {
                self.status = e.to_string();
                self.open_capture_controller();
            }
        }
    }
    pub(super) fn show_capture_controller(&mut self, ctx: &egui::Context) {
        if !self.capture_controller.open {
            return;
        }
        let id = egui::ViewportId::from_hash_of("clipcraft-capture-controller");
        let restore_focus = self.capture_controller.restore_focus.take();
        let before = self.capture_controller.settings.clone();
        let mut action = None;
        ctx.show_viewport_immediate(
            id,
            egui::ViewportBuilder::default()
                .with_icon(crate::brand::controller_icon())
                .with_title("ClipCraft Capture")
                .with_inner_size([714., 190.])
                .with_min_inner_size([714., 190.])
                .with_resizable(false)
                .with_active(false)
                // On Windows, establish both native icons and the shell's
                // group icon while hidden, before its first taskbar appearance.
                .with_visible(!cfg!(windows) && !self.capture_controller.suspended),
            |ui, class| {
                #[cfg(windows)]
                if class != egui::ViewportClass::EmbeddedWindow {
                    let shown = crate::brand::prepare_controller_window(self.hotkey_owner);
                    let visible = !self.capture_controller.suspended;
                    if shown != Some(visible) {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Visible(visible));
                    }
                }
                #[cfg(not(windows))]
                let _ = class;
                if std::mem::take(&mut self.capture_controller.focus_requested) {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                if ui.input(|i| i.viewport().close_requested()) {
                    action = Some(Action::Close);
                }
                let inner = panel(
                    ui,
                    &mut self.capture_controller.mode,
                    &mut self.capture_controller.settings,
                    crate::CAPTURE_ENABLED,
                );
                action = inner.or(action);
            },
        );
        if before.selection != self.capture_controller.settings.selection {
            self.capture_controller.settings = self
                .quick_controls
                .capture_for(self.capture_controller.settings.selection);
            ctx.request_repaint();
        }
        if before != self.capture_controller.settings {
            self.quick_controls
                .remember_capture(self.capture_controller.settings.clone());
        }
        match action {
            Some(Action::Capture) => match self
                .capture_controller
                .settings
                .request(self.capture_controller.mode)
            {
                Ok(settings) => {
                    self.request_action(FileAction::StartCapture(settings), ctx);
                }
                Err(e) => self.status = e.to_string(),
            },
            Some(Action::Editor | Action::Close) => {
                if action == Some(Action::Close) {
                    self.close_controller_window(ctx);
                    ctx.send_viewport_cmd_to(id, egui::ViewportCommand::Close);
                }
                if action == Some(Action::Editor) {
                    self.shell.return_to_editor();
                    self.show_editor_window(ctx);
                }
            }
            None => {}
        }
        if restore_focus == Some(true) {
            self.show_editor_window(ctx);
        }
        if let Some(editor) = restore_focus {
            ctx.send_viewport_cmd_to(
                if editor { egui::ViewportId::ROOT } else { id },
                egui::ViewportCommand::Focus,
            );
        }
    }
}

fn switch(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    ui.horizontal(|ui| {
        crate::controls::field_label(ui, label, [142., 24.]);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(36., 20.), egui::Sense::click());
        if response.clicked() {
            *value = !*value;
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *value, label)
        });
        ui.painter().rect_filled(
            rect,
            10,
            if *value {
                Color32::from_rgb(0, 144, 190)
            } else {
                Color32::from_gray(90)
            },
        );
        let center = egui::pos2(
            if *value {
                rect.right() - 10.
            } else {
                rect.left() + 10.
            },
            rect.center().y,
        );
        ui.painter().circle_filled(center, 8., Color32::WHITE);
        if response.hovered() || response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                10,
                Stroke::new(1., Color32::WHITE),
                egui::StrokeKind::Outside,
            );
        }
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(("capture-switch", label)), rect));
    });
}

fn panel(
    ui: &mut egui::Ui,
    mode: &mut Mode,
    settings: &mut Settings,
    platform_available: bool,
) -> Option<Action> {
    let mut action = None;
    let blue = Color32::from_rgb(0, 139, 185);
    egui::Frame::new().fill(Color32::from_rgb(29,33,35)).inner_margin(8).show(ui, |ui| {
        ui.set_min_size(egui::vec2(698.,174.));
        ui.spacing_mut().item_spacing=egui::vec2(8.,4.);
        ui.visuals_mut().widgets.inactive.weak_bg_fill=Color32::from_rgb(68,74,77);
        ui.visuals_mut().widgets.hovered.weak_bg_fill=Color32::from_rgb(83,91,96);
        ui.visuals_mut().widgets.inactive.bg_stroke=Stroke::new(1.,Color32::from_gray(103));
        ui.horizontal_top(|ui| {
            let mut active_tab=None;
            ui.vertical(|ui| {
                ui.set_width(64.);
                for choice in [Mode::AllInOne,Mode::Image,Mode::Video] {
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(64.,44.),egui::Sense::click());
                    let active=*mode==choice;
                    if active {active_tab=Some(rect);}
                    ui.painter().rect_filled(rect,5,if active {blue} else if response.hovered(){Color32::from_gray(65)}else{Color32::from_gray(37)});
                    crate::ribbon::draw_icon(ui.painter(),rect.center_top()+egui::vec2(0.,14.),if choice==Mode::Video{"Record"}else if choice==Mode::Image{"Capture"}else{"Editor"},if choice==Mode::Video{Color32::from_rgb(242,100,100)}else{Color32::WHITE});
                    ui.painter().text(rect.center_bottom()-egui::vec2(0.,5.),egui::Align2::CENTER_BOTTOM,choice.label(),egui::FontId::proportional(11.),Color32::WHITE);
                    response.widget_info(||egui::WidgetInfo::selected(egui::Role::Tab,ui.is_enabled(),active,choice.label()));
                    #[cfg(test)]ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique(("capture-mode",choice.label())),rect));
                    if response.clicked(){*mode=choice;}
                }
            });
            let content=egui::Frame::new().fill(Color32::from_rgb(56,62,65)).corner_radius(8).stroke(Stroke::new(2.,blue)).inner_margin(10).show(ui,|ui| {
                ui.set_width(604.);ui.set_min_height(118.);
                if *mode!=Mode::Image {
                    ui.vertical(|ui| {
                    ui.heading(format!("{} capture",mode.label()));ui.label("Planned after v1");
                    ui.label(if *mode==Mode::Video{"Screen recording, camera and audio will be added in a later release."}else{"Combined image/video capture will be added in a later release."});
                    if ui.button("Use Image capture").clicked(){*mode=Mode::Image;}
                    });
                } else {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(252.);
                            ui.horizontal(|ui| {
                                crate::controls::field_label(ui, "Selection", [60., 24.]);
                                egui::ComboBox::from_id_salt("capture-selection").width(if settings.selection==Selection::Fullscreen{142.}else{176.}).selected_text(settings.selection.label()).show_ui(ui,|ui| {
                                    for choice in Selection::ALL {ui.selectable_value(&mut settings.selection,choice,if choice.available(){choice.label().to_owned()}else{format!("{} · Planned",choice.label())});}
                                });
                                if settings.selection==Selection::Fullscreen {fullscreen_options(ui,&mut settings.fullscreen);}
                            });
                            for label in ["Effects","Share"] {ui.horizontal(|ui| {crate::controls::field_label(ui, label, [60., 24.]);ui.add_enabled(false,egui::Button::new("None · Planned").min_size(egui::vec2(176.,24.)));});}
                        });
                        let (divider,_) = ui.allocate_exact_size(egui::vec2(1.,112.),egui::Sense::hover());
                        ui.painter().line_segment([divider.center_top(),divider.center_bottom()],Stroke::new(1.,Color32::from_gray(78)));
                        ui.vertical(|ui| {
                            ui.set_width(208.);
                            switch(ui,"Preview in Editor",&mut settings.preview);
                            switch(ui,"Copy to Clipboard",&mut settings.clipboard);
                            switch(ui,"Include Cursor",&mut settings.cursor);
                            ui.horizontal(|ui| {crate::controls::field_label(ui, "Time Delay", [114., 24.]);egui::ComboBox::from_id_salt("capture-delay").width(70.).selected_text(if settings.delay_seconds==0{"Off".into()}else{format!("{} sec",settings.delay_seconds)}).show_ui(ui,|ui| {for seconds in [0,3,5,10]{ui.selectable_value(&mut settings.delay_seconds,seconds,if seconds==0{"Off".into()}else{format!("{seconds} sec")});}});});
                        });
                        ui.vertical_centered(|ui| {
                            ui.add_space(16.);
                            let enabled=platform_available && settings.request(*mode).is_ok();
                            let r=ui.add_enabled(enabled,egui::Button::new(egui::RichText::new("Capture").color(Color32::WHITE).strong()).fill(Color32::from_rgb(193,46,48)).corner_radius(40).min_size(egui::vec2(80.,80.)));
                            #[cfg(test)]ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique("controller-capture"),(r.rect,enabled)));
                            if r.clicked(){action=Some(Action::Capture);}
                        });
                    });
                }
            });
            if let Some(tab)=active_tab {ui.painter().rect_filled(egui::Rect::from_min_max(egui::pos2(tab.right()-7.,tab.top()),egui::pos2(content.response.rect.left()+2.,tab.bottom())),0,blue);}
        });
        ui.add_space(4.);
        ui.horizontal(|ui| {
            ui.add_enabled(false,egui::Button::new("Presets · Planned"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                let (rect,r)=ui.allocate_exact_size(egui::vec2(176.,24.),egui::Sense::click());
                if r.hovered(){ui.painter().rect_filled(rect,4,Color32::from_gray(66));}
                crate::brand::paint(ui,egui::Rect::from_center_size(rect.left_center()+egui::vec2(12.,0.),egui::vec2(20.,20.)));
                ui.painter().text(rect.left_center()+egui::vec2(27.,0.),egui::Align2::LEFT_CENTER,"Open ClipCraft Editor",egui::FontId::proportional(12.),Color32::WHITE);
                r.widget_info(||egui::WidgetInfo::labeled(egui::Role::Button,true,"Open ClipCraft Editor"));
                if r.clicked(){action=Some(Action::Editor);}
            });
        });
    });
    action
}

#[cfg(test)]
mod tests;

fn fullscreen_options(ui: &mut egui::Ui, target: &mut capture_core::capture_options::Fullscreen) {
    use capture_core::capture_options::{Display, Fullscreen};
    let id = egui::Id::unique("capture-display-list");
    let response = ui
        .menu_button("   ", |ui| {
            ui.set_min_width(220.);
            ui.heading("Fullscreen displays");
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
