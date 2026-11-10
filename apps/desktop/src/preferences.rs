//! Preferences (UX-006, UX-007, LIB-003, approved mockup
//! `artifacts/br-proposal/roadmap-ui.html`). Apply saves, Cancel leaves
//! everything as it was, and Restore defaults affects only the open section.
use crate::tips::Tip as _;
use crate::{Editor, egui, property_layout as layout};
use capture_core::quick_preferences::{
    BypassKey, CanvasBackground, Choice, Folder, Preferences as Prefs, Workspace,
};
use capture_core::toolbar::ToolId;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    General,
    Editor,
    Capture,
    Library,
    Shortcuts,
    Toolbar,
}
impl Section {
    const ALL: [Self; 6] = [
        Self::General,
        Self::Editor,
        Self::Capture,
        Self::Library,
        Self::Shortcuts,
        Self::Toolbar,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Editor => "Editor",
            Self::Capture => "Capture",
            Self::Library => "Library",
            Self::Shortcuts => "Shortcuts",
            Self::Toolbar => "Toolbar",
        }
    }
}
struct Open {
    section: Section,
    draft: Prefs,
    start_with_windows: bool,
    toolbar: (Vec<ToolId>, Vec<ToolId>),
    folders: BTreeMap<Folder, PathBuf>,
}
/// A remembered zoom (None is fit) and Properties width.
type ViewMemory = (Option<f32>, Option<f32>);
#[derive(Default)]
pub(crate) struct Dialog {
    open: Option<Open>,
    /// A zoom or panel width waiting to be remembered, and since when.
    view: Option<(ViewMemory, Instant)>,
    view_restored: bool,
    /// Where the latest capture came from, until its Library item exists.
    pub(crate) pending_source: Option<crate::capture_source::Source>,
    pub(crate) panel_width: Option<f32>,
}
impl Dialog {
    pub(crate) fn section(&self) -> Option<Section> {
        self.open.as_ref().map(|o| o.section)
    }
}

impl Editor {
    pub(crate) fn open_preferences(&mut self, section: Section) {
        let draft = self.quick_controls.snapshot();
        self.preferences.open = Some(Open {
            section,
            start_with_windows: crate::windows_startup::enabled(),
            toolbar: (draft.toolbar_order.clone(), draft.favorite_tools.clone()),
            folders: draft.folders.clone(),
            draft,
        });
    }
    #[cfg(test)]
    pub(crate) fn edit_preferences_draft(&mut self, edit: impl FnOnce(&mut Prefs)) {
        if let Some(open) = self.preferences.open.as_mut() {
            edit(&mut open.draft);
        }
    }
    /// Ctrl+Comma, from anywhere in the editor.
    pub(crate) fn preferences_shortcut(&mut self, ctx: &egui::Context) {
        if self.preferences.open.is_none()
            && !ctx.egui_wants_keyboard_input()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma))
        {
            self.open_preferences(Section::General);
        }
    }
    pub(crate) fn show_preferences(&mut self, ctx: &egui::Context) {
        let Some(mut open) = self.preferences.open.take() else {
            return;
        };
        let (mut apply, mut cancel) = (false, false);
        let screen = ctx.content_rect().size();
        let height = (screen.y - 170.).clamp(220., 560.);
        let width = (screen.x - 80.).clamp(420., 740.);
        let modal = crate::controls::Modal::new(egui::Id::unique("preferences")).show(ctx, |ui| {
            ui.set_width(width);
            crate::controls::dialog_title(ui, "Preferences");
            // Fixed places, so the rail and the section never depend on each
            // other's width or on the first frame's measurements.
            let top = ui.cursor().min;
            let rail = egui::Rect::from_min_size(top, egui::vec2(116., height));
            let body = egui::Rect::from_min_size(
                top + egui::vec2(124., 0.),
                egui::vec2(width - 124., height),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(rail), |ui| {
                for section in Section::ALL {
                    let name = section.label();
                    if crate::panel_ui::tab(ui, name, name, open.section == section, 112., true)
                        .tip_title(name)
                        .tip("Show this group of settings.")
                        .clicked()
                    {
                        open.section = section;
                        self.options.recording = None;
                    }
                }
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                ui.set_min_size(body.size());
                self.preferences_section(ui, &mut open);
            });
            ui.advance_cursor_after_rect(egui::Rect::from_min_size(top, egui::vec2(width, height)));
            ui.add_space(6.);
            ui.allocate_ui_with_layout(
                egui::vec2(width, crate::controls::DIALOG_ACTION_HEIGHT),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    apply = ui
                        .add(crate::controls::dialog_button("Apply", true))
                        .clicked();
                    cancel = ui
                        .add(crate::controls::dialog_button("Cancel", false))
                        .clicked();
                    // Shortcuts has its own Restore defaults on its screen.
                    if open.section != Section::Shortcuts
                        && ui
                            .add(crate::controls::dialog_button("Restore defaults", false))
                            .tip_title("Restore defaults")
                            .tip("Reset only the settings in this section. Apply keeps the change.")
                            .clicked()
                    {
                        self.restore_section(&mut open);
                    }
                },
            );
        });
        if self.options.recording.is_none() {
            cancel |= crate::text_edit::cancel_on_escape(&modal);
        }
        if apply {
            self.apply_preferences(ctx, &open);
        } else if cancel {
            self.cancel_preferences(ctx, &open);
        } else {
            self.preferences.open = Some(open);
        }
    }
    fn preferences_section(&mut self, ui: &mut egui::Ui, open: &mut Open) {
        if open.section == Section::Shortcuts {
            self.shortcuts_section(ui);
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("preferences-section", open.section.label()))
            .auto_shrink([false, false])
            .show(ui, |ui| match open.section {
                Section::General => general(ui, open),
                Section::Editor => editor(ui, &mut open.draft),
                Section::Capture => capture(ui, &mut open.draft),
                Section::Library => {
                    self.preferences_library(ui, &mut open.draft.workspace.keep_versions)
                }
                Section::Toolbar => self.toolbar_section(ui),
                Section::Shortcuts => {}
            });
    }
    /// Library settings: the snapshot switch, then the existing storage screens.
    pub(crate) fn preferences_library(&mut self, ui: &mut egui::Ui, keep: &mut bool) {
        crate::panel_ui::heading(ui, "Library", "Versions");
        crate::panel_ui::setting(
            ui,
            "Keep previous versions",
            "Save a copy of the last version each time a Library item changes.",
            keep,
        );
        crate::panel_ui::divider(ui);
        self.library_storage_settings(ui);
        self.storage_location_options(ui);
    }
    fn toolbar_section(&mut self, ui: &mut egui::Ui) {
        crate::panel_ui::heading(ui, "Tools", "Toolbar");
        let order = self.quick_controls.snapshot().toolbar_order;
        let mut choice = None;
        for tool in order.iter().copied() {
            let Some(info) = crate::shell::TOOLS.iter().find(|t| t.id == tool) else {
                continue;
            };
            let group = self.quick_controls.tool_group(tool);
            let section: Vec<ToolId> = order
                .iter()
                .copied()
                .filter(|t| self.quick_controls.tool_group(*t) == group)
                .collect();
            let i = section.iter().position(|t| *t == tool).unwrap_or(0);
            ui.push_id(info.name, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([150., 24.], egui::Label::new(info.name));
                    let mut favorite = self.quick_controls.tool_group(tool)
                        == capture_core::toolbar::Group::Favorites;
                    if crate::controls::toggle(ui, &mut favorite, "Favorite").changed() {
                        choice = Some(Choice::FavoriteTool { tool, favorite });
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move up"))
                        .tip_title("Move up")
                        .tip("Show this tool earlier in its toolbar section.")
                        .clicked()
                    {
                        choice = Some(Choice::MoveSectionTool {
                            tool,
                            before: section.get(i - 1).copied(),
                        });
                    }
                    if ui
                        .add_enabled(i + 1 < section.len(), egui::Button::new("Move down"))
                        .tip_title("Move down")
                        .tip("Show this tool later in its toolbar section.")
                        .clicked()
                    {
                        choice = Some(Choice::MoveSectionTool {
                            tool,
                            before: section.get(i + 2).copied(),
                        });
                    }
                });
            });
        }
        if let Some(choice) = choice {
            self.apply_toolbar_choice(choice);
        }
    }
    fn restore_section(&mut self, open: &mut Open) {
        let defaults = Prefs::default();
        let d = &mut open.draft;
        match open.section {
            Section::General => {
                d.close_to_tray = defaults.close_to_tray;
                d.ask_before_clip = defaults.ask_before_clip;
                d.workspace.show_tooltips = true;
                open.start_with_windows = false;
            }
            Section::Editor => {
                let w = Workspace {
                    panel_width: d.workspace.panel_width,
                    zoom: d.workspace.zoom,
                    show_tooltips: d.workspace.show_tooltips,
                    keep_versions: d.workspace.keep_versions,
                    ..Workspace::default()
                };
                d.workspace = w;
                d.auto_switch_tool = defaults.auto_switch_tool;
                d.canvas_snapping = defaults.canvas_snapping;
            }
            Section::Capture => {
                d.video.fps = defaults.video.fps;
                d.video.quality = defaults.video.quality;
            }
            Section::Library => d.workspace.keep_versions = true,
            Section::Toolbar => self.apply_toolbar_choice(Choice::ResetToolbar),
            Section::Shortcuts => {}
        }
    }
    fn apply_preferences(&mut self, ctx: &egui::Context, open: &Open) {
        self.quick_controls.apply_preferences(&open.draft);
        if let Err(e) = crate::windows_startup::set(open.start_with_windows) {
            self.status = e;
        }
        self.save_library_capacity(ctx);
        if self.options.draft.is_some() {
            self.apply_shortcut_draft(ctx);
        }
        self.needs_render = true;
    }
    fn cancel_preferences(&mut self, ctx: &egui::Context, open: &Open) {
        self.quick_controls
            .restore_toolbar(open.toolbar.0.clone(), open.toolbar.1.clone());
        self.quick_controls.restore_folders(open.folders.clone());
        self.discard_library_capacity(ctx);
        self.discard_shortcut_draft();
    }
    /// Keeps the window's working layout between sessions, and applies settings
    /// other parts of the app read each frame.
    pub(crate) fn sync_workspace(&mut self, ctx: &egui::Context) {
        let workspace = self.quick_controls.workspace();
        crate::tips::set_enabled(ctx, workspace.show_tooltips);
        if let Some(store) = self.history.store.as_mut() {
            store.keep_versions = workspace.keep_versions;
        }
        if self.preferences.pending_source.is_some()
            && let Some(id) = self.history.active.clone()
            && let Some(store) = self.history.store.as_mut()
            && let Some((app, site)) = self.preferences.pending_source.take()
        {
            let _ = store.set_source(&id, app, site);
        }
        if !self.preferences.view_restored {
            self.preferences.view_restored = true;
            if workspace.remember_view
                && let Some(zoom) = workspace.zoom
            {
                use crate::canvas_view::{ZoomAction, ZoomMode};
                self.view
                    .apply(ZoomAction::Set(ZoomMode::Manual(zoom)), None);
            }
        }
        if !workspace.remember_view || self.preferences.open.is_some() {
            return;
        }
        let zoom = match self.view.mode {
            crate::canvas_view::ZoomMode::Manual(zoom) => Some(zoom),
            _ => None,
        };
        let now = (zoom, self.preferences.panel_width.or(workspace.panel_width));
        if now == (workspace.zoom, workspace.panel_width) {
            self.preferences.view = None;
            return;
        }
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        match self.preferences.view {
            Some((value, since)) if value == now => {
                if since.elapsed() >= Duration::from_secs(1) {
                    self.quick_controls.remember_view(now.0, now.1);
                    self.preferences.view = None;
                } else {
                    ctx.request_repaint_after(Duration::from_millis(300));
                }
            }
            _ => {
                self.preferences.view = Some((now, Instant::now()));
                ctx.request_repaint_after(Duration::from_millis(300));
            }
        }
    }
    /// A guide over the picture only. It is never part of an export.
    pub(crate) fn paint_grid(&self, ui: &egui::Ui, transform: crate::canvas_view::Transform) {
        let Some(size) = self.quick_controls.snap_grid() else {
            return;
        };
        let step = size as f32 * transform.scale;
        if step < 4. {
            return;
        }
        let visible = transform.image.intersect(transform.viewport);
        let painter = ui.painter().with_clip_rect(visible);
        let stroke =
            egui::Stroke::new(1., egui::Color32::from_rgba_unmultiplied(120, 200, 230, 70));
        let first = |start: f32, low: f32| start + ((low - start) / step).floor() * step;
        let mut x = first(transform.image.left(), visible.left());
        while x <= visible.right() {
            painter.vline(x, visible.y_range(), stroke);
            x += step;
        }
        let mut y = first(transform.image.top(), visible.top());
        while y <= visible.bottom() {
            painter.hline(visible.x_range(), y, stroke);
            y += step;
        }
    }
}

fn general(ui: &mut egui::Ui, open: &mut Open) {
    crate::panel_ui::heading(ui, "Startup", "General");
    crate::panel_ui::setting(
        ui,
        "Start with Windows",
        "Open ClipDesigner when you sign in.",
        &mut open.start_with_windows,
    );
    let d = &mut open.draft;
    crate::panel_ui::setting(
        ui,
        "Keep running in the tray",
        "Keep capture shortcuts available when the window closes.",
        &mut d.close_to_tray,
    );
    crate::panel_ui::setting(
        ui,
        "Ask before Capture Clip",
        "Show the capture settings first. Hotkeys and the tray capture directly.",
        &mut d.ask_before_clip,
    );
    crate::panel_ui::setting(
        ui,
        "Show tooltips",
        "Explain controls when you hover over them.",
        &mut d.workspace.show_tooltips,
    );
}
fn editor(ui: &mut egui::Ui, d: &mut Prefs) {
    crate::panel_ui::heading(ui, "Properties", "Editor");
    crate::panel_ui::setting(
        ui,
        "Switch tools on selection",
        "Use the selected object's tool automatically.",
        &mut d.auto_switch_tool,
    );
    crate::panel_ui::divider(ui);
    crate::panel_ui::heading(ui, "Snapping", "Snapping");
    crate::panel_ui::setting(
        ui,
        "Canvas snapping",
        "The quick on or off for all of these, also in the footer.",
        &mut d.canvas_snapping,
    );
    let w = &mut d.workspace;
    for (name, text, value) in [
        (
            "Snap while moving",
            "Objects line up with each other, the canvas edges and the grid.",
            &mut w.snap_move,
        ),
        (
            "Snap while resizing",
            "Dragged handles land on the same places.",
            &mut w.snap_resize,
        ),
        (
            "Snap while drawing",
            "New shapes start and end on the same places.",
            &mut w.snap_draw,
        ),
        (
            "Show the grid",
            "A guide over the picture. It never exports.",
            &mut w.show_grid,
        ),
    ] {
        crate::panel_ui::setting(ui, name, text, value);
    }
    if w.show_grid {
        layout::slider(
            ui,
            "Grid size",
            egui::Slider::new(&mut w.grid_size, 4..=200).suffix(" px"),
        )
        .tip("The distance between grid lines.");
    }
    layout::choice(
        ui,
        "Hold to bypass",
        &mut w.bypass,
        &[
            (BypassKey::Ctrl, "Ctrl"),
            (BypassKey::Shift, "Shift"),
            (BypassKey::Alt, "Alt"),
        ],
        "Hold this key to turn snapping off for one drag.",
    );
    crate::panel_ui::divider(ui);
    crate::panel_ui::heading(ui, "Canvas", "Workspace");
    layout::choice(
        ui,
        "Background",
        &mut w.background,
        &[
            (CanvasBackground::Charcoal, "Charcoal"),
            (CanvasBackground::Graphite, "Graphite"),
            (CanvasBackground::Black, "Black"),
            (CanvasBackground::Light, "Light gray"),
        ],
        "The color around the picture.",
    );
    crate::panel_ui::setting(
        ui,
        "Remember zoom and panel sizes",
        "Start with the zoom and Properties width you last used.",
        &mut w.remember_view,
    );
}
fn capture(ui: &mut egui::Ui, d: &mut Prefs) {
    use capture_core::video::Quality;
    crate::panel_ui::heading(ui, "Camera", "Video recording");
    layout::choice(
        ui,
        "Frame rate",
        &mut d.video.fps,
        &[(15, "15 fps"), (30, "30 fps"), (60, "60 fps")],
        "How many frames new recordings capture each second.",
    );
    let qualities = Quality::ALL.map(|q| (q, q.label()));
    layout::choice(
        ui,
        "Quality",
        &mut d.video.quality,
        &qualities,
        "Higher quality makes larger recordings.",
    );
}
