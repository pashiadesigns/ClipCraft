use crate::{Editor, Kind, egui};
use capture_core::toolbar::ToolId;
use egui::Color32;

pub(super) const SURROUND: Color32 = Color32::from_rgb(37, 39, 41);
pub(super) const BAR: Color32 = Color32::from_rgb(46, 51, 53);
const PANE: Color32 = Color32::from_rgb(56, 62, 65);
pub const CANVAS: Color32 = Color32::from_rgb(19, 22, 22);
pub(super) const ACCENT: Color32 = Color32::from_rgb(24, 145, 193);

#[derive(Clone, Copy)]
pub struct Tool {
    pub(super) id: ToolId,
    pub name: &'static str,
    kind: Option<Kind>,
}
pub const TOOLS: &[Tool] = &[
    Tool {
        id: ToolId::Move,
        name: "Move",

        kind: None,
    },
    Tool {
        id: ToolId::Lines,
        name: "Lines",

        kind: Some(Kind::Arrow),
    },
    Tool {
        id: ToolId::Text,
        name: "Text",

        kind: Some(Kind::Text),
    },
    Tool {
        id: ToolId::Shape,
        name: "Shape",

        kind: Some(Kind::Rectangle),
    },
    Tool {
        id: ToolId::Icons,
        name: "Icons",

        kind: None,
    },
    Tool {
        id: ToolId::Steps,
        name: "Steps",

        kind: Some(Kind::Step),
    },
    Tool {
        id: ToolId::Penlighter,
        name: "Penlighter",

        kind: Some(Kind::Pen),
    },
    Tool {
        id: ToolId::Obfuscate,
        name: "Obfuscate",

        kind: Some(Kind::Blur),
    },
    Tool {
        id: ToolId::Crop,
        name: "Crop",

        kind: None,
    },
    Tool {
        id: ToolId::CutOut,
        name: "Cut Out",

        kind: None,
    },
    Tool {
        id: ToolId::Eraser,
        name: "Eraser",

        kind: Some(Kind::Eraser),
    },
    Tool {
        id: ToolId::Focus,
        name: "Focus",
        kind: Some(Kind::Rectangle),
    },
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum PropertyTab {
    #[default]
    Tool,
    Objects,
}

pub struct Shell {
    pub(super) focus: crate::focus_properties::State,
    pub(super) preview_ui: bool,
    pub pixel_grid: bool,
    pub annotations_hidden: bool,
    pub raster_preview: bool,
    pub redaction_labels: bool,
    pub resize_anchor: Option<egui::Rect>,
    pub resize_advanced: bool,
    pub canvas_relative: bool,
    pub(super) properties: bool,
    pub(super) property_tab: PropertyTab,
    pub(super) ribbon_tab: crate::ribbon::RibbonTab,

    pub(super) library: bool,
    pub(super) library_tab_rect: Option<egui::Rect>,
    pub(super) backstage: Option<crate::backstage::Page>,
    pub(super) recent_query: String,
    pub(super) recent_limit: usize,
    pub(crate) help: bool,
}
impl Default for Shell {
    fn default() -> Self {
        Self {
            focus: Default::default(),
            preview_ui: false,
            pixel_grid: false,
            annotations_hidden: false,
            raster_preview: false,
            redaction_labels: true,
            resize_anchor: None,
            resize_advanced: false,
            canvas_relative: false,
            properties: true,
            property_tab: PropertyTab::Tool,
            ribbon_tab: crate::ribbon::RibbonTab::Insert,

            library: false,
            library_tab_rect: None,
            backstage: None,
            recent_query: String::new(),
            recent_limit: 10,
            help: false,
        }
    }
}
impl Shell {
    pub(crate) fn library_open(&self) -> bool {
        self.library
    }
    pub fn return_to_editor(&mut self) {
        self.preview_ui = false;
        self.library = false;
        self.backstage = None;
    }
    pub fn canvas_enabled(&self) -> bool {
        !self.library && self.backstage.is_none()
    }
}

pub fn configure(ctx: &egui::Context) {
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANE;
    style.visuals.window_fill = BAR;
    style.visuals.override_text_color = Some(Color32::from_gray(240));
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(73, 80, 83);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(86, 100, 105);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(56, 60, 63);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(86, 100, 105);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(65, 112, 133);
    style.visuals.widgets.open.weak_bg_fill = Color32::from_rgb(65, 85, 96);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(65, 112, 133);
    style.visuals.widgets.open.bg_fill = Color32::from_rgb(65, 85, 96);
    style.visuals.text_edit_bg_color = Some(Color32::from_rgb(65, 71, 74));
    // Allocate a themed rail only while content overflows; never cover controls.
    style.spacing.scroll = egui::style::ScrollStyle {
        // Allocate instantly when needed. Animated layout widths move canvas
        // coordinates during a drag; the rail itself may still fade in.
        floating: true,
        floating_allocated_width: 13.,
        foreground_color: false,
        bar_width: 9.,
        floating_width: 6.,
        handle_min_length: 28.,
        dormant_background_opacity: 1.,
        dormant_handle_opacity: 1.,
        active_background_opacity: 1.,
        active_handle_opacity: 1.,
        interact_background_opacity: 1.,
        interact_handle_opacity: 1.,
        ..egui::style::ScrollStyle::floating()
    };
    style.visuals.extreme_bg_color = Color32::from_rgb(43, 49, 52);
    style.spacing.slider_rail_height = 4.0;
    style.visuals.slider_trailing_fill = false;
    style.spacing.item_spacing = egui::vec2(6.0, 7.0);
    ctx.set_style_of(egui::Theme::Dark, style);
    crate::controls::configure(ctx);
}

#[cfg(test)]
pub(super) fn tool_button(ui: &mut egui::Ui, tool: Tool, selected: bool) -> egui::Response {
    tool_button_label(ui, tool, tool.name, selected)
}

/// Common geometry and selection treatment for ribbon tools and capture methods.
pub(super) fn ribbon_tile(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    selected: bool,
    color: Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(72., 58.), egui::Sense::click_and_drag());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            4.,
            if selected {
                Color32::from_rgb(65, 74, 78)
            } else {
                ui.visuals().widgets.hovered.weak_bg_fill
            },
        );
    }
    if selected {
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(2., ACCENT),
        );
    }
    let draw_icon = if matches!(icon, "Voiceover" | "Narration" | "Audio effects") {
        crate::video_editor::ui::icon
    } else {
        crate::icons::draw
    };
    draw_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0., 19.),
            egui::vec2(28., 28.),
        ),
        icon,
        color,
    );
    ui.painter().text(
        rect.center_top() + egui::vec2(0., 43.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.),
        Color32::WHITE,
    );
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("ribbon-tile", label)),
            (rect, selected, ui.is_enabled()),
        )
    });
    response
}

pub(super) fn tool_button_label(
    ui: &mut egui::Ui,
    tool: Tool,
    label: &str,
    selected: bool,
) -> egui::Response {
    let color = match tool.name {
        "Text" => Color32::from_rgb(225, 190, 95),
        "Lines" => Color32::from_rgb(75, 170, 235),
        "Shape" => Color32::from_rgb(81, 190, 165),
        "Steps" => Color32::from_rgb(241, 151, 69),
        _ => Color32::WHITE,
    };
    ribbon_tile(ui, tool.name, label, selected, color)
}

impl Editor {
    pub(super) fn choose_tool(&mut self, index: usize) {
        if self.is_video()
            && matches!(
                TOOLS[index].id,
                ToolId::Crop | ToolId::CutOut | ToolId::Eraser
            )
        {
            return;
        }
        if self.is_video() {
            self.video.pause();
        }
        if !self.commit_text() {
            return;
        }
        self.cancel_gesture();
        if self.shell.annotations_hidden {
            self.shell.annotations_hidden = false;
            self.needs_render = true;
        }

        self.shell.library = false;

        self.shell.properties = true;
        self.shell.property_tab = PropertyTab::Tool;
        let tool = TOOLS[index];
        self.shell.focus.active = tool.id == ToolId::Focus;
        self.icon_library.active = tool.id == ToolId::Icons;
        if !self.icon_library.active {
            self.icon_library.disarm();
        }
        if tool.id == ToolId::Focus {
            self.tool = Some(if self.shell.focus.shape == 2 {
                Kind::Ellipse
            } else {
                Kind::Rectangle
            });
            self.select_only(None);
            self.status =
                "Focus: draw a clear window; Blur and Darken affect only content below it.".into();
            return;
        }
        if tool.id == ToolId::Icons {
            self.tool = None;
            self.select_only(None);
            return;
        }
        if tool.name == "Cut Out" {
            self.begin_cut_out();
            return;
        }

        if tool.name == "Crop" {
            self.begin_crop();
            return;
        }

        self.tool = self
            .selected_annotation()
            .filter(|a| Self::tool_for_kind(Some(a.kind)) == tool.id)
            .map(|a| a.kind)
            .or_else(|| {
                self.presets
                    .drawing_styles
                    .iter()
                    .rev()
                    .find(|s| Self::tool_for_kind(Some(s.kind)) == tool.id)
                    .map(|s| s.kind)
            })
            .or_else(|| self.quick_controls.recent_drawing_kind(tool.id))
            .or(tool.kind);
        if self
            .selected_annotation()
            .is_some_and(|a| !Self::tool_matches(self.tool, a.kind))
        {
            self.select_only(None);
        }
        self.sync_step_sequence();
        if tool.kind == Some(Kind::Blur) {
            self.width = if self.tool == Some(Kind::Pixelate) {
                8
            } else {
                4
            };
        } else if tool.kind == Some(Kind::Eraser) {
            self.select_only(None);
            self.width = 24;
            self.presets.alpha = 255;
        } else if tool.kind == Some(Kind::Highlighter) {
            self.color = [255, 255, 0];
            self.width = 16;
            self.presets.alpha = 80;
        } else if matches!(tool.kind, Some(Kind::Pen | Kind::Line)) {
            self.width = 3;
            self.presets.alpha = 255;
        }
        self.restore_drawing_defaults(self.tool);
        self.status = match tool.id {
            ToolId::Move => "Select an object, then drag it or its handles.".into(),
            ToolId::Steps => "Steps: click to place the next marker; select an existing marker to reuse its formatting.".into(),
            _ => format!("{}: drag on the canvas.", tool.name),
        };
    }

    pub(super) fn tool_for_kind(kind: Option<Kind>) -> ToolId {
        match kind {
            None | Some(Kind::Image) => ToolId::Move,
            Some(Kind::Arrow | Kind::Line) => ToolId::Lines,
            Some(Kind::Rectangle | Kind::Ellipse | Kind::Polygon | Kind::Callout) => ToolId::Shape,
            Some(Kind::Text) => ToolId::Text,
            Some(Kind::Step) => ToolId::Steps,
            Some(Kind::Pen | Kind::Highlighter) => ToolId::Penlighter,
            Some(Kind::Redact | Kind::Blur | Kind::Pixelate) => ToolId::Obfuscate,
            Some(Kind::Eraser) => ToolId::Eraser,
        }
    }
    pub(super) fn current_tool(&self) -> Tool {
        let id = if self.shell.focus.active
            && matches!(self.tool, Some(Kind::Rectangle | Kind::Ellipse))
        {
            ToolId::Focus
        } else if self.icon_library.active {
            ToolId::Icons
        } else if self.cut_out.is_some() {
            ToolId::CutOut
        } else if self.crop.is_some() {
            ToolId::Crop
        } else {
            Self::tool_for_kind(self.tool)
        };
        *TOOLS.iter().find(|t| t.id == id).expect("supported tool")
    }

    pub fn show_shell(&mut self, ui: &mut egui::Ui) {
        if self.is_video() && (self.shell.library || self.shell.backstage.is_some()) {
            self.video.pause();
        }
        self.preview_ui_input(ui.ctx());
        self.show_quick_access(ui);
        if self.shell.backstage.is_some() {
            self.show_backstage(ui);
            return;
        }
        self.show_ribbon(ui);
        self.show_footer(ui);
        if !self.is_video() && !self.shell.preview_ui {
            self.show_recent_history(ui);
        }
        let properties_first = self.is_video() && self.video.properties_priority;
        if properties_first {
            self.show_properties_panel(ui);
        }
        if self.is_video() && !self.shell.preview_ui {
            self.show_recent_history(ui);
        }
        self.show_video_timeline(ui);
        if !properties_first {
            self.show_properties_panel(ui);
        }
    }

    fn show_properties_panel(&mut self, ui: &mut egui::Ui) {
        if self.shell.properties
            && !self.shell.preview_ui
            && !self.shell.library
            && self.crop.is_none()
        {
            egui::Panel::right(if self.is_video() {
                "video-properties"
            } else {
                "properties"
            })
            .default_size(390.)
            .min_size(260.)
            .max_size(460.)
            .resizable(true)
            .show(ui, |ui| {
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("properties-panel"), ui.max_rect())
                });
                if self.is_video() {
                    self.video_properties_pane(ui);
                    return;
                }
                self.image_properties_pane(ui);
            });
        }
    }

    #[cfg(test)]
    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        if self.is_video() {
            self.video_properties(ui);
        } else {
            self.image_properties(ui);
        }
    }
    pub(super) fn image_properties(&mut self, ui: &mut egui::Ui) {
        if self.shell.property_tab == PropertyTab::Objects {
            self.object_properties(ui);
            return;
        }
        if self.shell.annotations_hidden {
            ui.label("Annotations are hidden for comparison.");
            crate::property_layout::section_help(
                ui,
                "Properties",
                "Use Show annotations to resume editing. Saved objects and exports are unchanged.",
            );
            return;
        }
        if self.icon_library.active
            && !self
                .selected_annotation()
                .is_some_and(|a| a.kind == Kind::Image)
        {
            self.icon_properties(ui);
            return;
        }
        if self.text_draft.is_some() {
            self.inline_text_properties(ui);
            return;
        }
        if self.property_suppressed {
            ui.disable();
        }
        if self.current_tool().id == ToolId::Move && self.selected.is_none() {
            self.move_functions(ui);
            return;
        }
        if self.cut_out.is_some() {
            self.cut_out_properties(ui);
            return;
        }

        let mut tool = self
            .selected_annotation()
            .and_then(|a| {
                TOOLS
                    .iter()
                    .find(|t| t.id == Self::tool_for_kind(Some(a.kind)))
            })
            .copied()
            .unwrap_or_else(|| self.current_tool());
        tool.kind = self
            .selected_annotation()
            .map(|a| a.kind)
            .or(self.tool)
            .or(tool.kind);
        let image_object = self
            .selected_annotation()
            .is_some_and(|a| a.kind == Kind::Image || a.is_image_frame());
        if self
            .selected_annotation()
            .is_some_and(|a| a.is_image_frame())
        {
            crate::property_layout::section_help(
                ui,
                "Image frame styles",
                "Choose a frame shape; double-click the frame to edit its image.",
            );
            self.frame_style_choices(ui);
        }
        if !image_object {
            ui.vertical_centered(|ui| {
                self.presets_header(
                    ui,
                    self.selected_annotation()
                        .map(|a| a.kind)
                        .or(self.tool)
                        .or(tool.kind),
                );
            });
            if tool.kind == Some(Kind::Arrow) {
                self.arrow_styles(ui);
            } else if let Some(kind) = self
                .selected_annotation()
                .map(|a| a.kind)
                .or(self.tool)
                .or(tool.kind)
            {
                self.object_templates(ui, kind);
            }
        }
        ui.vertical_centered(|ui| {
            let help = if image_object {
                "Pasted images remain independent objects. Move, resize, rotate or adjust opacity without changing their original pixels. Undo restores each edit; Delete reveals the canvas underneath.".to_owned()
            } else {
                crate::property_layout::tool_help(tool.kind).to_owned()
            };
            crate::property_layout::section_help(ui, "Properties", &help);

        });
        if matches!(tool.kind, Some(Kind::Redact | Kind::Blur | Kind::Pixelate))
            && crate::video_editor::ui::button(
                ui,
                "Sensitive search",
                "Find Sensitive Info…",
                205.,
                true,
            )
            .clicked()
        {
            self.open_privacy_review();
        }
        if tool.kind == Some(Kind::Redact) {
            ui.label("Solid black · fully opaque");
            if crate::controls::toggle(
                ui,
                &mut self.shell.redaction_labels,
                "Show redaction labels",
            )
            .changed()
            {
                self.needs_render = true;
            }
            ui.weak("Export labels are controlled separately. Redaction remains opaque.");
        }
        if self.selected_annotation().is_none() && self.tool.is_some() {
            self.image_tool_defaults(ui);
        } else if self.selected.is_none() {
        }
        let count = self.selected_objects().len();
        if count > 1 {
            ui.strong(format!("{count} objects selected"));
        }
        if self.selection_same_kind()
            && let Some(annotation) = self.selected_annotation()
        {
            if annotation.kind == Kind::Text {
                self.dedicated_text_properties(ui);
                return;
            }
            let mut edited = annotation.clone();
            if let Some(bitmap) = &edited.image
                && bitmap.focus().is_none()
                && let Some(focus) = self.session.document.image_focus(&edited)
                && let Ok(focused) = bitmap.with_focus(focus)
            {
                edited.image = Some(std::sync::Arc::new(focused));
            }
            let before_properties = edited.clone();
            let rotation_requested = crate::property_layout::annotation_properties(
                ui,
                &mut edited,
                [self.session.document.width, self.session.document.height],
            );
            if edited.kind == Kind::Step {
                self.step_properties(ui);
            }
            if edited != before_properties || rotation_requested {
                self.preview_property_edit_with_rotation(edited, rotation_requested);
            }
        }
    }

    pub(super) fn image_tool_defaults(&mut self, ui: &mut egui::Ui) {
        let before_defaults = (self.color, self.width, self.presets.alpha, self.tool);
        if !matches!(self.tool, Some(Kind::Redact | Kind::Eraser))
            && !crate::obscure_properties::is_effect(self.tool)
        {
            ui.horizontal(|ui| {
                ui.label("Color");
                let preview = match self.tool {
                    Some(Kind::Text) => crate::color_widgets::Preview::Text,
                    Some(Kind::Rectangle | Kind::Ellipse | Kind::Polygon) => {
                        crate::color_widgets::Preview::Outline
                    }
                    _ => crate::color_widgets::Preview::Fill,
                };
                crate::color_widgets::rgb_preview(ui, &mut self.color, preview);
            });
        }
        if crate::obscure_properties::is_effect(self.tool) {
            crate::property_layout::slider(
                ui,
                "New intensity",
                egui::Slider::new(&mut self.width, 1..=32),
            );
        } else if self.tool == Some(Kind::Step) {
            self.step_properties(ui);
        } else if matches!(self.tool, Some(Kind::Text | Kind::Callout)) {
        } else if self.tool == Some(Kind::Highlighter) && self.drawing_rectangle_highlighter() {
            crate::property_layout::slider(
                ui,
                "Opacity",
                egui::Slider::new(&mut self.presets.alpha, 0..=255),
            );
        } else if self.tool != Some(Kind::Redact) {
            crate::property_layout::slider(
                ui,
                "Width",
                egui::Slider::new(
                    &mut self.width,
                    1..=if matches!(
                        self.tool,
                        Some(
                            Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlighter | Kind::Eraser
                        )
                    ) {
                        32
                    } else {
                        16
                    },
                ),
            );
            if matches!(
                self.tool,
                Some(Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlighter | Kind::Eraser)
            ) {
                crate::property_layout::slider(
                    ui,
                    "Opacity",
                    egui::Slider::new(&mut self.presets.alpha, 0..=255),
                );
            }
        }
        if matches!(
            self.tool,
            Some(Kind::Rectangle | Kind::Ellipse | Kind::Polygon)
        ) {
            let mut kind = self.tool;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut kind, Some(Kind::Rectangle), "Rectangle");
                ui.selectable_value(&mut kind, Some(Kind::Ellipse), "Ellipse");
                ui.selectable_value(&mut kind, Some(Kind::Polygon), "Polygon");
            });

            if self.tool == Some(Kind::Polygon) {
                crate::property_layout::slider(
                    ui,
                    "New sides",
                    egui::Slider::new(&mut self.shape_sides, 3..=12),
                );
            }
            if kind != self.tool {
                self.cancel_gesture();
                self.tool = kind;
                self.restore_drawing_defaults(self.tool);
                self.select_only(None);
                self.status =
                    "Drag to draw the selected shape. Move edits its handles live.".into();
            }
        }
        if before_defaults.3 == self.tool
            && (before_defaults.0, before_defaults.1, before_defaults.2)
                != (self.color, self.width, self.presets.alpha)
            && let Some(kind) = self.tool
        {
            let example = self.new_annotation(kind, [20.0, 20.0], [80.0, 60.0]);
            self.remember_annotation_style(&example);
        }
        if let Some(kind) = self.tool {
            let mut example = if kind == Kind::Step {
                self.step_marker([50., 50.])
                    .unwrap_or_else(|_| self.new_annotation(kind, [20., 20.], [80., 60.]))
            } else {
                self.new_annotation(kind, [20., 20.], [80., 60.])
            };
            let before = (example.shadow, example.line_style);
            crate::line_properties::show(ui, &mut example);
            crate::shadow_properties::show(ui, &mut example);
            if (example.shadow, example.line_style) != before {
                self.remember_annotation_style(&example);
            }
        }
    }

    fn object_properties(&mut self, ui: &mut egui::Ui) {
        self.show_objects(ui);
    }

    pub fn show_workspace(&mut self, ui: &mut egui::Ui) {
        if self.shell.library {
            let frame = egui::Frame::new()
                .fill(CANVAS)
                .corner_radius(10)
                .stroke(egui::Stroke::new(2., crate::tabs::ACTIVE_BLUE))
                .inner_margin(2)
                .show(ui, |ui| {
                    ui.set_min_size(ui.available_size());
                    self.show_history(ui, false);
                });
            if let Some(active) = self.shell.library_tab_rect {
                // A separate screen-clipped layer bridges the two panels.
                // ui.painter().with_clip_rect cannot widen the panel's clip.
                ui.ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Middle,
                        egui::Id::unique("library-tab-join"),
                    ))
                    .rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(active.left(), active.bottom() - 4.),
                            egui::pos2(active.right(), frame.response.rect.top() + 2.),
                        ),
                        0.,
                        crate::tabs::ACTIVE_BLUE,
                    );
            }
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::unique("library-surround"), frame.response.rect)
            });
        } else {
            self.show_canvas(ui);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn library_disables_canvas() {
        let mut shell = Shell::default();
        assert!(shell.canvas_enabled());
        shell.library = true;
        assert!(!shell.canvas_enabled());
    }
}
