//! Image and video panes share the same visual structure, while their content
//! remains contextual. Presentation choices are session-only and never Undo.
use crate::video_editor::property_design as design;
use crate::{Annotation, Editor, Kind, egui};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
enum Tab {
    #[default]
    Style,
    Text,
    Image,
    Layout,
}
impl Tab {
    fn label(self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Text => "Text",
            Self::Image => "Image",
            Self::Layout => "Layout",
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Self::Style => "Style",
            Self::Text => "Text",
            Self::Image => "Image",
            Self::Layout => "Position",
        }
    }
}
pub(crate) fn identity(a: &Annotation) -> (&'static str, &'static str) {
    if a.focus.is_some() {
        return ("Focus", "Focus");
    }
    if a.is_image_frame() {
        return ("Image", "Image frame");
    }
    match a.kind {
        Kind::Image if a.image.as_ref().is_some_and(|b| b.vector().is_some()) => {
            ("Icons", "Vector icon")
        }
        Kind::Image => ("Image", "Image"),
        Kind::Text => ("Text", "Text object"),
        Kind::Callout => ("Callout", "Callout"),
        Kind::Step => ("Steps", "Step marker"),
        Kind::Arrow | Kind::Line => ("Lines", "Line"),
        Kind::Pen | Kind::Highlighter => ("Penlighter", "Penlighter"),
        Kind::Eraser => ("Eraser", "Eraser"),
        Kind::Redact => ("Obfuscate", "Redaction"),
        Kind::Blur => ("Obfuscate", "Blur"),
        Kind::Pixelate => ("Obfuscate", "Pixelate"),
        _ => ("Shape", "Shape"),
    }
}
impl Editor {
    pub(super) fn image_properties_pane(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8., 8.);
        ui.spacing_mut().interact_size.y = 28.;
        ui.visuals_mut().widgets.inactive.bg_fill = egui::Color32::from_rgb(69, 78, 82);
        ui.visuals_mut().widgets.inactive.bg_stroke =
            egui::Stroke::new(1., egui::Color32::from_rgb(98, 112, 118));
        self.property_tabs(ui);
        let layers = self.shell.property_tab == crate::shell::PropertyTab::Objects;
        let selected = self.selected_annotation();
        let context = selected
            .as_ref()
            .map(|a| (a.kind as u8, a.is_image_frame()));
        let tab_key = egui::Id::unique(("image-properties-tab", context));
        let mut tab = ui
            .ctx()
            .data(|d| d.get_temp::<Tab>(tab_key))
            .unwrap_or_else(|| {
                if selected.as_ref().is_some_and(|a| a.kind == Kind::Text) {
                    Tab::Text
                } else {
                    Tab::Style
                }
            });
        if let Some(draft) = &self.text_draft {
            let key = egui::Id::unique("image-properties-text-entry");
            let changed = ui.ctx().data_mut(|d| {
                let changed = d.get_temp::<u64>(key) != Some(draft.preview.id);
                d.insert_temp(key, draft.preview.id);
                changed
            });
            if changed {
                tab = Tab::Text;
            }
        } else {
            ui.ctx()
                .data_mut(|d| d.remove::<u64>(egui::Id::unique("image-properties-text-entry")));
        }
        let content = selected
            .as_ref()
            .filter(|a| a.is_image_frame() && crate::image_frame::content_mode(ui.ctx(), a.id))
            .map(|a| a.id);
        let frame_entry = egui::Id::unique("image-properties-frame-entry");
        let changed = ui.ctx().data_mut(|d| {
            let before = d.get_temp::<u64>(frame_entry);
            if let Some(id) = content {
                d.insert_temp(frame_entry, id);
            } else {
                d.remove::<u64>(frame_entry);
            }
            before != content
        });
        if changed && content.is_some() {
            tab = Tab::Image;
        }
        if let Some(a) = &selected.filter(|_| !layers) {
            let (glyph, kind) = identity(a);
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(38., 38.), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(r, 6., egui::Color32::from_rgb(37, 50, 56));
                        crate::video_editor::ui::icon(
                            ui.painter(),
                            r.shrink(9.),
                            glyph,
                            egui::Color32::from_rgb(83, 194, 220),
                        );
                        ui.vertical(|ui| {
                            let title = a
                                .text
                                .as_ref()
                                .filter(|t| !t.content.is_empty())
                                .map(|t| {
                                    t.content
                                        .lines()
                                        .next()
                                        .unwrap_or("")
                                        .chars()
                                        .take(32)
                                        .collect::<String>()
                                })
                                .unwrap_or_else(|| kind.into());
                            ui.label(crate::panel_ui::title(
                                if self.selected_objects().len() > 1 {
                                    format!("{} objects selected", self.selected_objects().len())
                                } else {
                                    title
                                },
                                14.,
                            ));
                            ui.label(
                                egui::RichText::new(kind)
                                    .size(12.)
                                    .color(crate::panel_ui::MUTED),
                            );
                        });
                    });
                });
            let mut tabs = vec![Tab::Style];
            if a.focus.is_none() && !a.is_image_frame() && (a.text.is_some() || a.is_shape()) {
                tabs.push(Tab::Text);
            }
            if a.is_image_frame()
                || (a.kind == Kind::Image
                    && !a.image.as_ref().is_some_and(|b| b.vector().is_some()))
            {
                tabs.push(Tab::Image);
            }
            tabs.push(Tab::Layout);
            if !self.selection_same_kind() {
                tabs = vec![Tab::Layout];
            }
            if !tabs.contains(&tab) {
                tab = tabs[0];
            }
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(8, 0))
                .show(ui, |ui| {
                    crate::panel_ui::rail(ui, |ui| {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - (tabs.len() - 1) as f32 * 4.)
                                / tabs.len() as f32;
                            let labels = tabs.iter().all(|t| {
                                ui.painter()
                                    .layout_no_wrap(
                                        t.label().into(),
                                        crate::panel_ui::font(12.),
                                        egui::Color32::WHITE,
                                    )
                                    .size()
                                    .x
                                    + 35.
                                    <= width
                            });
                            for t in tabs {
                                let response = crate::panel_ui::tab(
                                    ui,
                                    t.label(),
                                    t.glyph(),
                                    tab == t,
                                    width,
                                    labels,
                                );
                                #[cfg(test)]
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::unique(("image-property-tab", t.label())),
                                        response.rect,
                                    )
                                });
                                if response.clicked() {
                                    tab = t;
                                }
                            }
                        });
                    });
                });
        }
        ui.ctx().data_mut(|d| d.insert_temp(tab_key, tab));
        let area = ui.available_rect_before_wrap();
        let convertible = !layers
            && self.selected_objects().len() == 1
            && self
                .selected_annotation()
                .is_some_and(|a| a.focus.is_none() && (a.is_shape() || a.is_image_frame()));
        let footer_height = if convertible { 76. } else { 40. };
        let available = (area.height() - footer_height).max(0.);
        let key = ("image-property-content", context, layers, tab);
        let scroll = egui::ScrollArea::vertical()
            .id_salt(key)
            .max_height(available)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        design::begin(ui, key);
                        if layers {
                            self.show_objects(ui);
                        } else if self.selected_annotation().is_some()
                            && !self.shell.annotations_hidden
                        {
                            match tab {
                                Tab::Style => self.image_style_groups(ui),
                                Tab::Text => self.image_text_groups(ui),
                                Tab::Image => self.image_content_groups(ui),
                                Tab::Layout => self.image_layout_groups(ui),
                            }
                        } else {
                            self.image_tool_groups(ui);
                        }
                    });
            });
        design::finish(ui, scroll.content_size.y, available);
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("properties-scroll-content"),
                scroll.inner_rect,
            )
        });
        let bottom = egui::Rect::from_min_max(
            egui::pos2(area.left(), area.bottom() - footer_height + 8.),
            area.max,
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| {
            if convertible {
                self.frame_conversion_button(
                    ui,
                    egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), 28.),
                    ),
                );
            }
            let count = self.selected_objects().len();
            if crate::object_pane::delete_button(
                ui,
                count > 0 && !self.shell.annotations_hidden,
                if count > 1 {
                    "Delete selected objects"
                } else {
                    "Delete object"
                },
                ui.available_width(),
            )
            .clicked()
            {
                self.delete_selection();
            }
        });
    }
    fn image_tool_groups(&mut self, ui: &mut egui::Ui) {
        if self.shell.focus.active && matches!(self.tool, Some(Kind::Rectangle | Kind::Ellipse)) {
            self.focus_properties(ui, None);
            return;
        }
        if self.shell.annotations_hidden {
            design::heading(ui, "Hide annotations", "Annotations hidden");
            ui.label("Show annotations to resume editing.");
            return;
        }
        if self.icon_library.active {
            self.icon_properties(ui);
            return;
        }
        if self.cut_out.is_some() {
            self.cut_out_properties(ui);
            return;
        }
        let Some(kind) = self.tool else {
            self.move_functions(ui);
            return;
        };
        design::heading(ui, "Presets", "Presets");
        if matches!(kind, Kind::Arrow | Kind::Line) {
            self.arrow_styles(ui);
        } else {
            self.object_templates(ui, kind);
        }
        design::end(ui);
        design::heading(ui, "Style", "Next object");
        self.image_tool_defaults(ui);
        design::end(ui);
        if matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Redact) {
            if kind == Kind::Redact {
                crate::controls::toggle(
                    ui,
                    &mut self.shell.redaction_labels,
                    "Show redaction labels",
                );
            }
            if crate::video_editor::ui::button(
                ui,
                "Sensitive search",
                "Find Sensitive Info…",
                ui.available_width(),
                true,
            )
            .clicked()
            {
                self.open_privacy_review();
            }
        }
    }
    fn image_style_groups(&mut self, ui: &mut egui::Ui) {
        let Some(mut a) = self.selected_annotation() else {
            return;
        };
        if a.focus.is_some() {
            self.focus_properties(ui, Some(a));
            return;
        }
        if a.is_image_frame() {
            design::heading(ui, "Presets", "Presets");
            self.frame_style_choices(ui);
            design::end(ui);
        } else if a.kind != Kind::Image {
            design::heading(ui, "Presets", "Presets");
            {
                if matches!(a.kind, Kind::Arrow | Kind::Line) {
                    self.arrow_styles(ui)
                } else {
                    self.object_templates(ui, a.kind)
                }
            }
            design::end(ui);
        }
        // A preset may have replaced the selection in this very frame.
        a = self.selected_annotation().unwrap_or(a);
        let before = a.clone();
        if a.kind == Kind::Step {
            design::heading(ui, "Steps", "Marker");
            let (lo, hi) = a.body_bounds();
            let mut size = hi[0] - lo[0];
            if crate::property_layout::slider(
                ui,
                "Marker size",
                egui::Slider::new(&mut size, 1. ..=16384.).logarithmic(true),
            )
            .changed()
                && let Ok(next) = capture_core::steps::resized_marker(
                    &a,
                    size,
                    [self.session.document.width, self.session.document.height],
                )
            {
                a = next;
            }
            if let Some(t) = &mut a.text {
                ui.horizontal(|ui| {
                    ui.label("Step label");
                    ui.add_sized(
                        [90., 28.],
                        crate::unicode_input::TextEdit::singleline(&mut t.content)
                            .char_limit(8)
                            .align(egui::Align2::CENTER_CENTER),
                    );
                });
            }
            self.step_properties(ui);
            design::end(ui);
        }
        if matches!(a.kind, Kind::Redact | Kind::Blur | Kind::Pixelate) {
            design::heading(ui, "Sensitive search", "Privacy & Security");
        } else {
            design::heading(ui, "Palette", "Color & outline");
        }
        match a.kind {
            Kind::Rectangle | Kind::Ellipse | Kind::Polygon => {
                crate::shape_properties::show(ui, &mut a)
            }
            Kind::Image => {
                if a.image.as_ref().is_some_and(|b| b.vector().is_some()) {
                    crate::image_frame::properties_content(ui, &mut a, false);
                }
                crate::property_layout::slider(
                    ui,
                    "Opacity",
                    egui::Slider::new(&mut a.color[3], 0..=255),
                );
            }
            Kind::Callout => {
                crate::property_layout::color_row(ui, 5, |ui| {
                    if let Some(c) = &mut a.callout {
                        crate::property_layout::swatch(ui, "Fill", &mut c.fill);
                        crate::property_layout::swatch(ui, "Outline", &mut c.outline);
                    }
                    crate::text_properties::color_swatch(ui, &mut a.text, a.color);
                });
                crate::property_layout::slider(
                    ui,
                    "Outline width",
                    egui::Slider::new(&mut a.width, 1..=32),
                );
                crate::line_properties::outline(ui, &mut a);
            }
            Kind::Text => crate::text_properties::show(ui, &mut a),
            Kind::Step => {
                let mut stroke = a.step_outline.clone().unwrap_or_default();
                let original = stroke.color;
                crate::property_layout::color_row(ui, 5, |ui| {
                    crate::property_layout::swatch(ui, "Fill", &mut a.color);
                    crate::property_layout::swatch(ui, "Outline", &mut stroke.color);
                    let ink = capture_core::steps::foreground(&a).color;
                    crate::text_properties::color_swatch(ui, &mut a.text, ink);
                });
                let mut outline = a.step_outline.is_some() || stroke.color != original;
                crate::controls::toggle(ui, &mut outline, "Show outline");
                if outline {
                    crate::property_layout::slider(
                        ui,
                        "Outline width",
                        egui::Slider::new(&mut stroke.width, 0. ..=32.),
                    );
                    a.step_outline = Some(stroke);
                } else {
                    a.step_outline = None;
                }
            }
            Kind::Redact | Kind::Blur | Kind::Pixelate => {
                crate::obscure_properties::show(ui, &mut a);
                if a.kind == Kind::Redact {
                    crate::controls::toggle(
                        ui,
                        &mut self.shell.redaction_labels,
                        "Show redaction labels",
                    );
                }
                if crate::video_editor::ui::button(
                    ui,
                    "Sensitive search",
                    "Find Sensitive Info…",
                    ui.available_width(),
                    true,
                )
                .clicked()
                {
                    self.open_privacy_review();
                }
            }
            _ => {
                if a.kind != Kind::Eraser {
                    ui.horizontal(|ui| crate::property_layout::swatch(ui, "Color", &mut a.color));
                }
                crate::property_layout::slider(
                    ui,
                    "Width",
                    egui::Slider::new(&mut a.width, 1..=32),
                );
                crate::property_layout::slider(
                    ui,
                    "Opacity",
                    egui::Slider::new(&mut a.color[3], 0..=255),
                );
                crate::line_properties::show(ui, &mut a);
            }
        }
        design::end(ui);
        if !matches!(
            a.kind,
            Kind::Redact | Kind::Blur | Kind::Pixelate | Kind::Eraser
        ) {
            design::group(
                ui,
                "Style",
                "Appearance",
                if a.shadow.is_some() {
                    "Shadow on"
                } else {
                    "No shadow"
                },
                |ui| {
                    if matches!(a.kind, Kind::Rectangle | Kind::Callout) {
                        let max = a.max_corner_radius();
                        crate::property_layout::slider(
                            ui,
                            "Corner radius",
                            egui::Slider::new(&mut a.corner_radius, 0. ..=max),
                        );
                    }
                    crate::shadow_properties::show(ui, &mut a);
                },
            );
        }
        if a != before {
            self.update_text_sidebar(a, false);
        }
    }
    pub(crate) fn image_text_groups(&mut self, ui: &mut egui::Ui) {
        let Some(mut a) = self.selected_annotation() else {
            return;
        };
        if a.text.is_none() {
            if ui.button("Add text").clicked() {
                self.begin_text_edit();
            }
            return;
        }
        design::heading(ui, "Text", "Font");
        self.show_font_controls(ui, true);
        let Some(current) = self.selected_annotation() else {
            return;
        };
        a = current;
        let before = a.clone();
        crate::property_layout::color_row(ui, 3, |ui| {
            let ink = if a.kind == Kind::Step {
                capture_core::steps::foreground(&a).color
            } else {
                a.color
            };
            crate::text_properties::color_swatch(ui, &mut a.text, ink);
        });
        if a != before {
            self.update_text_sidebar(a, false);
        }
        design::end(ui);
        design::heading(ui, "Align left", "Paragraph");
        self.common_paragraph_controls(ui);
        self.show_justification_grid(ui);
        if let Some(mut a) = self.selected_annotation() {
            let before = a.clone();
            if let Some(s) = &mut a.text {
                use capture_core::text::rich;
                let range = self
                    .text_draft
                    .as_ref()
                    .filter(|d| {
                        d.selection_active
                            && (!d.selection.is_empty() || self.rich_text.caret_formatting)
                    })
                    .map_or(0..s.content.chars().count(), |d| d.selection.clone());
                let current = s
                    .rich
                    .as_ref()
                    .and_then(|r| r.paragraphs.iter().rev().find(|p| p.start <= range.start))
                    .cloned()
                    .unwrap_or_else(|| rich::Paragraph::for_style(s, range.start));
                let mut spacing = current.line_spacing;
                let mut after = current.after;
                ui.columns(2, |col| {
                    col[0].label("Line spacing");
                    col[0].add(
                        egui::DragValue::new(&mut spacing)
                            .range(0.5..=4.)
                            .speed(0.05)
                            .suffix(" ×"),
                    );
                    col[1].label("Paragraph after");
                    col[1].add(
                        egui::DragValue::new(&mut after)
                            .range(0. ..=256.)
                            .speed(1.)
                            .suffix(" px"),
                    );
                });
                if spacing != current.line_spacing || after != current.after {
                    rich::paragraph_range(s, range, |p| {
                        p.line_spacing = spacing;
                        p.after = after;
                    });
                }
                crate::controls::toggle(ui, &mut s.wrap, "Wrap text");
            }
            if a != before {
                self.update_text_sidebar(a, false);
            }
        }
        self.rich_text_buttons(ui);
        design::end(ui);
        design::group(ui, "Motion", "Text effects", "Text stroke & shadow", |ui| {
            if let Some(mut a) = self.selected_annotation() {
                let before = a.clone();
                crate::text_properties::outline_controls(ui, &mut a);
                crate::shadow_properties::show(ui, &mut a);
                if a != before {
                    self.update_text_sidebar(a, false);
                }
            }
        });
        design::group(
            ui,
            "Text",
            "Editing & fit",
            if self.rich_text.caret_formatting {
                "Typing at caret"
            } else {
                "Whole block"
            },
            |ui| {
                self.caret_formatting_toggle(ui);
                if let Some(mut a) = self.selected_annotation() {
                    let before = a.clone();
                    crate::text_properties::advanced(ui, &mut a);
                    if a != before {
                        self.update_text_sidebar(a, false);
                    }
                }
            },
        );
    }
    fn image_content_groups(&mut self, ui: &mut egui::Ui) {
        if let Some(a) = self.selected_annotation() {
            let empty = a.image.as_ref().is_none_or(|b| b.is_placeholder());
            design::heading(ui, "Image", "Image source");
            let response = crate::video_editor::ui::button(
                ui,
                "Image",
                if empty {
                    "Choose image"
                } else {
                    "Replace image"
                },
                ui.available_width(),
                empty,
            );
            self.choose_frame_source(ui, &response, a.id);
            if !empty
                && a.is_image_frame()
                && crate::video_editor::ui::button(
                    ui,
                    "Delete effect",
                    "Clear image",
                    ui.available_width(),
                    false,
                )
                .clicked()
            {
                self.frame_action(ui, a.id, Some("Clear Image"));
            }
            design::end(ui);
            if empty {
                return;
            }
        }
        if let Some(mut a) = self.selected_annotation() {
            let before = a.clone();
            design::heading(ui, "Crop", "Crop & zoom");
            crate::image_frame::properties_content(ui, &mut a, false);
            if a.is_shape() {
                let mut opacity = a.image_opacity();
                if crate::property_layout::slider(
                    ui,
                    "Image opacity",
                    egui::Slider::new(&mut opacity, 0..=255),
                )
                .changed()
                    && let Some(b) = &a.image
                {
                    a.image = Some(std::sync::Arc::new(b.with_frame_opacity(opacity)));
                }
            }
            design::end(ui);
            if a != before {
                self.preview_property_edit_with_rotation(a, false);
            }
        }
    }
    fn image_layout_groups(&mut self, ui: &mut egui::Ui) {
        if let Some(mut a) = self.selected_annotation() {
            let before = a.clone();
            design::heading(ui, "Position", "Position & size");
            let reset_key = egui::Id::unique((
                "image-layout-original",
                &self.history.active,
                &self.path,
                a.id,
            ));
            let original = ui.ctx().data_mut(|d| {
                let original = d
                    .get_temp::<Annotation>(reset_key)
                    .unwrap_or_else(|| a.clone());
                d.insert_temp(reset_key, original.clone());
                original
            });
            let ratio_key = egui::Id::unique("image-layout-keep-ratio");
            let mut keep_ratio = ui
                .ctx()
                .data(|d| d.get_temp::<bool>(ratio_key))
                .unwrap_or(false);
            design::geometry_fields(
                ui,
                &mut a,
                [self.session.document.width, self.session.document.height],
            );
            if keep_ratio {
                let (lo, hi) = before.body_bounds();
                let (next_lo, next_hi) = a.body_bounds();
                let old = [hi[0] - lo[0], hi[1] - lo[1]];
                let mut next = [next_hi[0] - next_lo[0], next_hi[1] - next_lo[1]];
                if next != old && old.iter().all(|n| *n > 0.) {
                    if next[0] != old[0] {
                        next[1] = next[0] * old[1] / old[0];
                    } else {
                        next[0] = next[1] * old[0] / old[1];
                    }
                    if let Ok(resized) = a.resized(
                        capture_core::selection::Handle::BottomRight,
                        [next_lo[0] + next[0], next_lo[1] + next[1]],
                        [self.session.document.width, self.session.document.height],
                    ) {
                        a = resized;
                    }
                }
            }
            crate::controls::toggle(ui, &mut keep_ratio, "Keep aspect ratio");
            ui.ctx().data_mut(|d| d.insert_temp(ratio_key, keep_ratio));
            if crate::video_editor::ui::button(
                ui,
                "Restart",
                "Reset position & size",
                ui.available_width(),
                false,
            )
            .on_hover_text(
                "Restore the position and size from when Layout was first opened for this object.",
            )
            .clicked()
            {
                a.start = original.start;
                a.end = original.end;
                a.points = original.points;
            }
            design::end(ui);
            design::heading(ui, "Rotation", "Transform");
            let rotation = crate::property_layout::rotation_controls(ui, &mut a)
                .is_some_and(|r| r[0].changed() || r[1].clicked());
            if a != before || rotation {
                self.update_text_sidebar(a, rotation);
            }
            ui.horizontal_wrapped(|ui| {
                let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.;
                for (label, glyph, action) in [
                    (
                        "Flip horizontal",
                        "Flip horizontally",
                        capture_core::ImageTransform::FlipHorizontal,
                    ),
                    (
                        "Flip vertical",
                        "Flip vertically",
                        capture_core::ImageTransform::FlipVertical,
                    ),
                    (
                        "Rotate left",
                        "Rotate left",
                        capture_core::ImageTransform::CounterClockwise,
                    ),
                    (
                        "Rotate right",
                        "Rotate right",
                        capture_core::ImageTransform::Clockwise,
                    ),
                ] {
                    if crate::video_editor::ui::button(
                        ui,
                        glyph,
                        if width < 135. { "" } else { label },
                        width,
                        false,
                    )
                    .on_hover_text(label)
                    .clicked()
                    {
                        self.begin_image_transform(action);
                    }
                }
            });
            design::end(ui);
            design::group(ui, "Position", "Alignment", "Selected objects", |ui| {
                let key = egui::Id::unique("image-align-to-canvas");
                let mut canvas = ui.ctx().data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
                egui::ComboBox::from_id_salt("image-align-to")
                    .selected_text(if canvas { "Canvas" } else { "Selected objects" })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut canvas, true, "Canvas");
                        ui.selectable_value(&mut canvas, false, "Selected objects");
                    });
                ui.ctx().data_mut(|d| d.insert_temp(key, canvas));
                if canvas {
                    let size = [
                        self.session.document.width as f32,
                        self.session.document.height as f32,
                    ];
                    for axis in 0..2 {
                        ui.horizontal(|ui| {
                            for anchor in 0..3 {
                                let label = [
                                    ["Align left", "Align center", "Align right"],
                                    ["Align top", "Align middle", "Align bottom"],
                                ][axis][anchor];
                                if crate::video_editor::ui::button(ui, label, "", 32., false)
                                    .on_hover_text(label)
                                    .clicked()
                                {
                                    let annotations = self
                                        .selected_objects()
                                        .into_iter()
                                        .filter_map(|a| {
                                            let (lo, hi) = a.body_bounds();
                                            let mut delta = [0.; 2];
                                            let page = self
                                                .session
                                                .document
                                                .page_rect(self.session.document.object_page(&a));
                                            delta[axis] = page[axis]
                                                + (page[axis + 2] - (hi[axis] - lo[axis]))
                                                    * anchor as f32
                                                    / 2.
                                                - lo[axis];
                                            a.translated(delta, [size[0] as u32, size[1] as u32])
                                                .ok()
                                        })
                                        .collect();
                                    self.execute(crate::Command::UpdateMany { annotations });
                                }
                            }
                        });
                    }
                } else {
                    self.show_layout_grid(ui, false);
                    self.show_layout_grid(ui, true);
                }
            });
            design::group(ui, "Layers", "Arrange & lock", "", |ui| {
                self.settings_grid(
                    ui,
                    &[
                        crate::ribbon_settings::Action::Forward,
                        crate::ribbon_settings::Action::Front,
                        crate::ribbon_settings::Action::Backward,
                        crate::ribbon_settings::Action::Back,
                    ],
                    4,
                );
                let ids: Vec<_> = self.selected_objects().iter().map(|a| a.id).collect();
                let mut locked = ids
                    .iter()
                    .all(|id| self.session.document.object_state(*id).locked);
                let mut visible = ids
                    .iter()
                    .all(|id| self.session.document.object_visible(*id));
                if crate::controls::toggle(ui, &mut locked, "Lock object").changed() {
                    self.execute(crate::Command::SetObjectsLocked {
                        ids: ids.clone(),
                        locked,
                    });
                }
                if crate::controls::toggle(ui, &mut visible, "Visible").changed() {
                    self.execute(crate::Command::SetObjectsVisible { ids, visible });
                }
            });
        }
    }
}
