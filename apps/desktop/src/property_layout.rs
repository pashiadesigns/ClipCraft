use crate::tips::Tip as _;
use crate::{Annotation, Editor, Kind, egui, shell::PropertyTab};
use egui::{Color32, Stroke};

/// Legacy callers use the same open, left-aligned headings as the current
/// Properties groups. No filled bar or pair of enclosing rules.
pub(super) fn section(ui: &mut egui::Ui, title: &str) -> egui::Rect {
    let rect = section_frame(ui);
    let glyph = match title {
        "Presets" => "Presets",
        "Vector icon" => "Icons",
        "Image frame" | "Image frame styles" => "Image",
        "Cut Out Properties" => "Crop",
        "Text" | "Character spacing" => "Text",
        _ => "Style",
    };
    crate::video_editor::ui::icon(
        ui.painter(),
        egui::Rect::from_center_size(
            rect.left_center() + egui::vec2(9., 0.),
            egui::vec2(17., 17.),
        ),
        glyph,
        crate::panel_ui::MUTED,
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(27., 0.),
        egui::Align2::LEFT_CENTER,
        title,
        crate::panel_ui::font(13.),
        Color32::WHITE,
    );
    rect
}

pub(super) fn section_help(ui: &mut egui::Ui, title: &str, help: &str) -> egui::Rect {
    let rect = section(ui, title);
    help_at(ui, beside_title(ui, rect, title), title, help);
    rect
}
/// Where a section's ? sits: just right of its title (BB8), not at the far edge.
pub(super) fn beside_title(ui: &egui::Ui, rect: egui::Rect, title: &str) -> egui::Pos2 {
    let width = ui
        .painter()
        .layout_no_wrap(title.into(), crate::panel_ui::font(13.), Color32::WHITE)
        .size()
        .x;
    rect.left_center() + egui::vec2(27. + width + 14., 0.)
}

/// Help occupies the header, never a row in the property controls.
pub(super) fn help_at(ui: &mut egui::Ui, center: egui::Pos2, title: &str, help: &str) {
    let rect = egui::Rect::from_center_size(center, egui::vec2(24.0, 24.0));
    let response = ui.interact(
        rect,
        ui.make_persistent_id(("property-help", title)),
        egui::Sense::hover(),
    );
    let color = if response.hovered() {
        Color32::WHITE
    } else {
        Color32::from_gray(195)
    };
    ui.painter()
        .circle_stroke(center, 9.0, Stroke::new(1.0, color));
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        "?",
        egui::FontId::proportional(13.0),
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{title} help"))
    });
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("property-help", title)), rect));
    response
        .on_hover_cursor(egui::CursorIcon::Help)
        .on_hover_ui(|ui| {
            ui.set_max_width(310.0);
            ui.label(help);
        });
}

pub(super) fn tool_help(kind: Option<Kind>) -> &'static str {
    match kind {
        Some(Kind::Redact) => {
            "Draw an opaque black cover. PNG export and Copy All flatten the covering pixels. Editable projects retain the original image; share the flattened output for redaction."
        }
        Some(Kind::Blur | Kind::Pixelate) => {
            "Drag out the region, then move or resize it with its handles. Choose Rectangle, Ellipse or Freehand under Next object. Intensity updates live. These are visual effects: use Redact for an opaque cover. Editable projects retain the original image."
        }
        Some(Kind::Eraser) => {
            "Drag to erase to transparency; release to keep, Undo to restore. Choose a circle preset for brush size. Strokes remain editable. The configured stroke-size modifier + mouse wheel adjusts the brush width."
        }
        Some(Kind::Text | Kind::Callout) => TEXT_HELP,
        Some(Kind::Step) => {
            "Press empty space to place a marker; hold and drag to position, then release. Click an existing marker to select it. Numbering continues after the highest label on this canvas; edit a marker's label to change the sequence. Shift-click selects multiple objects for shared formatting."
        }
        Some(Kind::Rectangle | Kind::Ellipse | Kind::Polygon) => {
            "Drag to draw; hold Shift for equal proportions. Handles resize the object live; corner nodes round supported shapes. Double-click to edit text. Shift-click selects multiple objects for shared formatting."
        }
        Some(Kind::Arrow | Kind::Line) => {
            "Drag to draw; hold Shift to constrain the angle. The configured stroke-size modifier + mouse wheel adjusts the stroke width. Drag endpoints to resize, or the middle diamond to bend. Choose circular or Bézier curves, endpoint shapes/sizes and line patterns. The configured duplicate-drag modifier copies an object; Shift-click selects multiple objects for shared formatting."
        }
        Some(Kind::Highlighter) => {
            "Choose Highlighter pen for freehand strokes or Highlighter box for a filled region. The configured stroke-size modifier + mouse wheel adjusts pen width. Drag to create; Shift constrains a rectangle to a square. Color and opacity update live. Release commits one Undo."
        }
        Some(Kind::Pen) => {
            "Drag to draw a stroke. The configured stroke-size modifier + mouse wheel adjusts the stroke width. Select a stroke to change its color, width or opacity live. Shift-click selects multiple objects for shared formatting."
        }
        _ => {
            "Click an object to select it; Shift-click toggles multiple selection. Drag objects or their handles. Arrow keys nudge; Shift moves 10 pixels. Same-type selections share formatting; mixed selections move together."
        }
    }
}

pub(super) const TEXT_HELP: &str = "Drag a box and type directly. Double-click text in an existing object to edit it. Font and justification changes apply live; Shift-click selects multiple objects for shared formatting. Advanced controls text fitting, wrapping, padding and opacity.";

fn section_frame(ui: &mut egui::Ui) -> egui::Rect {
    ui.add_space(10.);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::hover());
    ui.add_space(4.);
    rect
}

/// Contextual navigation stays in the properties section, not its outer tab.
pub(super) fn text_section_with_back(ui: &mut egui::Ui) -> egui::Response {
    let rect = section_frame(ui);
    let button = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 3.0, rect.center().y - 12.0),
        egui::vec2(116.0, 24.0),
    );
    let response = ui
        .put(
            button,
            egui::Button::new(egui::RichText::new("    Back to Object").size(12.0)),
        )
        .tip("Finish editing text and return to this object's properties");
    let c = egui::pos2(button.left() + 10.0, button.center().y);
    let stroke = Stroke::new(1.5, Color32::WHITE);
    ui.painter()
        .line_segment([c + egui::vec2(4.0, 0.0), c - egui::vec2(4.0, 0.0)], stroke);
    ui.painter().add(egui::Shape::line(
        vec![
            c + egui::vec2(0.0, -4.0),
            c - egui::vec2(4.0, 0.0),
            c + egui::vec2(0.0, 4.0),
        ],
        stroke,
    ));
    ui.painter().text(
        egui::pos2(
            (button.right() + rect.right() - 27.0) * 0.5,
            rect.center().y,
        ),
        egui::Align2::CENTER_CENTER,
        "Properties",
        crate::panel_ui::font(13.),
        Color32::WHITE,
    );
    help_at(
        ui,
        rect.right_center() - egui::vec2(15.0, 0.0),
        "Properties",
        TEXT_HELP,
    );
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("text-back-to-object"), response.rect));
    response
}

pub(super) fn divider(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 9.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, Color32::from_gray(108)),
    );
}

/// Left-align titles at the pane margin. Longer titles may borrow rail space,
/// while the slider reserves the same right-hand numeric column in every row.
pub(super) fn field_label(ui: &mut egui::Ui, label: &str, height: f32) -> egui::Response {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_width = ui.fonts_mut(|f| {
        f.layout_no_wrap(label.into(), font, Color32::WHITE)
            .size()
            .x
    });
    let max_width =
        (ui.available_width() - 64.0 - 36.0 - 2.0 * ui.spacing().item_spacing.x).max(70.0);
    crate::controls::field_label(
        ui,
        label,
        [text_width.ceil().clamp(70.0, max_width), height],
    )
}

pub(super) fn slider(ui: &mut egui::Ui, label: &str, slider: egui::Slider<'_>) -> egui::Response {
    let row = ui.horizontal(|ui| {
        field_label(ui, label, 20.0);
        // Reserve a fixed numeric field before laying out the rail. Float
        // formatting must not enlarge the row (and the containing side panel).
        ui.spacing_mut().interact_size = egui::vec2(64.0, 22.0);
        ui.spacing_mut().slider_width =
            (ui.available_width() - 64.0 - ui.spacing().item_spacing.x).max(36.0);
        crate::controls::slider(ui, slider.max_decimals(1)).tip_title(label)
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("property-slider", label)),
            (row.response.rect, row.inner.rect),
        )
    });
    row.inner
}

/// A named dropdown on one row, the same size as the rows around it. Returns
/// true when the choice changed.
pub(super) fn choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
    tip: &str,
) -> bool {
    use crate::tips::Tip as _;
    let before = *value;
    ui.horizontal(|ui| {
        field_label(ui, label, 24.);
        let text = options
            .iter()
            .find(|(option, _)| option == value)
            .map_or("", |(_, name)| name);
        egui::ComboBox::from_id_salt(("property-choice", label))
            .width(ui.available_width())
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (option, name) in options {
                    ui.selectable_value(value, *option, *name);
                }
            })
            .response
            .tip_title(label)
            .tip(tip);
    });
    *value != before
}
pub(super) fn swatch(ui: &mut egui::Ui, label: &str, color: &mut [u8; 4]) {
    let preview = match label {
        "Outline" => crate::color_widgets::Preview::Outline,
        "Text" => crate::color_widgets::Preview::Text,
        "Stroke" => crate::color_widgets::Preview::Stroke,
        "Highlight" => crate::color_widgets::Preview::Highlight,
        _ => crate::color_widgets::Preview::Fill,
    };
    swatch_preview(ui, label, color, preview);
}
pub(super) fn swatch_preview(
    ui: &mut egui::Ui,
    label: &str,
    color: &mut [u8; 4],
    preview: crate::color_widgets::Preview,
) {
    ui.push_id(label, |ui| {
        ui.vertical(|ui| {
            ui.set_width(swatch_width(ui));
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(label).size(if swatch_width(ui) < 44. {
                        11.
                    } else {
                        13.
                    }))
                    .truncate(),
                );
                crate::color_widgets::property_rgba(ui, color, preview).tip_title(label);
            });
        });
    });
}
pub(super) fn swatch_width(ui: &egui::Ui) -> f32 {
    ui.ctx()
        .data(|d| d.get_temp::<f32>(egui::Id::unique("property-color-row-width")))
        .unwrap_or(56.)
}
pub(super) fn color_row(ui: &mut egui::Ui, count: usize, body: impl FnOnce(&mut egui::Ui)) {
    let key = egui::Id::unique("property-color-row-width");
    let old = ui.ctx().data(|d| d.get_temp::<f32>(key));
    let width = ((ui.available_width()
        - ui.spacing().item_spacing.x * (count.saturating_sub(1)) as f32)
        / count.max(1) as f32)
        .clamp(34., 56.);
    ui.ctx().data_mut(|d| d.insert_temp(key, width));
    ui.horizontal_top(body);
    ui.ctx().data_mut(|d| {
        d.remove::<f32>(key);
        if let Some(old) = old {
            d.insert_temp(key, old);
        }
    });
}

impl Editor {
    pub(super) fn property_tabs(&mut self, ui: &mut egui::Ui) -> [egui::Response; 2] {
        let old = self.shell.property_tab;
        let result = crate::panel_ui::rail(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                let width = ui.available_width() * 0.5;
                [
                    (PropertyTab::Tool, "Properties"),
                    (PropertyTab::Objects, "Layers"),
                ]
                .map(|(tab, label)| {
                    let r = crate::panel_ui::tab(
                        ui,
                        label,
                        label,
                        self.shell.property_tab == tab,
                        width,
                        true,
                    );
                    if r.clicked() {
                        self.shell.property_tab = tab;
                    }
                    r
                })
            })
            .inner
        });

        if old != self.shell.property_tab && !self.commit_text() {
            self.shell.property_tab = old;
        }
        divider(ui);
        result
    }
}

pub(super) fn annotation_properties(
    ui: &mut egui::Ui,
    a: &mut Annotation,
    canvas: [u32; 2],
) -> bool {
    crate::obscure_properties::show(ui, a);
    if a.kind == Kind::Image
        || (a.is_image_frame() && crate::image_frame::content_mode(ui.ctx(), a.id))
    {
        crate::image_frame::properties(ui, a);
        if a.is_shape() {
            let mut opacity = a.image_opacity();
            if slider(
                ui,
                "Image opacity",
                egui::Slider::new(&mut opacity, 0..=255),
            )
            .changed()
                && let Some(b) = &a.image
            {
                a.image = Some(std::sync::Arc::new(b.with_frame_opacity(opacity)));
            }
        } else {
            slider(ui, "Opacity", egui::Slider::new(&mut a.color[3], 0..=255));
        }
    }
    if a.kind == Kind::Eraser {
        slider(ui, "Brush width", egui::Slider::new(&mut a.width, 1..=32));
        slider(ui, "Opacity", egui::Slider::new(&mut a.color[3], 0..=255));
    }
    if !a.is_image_frame() || !crate::image_frame::content_mode(ui.ctx(), a.id) {
        crate::shape_properties::show(ui, a);
    }
    if matches!(
        a.kind,
        Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlighter
    ) {
        ui.horizontal(|ui| swatch(ui, "Color", &mut a.color));
        divider(ui);
        if !a.highlighter_rect {
            slider(ui, "Width", egui::Slider::new(&mut a.width, 1..=32));
        }
        slider(ui, "Opacity", egui::Slider::new(&mut a.color[3], 0..=255));
    }
    if let Some(style) = &mut a.callout {
        color_row(ui, 5, |ui| {
            swatch(ui, "Fill", &mut style.fill);
            swatch(ui, "Outline", &mut style.outline);
            crate::text_properties::color_swatch(ui, &mut a.text, a.color);
        });
        divider(ui);
        slider(ui, "Outline size", egui::Slider::new(&mut a.width, 1..=32));
        crate::line_properties::outline(ui, a);
    }
    if a.kind == Kind::Step {
        let mut outline = a.step_outline.clone().unwrap_or_default();
        let before_color = outline.color;
        color_row(ui, 5, |ui| {
            swatch(ui, "Fill", &mut a.color);
            swatch(ui, "Outline", &mut outline.color);
            let foreground = capture_core::steps::foreground(a).color;
            crate::text_properties::color_swatch(ui, &mut a.text, foreground);
        });
        divider(ui);
        let mut enabled = a.step_outline.is_some();
        if outline.color != before_color {
            enabled = true;
        }
        crate::controls::toggle(ui, &mut enabled, "Show outline");
        if enabled {
            slider(
                ui,
                "Outline size",
                egui::Slider::new(&mut outline.width, 0.0..=32.0),
            );
            a.step_outline = Some(outline);
        } else {
            a.step_outline = None;
        }
        let (min, max) = a.body_bounds();
        let mut diameter = max[0] - min[0];
        if slider(
            ui,
            "Marker size",
            egui::Slider::new(&mut diameter, 1.0..=16384.0).logarithmic(true),
        )
        .changed()
            && let Ok(resized) = capture_core::steps::resized_marker(a, diameter, canvas)
        {
            *a = resized;
        }
        if let Some(style) = &mut a.text {
            ui.horizontal(|ui| {
                let label = ui.label("Step label");
                ui.add_sized(
                    [90., 28.],
                    crate::unicode_input::TextEdit::singleline(&mut style.content)
                        .align(egui::Align2::CENTER_CENTER)
                        .desired_width(90.0)
                        .char_limit(8),
                )
                .labelled_by(label.id);
            });
        }
    } else if !a.is_image_frame() {
        crate::text_properties::show(ui, a);
    }
    if a.kind != Kind::Text && !a.is_image_frame() {
        crate::text_properties::outline_controls(ui, a);
    }
    if matches!(a.kind, Kind::Rectangle | Kind::Callout) {
        let max = a.max_corner_radius();
        slider(
            ui,
            "Corners",
            egui::Slider::new(&mut a.corner_radius, 0.0..=max),
        );
    }
    crate::line_properties::show(ui, a);
    crate::shadow_properties::show(ui, a);
    let requested = rotation_controls(ui, a).is_some_and(|r| r[0].changed() || r[1].clicked());
    if a.kind != Kind::Step && !a.is_image_frame() {
        crate::text_properties::advanced(ui, a);
    }
    requested
}

/// Numeric rotation shares the selected-property transaction and batch path.
pub(super) fn rotation_controls(
    ui: &mut egui::Ui,
    a: &mut Annotation,
) -> Option<[egui::Response; 2]> {
    if !a.can_rotate() {
        return None;
    }
    Some(
        ui.horizontal(|ui| {
            let label = field_label(ui, "Rotation", 20.0);
            let mut degrees = a.rotation_degrees;
            let value = ui
                .add(
                    egui::DragValue::new(&mut degrees)
                        .range(-180.0..=180.0)
                        .speed(1.0)
                        .fixed_decimals(1)
                        .suffix("°"),
                )
                .labelled_by(label.id)
                .tip("Clockwise rotation in degrees; drag or click to type");
            let reset = ui
                .add(egui::Button::new("Reset"))
                .tip("Reset rotation to 0°");
            if reset.clicked() {
                degrees = 0.0;
            }
            if (value.changed() || reset.clicked())
                && let Ok(rotated) = a.with_rotation(degrees)
            {
                *a = rotated;
            }
            [value, reset]
        })
        .inner,
    )
}

#[cfg(test)]
pub(crate) mod tests;
