use crate::{Editor, Kind, egui, readiness};
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
    pub feature: &'static str,
    kind: Option<Kind>,
    implemented: bool,
}
pub const TOOLS: &[Tool] = &[
    Tool {
        id: ToolId::Favorites,
        name: "Favorites",
        feature: "UX-005",
        kind: None,
        implemented: false,
    },
    Tool {
        id: ToolId::Arrow,
        name: "Arrow",
        feature: "EDIT-001",
        kind: Some(Kind::Arrow),
        implemented: true,
    },
    Tool {
        id: ToolId::Text,
        name: "Text",
        feature: "EDIT-004",
        kind: Some(Kind::Text),
        implemented: true,
    },
    Tool {
        id: ToolId::Callout,
        name: "Callout",
        feature: "EDIT-006",
        kind: Some(Kind::Callout),
        implemented: true,
    },
    Tool {
        id: ToolId::Shape,
        name: "Shape",
        feature: "EDIT-009",
        kind: Some(Kind::Rectangle),
        implemented: true,
    },
    Tool {
        id: ToolId::Stamp,
        name: "Stamp",
        feature: "EDIT-018",
        kind: None,
        implemented: true,
    },
    Tool {
        id: ToolId::Selection,
        name: "Selection",
        feature: "EDIT-043",
        kind: None,
        implemented: true,
    },
    Tool {
        id: ToolId::Fill,
        name: "Fill",
        feature: "EDIT-042",
        kind: None,
        implemented: true,
    },
    Tool {
        id: ToolId::Move,
        name: "Move",
        feature: "EDIT-005",
        kind: None,
        implemented: true,
    },
    Tool {
        id: ToolId::Blur,
        name: "Blur",
        feature: "EDIT-040",
        kind: Some(Kind::Blur),
        implemented: true,
    },
    Tool {
        id: ToolId::Simplify,
        name: "Simplify",
        feature: "EDIT-021",
        kind: Some(Kind::Simplify),
        implemented: true,
    },
    Tool {
        id: ToolId::MagicWand,
        name: "Magic Wand",
        feature: "EDIT-015",
        kind: None,
        implemented: false,
    },
    Tool {
        id: ToolId::Crop,
        name: "Crop",
        feature: "EDIT-039",
        kind: None,
        implemented: true,
    },
    Tool {
        id: ToolId::CutOut,
        name: "Cut Out",
        feature: "EDIT-044",
        kind: None,
        implemented: true,
    },
    Tool {
        id: ToolId::Pen,
        name: "Pen",
        feature: "EDIT-013",
        kind: Some(Kind::Pen),
        implemented: true,
    },
    Tool {
        id: ToolId::Line,
        name: "Line",
        feature: "EDIT-013",
        kind: Some(Kind::Line),
        implemented: true,
    },
    Tool {
        id: ToolId::Highlighter,
        name: "Highlighter",
        feature: "EDIT-013",
        kind: Some(Kind::Highlighter),
        implemented: true,
    },
    Tool {
        id: ToolId::Step,
        name: "Step",
        feature: "EDIT-034",
        kind: Some(Kind::Step),
        implemented: true,
    },
    Tool {
        id: ToolId::Eraser,
        name: "Eraser",
        feature: "EDIT-041",
        kind: Some(Kind::Eraser),
        implemented: true,
    },
    Tool {
        id: ToolId::Magnify,
        name: "Magnify",
        feature: "EDIT-020",
        kind: None,
        implemented: false,
    },
    Tool {
        id: ToolId::Redact,
        name: "Redact",
        feature: "EDIT-003",
        kind: Some(Kind::Redact),
        implemented: true,
    },
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum PropertyTab {
    #[default]
    Tool,
    Objects,
}

pub struct Shell {
    pub badges: bool,
    pub pixel_grid: bool,
    pub annotations_hidden: bool,
    pub raster_preview: bool,
    pub resize_anchor: Option<egui::Rect>,
    pub resize_advanced: bool,
    pub canvas_relative: bool,
    pub planned_tool: Option<usize>,
    pub(super) properties: bool,
    pub(super) property_tab: PropertyTab,
    pub(super) ribbon_tab: crate::ribbon::RibbonTab,
    pub(super) effects: bool,
    pub(super) library: bool,
    pub(super) library_tab_rect: Option<egui::Rect>,
    pub(super) backstage: Option<crate::backstage::Page>,
    pub(super) recent_query: String,
    pub(crate) help: bool,
}
impl Default for Shell {
    fn default() -> Self {
        Self {
            badges: true,
            pixel_grid: false,
            annotations_hidden: false,
            raster_preview: false,
            resize_anchor: None,
            resize_advanced: false,
            canvas_relative: false,
            planned_tool: None,
            properties: true,
            property_tab: PropertyTab::Tool,
            ribbon_tab: crate::ribbon::RibbonTab::Home,
            effects: false,
            library: false,
            library_tab_rect: None,
            backstage: None,
            recent_query: String::new(),
            help: false,
        }
    }
}
impl Shell {
    pub(crate) fn library_open(&self) -> bool {
        self.library
    }
    pub fn return_to_editor(&mut self) {
        self.library = false;
        self.backstage = None;
        self.planned_tool = None;
    }
    pub fn canvas_enabled(&self) -> bool {
        self.planned_tool.is_none() && !self.library && self.backstage.is_none()
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
    // Shared scrollbars retain a zero-width floating allocation: overflow must
    // not resize property rows or shift a live canvas gesture.
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: true,
        floating_allocated_width: 0.,
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

pub(super) fn badge(ui: &mut egui::Ui, visible: bool, id: &str) {
    if !visible {
        return;
    }
    let badge = readiness::badge(id);
    let color = if badge.ready {
        Color32::from_rgb(28, 113, 60)
    } else {
        Color32::from_rgb(154, 46, 47)
    };
    let label = badge.label.replace("alpha.", "α.");
    let galley = ui.painter().layout_no_wrap(
        label.clone(),
        egui::FontId::proportional(10.0),
        Color32::WHITE,
    );
    let (rect, response) =
        ui.allocate_exact_size(galley.size() + egui::vec2(8.0, 4.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
    ui.painter()
        .galley(rect.min + egui::vec2(4.0, 2.0), galley, Color32::WHITE);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &label));
    response.on_hover_text(badge.detail);
}

fn menu_item(ui: &mut egui::Ui, badges: bool, label: &str, id: &str, enabled: bool) -> bool {
    let mut clicked = false;
    // Reserve the full label before allocating the separate readiness badge.
    // Menu styles can otherwise truncate long shortcut labels to the row width.
    let label_width = ui
        .painter()
        .layout_no_wrap(
            label.into(),
            egui::TextStyle::Button.resolve(ui.style()),
            ui.visuals().text_color(),
        )
        .size()
        .x;
    let button_width = (label_width + 2.0 * ui.spacing().button_padding.x).max(150.0);
    ui.horizontal(|ui| {
        clicked = ui
            .add_enabled(
                enabled,
                egui::Button::new(label)
                    .frame(false)
                    .wrap_mode(egui::TextWrapMode::Extend)
                    .min_size(egui::vec2(button_width, 22.0)),
            )
            .clicked();
        badge(ui, badges, id);
    });
    if clicked {
        ui.close();
    }
    clicked
}

pub(super) fn tool_button(
    ui: &mut egui::Ui,
    tool: Tool,
    selected: bool,
    badges: bool,
) -> egui::Response {
    tool_button_label(ui, tool, tool.name, selected, badges)
}

pub(super) fn tool_button_label(
    ui: &mut egui::Ui,
    tool: Tool,
    label: &str,
    selected: bool,
    badges: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(72.0, if badges { 76.0 } else { 58.0 }),
        egui::Sense::click_and_drag(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            4.0,
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
            egui::Stroke::new(2.0, ACCENT),
        );
    }
    crate::icons::draw(
        ui.painter(),
        egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0.0, 19.0),
            egui::vec2(28.0, 28.0),
        ),
        tool.name,
        match tool.name {
            "Text" => Color32::from_rgb(225, 190, 95),
            "Arrow" | "Line" => Color32::from_rgb(75, 170, 235),
            "Shape" => Color32::from_rgb(81, 190, 165),
            "Callout" => Color32::from_rgb(187, 133, 231),
            "Step" => Color32::from_rgb(241, 151, 69),
            "Magic Wand" => Color32::from_rgb(187, 154, 239),
            "Fill" | "Stamp" | "Favorites" | "Simplify" | "Cut Out" | "Pen" | "Highlighter"
            | "Eraser" | "Redact" => Color32::from_gray(238),
            _ => {
                if selected {
                    ACCENT
                } else {
                    Color32::WHITE
                }
            }
        },
    );
    ui.painter().text(
        rect.center_top() + egui::vec2(0.0, 43.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
    let state = readiness::badge(tool.feature);
    if badges {
        let b = egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0.0, 62.0),
            egui::vec2(69.0, 15.0),
        );
        ui.painter().rect_filled(
            b,
            2.0,
            if state.ready {
                Color32::from_rgb(28, 113, 60)
            } else {
                Color32::from_rgb(154, 46, 47)
            },
        );
        ui.painter().text(
            b.center(),
            egui::Align2::CENTER_CENTER,
            state.label.replace("alpha.", "α."),
            egui::FontId::proportional(10.0),
            Color32::WHITE,
        );
    }
    response
}

impl Editor {
    pub(super) fn choose_tool(&mut self, index: usize) {
        if !self.commit_text() {
            return;
        }
        self.cancel_gesture();
        if self.shell.annotations_hidden {
            self.shell.annotations_hidden = false;
            self.needs_render = true;
        }
        self.fill_tool.active = false;
        self.shell.library = false;
        self.shell.effects = false;
        self.shell.properties = true;
        self.shell.property_tab = PropertyTab::Tool;
        let tool = TOOLS[index];
        self.stamps.active = tool.id == ToolId::Stamp;
        if !self.stamps.active {
            self.stamps.disarm();
        }
        if matches!(tool.id, ToolId::Crop | ToolId::CutOut)
            && self.quick_controls.canvas_tool() != tool.id
        {
            self.apply_toolbar_choice(capture_core::quick_preferences::Choice::CanvasTool(tool.id));
        }
        self.shell.planned_tool = (!tool.implemented).then_some(index);
        if tool.id == ToolId::Stamp {
            self.tool = None;
            self.select_only(None);
            return;
        }
        if tool.name == "Cut Out" {
            self.begin_cut_out();
            return;
        }
        if tool.name == "Selection" {
            self.begin_pixel_selection();
            return;
        }
        if tool.name == "Fill" {
            self.fill_tool.active = true;
            self.tool = None;
            self.select_only(None);
            self.status = "Fill: click a connected color region. Images with objects require Flatten and fill.".into();
            return;
        }
        if tool.name == "Crop" {
            self.begin_crop();
            return;
        }
        if tool.kind == Some(Kind::Simplify) {
            self.refresh_simplify_palette();
            self.color = self
                .simplify_palette
                .first()
                .map_or([190, 198, 210], |c| [c[0], c[1], c[2]]);
            self.quick_styles.alpha = 255;
        }
        if tool.implemented {
            self.tool = tool.kind;
            if self
                .selected_annotation()
                .is_some_and(|a| !Self::tool_matches(self.tool, a.kind))
            {
                self.select_only(None);
            }
            self.sync_step_sequence();
            if tool.kind == Some(Kind::Blur) {
                self.width = 8;
            } else if tool.kind == Some(Kind::Eraser) {
                self.select_only(None);
                self.width = 24;
                self.quick_styles.alpha = 255;
            } else if tool.kind == Some(Kind::Highlighter) {
                self.color = [255, 220, 40];
                self.width = 16;
                self.quick_styles.alpha = 80;
            } else if matches!(tool.kind, Some(Kind::Pen | Kind::Line)) {
                self.width = 3;
                self.quick_styles.alpha = 255;
            }
            self.restore_drawing_defaults(self.tool);
        }
        self.status = if tool.implemented {
            if tool.kind.is_none() {
                "Select an object, then drag it or its handles.".into()
            } else if tool.kind == Some(Kind::Step) {
                "Step: click an existing marker to select it, or empty space to place the next. Drag a marker or its handles to edit it."
                    .into()
            } else {
                format!(
                    "{}: drag on the canvas. {}",
                    tool.name,
                    readiness::badge(tool.feature)
                        .detail
                        .lines()
                        .last()
                        .unwrap_or("")
                )
            }
        } else {
            format!(
                "{} is not implemented yet. {} (provisional).",
                tool.name,
                readiness::badge(tool.feature).label
            )
        };
    }
    pub(super) fn current_tool(&self) -> Tool {
        if self.stamps.active {
            *TOOLS.iter().find(|t| t.id == ToolId::Stamp).unwrap()
        } else if self.cut_out.is_some() {
            *TOOLS.iter().find(|t| t.name == "Cut Out").unwrap()
        } else if self.pixel_selection.is_some() {
            *TOOLS.iter().find(|t| t.name == "Selection").unwrap()
        } else if self.fill_tool.active {
            *TOOLS.iter().find(|t| t.name == "Fill").unwrap()
        } else if self.crop.is_some() {
            *TOOLS.iter().find(|t| t.name == "Crop").unwrap()
        } else if let Some(index) = self.shell.planned_tool {
            TOOLS[index]
        } else if self.tool == Some(Kind::Pixelate) {
            *TOOLS.iter().find(|t| t.name == "Blur").unwrap()
        } else if matches!(self.tool, Some(Kind::Ellipse | Kind::Polygon)) {
            TOOLS[4]
        } else {
            *TOOLS
                .iter()
                .find(|t| {
                    t.implemented
                        && t.kind == self.tool
                        && !matches!(t.name, "Fill" | "Selection" | "Crop" | "Cut Out" | "Stamp")
                })
                .expect("every engine tool has a shell mapping")
        }
    }

    pub fn show_shell(&mut self, ui: &mut egui::Ui) {
        self.show_quick_access(ui);
        if self.shell.backstage.is_some() {
            self.show_backstage(ui);
            return;
        }
        self.show_ribbon(ui);
        self.show_recent_history(ui);
        self.show_footer(ui);
        if self.shell.properties && !self.shell.library && self.crop.is_none() {
            egui::Panel::right("properties")
                .default_size(262.0)
                .min_size(262.0)
                .max_size(262.0)
                .resizable(false)
                .show(ui, |ui| {
                    self.property_tabs(ui);
                    let area = ui.available_rect_before_wrap();
                    let footer = egui::Rect::from_min_max(
                        egui::pos2(area.left(), area.bottom() - 36.0),
                        area.max,
                    );
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height((area.height() - 44.0).max(0.0))
                        .id_salt(("property_scroll", self.shell.property_tab as u8))
                        .show(ui, |ui| self.properties(ui));
                    let count = self.selected_objects().len();
                    let delete = ui
                        .scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
                            crate::object_pane::delete_button(
                                ui,
                                count > 0 && !self.shell.annotations_hidden,
                                if count > 1 {
                                    "Delete selected objects"
                                } else {
                                    "Delete object"
                                },
                                footer.width(),
                            )
                        })
                        .inner;
                    if delete.clicked() {
                        self.delete_selection();
                    }
                });
        }
    }

    pub(super) fn capture_delay_menu(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Capture delay",
            "Saved capture setting. Applies to Capture and capture keybinds. Arrange your screen during the countdown, then drag a region. Escape or Cancel capture keeps the current document.",
        );
        let mut capture_settings = self.quick_controls.capture_settings();
        for (seconds, label) in [
            (0, "No delay"),
            (3, "3 seconds"),
            (5, "5 seconds"),
            (10, "10 seconds"),
        ] {
            if ui
                .selectable_value(&mut capture_settings.delay_seconds, seconds, label)
                .clicked()
            {
                self.quick_controls
                    .remember_capture(capture_settings.clone());
                ui.close();
            }
        }
        badge(ui, self.shell.badges, "CAP-006");
    }
    fn effects_list(ui: &mut egui::Ui, badges: bool) {
        for (name, id) in [
            ("Border", "FX-001"),
            ("Edges", "FX-001"),
            ("Shadow", "FX-001"),
            ("Perspective", "FX-003"),
            ("Page Curl", "FX-003"),
            ("Filters", "FX-002"),
            ("Color Adjustment", "FX-002"),
            ("Spotlight & Magnify", "FX-003"),
            ("Watermark", "FX-003"),
            ("Capture Info", "FX-003"),
        ] {
            menu_item(ui, badges, name, id, false);
        }
    }
    pub(super) fn properties(&mut self, ui: &mut egui::Ui) {
        if self.shell.property_tab == PropertyTab::Objects {
            self.object_properties(ui);
            return;
        }
        if self.shell.annotations_hidden {
            ui.label("Annotations are hidden for comparison.");
            crate::property_layout::section_help(
                ui,
                "Tool Properties",
                "Use Show annotations to resume editing. Saved objects and exports are unchanged.",
            );
            return;
        }
        if self.stamps.active
            && !self.shell.effects
            && !self
                .selected_annotation()
                .is_some_and(|a| a.kind == Kind::Image)
        {
            self.stamp_properties(ui);
            return;
        }
        if self.text_draft.is_some() {
            self.inline_text_properties(ui);
            return;
        }
        if self.property_suppressed {
            ui.disable();
        }
        if !self.shell.effects && self.current_tool().id == ToolId::Move && self.selected.is_none()
        {
            self.move_functions(ui);
            return;
        }
        let badges = self.shell.badges;
        if !self.shell.effects && !self.current_tool().implemented {
            let tool = self.current_tool();
            crate::property_layout::section_help(
                ui,
                "Tool Functions",
                "This tool is planned and does not modify the image yet.",
            );
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(25., 28.), egui::Sense::hover());
                crate::icons::draw_scaled(
                    ui.painter(),
                    rect,
                    tool.name,
                    ui.visuals().text_color(),
                    0.65,
                );
                ui.label(format!("{} · not available yet", tool.name));
            });
            badge(ui, badges, tool.feature);
            return;
        }
        if self.shell.effects {
            crate::property_layout::section_help(
                ui,
                "Effects",
                "Effects are planned; your image is unchanged.",
            );
            Self::effects_list(ui, badges);
            return;
        }
        if self.cut_out.is_some() {
            self.cut_out_properties(ui);
            return;
        }
        if self.pixel_selection.is_some() {
            self.pixel_selection_properties(ui);
            return;
        }
        if self.fill_tool.active {
            self.fill_properties(ui);
            return;
        }
        let tool = self
            .selected_annotation()
            .and_then(|annotation| {
                TOOLS.iter().find(|tool| {
                    tool.implemented
                        && tool.kind
                            == Some(if annotation.kind == Kind::Pixelate {
                                Kind::Blur
                            } else if matches!(annotation.kind, Kind::Ellipse | Kind::Polygon) {
                                Kind::Rectangle
                            } else {
                                annotation.kind
                            })
                })
            })
            .copied()
            .unwrap_or_else(|| self.current_tool());
        let image_object = self
            .selected_annotation()
            .is_some_and(|a| a.kind == Kind::Image);
        if !image_object {
            ui.vertical_centered(|ui| {
                self.quick_styles_header(ui, tool.kind);
                badge(
                    ui,
                    badges,
                    if tool.kind == Some(Kind::Arrow) {
                        "UX-014"
                    } else {
                        "UX-023"
                    },
                );
            });
            if tool.kind == Some(Kind::Arrow) {
                self.arrow_styles(ui);
            } else if let Some(kind) = self.selected_annotation().map(|a| a.kind).or(tool.kind) {
                self.object_templates(ui, kind);
            }
        }
        ui.vertical_centered(|ui| {
            let help = if image_object {
                "Pasted images remain independent objects. Move, resize, rotate or adjust opacity without changing their original pixels. Undo restores each edit; Delete reveals the canvas underneath.".to_owned()
            } else if tool.implemented {
                crate::property_layout::tool_help(tool.kind).to_owned()
            } else {
                format!(
                    "This area is a layout preview. Drawing with this tool is unavailable. {}",
                    readiness::badge(tool.feature).detail
                )
            };
            crate::property_layout::section_help(ui, "Tool Properties", &help);
            badge(ui, badges, tool.feature);
        });
        if !tool.implemented {
            ui.label(format!("{} is planned.", tool.name));
            return;
        }
        if tool.kind == Some(Kind::Redact) {
            ui.label("Solid black · fully opaque");
        }
        if tool.kind == Some(Kind::Eraser) {
            badge(ui, badges, "EDIT-019");
        }
        if crate::obscure_properties::is_effect(tool.kind) {
            badge(ui, badges, "EDIT-008");
        }
        if tool.kind == Some(Kind::Simplify) {
            self.simplify_properties(ui);
            return;
        }
        if self.selected.is_none() && self.tool.is_some() {
            let before_defaults = (self.color, self.width, self.quick_styles.alpha, self.tool);
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
                badge(ui, badges, "EDIT-023");
                badge(ui, badges, "EDIT-012");
            } else if self.tool == Some(Kind::Highlighter) && self.drawing_rectangle_highlighter() {
                crate::property_layout::slider(
                    ui,
                    "Opacity",
                    egui::Slider::new(&mut self.quick_styles.alpha, 0..=255),
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
                                Kind::Arrow
                                    | Kind::Line
                                    | Kind::Pen
                                    | Kind::Highlighter
                                    | Kind::Eraser
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
                        egui::Slider::new(&mut self.quick_styles.alpha, 0..=255),
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
                badge(ui, badges, "EDIT-036");
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
                badge(ui, badges, "EDIT-014");
            }
            if before_defaults.3 == self.tool
                && (before_defaults.0, before_defaults.1, before_defaults.2)
                    != (self.color, self.width, self.quick_styles.alpha)
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
        } else if self.selected.is_none() {
            badge(ui, badges, "EDIT-021");
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
                badges,
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
    fn every_tool_has_a_truthful_status_and_unavailable_tools_cannot_draw() {
        for tool in TOOLS {
            let label = readiness::badge(tool.feature);
            assert_ne!(label.label, "Unmapped", "{}", tool.name);
            if !tool.implemented {
                assert!(!label.ready, "{}", tool.name);
            }
        }
        let mut shell = Shell::default();
        assert!(shell.canvas_enabled());
        shell.planned_tool = Some(3);
        assert!(!shell.canvas_enabled());
        shell.planned_tool = None;
        shell.library = true;
        assert!(!shell.canvas_enabled());
    }
}
