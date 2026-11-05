#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use capture_core::{Annotation, Command, Document, Kind, Session};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

mod backstage;
mod brand;
mod canvas;
mod canvas_buttons;
mod canvas_cursor;
mod canvas_display;
mod canvas_expansion;
mod canvas_view;
mod capture;
mod chrome;
mod clip;
mod clipboard;
mod clipboard_edit;
mod color_widgets;
mod controls;
mod creation;
mod crop;
mod cut_out;
mod editor_keybinds;
mod export_ui;
mod ui_glyphs;
mod unicode_input;

mod flatten;
mod footer;
mod gesture_input;
mod group_feedback;
mod group_transform;
mod guide_display;
mod guide_sections;
mod history;
mod hotkey;
mod icons;
mod image_frame;
mod image_properties;
mod image_transform;
mod launch;
mod lifecycle;
mod line_properties;
mod motion;
mod move_tool;
mod multi_select;
mod new_image;
mod object_pane;
mod object_rotation;
mod object_templates;
mod obscure_properties;
mod ocr;
mod options;
mod page_setup;
mod page_setup_fields;
mod panel_ui;
mod pixel_grid;
mod privacy_review;
mod project_sessions;
mod rich_text;
mod single_instance;
mod spelling;
mod storage_locations;
mod text_clipboard;

mod focus_properties;
mod icon_library;
mod presets;
mod preview_ui;
mod property_layout;
mod property_preview;
mod quick_controls;
mod redact_placement;
mod resize_canvas;
mod resize_image;
mod reusable_presets;
mod ribbon;
mod ribbon_settings;
mod shadow_properties;
mod shape_properties;
mod shell;
mod shortcuts;
mod split_button;
mod step_tool;
mod stroke_input;
mod style_tile;
mod tabs;
mod template_picker;
mod text_edit;
mod text_layout;
mod text_properties;
mod toolbar;
mod tray;

// Region capture is verified on the Windows target. Other platforms keep the
// workflow unavailable until they have a native implementation and evidence.
const CAPTURE_ENABLED: bool = cfg!(windows);

fn main() -> eframe::Result {
    let launch = launch::Request::read();
    #[cfg(windows)]
    let _owner = match single_instance::enter(&launch) {
        Ok(Some(owner)) => owner,
        Ok(None) => return Ok(()),
        Err(message) => {
            rfd::MessageDialog::new()
                .set_title("ClipCraft")
                .set_description(message)
                .show();
            return Ok(());
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_icon(brand::icon())
            .with_decorations(false)
            .with_visible(true)
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
mod video_editor;
struct Editor {
    video: video_editor::State,
    group_feedback: Option<group_feedback::Feedback>,
    project_sessions: project_sessions::Sessions,
    lifecycle: lifecycle::Lifecycle,
    editing_group: Option<u64>,

    quick_controls: quick_controls::QuickControls,
    presets: presets::Presets,
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
    template_picker: Option<template_picker::Picker>,
    resize_image: Option<resize_image::ResizeImage>,
    resize_canvas: Option<resize_canvas::ResizeCanvas>,
    export_dialog: Option<export_ui::Dialog>,
    export_preferences: export_ui::Preferences,
    icon_library: icon_library::Panel,
    image_transform: Option<capture_core::ImageTransform>,
    crop: Option<crop::Crop>,
    cut_out: Option<cut_out::CutOut>,

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
    clip: clip::State,
    ocr: ocr::State,
    privacy_review: privacy_review::State,
    rich_text: rich_text::State,
    capture_result: Option<Receiver<capture::CaptureResult>>,
    global_capture: Option<hotkey::CaptureHotkey>,
    global_capture_status: String,
    hotkey_owner: Option<isize>,
    options: options::Options,
}
enum FileAction {
    VideoLoaded {
        session: Session,
        path: PathBuf,
    },
    OpenPath(PathBuf),
    New,
    CreateCanvas(new_image::CanvasSpec),
    FromClipboard(Session),
    Template {
        session: Session,
        title: String,
    },
    HistorySnapshot {
        session: Session,
        id: String,
    },
    Open,
    Close,
    Capture,
    StartCapture(capture_core::capture_options::Settings),
    StartVideo(
        capture_core::capture_options::Settings,
        capture::VideoRequest,
    ),
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
        chrome::install_native(cc);
        shell::configure(&cc.egui_ctx);
        text_layout::configure(&cc.egui_ctx);
        let mut startup_path = None;
        let mut startup_history = None;
        let mut last_directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut imported_image = false;
        let mut startup_status = "Open an image, then drag to annotate.".to_string();
        let document = launch::Request::read()
            .file
            .and_then(|arg| {
                let path = std::path::absolute(&arg).unwrap_or(arg);
                let is_project = capture_core::project_file::is_image_project(&path)
                    || capture_core::project_file::is_video_project(&path);
                let result = if video_editor::supported(&path) {
                    video_editor::load(&path).map_err(capture_core::Error::Invalid)
                } else {
                    capture_core::project_file::open(&path)
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
        let (global_capture, global_capture_status) = match hotkey::CaptureHotkey::start_available(
            cc.egui_ctx.clone(),
            hotkey_owner,
            &keys,
        ) {
            Ok((hotkey, status)) => (Some(hotkey), status),
            Err(error) => (None, error),
        };
        let mut editor = Self {
            group_feedback: None,
            project_sessions: Default::default(),
            video: Default::default(),
            lifecycle: lifecycle::Lifecycle::new(cc.egui_ctx.clone()),
            editing_group: None,

            quick_controls,
            presets: presets::Presets::new(),
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
            template_picker: None,
            resize_image: None,
            resize_canvas: None,
            export_dialog: None,
            export_preferences: Default::default(),
            icon_library: Default::default(),
            image_transform: None,
            crop: None,

            cut_out: None,

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
            clip: Default::default(),
            ocr: Default::default(),
            privacy_review: Default::default(),
            rich_text: Default::default(),
            capture_result: None,
            global_capture,
            global_capture_status,
            hotkey_owner,
            options: Default::default(),
        };
        if let Some((label, origin)) = startup_history {
            editor.record_history(&label, origin);
        }
        editor.reset_document_tools();
        editor.restore_drawing_defaults(editor.tool);
        editor
    }
    fn execute(&mut self, command: Command) {
        let command = self.placement_command(command);
        let motion_changes = self.changed_placement_motion(&command);
        let live_video = matches!(&command,Command::Video{project} if self.session.document.video.as_ref().is_some_and(|old|old.same_clock(project)));
        if !live_video {
            self.video.pause();
        }
        let grouping = if let Command::GroupObjects { ids } = &command {
            Some((
                ids.clone(),
                self.session
                    .document
                    .annotations
                    .iter()
                    .map(|a| a.group)
                    .collect::<Vec<_>>(),
            ))
        } else {
            None
        };
        let remembered_ids: Vec<_> = match &command {
            Command::Add { annotation } | Command::Update { annotation } => vec![annotation.id],
            Command::AddMany { annotations }
            | Command::AddTimed { annotations, .. }
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
                for a in &motion_changes {
                    self.remember_placement_template(a);
                }
                if let Some((ids, previous)) = grouping
                    && previous
                        != self
                            .session
                            .document
                            .annotations
                            .iter()
                            .map(|a| a.group)
                            .collect::<Vec<_>>()
                {
                    self.group_feedback = Some(group_feedback::Feedback {
                        ids,
                        started: std::time::Instant::now(),
                    });
                    self.status = "Objects grouped.".into();
                }
                for id in remembered_ids {
                    if let Some(annotation) = self
                        .session
                        .document
                        .annotations
                        .iter()
                        .find(|a| a.id == id)
                        .cloned()
                    {
                        self.quick_controls.touch_snap(annotation.id);
                        self.remember_placement_template(&annotation);
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
                if live_video
                    && let (Some(tx), Some(p)) =
                        (&self.video.audio_updates, &self.session.document.video)
                {
                    let _ = tx.send(p.clone());
                }
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
            .add_filter(
                "Images, videos and ClipCraft projects",
                &["iclip", "vclip", "mp4", "png", "jpg", "jpeg"],
            )
            .add_filter("Image clips", &["iclip"])
            .add_filter("JSON project source", &["json"])
            .pick_file()
        {
            self.open_path(path);
        }
    }
    fn open_path(&mut self, path: PathBuf) {
        if video_editor::supported(&path) {
            self.open_video(path);
            return;
        }
        {
            let is_project = capture_core::project_file::is_image_project(&path)
                || capture_core::project_file::is_video_project(&path);
            let document = capture_core::project_file::open(&path);
            match document.and_then(if is_project {
                Session::new
            } else {
                Session::unsaved
            }) {
                Ok(session) => {
                    self.switch_session(session, None, Some(&path));
                    self.reset_document_tools();
                    self.remember_dialog_folder(
                        capture_core::quick_preferences::Folder::Open,
                        &path,
                    );
                    let history_label = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    self.select_only(None);
                    self.needs_render = true;
                    self.drag_start = None;
                    self.status = "Image opened. Drag on the canvas to annotate.".into();
                    self.shell.return_to_editor();
                    if let Some(id) = self.history.active.clone() {
                        self.autosave.reset(
                            &self.session.document,
                            self.history.entry(&id),
                            &history_label,
                            capture_core::history::Origin::Project,
                        );
                    } else {
                        self.record_history(
                            &history_label,
                            if is_project {
                                capture_core::history::Origin::Project
                            } else {
                                capture_core::history::Origin::Image
                            },
                        );
                    }
                }
                Err(e) => self.status = e.to_string(),
            }
        }
    }
    fn save(&mut self, save_as: bool) -> bool {
        if self.is_video() {
            return self.save_video(save_as);
        }
        if !self.commit_text() {
            return false;
        }
        let path = self
            .path
            .clone()
            .filter(|p| {
                !save_as
                    && p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("iclip"))
            })
            .or_else(|| {
                self.suspend_global_capture();
                let dialog = rfd::FileDialog::new()
                    .set_directory(
                        self.dialog_folder(capture_core::quick_preferences::Folder::Project),
                    )
                    .set_file_name(
                        self.path
                            .as_ref()
                            .map(|p| capture_core::project_file::image_path(p))
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                            .unwrap_or_else(|| "capture.iclip".into()),
                    )
                    .add_filter("ClipCraft image clip", &["iclip"]);
                dialog
                    .save_file()
                    .map(|p| capture_core::project_file::image_path(&p))
            });
        if let Some(path) = path {
            match self.session.save(&path) {
                Ok(()) => {
                    self.status = format!("Saved {}", path.display());
                    self.remember_dialog_folder(
                        capture_core::quick_preferences::Folder::Project,
                        &path,
                    );
                    self.path = Some(path);
                    return true;
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        false
    }
    fn export(&mut self) {
        if self.is_video() {
            self.export_video();
            return;
        }
        self.begin_export(false);
    }

    fn copy_all(&mut self) {
        self.copy_all_with(clipboard::copy);
    }

    fn copy_all_with(&mut self, write: impl FnOnce(&Document) -> Result<(), String>) {
        if self.is_video() {
            match self.video_frame_document().and_then(|d| write(&d)) {
                Ok(()) => self.status = "Video frame copied.".into(),
                Err(e) => self.status = e,
            }
            return;
        }
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
            && !self.clip.video_dialog
            && !self.history_dialog_open()
            && self.shell.backstage.is_none()
            && self.pending.is_none()
            && self.autosave.action.is_none()
            && self.template_picker.is_none()
            && self.new_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.crop.is_none()
            && !self.cut_out.as_ref().is_some_and(|cut| cut.busy())
            && self.capture_result.is_none()
            && !self.ocr.is_open()
            && !self.privacy_review.open
            && !self.rich_text.open
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
        self.dialog_folder(capture_core::quick_preferences::Folder::Open)
    }
    fn dialog_folder(&self, kind: capture_core::quick_preferences::Folder) -> PathBuf {
        self.quick_controls.folder(kind).unwrap_or_else(|| {
            if self.last_directory.is_dir() {
                self.last_directory.clone()
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        })
    }
    fn remember_dialog_folder(
        &mut self,
        kind: capture_core::quick_preferences::Folder,
        path: &std::path::Path,
    ) {
        if let Some(folder) = path.parent().filter(|p| p.is_dir()) {
            self.last_directory = folder.to_path_buf();
            self.quick_controls
                .remember_folder(kind, folder.to_path_buf());
        }
    }

    fn request_action(&mut self, action: FileAction, ctx: &egui::Context) {
        if matches!(action, FileAction::Capture) {
            self.show_clip(ctx);
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
            && ctx.memory(|m| m.top_modal_layer().is_none())
            && self.export_dialog.is_none()
            && !self.history_dialog_open()
            && !self.ocr.is_open()
            && !self.privacy_review.open
            && !self.rich_text.open
            && !self.ocr.busy()
            && self.pending.is_none()
            && self.autosave.action.is_none()
            && !self.view.panning
            && self.template_picker.is_none()
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
            FileAction::VideoLoaded { session, path } => self.accept_video(session, path),
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
                self.switch_session(session, None, None);
                self.reset_document_tools();
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
            FileAction::OpenPath(path) => self.open_path(path),
            FileAction::Template { session, title } => {
                self.switch_session(session, None, None);
                self.reset_document_tools();
                self.view = Default::default();
                self.path = None;
                self.select_only(None);
                self.drag_start = None;
                self.needs_render = true;
                self.shell.return_to_editor();
                self.status =
                    "Template created. Edit images, captions and sections on the canvas.".into();
                self.record_history(&title, capture_core::history::Origin::Canvas);
            }
            FileAction::FromClipboard(session) => {
                self.switch_session(session, None, None);
                self.reset_document_tools();
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
                self.switch_session(session, Some(&id), None);
                self.reset_document_tools();
                let entry = self.history.entry(&id);
                self.autosave.reset(
                    &self.session.document,
                    entry,
                    "Library image",
                    capture_core::history::Origin::Project,
                );
                self.history.active = Some(id);
                self.select_only(None);
                self.drag_start = None;
                self.needs_render = true;
                self.shell.return_to_editor();
                self.status = "Library image opened. Changes save here automatically.".into();
            }
            FileAction::Capture => self.open_clip(),
            FileAction::StartVideo(settings, request) => {
                if let Err(e) = request.options.validate() {
                    self.status = e.to_string();
                    return;
                }
                self.clip.pending_video = Some(request);
                self.clip.active = Some(settings);
                self.capture_requested = CAPTURE_ENABLED;
                ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
            FileAction::StartCapture(settings) => match settings.request() {
                Ok(settings) => {
                    self.clip.pending_video = None;
                    self.clip.active = Some(settings);
                    self.capture_requested = CAPTURE_ENABLED;
                    ctx.request_repaint_of(egui::ViewportId::ROOT);
                }
                Err(e) => self.status = e.to_string(),
            },
            FileAction::Close => {
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn receive_capture(&mut self) {
        let mode = self
            .clip
            .scroll_mode
            .as_ref()
            .and_then(|rx| rx.try_iter().last());
        if let Some(automatic) = mode {
            let mut settings = self
                .quick_controls
                .capture_for(capture_core::capture_options::Selection::Scrolling);
            settings.scroll_automatic = automatic;
            self.quick_controls.remember_capture(settings);
            if let Some(active) = &mut self.clip.active {
                active.scroll_automatic = automatic;
            }
        }
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
        if let Some(automatic) = self
            .clip
            .scroll_mode
            .take()
            .and_then(|rx| rx.try_iter().last())
        {
            let mut settings = self
                .quick_controls
                .capture_for(capture_core::capture_options::Selection::Scrolling);
            settings.scroll_automatic = automatic;
            self.quick_controls.remember_capture(settings);
            if let Some(active) = &mut self.clip.active {
                active.scroll_automatic = automatic;
            }
        }
        let settings = self.clip.active.take().unwrap_or_default();
        self.clip.restore_focus = Some(!matches!(&result, Ok(Some(_))) || settings.preview);
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
        let modal =
            crate::controls::Modal::new(egui::Id::unique("unsaved_changes")).show(ctx, |ui| {
                ui.heading("Keep your changes");
                ui.label("Automatic Library saving could not finish. Your document is still open.");
                ui.label("Save an editable project to keep your annotations.");
                ui.label("Exporting a PNG does not save the editable project.");
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    cancel = ui
                        .add(crate::controls::dialog_button("Cancel", false))
                        .clicked();
                    if ui
                        .add(crate::controls::dialog_button("Discard changes", false))
                        .clicked()
                    {
                        proceed = true;
                    }
                    if ui
                        .add(crate::controls::dialog_button("Save and continue", true))
                        .clicked()
                        && self.save(false)
                    {
                        proceed = true;
                    }
                });
            });
        if cancel || modal.should_close() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
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
                self.clip
                    .active
                    .as_ref()
                    .map(|s| s.selection.label())
                    .unwrap_or("Region")
            );
            let (mode_tx, mode_rx) = std::sync::mpsc::channel();
            self.clip.scroll_mode = Some(mode_rx);
            self.capture_result = Some(capture::start(
                frame,
                ctx.clone(),
                self.clip.active.clone().unwrap_or_default(),
                self.history.root(),
                mode_tx,
                self.clip.pending_video.take(),
            ));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if ui.input(|i| i.pointer.primary_pressed()) {
            self.session.begin_gesture();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.session.cancel_edit_gesture();
            self.needs_render = true;
        }
        self.video_tick(ui.ctx());
        chrome::record_ui_geometry(ui);
        let ctx = ui.ctx().clone();
        controls::prepare_dialog_keys(&ctx);
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
        self.show_template_picker(&ctx);
        self.show_template_management(&ctx);
        self.show_library_recovery(&ctx);
        self.replace_frame_image_request(&ctx);
        self.show_frame_library_picker(&ctx);
        self.show_new_image(&ctx);
        self.show_resize(&ctx);
        self.show_resize_canvas(&ctx);
        self.show_export(&ctx, Some(_frame));
        self.show_video_export(&ctx);
        self.show_still_frame(&ctx);
        self.show_effects_workstation(&ctx);
        self.show_voiceover_recorder(&ctx);
        self.show_page_setup(&ctx);
        self.show_image_transform(&ctx);
        self.editor_keybind_input(&ctx);

        self.shared_format_shortcuts(&ctx);
        self.clipboard_shortcuts(&ctx);
        self.pixel_grid_shortcut(&ctx);
        self.flatten_shortcut(&ctx);
        if self.editor_shortcuts_idle() && ctx.memory(|m| m.top_modal_layer().is_none()) {
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
                && (!self.shell.annotations_hidden || self.is_video())
                && self.template_picker.is_none()
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
                && (!self.shell.annotations_hidden || self.is_video())
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
                if self.selected.is_some() || self.has_video_selection() {
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
                if self.cut_out.is_none()
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
        self.video_capture_dialog(&ctx);
        self.poll_library_import();
        self.show_history_dialog(&ctx);
        self.presets.reusable.show(&ctx);
        #[cfg(windows)]
        brand::sync_window_icons(self.hotkey_owner);
        self.show_ocr(&ctx);
        self.show_privacy_review(&ctx);
        self.show_rich_text(&ctx);
        if ctx.input(|i| !i.pointer.primary_down() || !i.focused) {
            self.session.finish_gesture();
        }
        chrome::resize_edges(&ctx);
        gesture_input::latch_cursor(&ctx);
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
