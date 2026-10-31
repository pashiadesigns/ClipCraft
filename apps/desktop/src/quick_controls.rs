use crate::{Command, Editor, egui};
use capture_core::{
    layout_selection::LayoutAction as L,
    quick_preferences::{Choice, Justification as J, Preferences, Store},
};

#[cfg(test)]
mod live_font_tests;
#[cfg(test)]
mod obfuscate_tests;
#[cfg(test)]
mod shared_format_tests;
mod typography_controls;

#[derive(Default)]
pub(super) struct QuickControls {
    prefs: Preferences,
    store: Option<Store>,
    warning: Option<String>,
    pub(super) format_source: Option<capture_core::Annotation>,
    pub(super) copied_format: Option<capture_core::Annotation>,
    pub(super) format_locked: bool,
}
impl QuickControls {
    #[cfg(test)]
    pub(super) fn test_lifecycle(
        &mut self,
        close: bool,
        startup: capture_core::quick_preferences::StartupWindow,
    ) {
        self.prefs.choose(Choice::CloseToTray(close)).unwrap();
        self.prefs.choose(Choice::StartupWindow(startup)).unwrap();
    }
    pub(super) fn close_to_tray(&self) -> bool {
        self.prefs.close_to_tray
    }
    pub(super) fn startup_window(&self) -> capture_core::quick_preferences::StartupWindow {
        self.prefs.startup_window
    }
    pub(super) fn save_lifecycle(&mut self, choice: Choice) -> Result<(), String> {
        let store = self
            .store
            .as_ref()
            .ok_or("Preferences cannot be saved on this computer.")?;
        self.prefs = store.remember(choice).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(super) fn keybinds(&self) -> capture_core::keybinds::Keybinds {
        self.prefs.keybinds.clone()
    }
    pub(super) fn save_keybinds(
        &mut self,
        keys: capture_core::keybinds::Keybinds,
    ) -> Result<(), String> {
        let choice = Choice::Keybinds(keys);
        let store = self.store.as_ref().ok_or_else(|| {
            self.warning
                .clone()
                .unwrap_or_else(|| "Preferences cannot be saved on this computer.".into())
        })?;
        let next = store.remember(choice).map_err(|e| e.to_string())?;
        self.prefs = next;
        self.warning = None;
        Ok(())
    }

    pub(super) fn capture_settings(&self) -> capture_core::capture_options::Settings {
        self.prefs.capture.clone()
    }
    pub(super) fn remember_capture(&mut self, settings: capture_core::capture_options::Settings) {
        self.remember(Choice::Capture(settings));
    }
    pub(super) fn drawing_style(
        &self,
        kind: crate::Kind,
    ) -> Option<&capture_core::object_styles::ObjectStyle> {
        self.prefs
            .drawing_styles
            .iter()
            .rev()
            .find(|s| s.kind == kind)
    }
    pub(super) fn drawing_variant(
        &self,
        kind: crate::Kind,
        rectangle: bool,
    ) -> Option<&capture_core::object_styles::ObjectStyle> {
        self.prefs
            .drawing_styles
            .iter()
            .find(|s| s.kind == kind && s.highlighter_rect == rectangle)
    }
    pub(super) fn remember_drawing_style(
        &mut self,
        style: capture_core::object_styles::ObjectStyle,
    ) {
        if self.drawing_style(style.kind) != Some(&style) {
            self.remember(Choice::DrawingStyle(style));
        }
    }
    pub(super) fn toolbar_order(&self) -> Vec<capture_core::toolbar::ToolId> {
        capture_core::toolbar::grouped_order(&self.prefs.toolbar_order, &self.prefs.favorite_tools)
    }
    pub(super) fn tool_group(
        &self,
        tool: capture_core::toolbar::ToolId,
    ) -> capture_core::toolbar::Group {
        capture_core::toolbar::group(tool, &self.prefs.favorite_tools)
    }
    pub(super) fn canvas_tool(&self) -> capture_core::toolbar::ToolId {
        self.prefs.canvas_tool
    }
    pub(super) fn change_toolbar(&mut self, choice: Choice) -> Result<bool, String> {
        let mut next = self.prefs.clone();
        next.choose(choice.clone()).map_err(|e| e.to_string())?;
        if let Some(store) = &self.store {
            // Apply against the locked on-disk state even if this window's
            // cached order already looks unchanged; another window may differ.
            next = store.remember(choice).map_err(|e| e.to_string())?;
        }
        let changed = next.toolbar_order != self.prefs.toolbar_order
            || next.favorite_tools != self.prefs.favorite_tools
            || next.canvas_tool != self.prefs.canvas_tool;
        self.prefs = next;
        Ok(changed)
    }
    pub(super) fn auto_switch_tool(&self) -> bool {
        self.prefs.auto_switch_tool
    }
    pub(super) fn set_auto_switch_tool(&mut self, value: bool) {
        self.remember(Choice::AutoSwitchTool(value));
    }
    pub(super) fn style_choice(&self, category: &str) -> Option<&str> {
        self.prefs.style_choices.get(category).map(String::as_str)
    }
    pub(super) fn remember_style(&mut self, category: &str, name: &str) {
        self.remember(Choice::Style {
            category: category.into(),
            name: name.into(),
        });
    }
    pub(super) fn snapping(&self) -> bool {
        self.prefs.canvas_snapping
    }

    pub(super) fn set_snapping(&mut self, value: bool) {
        self.remember(Choice::CanvasSnapping(value));
    }
    pub fn new() -> Self {
        let root = std::env::var_os("CLIPCRAFT_PREFERENCES_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA").map(|p| {
                    std::path::PathBuf::from(p)
                        .join("ClipCraft")
                        .join("preferences-v1")
                })
            });
        let Some(root) = root else {
            return Self {
                warning: Some("Preferences cannot be saved on this computer.".into()),
                ..Default::default()
            };
        };
        match Store::open(&root).and_then(|s| s.read().map(|p| (s, p))) {
            Ok((store, prefs)) => Self {
                store: Some(store),
                prefs,
                warning: None,
                format_source: None,
                copied_format: None,
                format_locked: false,
            },
            Err(error) => Self {
                warning: Some(error.to_string()),
                ..Default::default()
            },
        }
    }
    fn remember(&mut self, choice: Choice) {
        if let Err(error) = self.prefs.choose(choice.clone()) {
            self.warning = Some(error.to_string());
            return;
        }
        if let Some(store) = &self.store {
            match store.remember(choice) {
                Ok(prefs) => {
                    self.prefs = prefs;
                    self.warning = None;
                }
                Err(error) => self.warning = Some(error.to_string()),
            }
        }
    }
}

const ALIGNS: [L; 5] = [L::Top, L::Left, L::CenterBoth, L::Right, L::Bottom];
const DISTRIBUTIONS: [L; 4] = [
    L::HorizontalCenters,
    L::VerticalCenters,
    L::HorizontalGaps,
    L::VerticalGaps,
];
const JUSTIFICATIONS: [J; 9] = [
    J::TopLeft,
    J::TopCenter,
    J::TopRight,
    J::MiddleLeft,
    J::CenterMiddle,
    J::MiddleRight,
    J::BottomLeft,
    J::BottomCenter,
    J::BottomRight,
];
fn layout_label(a: L) -> &'static str {
    match a {
        L::Left => "Align left",
        L::HorizontalCenter => "Center horizontally",
        L::Right => "Align right",
        L::Top => "Align top",
        L::VerticalCenter => "Center vertically",
        L::Bottom => "Align bottom",
        L::CenterBoth => "Center both axes",
        L::TopLeft => "Align top left",
        L::TopCenter => "Align top center",
        L::TopRight => "Align top right",
        L::MiddleLeft => "Align middle left",
        L::MiddleRight => "Align middle right",
        L::BottomLeft => "Align bottom left",
        L::BottomCenter => "Align bottom center",
        L::BottomRight => "Align bottom right",
        L::HorizontalCenters => "Horizontal centers",
        L::VerticalCenters => "Vertical centers",
        L::HorizontalGaps => "Horizontal gaps",
        L::VerticalGaps => "Vertical gaps",
    }
}
fn text_label(a: J) -> &'static str {
    match a {
        J::TopLeft => "Top left",
        J::TopCenter => "Top center",
        J::TopRight => "Top right",
        J::MiddleLeft => "Middle left",
        J::CenterMiddle => "Center / middle",
        J::MiddleRight => "Middle right",
        J::BottomLeft => "Bottom left",
        J::BottomCenter => "Bottom center",
        J::BottomRight => "Bottom right",
    }
}

fn layout_icon(p: &egui::Painter, r: egui::Rect, a: L, color: egui::Color32) {
    if let Some(anchor) = a.alignment_anchor() {
        p.rect_stroke(
            r,
            0.0,
            egui::Stroke::new(0.7, color.gamma_multiply(0.5)),
            egui::StrokeKind::Inside,
        );
        let pos = r.min + egui::vec2(anchor[0] * 7.0, anchor[1] * 9.0);
        p.rect_filled(
            egui::Rect::from_min_size(pos, egui::vec2(7.0, 5.0)),
            1.0,
            color,
        );
        let x = r.left() + r.width() * anchor[0];
        let y = r.top() + r.height() * anchor[1];
        p.line_segment(
            [egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
            egui::Stroke::new(0.8, color),
        );
        p.line_segment(
            [egui::pos2(r.left(), y), egui::pos2(r.right(), y)],
            egui::Stroke::new(0.8, color),
        );
        return;
    }
    if a.minimum_objects() == 3 {
        let vertical = matches!(a, L::VerticalCenters | L::VerticalGaps);
        let gaps = matches!(a, L::HorizontalGaps | L::VerticalGaps);
        for (index, length) in [6.0, 12.0, 8.0].into_iter().enumerate() {
            let offset = index as f32 * 6.0;
            let start = if vertical {
                egui::vec2((14.0 - length) * 0.5, offset)
            } else {
                egui::vec2(offset, (14.0 - length) * 0.5)
            };
            let size = if vertical {
                egui::vec2(length, 2.0)
            } else {
                egui::vec2(2.0, length)
            };
            p.rect_filled(egui::Rect::from_min_size(r.min + start, size), 0.0, color);
        }
        for offset in if gaps { [4.0, 10.0] } else { [1.0, 7.0] } {
            let line = if vertical {
                [
                    r.min + egui::vec2(0.0, offset),
                    r.min + egui::vec2(14.0, offset),
                ]
            } else {
                [
                    r.min + egui::vec2(offset, 0.0),
                    r.min + egui::vec2(offset, 14.0),
                ]
            };
            p.line_segment(line, egui::Stroke::new(0.5, color.gamma_multiply(0.65)));
        }
        return;
    }
    let vertical = matches!(
        a,
        L::Top | L::VerticalCenter | L::Bottom | L::VerticalCenters | L::VerticalGaps
    );
    for (index, size) in [7.0, 12.0, 9.0].into_iter().enumerate() {
        let offset = match a {
            L::Right | L::Bottom => 14.0 - size,
            L::HorizontalCenter | L::VerticalCenter | L::CenterBoth => (14.0 - size) * 0.5,
            _ => 0.0,
        };
        let start = if vertical {
            r.min + egui::vec2(index as f32 * 5.0, offset)
        } else {
            r.min + egui::vec2(offset, index as f32 * 5.0)
        };
        let extent = if vertical {
            egui::vec2(2.0, size)
        } else {
            egui::vec2(size, 2.0)
        };
        p.rect_filled(egui::Rect::from_min_size(start, extent), 0.0, color);
    }
}

fn text_icon(p: &egui::Painter, r: egui::Rect, choice: J, color: egui::Color32) {
    use capture_core::text::{HorizontalAlignment as H, VerticalAlignment as V};
    let (horizontal, vertical) = choice.alignments();
    let top = match vertical {
        V::Top => 0.0,
        V::Middle => 3.0,
        V::Bottom => 6.0,
    };
    for (row, width) in [14.0, 9.0, 11.0].into_iter().enumerate() {
        let left = match horizontal {
            H::Left => 0.0,
            H::Center => (14.0 - width) * 0.5,
            H::Right => 14.0 - width,
        };
        p.rect_filled(
            egui::Rect::from_min_size(
                r.min + egui::vec2(left, top + row as f32 * 3.0),
                egui::vec2(width, 1.5),
            ),
            0.0,
            color,
        );
    }
}

fn grid_icon(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    selected: bool,
    paint: impl FnOnce(&egui::Painter, egui::Rect, egui::Color32),
) -> egui::Response {
    let r = ui
        .push_id(label, |ui| {
            ui.add_enabled(
                enabled,
                egui::Button::new("")
                    .min_size(egui::vec2(22.0, 22.0))
                    .selected(selected),
            )
        })
        .inner;
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, label));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("format-grid", label)), r.rect));
    let color = ui
        .visuals()
        .text_color()
        .gamma_multiply(if enabled { 1.0 } else { 0.35 });
    paint(
        ui.painter(),
        egui::Rect::from_center_size(r.rect.center(), egui::vec2(14.0, 14.0)),
        color,
    );
    r.on_hover_text(label)
}

#[derive(Clone, Copy)]
enum TextAction {
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Strikethrough(bool),
    Size(f32),
    Font(capture_core::text::TextFont),
    SetSize(f32),
    Color([u8; 4]),
    Script(capture_core::text::typography::Script),
    Spacing(f32),
    Highlight(Option<[u8; 4]>),
    Case(capture_core::text::typography::Case),
    Clear,
}
impl TextAction {
    fn apply_annotation(
        self,
        mut a: crate::Annotation,
        canvas: [u32; 2],
    ) -> capture_core::Result<crate::Annotation> {
        match self {
            Self::Size(delta) => a.change_text_size(delta, true, canvas),
            Self::SetSize(size) => a.change_text_size(size, false, canvas),
            _ => {
                if let Some(text) = a.text.as_mut() {
                    self.apply(text);
                    text.validate()?;
                }
                a.fit_text_shape(canvas)
            }
        }
    }
    fn apply(self, text: &mut capture_core::text::TextStyle) {
        match self {
            Self::Font(font) => text.font = font,
            Self::Bold(value) => text.bold = value,
            Self::Italic(value) => text.italic = value,
            Self::Underline(value) => text.underline = value,
            Self::Strikethrough(value) => text.strikethrough = value,
            Self::Size(delta) => text.size_points = (text.size_points + delta).clamp(6.0, 144.0),
            Self::SetSize(size) => text.size_points = size.clamp(6.0, 144.0),
            Self::Color(color) => text.color = Some(color),
            Self::Script(script) => {
                let mut t = text.typography();
                t.script = script;
                text.set_typography(t);
            }
            Self::Spacing(spacing) => {
                let mut t = text.typography();
                t.spacing = spacing;
                text.set_typography(t);
            }
            Self::Highlight(highlight) => {
                let mut t = text.typography();
                t.highlight = highlight;
                text.set_typography(t);
            }
            Self::Case(case) => text.content = case.apply(&text.content),
            Self::Clear => {
                let content = std::mem::take(&mut text.content);
                *text = capture_core::text::TextStyle {
                    content,
                    ..Default::default()
                };
            }
        }
    }
}

fn text_button(
    ui: &mut egui::Ui,
    icon: egui::RichText,
    label: &str,
    enabled: bool,
    selected: bool,
) -> bool {
    let size_action = matches!(label, "Increase font size" | "Decrease font size");
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(if size_action {
            egui::RichText::new("")
        } else {
            icon
        })
        .min_size(egui::vec2(25.0, 24.0))
        .selected(selected),
    );
    if size_action {
        let color = ui
            .visuals()
            .text_color()
            .gamma_multiply(if enabled { 1.0 } else { 0.35 });
        let center = response.rect.center();
        ui.painter().text(
            center - egui::vec2(3.0, 0.0),
            egui::Align2::CENTER_CENTER,
            "A",
            egui::FontId::proportional(12.0),
            color,
        );
        let direction = if label == "Increase font size" {
            -1.0
        } else {
            1.0
        };
        let center = center + egui::vec2(7.0, 0.0);
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                center + egui::vec2(-3.0, -direction * 2.0),
                center + egui::vec2(3.0, -direction * 2.0),
                center + egui::vec2(0.0, direction * 3.0),
            ],
            color,
            egui::Stroke::NONE,
        ));
    }
    if label == "Clear text formatting" {
        ui.painter().line_segment(
            [
                response.rect.right_bottom() - egui::vec2(8.0, 4.0),
                response.rect.right_bottom() - egui::vec2(3.0, 9.0),
            ],
            egui::Stroke::new(
                3.0,
                egui::Color32::from_rgb(221, 120, 181).gamma_multiply(if enabled {
                    1.0
                } else {
                    0.35
                }),
            ),
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    #[cfg(test)]
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::unique(("quick_text_button", label)),
            (response.rect, selected),
        )
    });
    response.on_hover_text(label).clicked()
}

impl Editor {
    pub(super) fn paste_shared_format(&mut self) {
        let Some(source) = self.quick_controls.copied_format.clone() else {
            self.status =
                "Copy an object first, then select targets and press Ctrl+Shift+V.".into();
            return;
        };
        let objects = self.selected_objects();
        if objects.is_empty() {
            self.status = "Select objects to paste formatting.".into();
            return;
        }
        match objects
            .iter()
            .map(|a| capture_core::object_styles::paint_format(&source, a))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(annotations) => {
                let changed = annotations != objects;
                let expected = annotations.clone();
                self.execute(Command::UpdateMany { annotations });
                if !expected
                    .iter()
                    .all(|a| self.session.document.annotations.iter().any(|b| b == a))
                {
                    return;
                }
                self.status = if changed {
                    "Shared formatting pasted. Undo restores the entire selection."
                } else {
                    "No supported formatting differs on the selected objects."
                }
                .into();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    pub(super) fn shared_format_shortcuts(&mut self, ctx: &egui::Context) {
        if !self.editor_shortcuts_idle()
            || self.text_draft.is_some()
            || ctx.egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
            || self.selected_pixels().is_some()
            || !self.shell.canvas_enabled()
        {
            return;
        }
        let paste = ctx.input_mut(|i| {
            crate::shortcuts::consume_exact(
                i,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::V,
            )
        });
        if paste {
            self.paste_shared_format();
        }
    }

    pub(super) fn arm_format_painter(&mut self) {
        if self.quick_controls.format_source.is_some() {
            self.quick_controls.format_source = None;
            self.quick_controls.format_locked = false;
            return;
        }
        if !self.commit_text() {
            return;
        }
        let objects = self.selected_objects();
        if objects.len() == 1 {
            self.cancel_gesture();
            self.fill_tool.active = false;
            self.quick_controls.format_source = objects.into_iter().next();
            self.status="Format Painter: click a target to apply its supported formatting. Double-click Format for repeated use.".into();
        }
    }
    pub(super) fn lock_format_painter(&mut self) {
        if self.quick_controls.format_source.is_none() {
            self.arm_format_painter();
        }
        if self.quick_controls.format_source.is_some() {
            self.quick_controls.format_locked = true;
            self.status =
                "Format Painter locked: click targets repeatedly. Click Format again to stop."
                    .into();
        }
    }
    pub(super) fn paint_format_to(&mut self, id: u64) {
        let Some(source) = self.quick_controls.format_source.clone() else {
            return;
        };
        let targets = if self.is_selected(id) {
            self.selected_objects()
        } else {
            self.session
                .document
                .annotations
                .iter()
                .filter(|a| a.id == id)
                .cloned()
                .collect()
        };
        let result: Result<Vec<_>, _> = targets
            .iter()
            .map(|a| capture_core::object_styles::paint_format(&source, a))
            .collect();
        match result {
            Ok(annotations) if !annotations.is_empty() => {
                let locked = self.quick_controls.format_locked;
                let expected = annotations.clone();
                self.execute(Command::UpdateMany { annotations });
                // execute cancels transient gestures. A locked painter owns a
                // separate immutable source and survives its own applications.
                if locked {
                    self.quick_controls.format_source = Some(source);
                    self.quick_controls.format_locked = true;
                }
                if !expected
                    .iter()
                    .all(|a| self.session.document.annotations.iter().any(|b| b == a))
                {
                    return;
                }
                if !self.is_selected(id) {
                    self.select_only(Some(id));
                }
                self.auto_activate_selected_tool();
                self.status = if locked {
                    "Formatting copied. Painter stays locked; click another target or click Format to stop."
                } else {
                    "Formatting copied. Undo restores all target objects."
                }.into();
            }
            Err(error) => self.status = error.to_string(),
            _ => {}
        }
    }
    pub(super) fn apply_quick_layout(&mut self, action: L) {
        let transform = self.view.current_transform();
        let before_base = self.session.document.auto_canvas_base.unwrap_or([
            0,
            0,
            self.session.document.width,
            self.session.document.height,
        ]);
        let before_dimensions = [self.session.document.width, self.session.document.height];
        let ids = self.selection_ordered_ids();
        self.status = format!(
            "{}. Undo reverses the selection arrangement.",
            layout_label(action)
        );
        self.execute(if ALIGNS.contains(&action) {
            Command::AlignToFirst { ids, action }
        } else {
            Command::LayoutSelection { ids, action }
        });
        let dimensions = [self.session.document.width, self.session.document.height];
        let base =
            self.session
                .document
                .auto_canvas_base
                .unwrap_or([0, 0, dimensions[0], dimensions[1]]);
        let offset = [
            base[0] as i32 - before_base[0] as i32,
            base[1] as i32 - before_base[1] as i32,
        ];
        if let Some(transform) = transform
            && (dimensions != before_dimensions || offset != [0, 0])
        {
            self.view
                .retain_expanded_position(transform, offset, dimensions);
        }
        self.quick_controls
            .remember(if action.minimum_objects() == 3 {
                Choice::Distribution(action)
            } else {
                Choice::Alignment(action)
            });
    }
    pub(super) fn apply_quick_justification(&mut self, choice: J) {
        let (horizontal, vertical) = choice.alignments();
        if let Some(draft) = self.text_draft.as_mut() {
            if let Some(text) = draft.preview.text.as_mut() {
                text.horizontal = horizontal;
                text.vertical = vertical;
                self.needs_render = true;
            }
            self.quick_controls.remember(Choice::Justification(choice));
            return;
        }
        let annotations = self
            .selected_objects()
            .into_iter()
            .filter_map(|mut a| {
                let text = a.text.as_mut()?;
                text.horizontal = horizontal;
                text.vertical = vertical;
                Some(a)
            })
            .collect();
        self.execute(Command::UpdateMany { annotations });
        self.quick_controls.remember(Choice::Justification(choice));
    }
    fn apply_quick_text(&mut self, action: TextAction) {
        self.commit_property_preview();
        let canvas = [self.session.document.width, self.session.document.height];
        if let Some(draft) = &mut self.text_draft {
            match action.apply_annotation(draft.preview.clone(), canvas) {
                Ok(preview) => {
                    draft.preview = preview;
                    self.needs_render = true;
                }
                Err(error) => self.status = error.to_string(),
            }
        } else {
            let annotations: capture_core::Result<Vec<_>> = self
                .selected_objects()
                .into_iter()
                .filter(|a| a.text.is_some())
                .map(|a| action.apply_annotation(a, canvas))
                .collect();
            match annotations {
                Ok(annotations) => self.execute(Command::UpdateMany { annotations }),
                Err(error) => self.status = error.to_string(),
            }
        }
    }

    fn preview_quick_text(&mut self, action: TextAction) {
        if self.text_draft.is_some() {
            self.apply_quick_text(action);
            return;
        }
        let canvas = [self.session.document.width, self.session.document.height];
        // Always calculate an absolute drag value from the unchanged document:
        // increasing then decreasing size must not retain a larger fitted frame.
        let annotations = self
            .selected_objects()
            .into_iter()
            .map(|a| action.apply_annotation(a, canvas))
            .collect::<capture_core::Result<Vec<_>>>();
        match annotations {
            Ok(annotations) => self.preview_property_batch(annotations),
            Err(error) => self.status = error.to_string(),
        }
    }
    pub(super) fn show_layout_grid(
        &mut self,
        ui: &mut egui::Ui,
        distribution: bool,
    ) -> Vec<(L, egui::Response)> {
        let actions: Vec<Option<L>> = if distribution {
            DISTRIBUTIONS.into_iter().map(Some).collect()
        } else {
            vec![
                None,
                Some(L::Top),
                None,
                Some(L::Left),
                Some(L::CenterBoth),
                Some(L::Right),
                None,
                Some(L::Bottom),
                None,
            ]
        };
        let count = self.selected_objects().len();
        let current = if distribution {
            self.quick_controls.prefs.distribution
        } else {
            self.quick_controls.prefs.alignment
        };
        let mut responses = vec![];
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            ui.spacing_mut().interact_size = egui::vec2(22.0, 22.0);
            for row in actions.chunks(if distribution { 2 } else { 3 }) {
                ui.horizontal(|ui| {
                    for &action in row {
                        let Some(action) = action else {
                            ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                            continue;
                        };
                        let response = grid_icon(
                            ui,
                            layout_label(action),
                            count >= action.minimum_objects(),
                            current == action,
                            |p, r, c| layout_icon(p, r, action, c),
                        );
                        if response.clicked() && self.commit_text() {
                            self.apply_quick_layout(action);
                        }
                        responses.push((action, response));
                    }
                });
            }
        });
        responses
    }

    pub(super) fn show_justification_grid(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Vec<(J, egui::Response)> {
        let objects = self.selected_objects();
        let enabled = !objects.is_empty() && objects.iter().all(|a| a.text.is_some());
        let mut responses = vec![];
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            ui.spacing_mut().interact_size = egui::vec2(22.0, 22.0);
            for row in JUSTIFICATIONS.chunks(3) {
                ui.horizontal(|ui| {
                    for &choice in row {
                        let (h, v) = choice.alignments();
                        let selected = enabled
                            && objects.iter().all(|a| {
                                a.text
                                    .as_ref()
                                    .is_some_and(|t| t.horizontal == h && t.vertical == v)
                            });
                        let response = grid_icon(
                            ui,
                            &format!("Text justification: {}", text_label(choice)),
                            enabled,
                            selected,
                            |p, r, c| text_icon(p, r, choice, c),
                        );
                        if response.clicked() {
                            self.apply_quick_justification(choice);
                        }
                        responses.push((choice, response));
                    }
                });
            }
        });
        responses
    }

    pub(super) fn show_quick_font_controls(&mut self, ui: &mut egui::Ui) {
        self.show_font_controls(ui, false);
    }

    pub(super) fn show_font_controls(&mut self, ui: &mut egui::Ui, sidebar: bool) {
        use capture_core::text::TextFont;
        let objects = self
            .property_preview
            .as_ref()
            .map_or_else(|| self.selected_objects(), |p| p.batch_preview.clone());
        let enabled = !objects.is_empty() && objects.iter().all(|a| a.text.is_some());
        let first = objects.first().and_then(|a| a.text.as_ref());
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 4.0);
            ui.spacing_mut().interact_size.y = 24.0;
            ui.spacing_mut().button_padding = egui::vec2(4.0, 3.0);
            ui.add_enabled_ui(enabled, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let font = first.map(|t| t.font);
                    let mixed = objects
                        .iter()
                        .any(|a| a.text.as_ref().map(|t| t.font) != font);
                    egui::ComboBox::from_id_salt("ribbon_font")
                        .width(if sidebar { 148.0 } else { 128.0 })
                        .selected_text(if mixed {
                            "Mixed fonts"
                        } else {
                            font.map_or("Font", TextFont::label)
                        })
                        .show_ui(ui, |ui| {
                            let key=egui::Id::unique("font-search");
                            let mut query=ui.ctx().data_mut(|d|d.get_temp::<String>(key).unwrap_or_default());
                            ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search installed fonts"));
                            ui.ctx().data_mut(|d|d.insert_temp(key,query.clone()));
                            for f in [TextFont::UbuntuLight, TextFont::HackRegular].into_iter().chain(capture_core::text::system_fonts::choices()).filter(|f|f.label().to_lowercase().contains(&query.to_lowercase())) {
                                if ui
                                    .selectable_label(!mixed && Some(f) == font, f.label())
                                    .clicked()
                                {
                                    self.apply_quick_text(TextAction::Font(f));
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .on_hover_text("Installed font family. Other computers need the same font; missing fonts use Ubuntu.");
                    let mut size = first.map_or(24.0, |t| t.size_points);
                    let mixed = objects
                        .iter()
                        .any(|a| a.text.as_ref().is_some_and(|t| t.size_points != size));
                    let r = ui.add_sized(
                        [52.0, 24.0],
                        egui::DragValue::new(&mut size)
                            .range(6.0..=144.0)
                            .speed(0.25)
                            .max_decimals(1)
                            .update_while_editing(false),
                    );
                    #[cfg(test)]
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::unique("ribbon_font_size"), r.rect));
                    if r.changed() {
                        self.preview_quick_text(TextAction::SetSize(size));
                    }
                    r.on_hover_text(if mixed {
                        "Mixed sizes. Enter a font size to apply to all selected labels."
                    } else {
                        "Font size · drag to resize live; click to type"
                    });
                    if !sidebar {
                        self.font_size_buttons(ui, enabled);
                    }
                    if text_button(
                        ui,
                        egui::RichText::new("A").size(13.0),
                        "Clear text formatting",
                        enabled,
                        false,
                    ) {
                        self.apply_quick_text(TextAction::Clear);
                    }
                })
            });
            ui.horizontal(|ui| {
                let bold = enabled && objects.iter().all(|a| a.text.as_ref().unwrap().bold);
                let italic = enabled && objects.iter().all(|a| a.text.as_ref().unwrap().italic);
                if text_button(ui, egui::RichText::new("B").strong(), "Bold", enabled, bold) {
                    self.apply_quick_text(TextAction::Bold(!bold));
                }
                if text_button(
                    ui,
                    egui::RichText::new("I").italics(),
                    "Italic",
                    enabled,
                    italic,
                ) {
                    self.apply_quick_text(TextAction::Italic(!italic));
                }
                let underline =
                    enabled && objects.iter().all(|a| a.text.as_ref().unwrap().underline);
                let strike = enabled
                    && objects
                        .iter()
                        .all(|a| a.text.as_ref().unwrap().strikethrough);
                if text_button(
                    ui,
                    egui::RichText::new("U").underline(),
                    "Underline",
                    enabled,
                    underline,
                ) {
                    self.apply_quick_text(TextAction::Underline(!underline));
                }
                if text_button(
                    ui,
                    egui::RichText::new("S").strikethrough(),
                    "Strikethrough",
                    enabled,
                    strike,
                ) {
                    self.apply_quick_text(TextAction::Strikethrough(!strike));
                }
                if sidebar {
                    self.font_size_buttons(ui, enabled);
                }
                if !sidebar {
                    self.typography_controls(ui, &objects, enabled, true);
                    ui.add_enabled_ui(enabled, |ui| {
                        if let Some(edited) = self.selected_annotation() {
                            let fallback = if edited.kind == crate::Kind::Step {
                                capture_core::steps::foreground(&edited).color
                            } else {
                                edited.color
                            };
                            if let Some(text) = edited.text.as_ref() {
                                let mut color = text.color.unwrap_or(fallback);
                                let response = crate::color_widgets::rgba_preview(
                                    ui,
                                    &mut color,
                                    crate::color_widgets::Preview::Text,
                                )
                                .on_hover_text("Text color");
                                #[cfg(test)]
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::unique("ribbon_font_color"),
                                        response.clone(),
                                    )
                                });
                                if response.changed() {
                                    self.preview_quick_text(TextAction::Color(color));
                                }
                            }
                        }
                        if ui
                            .button("Font…")
                            .on_hover_text("Open text properties and advanced text options")
                            .clicked()
                        {
                            self.shell.properties = true;
                            self.shell.effects = false;
                            self.shell.property_tab = crate::shell::PropertyTab::Tool;
                        }
                    });
                }
            });
        });
        if sidebar {
            self.typography_controls(ui, &objects, enabled, false);
        }
        if let Some(warning) = &self.quick_controls.warning {
            ui.small(format!("Preference save: {warning}"));
        }
    }

    fn font_size_buttons(&mut self, ui: &mut egui::Ui, enabled: bool) {
        for (label, delta) in [("Increase font size", 1.0), ("Decrease font size", -1.0)] {
            if text_button(ui, egui::RichText::new(""), label, enabled, false) {
                self.apply_quick_text(TextAction::Size(delta));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    fn grid_frame(
        ctx: &egui::Context,
        e: &mut Editor,
        events: Vec<egui::Event>,
    ) -> Vec<egui::Response> {
        let mut buttons = vec![];
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                ui.horizontal(|ui| {
                    buttons.extend(e.show_layout_grid(ui, false).into_iter().map(|(_, r)| r));
                    buttons.extend(e.show_layout_grid(ui, true).into_iter().map(|(_, r)| r));
                    buttons.extend(e.show_justification_grid(ui).into_iter().map(|(_, r)| r));
                });
            },
        );
        output.textures_delta.clear();
        buttons
    }
    fn click_grid(ctx: &egui::Context, e: &mut Editor, index: usize) {
        grid_frame(ctx, e, vec![]);
        let buttons = grid_frame(ctx, e, vec![]);
        let pos = buttons[index].rect.center();
        for pressed in [true, false] {
            grid_frame(
                ctx,
                e,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn alignment_grid_expands_canvas_keeps_reference_on_screen_and_undoes_once() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for (id, center, size) in [(1, [14.0, 14.0], 24), (2, [140.0, 100.0], 100)] {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    center,
                    size,
                    [30, 90, 150, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        let before = e.session.document.clone();
        e.select_only(Some(1));
        e.click_selection(Some(2), true);
        let viewport = egui::Rect::from_min_size(egui::pos2(20.0, 90.0), egui::vec2(800.0, 500.0));
        let old = e.view.transform(viewport, egui::vec2(640.0, 400.0), 1.0);
        let anchor = old.screen_point(before.annotations[0].start);
        click_grid(&ctx, &mut e, 2); // Center Both, in the middle of the cross.
        assert!(e.session.document.width > 640 && e.session.document.height > 400);
        let after = e.view.transform(
            viewport,
            egui::vec2(
                e.session.document.width as f32,
                e.session.document.height as f32,
            ),
            1.0,
        );
        assert!(
            (after.screen_point(e.session.document.annotations[0].start) - anchor).length() < 0.001
        );
        assert_eq!(e.selection_ordered_ids(), [1, 2]);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before.annotations);
        assert_eq!(
            (e.session.document.width, e.session.document.height),
            (640, 400)
        );
        assert!(!e.session.can_undo());
    }
    #[test]
    fn exposed_grids_apply_every_alignment_distribution_and_text_position_with_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for id in 1..=3 {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    [[100.0, 60.0], [170.0, 145.0], [300.0, 250.0]][id as usize - 1],
                    30 + 6 * id as u32,
                    [40, 80, 120, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        e.tool = None;
        e.select_all_matching();
        let original = e.session.document.annotations.clone();
        // Explicit selection chronology differs from document/z ordering.
        e.select_only(Some(2));
        e.click_selection(Some(3), true);
        e.click_selection(Some(1), true);
        assert_eq!(e.selection_ordered_ids(), [2, 3, 1]);
        for (index, action) in ALIGNS.into_iter().chain(DISTRIBUTIONS).enumerate() {
            click_grid(&ctx, &mut e, index);
            let current = if index < ALIGNS.len() {
                e.quick_controls.prefs.alignment
            } else {
                e.quick_controls.prefs.distribution
            };
            assert_eq!(current, action);
            if index < ALIGNS.len() {
                assert_eq!(
                    e.session.document.annotations[1], original[1],
                    "first selected object must remain fixed"
                );
            }
            assert_ne!(e.session.document.annotations, original, "{action:?}");
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, original);
            assert!(!e.session.can_undo());
        }
        e.click_selection(Some(2), true);
        assert_eq!(e.selection_ordered_ids(), [3, 1]);
        e.click_selection(Some(2), true);
        assert_eq!(e.selection_ordered_ids(), [3, 1, 2]);
        for (index, choice) in JUSTIFICATIONS.into_iter().enumerate() {
            click_grid(&ctx, &mut e, 9 + index);
            let (h, v) = choice.alignments();
            for a in e.selected_objects() {
                let t = a.text.unwrap();
                assert_eq!((t.horizontal, t.vertical), (h, v));
            }
            assert_eq!(e.quick_controls.prefs.justification, choice);
            if e.session.can_undo() {
                e.execute(Command::Undo);
            }
            assert_eq!(e.session.document.annotations, original);
        }
        e.select_only(Some(1));
        e.begin_text_edit();
        click_grid(&ctx, &mut e, 9);
        assert!(e.text_draft.is_some());
        assert_eq!(e.session.document.annotations, original);
        assert!(e.commit_text());
        let text = e.session.document.annotations[0].text.as_ref().unwrap();
        assert_eq!((text.horizontal, text.vertical), J::TopLeft.alignments());
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
    }

    #[test]
    fn ribbon_font_updates_selected_labels_as_one_undo_and_preserves_inline_transaction() {
        use super::*;
        use capture_core::text::TextFont;
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        e.session = capture_core::Session::new(fixture.clone()).unwrap();
        let ids: Vec<_> = fixture
            .annotations
            .iter()
            .filter(|a| a.text.is_some())
            .map(|a| a.id)
            .collect();
        for id in &ids {
            e.click_selection(Some(*id), true);
        }
        e.apply_quick_text(TextAction::Font(TextFont::HackRegular));
        assert!(
            e.selected_objects()
                .iter()
                .all(|a| a.text.as_ref().unwrap().font == TextFont::HackRegular)
        );
        for (before, after) in fixture
            .annotations
            .iter()
            .zip(&e.session.document.annotations)
        {
            assert_eq!(before.start, after.start);
            assert_eq!(before.end, after.end);
            assert_eq!(
                before.text.as_ref().map(|t| &t.content),
                after.text.as_ref().map(|t| &t.content)
            );
        }
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, fixture.annotations);
        assert!(!e.session.can_undo());
        let id = fixture
            .annotations
            .iter()
            .find(|a| a.kind == crate::Kind::Text)
            .unwrap()
            .id;
        e.select_only(Some(id));
        e.begin_text_edit();
        e.apply_quick_text(TextAction::Font(TextFont::HackRegular));
        assert!(e.text_draft.is_some());
        assert_eq!(e.session.document.annotations, fixture.annotations);
        assert!(!e.session.can_undo());
        assert!(e.commit_text());
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, fixture.annotations);
    }

    #[test]
    fn toolbar_save_failure_preserves_ui_state_and_retry_merges_other_choices() {
        use super::*;
        use capture_core::toolbar::ToolId;
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        store.remember(Choice::CanvasSnapping(false)).unwrap();
        let mut controls = QuickControls {
            prefs: store.read().unwrap(),
            store: Some(store),
            warning: None,
            format_source: None,
            copied_format: None,
            format_locked: false,
        };
        let original = controls.prefs.clone();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.path().join("quick-controls.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let choice = Choice::MoveTool {
            tool: ToolId::Step,
            before: Some(ToolId::Text),
        };
        assert!(controls.change_toolbar(choice.clone()).is_err());
        assert_eq!(controls.prefs, original);
        drop(lock);
        let other = Store::open(directory.path()).unwrap();
        other.remember(Choice::AutoSwitchTool(true)).unwrap();
        assert!(controls.change_toolbar(choice).unwrap());
        assert!(controls.prefs.auto_switch_tool);
        assert!(!controls.prefs.canvas_snapping);
        assert_eq!(controls.prefs.toolbar_order[0], ToolId::Step);
        assert_eq!(
            Store::open(directory.path()).unwrap().read().unwrap(),
            controls.prefs
        );
    }
    use super::*;
    #[test]
    fn plain_text_font_controls_resize_selection_and_inline_draft_atomically() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for id in [1, 2] {
            let mut a = e.new_annotation(crate::Kind::Text, [30.0, 30.0], [130.0, 60.0]);
            a.id = id;
            let t = a.text.as_mut().unwrap();
            t.content = "Visible font size".into();
            t.size_points = 48.0;
            e.session.document.annotations.push(a);
        }
        e.tool = None;
        e.select_all_matching();
        let before = e.session.document.annotations.clone();
        e.apply_quick_text(TextAction::Size(1.0));
        for (a, b) in e.session.document.annotations.iter().zip(&before) {
            assert_eq!(*a, b.change_text_size(1.0, true, [640, 400]).unwrap());
            assert_ne!(a.end, b.end);
        }
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        assert!(!e.session.can_undo());
        e.select_only(Some(1));
        e.begin_text_edit();
        e.apply_quick_text(TextAction::SetSize(24.0));
        let preview = e.text_draft.as_ref().unwrap().preview.clone();
        assert_eq!(e.session.document.annotations, before);
        assert_eq!(
            preview,
            before[0].change_text_size(24.0, false, [640, 400]).unwrap()
        );
        assert!(e.commit_text());
        assert_eq!(e.session.document.annotations[0], preview);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        // A failed member must not partially resize its multi-selection peers.
        e.session.document.annotations[1]
            .text
            .as_mut()
            .unwrap()
            .content = "overflow ".repeat(200);
        e.select_all_matching();
        let before = e.session.document.annotations.clone();
        e.apply_quick_text(TextAction::SetSize(144.0));
        assert_eq!(e.session.document.annotations, before);
        assert!(!e.session.can_undo());
        assert!(e.status.contains("larger canvas"));
    }

    #[test]
    fn quick_text_buttons_format_the_selection_and_draft_with_one_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for id in [1, 2] {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    [100.0 * id as f32, 100.0],
                    44,
                    [40, 60, 80, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        e.tool = None;
        e.select_all_matching();
        let before = e.session.document.annotations.clone();
        e.apply_quick_text(TextAction::Bold(true));
        assert!(
            e.session
                .document
                .annotations
                .iter()
                .all(|a| a.text.as_ref().unwrap().bold)
        );
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        assert!(!e.session.can_undo());
        e.apply_quick_text(TextAction::Italic(true));
        e.apply_quick_text(TextAction::Size(1.0));
        for (a, b) in e.session.document.annotations.iter().zip(&before) {
            let t = a.text.as_ref().unwrap();
            assert!(t.italic);
            assert_eq!(t.size_points, b.text.as_ref().unwrap().size_points + 1.0);
            assert_eq!(a.start, b.start);
            assert_eq!(t.content, b.text.as_ref().unwrap().content);
        }
        e.select_only(Some(1));
        e.begin_text_edit();
        let doc = e.session.document.annotations.clone();
        e.apply_quick_text(TextAction::Bold(true));
        e.apply_quick_text(TextAction::Size(-1000.0));
        assert_eq!(
            e.text_draft
                .as_ref()
                .unwrap()
                .preview
                .text
                .as_ref()
                .unwrap()
                .size_points,
            6.0
        );
        assert_eq!(e.session.document.annotations, doc);
        assert!(e.commit_text());
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, doc);
    }
    #[test]
    fn underline_and_strike_ribbon_clicks_format_mixed_selection_and_clear_once() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        e.session = capture_core::Session::new(fixture).unwrap();
        let ids: Vec<_> = e
            .session
            .document
            .annotations
            .iter()
            .filter(|a| a.text.is_some())
            .map(|a| a.id)
            .collect();
        e.select_only(Some(ids[0]));
        for id in ids.iter().skip(1) {
            e.click_selection(Some(*id), true);
        }
        let before = e.session.document.annotations.clone();
        let frame = |e: &mut Editor, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| e.show_quick_font_controls(ui),
            );
            output.textures_delta.clear();
        };
        let read = |name| {
            ctx.data(|d| {
                d.get_temp::<(egui::Rect, bool)>(egui::Id::unique(("quick_text_button", name)))
                    .unwrap()
            })
        };
        let click = |e: &mut Editor, name| {
            frame(e, vec![]);
            frame(e, vec![]);
            let pos = read(name).0.center();
            for pressed in [true, false] {
                frame(
                    e,
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
            frame(e, vec![]);
        };
        click(&mut e, "Underline");
        assert!(read("Underline").1);
        assert_eq!(e.session.document.version, 5);
        assert!(
            e.selected_objects()
                .iter()
                .all(|a| a.text.as_ref().unwrap().underline)
        );
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        assert_eq!(e.session.document.version, 1);
        assert!(!e.session.can_undo());
        click(&mut e, "Underline");
        click(&mut e, "Strikethrough");
        assert!(read("Underline").1 && read("Strikethrough").1);
        let ruled = e.session.document.annotations.clone();
        click(&mut e, "Clear text formatting");
        assert!(!read("Underline").1 && !read("Strikethrough").1);
        for (old, new) in ruled.iter().zip(&e.session.document.annotations) {
            if let Some(t) = &new.text {
                assert!(!t.decorated());
                assert_eq!(t.content, old.text.as_ref().unwrap().content);
            }
        }
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, ruled);
        e.select_only(Some(ids[0]));
        e.begin_text_edit();
        e.apply_quick_text(TextAction::Underline(false));
        e.apply_quick_text(TextAction::Strikethrough(false));
        assert_eq!(e.session.document.annotations, ruled);
        assert!(
            !e.text_draft
                .as_ref()
                .unwrap()
                .preview
                .text
                .as_ref()
                .unwrap()
                .decorated()
        );
        assert!(e.commit_text());
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, ruled);
    }
    #[test]
    fn quick_actions_repeat_on_new_selections_and_justify_without_changing_labels() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for (id, point) in [(1, [100.0, 80.0]), (2, [200.0, 150.0]), (3, [300.0, 240.0])] {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    point,
                    44,
                    [20, 90, 150, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        let original = e.session.document.annotations.clone();
        e.tool = None;
        e.select_all_matching();
        e.apply_quick_layout(L::Left);
        assert_eq!(e.quick_controls.prefs.alignment, L::Left);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        e.select_only(Some(2));
        e.click_selection(Some(3), true);
        e.apply_quick_layout(e.quick_controls.prefs.alignment);
        assert_eq!(
            e.session.document.annotations[1].start[0],
            e.session.document.annotations[2].start[0]
        );
        e.execute(Command::Undo);
        e.apply_quick_justification(J::BottomRight);
        assert_eq!(e.session.document.annotations[0], original[0]);
        for (a, b) in original.iter().zip(&e.session.document.annotations).skip(1) {
            assert_eq!(a.start, b.start);
            assert_eq!(
                a.text.as_ref().unwrap().content,
                b.text.as_ref().unwrap().content
            );
            assert_eq!(
                b.text.as_ref().unwrap().horizontal,
                capture_core::text::HorizontalAlignment::Right
            );
            assert_eq!(
                b.text.as_ref().unwrap().vertical,
                capture_core::text::VerticalAlignment::Bottom
            );
        }
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        assert_eq!(e.quick_controls.prefs.justification, J::BottomRight);
    }

    #[test]
    fn format_painter_batches_preserve_geometry_labels_and_skip_unsupported_fields() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let fixture: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        e.session = capture_core::Session::new(fixture).unwrap();
        let ids: Vec<_> = e
            .session
            .document
            .annotations
            .iter()
            .filter(|a| a.text.is_some())
            .map(|a| a.id)
            .collect();
        e.select_only(Some(ids[0]));
        e.apply_quick_text(TextAction::SetSize(36.5));
        e.apply_quick_text(TextAction::Bold(true));
        let before = e.session.document.annotations.clone();
        e.arm_format_painter();
        e.select_only(Some(ids[1]));
        for id in ids.iter().skip(2) {
            e.click_selection(Some(*id), true);
        }
        e.paint_format_to(ids[1]);
        for a in e.selected_objects() {
            let previous = before.iter().find(|b| b.id == a.id).unwrap();
            assert_eq!(a.kind, previous.kind);
            assert_eq!(a.start, previous.start);
            assert_eq!(a.end, previous.end);
            assert_eq!(
                a.text.as_ref().unwrap().content,
                previous.text.as_ref().unwrap().content
            );
            assert_eq!(a.text.as_ref().unwrap().size_points, 36.5);
        }
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        e.select_only(Some(ids[0]));
        e.arm_format_painter();
        let incompatible = e
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.kind == crate::Kind::Arrow)
            .unwrap()
            .id;
        e.select_only(Some(ids[1]));
        e.click_selection(Some(incompatible), true);
        e.paint_format_to(ids[1]);
        assert_ne!(e.session.document.annotations, before);
        assert_eq!(
            e.session
                .document
                .annotations
                .iter()
                .find(|a| a.id == incompatible),
            before.iter().find(|a| a.id == incompatible)
        );
        assert!(e.quick_controls.format_source.is_none());
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
    }

    #[test]
    fn creation_defaults_remember_each_type_edits_and_explicit_presets_across_restart() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let directory = tempfile::tempdir().unwrap();
        e.quick_controls.store = Some(Store::open(directory.path()).unwrap());
        e.tool = Some(crate::Kind::Rectangle);
        let mut rectangle = e.new_annotation(crate::Kind::Rectangle, [20.0, 20.0], [160.0, 120.0]);
        rectangle.color = [210, 20, 70, 255];
        rectangle.fill = Some([80, 220, 160, 140]);
        rectangle.corner_radius = 18.0;
        rectangle.width = 6;
        e.execute(Command::Add {
            annotation: rectangle.clone(),
        });
        e.choose_tool(15); // Line
        let mut line = e.new_annotation(crate::Kind::Line, [220.0, 180.0], [410.0, 230.0]);
        line.color = [10, 80, 240, 255];
        line.width = 3;
        e.execute(Command::Add { annotation: line });
        e.select_only(Some(rectangle.id));
        e.auto_activate_selected_tool();
        assert_eq!(e.tool, Some(crate::Kind::Rectangle));
        e.select_only(None);
        let next = e.new_annotation(crate::Kind::Rectangle, [210.0, 220.0], [380.0, 330.0]);
        assert_eq!(
            (next.color, next.width, next.fill, next.corner_radius),
            (
                rectangle.color,
                rectangle.width,
                rectangle.fill,
                rectangle.corner_radius
            )
        );
        rectangle.fill = None;
        rectangle.corner_radius = 25.0;
        e.execute(Command::Update {
            annotation: rectangle.clone(),
        });
        e.choose_tool(15);
        e.choose_tool(4);
        e.select_only(None);
        let next = e.new_annotation(crate::Kind::Rectangle, [210.0, 220.0], [380.0, 330.0]);
        assert_eq!(next.fill, None);
        assert_eq!(next.corner_radius, 25.0);
        let preset = capture_core::object_styles::presets(crate::Kind::Rectangle).remove(0);
        e.apply_object_template(&preset, false);
        let next = e.new_annotation(crate::Kind::Rectangle, [210.0, 220.0], [380.0, 330.0]);
        assert_eq!(next.color, preset.color);
        assert_eq!(next.fill, preset.fill);
        let loaded = Store::open(directory.path()).unwrap();
        assert_eq!(loaded.read().unwrap().version, 4);
        let mut restarted = crate::canvas::tests::editor(&ctx);
        restarted.quick_controls = QuickControls {
            prefs: loaded.read().unwrap(),
            store: Some(loaded),
            ..Default::default()
        };
        restarted.choose_tool(4);
        let next = restarted.new_annotation(crate::Kind::Rectangle, [210.0, 220.0], [380.0, 330.0]);
        assert_eq!(next.color, preset.color);
        assert_eq!(next.fill, preset.fill);
        restarted.choose_tool(15);
        let next = restarted.new_annotation(crate::Kind::Line, [20.0, 20.0], [160.0, 120.0]);
        assert_eq!(next.color, [10, 80, 240, 255]);
        assert_eq!(next.width, 3);
    }
}
