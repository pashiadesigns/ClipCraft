#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use capture_core::{Annotation, Command, Document, Kind, Session};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

mod backstage;
mod brand;
mod canvas;
mod canvas_cursor;
mod canvas_display;
mod canvas_expansion;
mod canvas_view;
mod capture;
mod capture_controller;
mod chrome;
mod clipboard;
mod clipboard_edit;
mod color_widgets;
mod controls;
mod creation;
mod crop;
mod cut_out;
mod editor_keybinds;
mod export_ui;
mod fill_tool;
mod flatten;
mod footer;
mod group_transform;
mod guide_display;
mod guide_sections;
mod history;
mod hotkey;
mod icons;
mod image_frame;
mod image_transform;
mod lifecycle;
mod line_properties;
mod move_tool;
mod multi_select;
mod new_image;
mod object_pane;
mod object_rotation;
mod object_templates;
mod obscure_properties;
mod ocr;
mod options;
mod pixel_grid;
mod pixel_selection;
mod property_layout;
mod property_preview;
mod quick_controls;
mod quick_styles;
mod readiness;
mod resize_canvas;
mod resize_image;
mod ribbon;
mod ribbon_settings;
mod shadow_properties;
mod shape_properties;
mod shell;
mod shortcuts;
mod simplify;
mod split_button;
mod stamps;
mod step_tool;
mod style_tile;
mod tabs;
mod text_edit;
mod text_layout;
mod text_properties;
mod toolbar;
mod tray;

// Region capture is verified on the Windows target. Other platforms keep the
// workflow unavailable until they have a native implementation and evidence.
const CAPTURE_ENABLED: bool = cfg!(windows);

fn main() -> eframe::Result {
    let show_editor = std::env::args_os().nth(1).is_some()
        || quick_controls::QuickControls::new().startup_window()
            == capture_core::quick_preferences::StartupWindow::Editor;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(brand::icon())
            .with_decorations(false)
            .with_visible(show_editor)
            .with_inner_size([981.0, 687.0])
            .with_maximized(true)
            .with_min_inner_size([880.0, 580.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ClipCraft",
        options,
        Box::new(|cc| Ok(Box::new(Editor::new(cc)))),
    )
}
struct Editor {
    lifecycle: lifecycle::Lifecycle,
    editing_group: Option<u64>,
    simplify_palette: Vec<[u8; 4]>,
    simplify_auto: bool,
    simplify_detail: u8,
    quick_controls: quick_controls::QuickControls,
    quick_styles: quick_styles::QuickStyles,
    history: history::HistoryPanel,
    autosave: history::autosave::Autosave,
    shell: shell::Shell,
    view: canvas_view::CanvasView,
    session: Session,
    texture: Option<egui::TextureHandle>,
    display: canvas_display::CanvasDisplay,
    needs_render: bool,
    tool: Option<Kind>,
    edit_drag: Option<canvas::EditDrag>,
    guide_drag: Option<guide_sections::SectionDrag>,
    rotation_drag: Option<object_rotation::RotateDrag>,
    property_preview: Option<property_preview::PropertyPreview>,
    property_suppressed: bool,
    text_draft: Option<text_edit::TextDraft>,
    step_placement: Option<Annotation>,
    snapping_guides: [Option<f32>; 2],
    new_image: Option<new_image::NewImage>,
    resize_image: Option<resize_image::ResizeImage>,
    resize_canvas: Option<resize_canvas::ResizeCanvas>,
    export_dialog: Option<export_ui::Dialog>,
    stamps: stamps::Panel,
    image_transform: Option<capture_core::ImageTransform>,
    crop: Option<crop::Crop>,
    cut_out: Option<cut_out::CutOut>,
    fill_tool: fill_tool::FillTool,
    pixel_selection: Option<pixel_selection::PixelSelection>,
    color: [u8; 3],
    width: u32,
    drag_start: Option<[f32; 2]>,
    creation: creation::Creation,
    stroke_points: Vec<[f32; 2]>,
    shape_sides: u32,
    step_number: u32,
    step_letters: bool,
    step_size: u32,
    path: Option<PathBuf>,
    last_directory: PathBuf,
    status: String,
    selected: Option<u64>,
    extra_selected: std::collections::BTreeSet<u64>,
    selection_order: Vec<u64>,
    group_drag: Option<multi_select::GroupDrag>,
    marquee: Option<move_tool::Marquee>,
    pending: Option<FileAction>,
    allow_close: bool,
    last_title: String,
    capture_requested: bool,
    capture_controller: capture_controller::Controller,
    ocr: ocr::State,
    capture_result: Option<Receiver<capture::CaptureResult>>,
    global_capture: Option<hotkey::CaptureHotkey>,
    global_capture_status: String,
    hotkey_owner: Option<isize>,
    options: options::Options,
}
enum FileAction {
    New,
    CreateCanvas(new_image::CanvasSpec),
    FromClipboard(Session),
    HistorySnapshot { session: Session, id: String },
    Open,
    Close,
    Capture,
    StartCapture(capture_core::capture_options::Settings),
}
impl Editor {
    fn window_title(&self) -> String {
        format!(
            "{}{} — ClipCraft",
            if self.is_modified() && !self.library_saved() {
                "*"
            } else {
                ""
            },
            self.path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|p| p.to_string_lossy().into_owned())
                .or_else(|| self
                    .history
                    .active
                    .as_deref()
                    .and_then(|id| self.history.entry(id))
                    .map(|e| e.label))
                .unwrap_or_else(|| "Untitled".into())
        )
    }
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        shell::configure(&cc.egui_ctx);
        text_layout::configure(&cc.egui_ctx);
        let mut startup_path = None;
        let mut startup_history = None;
        let mut last_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut imported_image = false;
        let mut startup_status = "Open an image, then drag to annotate.".to_string();
        let document = std::env::args_os()
            .nth(1)
            .and_then(|arg| {
                let path = std::path::absolute(&arg).unwrap_or_else(|_| PathBuf::from(arg));
                let is_project = path
                    .extension()
                    .is_some_and(|extension| extension == "json");
                let result = if is_project {
                    Document::load(&path)
                } else {
                    Document::open_image(&path)
                };
                match result {
                    Ok(document) => {
                        if let Some(parent) = path.parent() {
                            last_directory = parent.to_path_buf();
                        }
                        imported_image = !is_project;
                        startup_history = Some((
                            path.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                            if is_project {
                                capture_core::history::Origin::Project
                            } else {
                                capture_core::history::Origin::Image
                            },
                        ));
                        if is_project {
                            startup_path = Some(path);
                        }
                        Some(document)
                    }
                    Err(error) => {
                        startup_status = error.to_string();
                        None
                    }
                }
            })
            .unwrap_or_else(|| {
                Document::blank(960, 600).expect("fixed canvas dimensions are valid")
            });
        let hotkey_owner = hotkey::owner(cc);
        let quick_controls = quick_controls::QuickControls::new();
        let keys = quick_controls.keybinds();
        let (global_capture, global_capture_status) =
            match hotkey::CaptureHotkey::start(cc.egui_ctx.clone(), hotkey_owner, &keys) {
                Ok(hotkey) => (Some(hotkey), options::status(&keys)),
                Err(error) => (None, error),
            };
        let mut editor = Self {
            lifecycle: lifecycle::Lifecycle::new(cc.egui_ctx.clone()),
            editing_group: None,
            simplify_palette: vec![],
            simplify_auto: true,
            simplify_detail: 50,
            quick_controls,
            quick_styles: quick_styles::QuickStyles::new(),
            history: history::HistoryPanel::new(),
            autosave: Default::default(),
            shell: shell::Shell::default(),
            view: canvas_view::CanvasView::default(),
            session: if imported_image {
                Session::unsaved(document)
            } else {
                Session::new(document)
            }
            .expect("startup document is valid"),
            texture: None,
            display: Default::default(),
            needs_render: true,
            tool: Some(Kind::Arrow),
            edit_drag: None,
            guide_drag: None,
            rotation_drag: None,
            property_preview: None,
            property_suppressed: false,
            text_draft: None,
            step_placement: None,
            snapping_guides: [None; 2],
            new_image: None,
            resize_image: None,
            resize_canvas: None,
            export_dialog: None,
            stamps: Default::default(),
            image_transform: None,
            crop: None,
            pixel_selection: None,
            cut_out: None,
            fill_tool: fill_tool::FillTool::default(),
            color: [239, 85, 55],
            width: 3,
            drag_start: None,
            creation: Default::default(),
            stroke_points: Vec::new(),
            shape_sides: 3,
            step_number: 1,
            step_letters: false,
            step_size: 44,
            path: startup_path,
            last_directory,
            status: startup_status,
            selected: None,
            extra_selected: Default::default(),
            selection_order: vec![],
            group_drag: None,
            marquee: None,
            pending: None,
            allow_close: false,
            last_title: String::new(),
            capture_requested: false,
            capture_controller: Default::default(),
            ocr: Default::default(),
            capture_result: None,
            global_capture,
            global_capture_status,
            hotkey_owner,
            options: Default::default(),
        };
        if let Some((label, origin)) = startup_history {
            editor.record_history(&label, origin);
        }
        editor.restore_drawing_defaults(editor.tool);
        editor
    }
    fn execute(&mut self, command: Command) {
        let remembered_ids: Vec<_> = match &command {
            Command::Add { annotation } | Command::Update { annotation } => vec![annotation.id],
            Command::AddMany { annotations }
            | Command::UpdateMany { annotations }
            | Command::MoveAndExpand { annotations, .. } => {
                annotations.iter().map(|a| a.id).collect()
            }
            _ => vec![],
        };
        self.cancel_gesture();
        let dimensions = [self.session.document.width, self.session.document.height];
        let history_action = match &command {
            Command::Undo if self.session.can_undo() => Some("Undo"),
            Command::Redo if self.session.can_redo() => Some("Redo"),
            _ => None,
        };
        match self.session.execute(command) {
            Ok(()) => {
                for id in remembered_ids {
                    if let Some(annotation) = self
                        .session
                        .document
                        .annotations
                        .iter()
                        .find(|a| a.id == id)
                        .cloned()
                    {
                        self.remember_annotation_style(&annotation);
                    }
                }
                if dimensions != [self.session.document.width, self.session.document.height] {
                    self.view = canvas_view::CanvasView::default();
                    self.status = format!(
                        "Image size: {} × {} px.",
                        self.session.document.width, self.session.document.height
                    );
                } else if let Some(action) = history_action {
                    self.status = format!(
                        "{action} complete. {} editable objects.",
                        self.session.document.annotations.len()
                    );
                }
                self.extra_selected.retain(|id| {
                    self.session
                        .document
                        .annotations
                        .iter()
                        .any(|a| a.id == *id)
                });
                self.needs_render = true;
                if history_action.is_some() {
                    self.sync_step_sequence();
                }
                if !self
                    .session
                    .document
                    .annotations
                    .iter()
                    .any(|a| Some(a.id) == self.selected)
                {
                    self.select_only(None);
                }
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    fn open(&mut self) {
        self.suspend_global_capture();
        if let Some(path) = rfd::FileDialog::new()
            .set_directory(self.dialog_directory())
            .add_filter("Images and projects", &["png", "jpg", "jpeg", "json"])
            .pick_file()
        {
            let is_project = path.extension().is_some_and(|x| x == "json");
            let document = if is_project {
                Document::load(&path)
            } else {
                Document::open_image(&path)
            };
            match document.and_then(if is_project {
                Session::new
            } else {
                Session::unsaved
            }) {
                Ok(session) => {
                    self.session = session;
                    self.view = canvas_view::CanvasView::default();
                    if let Some(parent) = path.parent() {
                        self.last_directory = parent.to_path_buf();
                    }
                    let history_label = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    self.path = if is_project { Some(path) } else { None };
                    self.select_only(None);
                    self.needs_render = true;
                    self.drag_start = None;
                    self.status = "Image opened. Drag on the canvas to annotate.".into();
                    self.shell.return_to_editor();
                    self.record_history(
                        &history_label,
                        if is_project {
                            capture_core::history::Origin::Project
                        } else {
                            capture_core::history::Origin::Image
                        },
                    );
                }
                Err(e) => self.status = e.to_string(),
            }
        }
    }
    fn save(&mut self, save_as: bool) -> bool {
        if !self.commit_text() {
            return false;
        }
        let path = self.path.clone().filter(|_| !save_as).or_else(|| {
            self.suspend_global_capture();
            let dialog = rfd::FileDialog::new()
                .set_directory(self.dialog_directory())
                .set_file_name(
                    self.path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "capture.capture.json".into()),
                )
                .add_filter("Editable capture", &["json"]);
            dialog.save_file()
        });
        if let Some(path) = path {
            match self.session.save(&path) {
                Ok(()) => {
                    self.status = format!("Saved {}", path.display());
                    if let Some(parent) = path.parent() {
                        self.last_directory = parent.to_path_buf();
                    }
                    self.path = Some(path);
                    return true;
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        false
    }
    fn export(&mut self) {
        self.begin_export(false);
    }

    fn copy_all(&mut self) {
        self.copy_all_with(clipboard::copy);
    }

    fn copy_all_with(&mut self, write: impl FnOnce(&Document) -> Result<(), String>) {
        if !self.commit_text() {
            return;
        }
        self.cancel_gesture();
        self.status = match write(&self.session.document) {
            Ok(()) => {
                let objects = self.selected_objects();
                self.quick_controls.copied_format = if objects.len() == 1 {
                    objects.into_iter().next()
                } else {
                    None
                };
                if self.quick_controls.copied_format.is_some() {
                    "Copied the complete image. Selected object formatting is available with Ctrl+Shift+V.".into()
                } else {
                    "Copied the complete image, including annotations, to the clipboard.".into()
                }
            }
            Err(error) => error,
        };
    }

    fn editor_shortcuts_idle(&self) -> bool {
        self.export_dialog.is_none()
            && !self.history_dialog_open()
            && self.shell.backstage.is_none()
            && self.pending.is_none()
            && self.autosave.action.is_none()
            && self.new_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.crop.is_none()
            && !self.cut_out.as_ref().is_some_and(|cut| cut.busy())
            && self.capture_result.is_none()
            && !self.ocr.is_open()
            && self.group_drag.is_none()
            && self.marquee.is_none()
            && self.step_placement.is_none()
            && self.edit_drag.is_none()
            && self.rotation_drag.is_none()
            && self.property_preview.is_none()
            && self.drag_start.is_none()
    }

    fn new_from_clipboard(&mut self, ctx: &egui::Context) {
        // Capture and validate once, before prompting. If another program changes
        // the clipboard while the prompt is open, preserve this exact snapshot.
        match clipboard::read() {
            Ok(session) => self.request_action(FileAction::FromClipboard(session), ctx),
            Err(error) => self.status = format!("{error}. Your project is unchanged."),
        }
    }

    fn dialog_directory(&self) -> PathBuf {
        self.last_directory.clone()
    }

    fn request_action(&mut self, action: FileAction, ctx: &egui::Context) {
        if matches!(action, FileAction::Capture) {
            self.show_controller_window(ctx);
            return;
        }
        if matches!(action, FileAction::New) {
            self.cancel_gesture();
            self.new_image = Some(new_image::NewImage::default());
            return;
        }
        if !self.commit_text() {
            return;
        }
        self.cancel_gesture();
        self.protect_with_autosave(action, ctx);
    }

    fn suspend_global_capture(&self) {
        if let Some(hotkey) = &self.global_capture {
            hotkey.set_accepting(false);
        }
    }

    fn accepts_global_capture(&self, ctx: &egui::Context) -> bool {
        self.options.recording.is_none()
            && self.export_dialog.is_none()
            && !self.history_dialog_open()
            && !self.ocr.is_open()
            && !self.ocr.busy()
            && self.pending.is_none()
            && self.autosave.action.is_none()
            && !self.view.panning
            && self.new_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.crop.is_none()
            && !self.cut_out.as_ref().is_some_and(|cut| cut.busy())
            && self.group_drag.is_none()
            && self.marquee.is_none()
            && self.step_placement.is_none()
            && self.edit_drag.is_none()
            && self.rotation_drag.is_none()
            && self.property_preview.is_none()
            && self.drag_start.is_none()
            && self.capture_result.is_none()
            && !self.capture_requested
            && !self.shell.help
            && !egui::Popup::is_any_open(ctx)
    }

    fn perform_action(&mut self, action: FileAction, ctx: &egui::Context) {
        match action {
            FileAction::New => self.new_image = Some(new_image::NewImage::default()),
            FileAction::CreateCanvas(spec) => {
                let result = Document::canvas(spec.width, spec.height, spec.color)
                    .and_then(Session::unsaved);
                let session = match result {
                    Ok(session) => session,
                    Err(error) => {
                        self.status = error.to_string();
                        return;
                    }
                };
                self.session = session;
                self.view = canvas_view::CanvasView::default();
                self.path = None;
                self.select_only(None);
                self.drag_start = None;
                self.needs_render = true;
                self.shell.return_to_editor();
                self.status = format!(
                    "New {} × {} canvas. Save a project or export PNG.",
                    spec.width, spec.height
                );
                self.record_history("New canvas", capture_core::history::Origin::Canvas);
            }
            FileAction::Open => self.open(),
            FileAction::FromClipboard(session) => {
                self.session = session;
                self.view = canvas_view::CanvasView::default();
                self.path = None;
                self.select_only(None);
                self.drag_start = None;
                self.needs_render = true;
                self.shell.return_to_editor();
                self.status = "Clipboard image opened. Save a project or export PNG.".into();
                self.record_history("Clipboard image", capture_core::history::Origin::Clipboard);
            }
            FileAction::HistorySnapshot { session, id } => {
                self.session = session;
                let entry = self.history.entry(&id);
                self.autosave.reset(
                    &self.session.document,
                    entry,
                    "Library image",
                    capture_core::history::Origin::Project,
                );
                self.history.active = Some(id);
                self.view = canvas_view::CanvasView::default();
                self.path = None;
                self.select_only(None);
                self.drag_start = None;
                self.needs_render = true;
                self.shell.return_to_editor();
                self.status = "Library image opened. Changes save here automatically.".into();
            }
            FileAction::Capture => self.open_capture_controller(),
            FileAction::StartCapture(settings) => {
                match settings.request(capture_core::capture_options::Mode::Image) {
                    Ok(settings) => {
                        self.capture_controller.suspended = true;
                        ctx.send_viewport_cmd_to(
                            egui::ViewportId::from_hash_of("clipcraft-capture-controller"),
                            egui::ViewportCommand::Visible(false),
                        );
                        self.capture_controller.active = Some(settings);
                        self.capture_requested = CAPTURE_ENABLED;
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            FileAction::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn receive_capture(&mut self) {
        let Some(receiver) = &self.capture_result else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("Capture stopped unexpectedly. Your project is unchanged.".into())
            }
        };
        self.capture_result = None;
        let settings = self.capture_controller.active.take().unwrap_or_default();
        self.capture_controller.suspended = false;
        self.capture_controller.restore_focus =
            Some(settings.preview && matches!(&result, Ok(Some(_))));
        match result {
            Ok(Some(captured)) => {
                self.accept_capture_result(captured, settings, clipboard::copy);
            }
            Ok(None) => self.status = "Capture cancelled. Your project is unchanged.".into(),
            Err(error) => self.status = error,
        }
    }

    fn confirm_unsaved(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let mut proceed = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("unsaved_changes")).show(ctx, |ui| {
            ui.heading("Keep your changes");
            ui.label("Automatic Library saving could not finish. Your document is still open.");
            ui.label("Save an editable project to keep your annotations.");
            ui.label("Exporting a PNG does not save the editable project.");
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Save and continue").clicked() && self.save(false) {
                    proceed = true;
                }
                if ui.button("Discard changes").clicked() {
                    proceed = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel || ctx.input_mut(|i| i.key_pressed(egui::Key::Escape)) {
            self.pending = None;
        } else if proceed && let Some(action) = self.pending.take() {
            self.perform_action(action, ctx);
        }
    }
}
impl eframe::App for Editor {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.background_logic(ctx);
        if self.capture_requested && self.capture_result.is_none() {
            self.capture_requested = false;
            self.status = format!(
                "{} capture… Escape or right-click cancels selection.",
                self.capture_controller
                    .active
                    .as_ref()
                    .map(|s| s.selection.label())
                    .unwrap_or("Region")
            );
            self.capture_result = Some(capture::start(
                frame,
                ctx.clone(),
                self.capture_controller.active.clone().unwrap_or_default(),
                self.history.root(),
            ));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.finish_text_on_outside_click(&ctx);
        self.prepare_property_input(&ctx);
        self.process_keybind_input(&ctx);
        if self.capture_result.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            ui.disable();
        }
        if self.autosave.action.is_some() {
            ui.disable();
        }
        let file_workspace_visible = self.shell.backstage.is_some();
        self.show_shell(ui);
        self.settle_property_preview(&ctx);
        if self.needs_render {
            self.refresh_canvas_display(&ctx);
        }
        if !file_workspace_visible {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(if self.shell.library {
                            shell::SURROUND
                        } else {
                            shell::CANVAS
                        })
                        .inner_margin(if self.shell.library {
                            egui::Margin {
                                left: 5,
                                right: 5,
                                top: 0,
                                bottom: 3,
                            }
                        } else {
                            egui::Margin::same(12)
                        }),
                )
                .show(ui, |ui| self.show_workspace(ui));
        }
        self.show_new_image(&ctx);
        self.show_resize(&ctx);
        self.show_resize_canvas(&ctx);
        self.show_export(&ctx, Some(_frame));
        self.show_image_transform(&ctx);
        self.editor_keybind_input(&ctx);
        self.pixel_selection_shortcuts(&ctx);
        self.shared_format_shortcuts(&ctx);
        self.clipboard_shortcuts(&ctx);
        self.pixel_grid_shortcut(&ctx);
        self.flatten_shortcut(&ctx);
        if self.editor_shortcuts_idle() {
            if ctx.input_mut(|i| {
                i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::S,
                )
            }) {
                self.save(true);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S)) {
                self.save(false);
            }
            if self.text_draft.is_none() && !ctx.egui_wants_keyboard_input() {
                if ctx.input_mut(|i| {
                    shortcuts::consume_exact(
                        i,
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::C,
                    )
                }) {
                    self.copy_all();
                }
                if ctx.input_mut(|i| {
                    shortcuts::consume_exact(
                        i,
                        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                        egui::Key::N,
                    )
                }) {
                    self.new_from_clipboard(&ctx);
                }
            }
            if self.pending.is_none()
                && self.text_draft.is_none()
                && ctx.input_mut(|i| {
                    shortcuts::consume_exact(i, egui::Modifiers::COMMAND, egui::Key::N)
                })
            {
                self.request_action(FileAction::New, &ctx);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::O)) {
                self.request_action(FileAction::Open, &ctx);
            }
            let canvas_owns_input = self.pending.is_none()
                && !self.shell.annotations_hidden
                && self.new_image.is_none()
                && self.text_draft.is_none()
                && !self.capture_requested
                && self.capture_result.is_none()
                && self.extra_selected.is_empty()
                && self.tool.is_none()
                && self.shell.canvas_enabled()
                && !ctx.egui_wants_keyboard_input();
            if let Some(command) = ctx.input_mut(|input| {
                shortcuts::arrange_command(
                    input,
                    &self.session.document,
                    self.selected,
                    canvas_owns_input,
                )
            }) {
                self.execute(command);
            }
            if self.text_draft.is_none()
                && !self.shell.annotations_hidden
                && self.shell.canvas_enabled()
                && !ctx.egui_wants_keyboard_input()
            {
                let zoom_owns_input =
                    !self.shell.help && !egui::Popup::is_any_open(&ctx) && !self.view.panning;
                if let Some(action) =
                    ctx.input_mut(|input| canvas_view::shortcut(input, zoom_owns_input))
                {
                    self.view.apply(action, None);
                    ctx.request_repaint();
                }
                if self.selected.is_some() {
                    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                        self.delete_selection();
                    } else {
                        let mut delta = [0.0, 0.0];
                        for (key, axis, direction) in [
                            (egui::Key::ArrowLeft, 0, -1.0),
                            (egui::Key::ArrowRight, 0, 1.0),
                            (egui::Key::ArrowUp, 1, -1.0),
                            (egui::Key::ArrowDown, 1, 1.0),
                        ] {
                            // Match modifiers on the key event, not their state
                            // at frame end: Shift may already have been released.
                            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, key)) {
                                delta[axis] += 10.0 * direction;
                            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                                delta[axis] += direction;
                            }
                        }
                        if delta != [0.0, 0.0] {
                            self.move_selection(delta);
                        }
                    }
                }
                if self.pixel_selection.is_none()
                    && !self.fill_tool.active
                    && self.cut_out.is_none()
                    && self.crop.is_none()
                    && ctx.input_mut(|i| {
                        shortcuts::consume_exact(i, egui::Modifiers::COMMAND, egui::Key::A)
                    })
                {
                    self.select_all_matching();
                }
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
                    self.execute(Command::Undo);
                }
                if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)) {
                    self.execute(Command::Redo);
                }
            }
        }
        if !self.allow_close && ctx.input(|i| i.viewport().close_requested()) {
            self.close_editor_window(&ctx);
        }
        self.confirm_unsaved(&ctx);
        self.show_history_dialog(&ctx);
        self.show_capture_controller(&ctx);
        self.show_ocr(&ctx);
        chrome::resize_edges(&ctx);
        let title = self.window_title();
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
        if let Some(hotkey) = &self.global_capture {
            hotkey.set_recording(self.options.recording.is_some());
            hotkey.set_accepting(!self.allow_close && self.accepts_global_capture(&ctx));
        }
    }
}
