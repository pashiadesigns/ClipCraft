use crate::{Annotation, Editor, egui};
use capture_core::text::TextFit;

#[cfg(test)]
mod tests;

impl Editor {
    /// Text's dedicated sidebar uses the same actions as the ribbon, including
    /// fitted font sizing, batch edits and the active inline-text transaction.
    pub(super) fn dedicated_text_properties(&mut self, ui: &mut egui::Ui) {
        let Some(mut edited) = self.selected_annotation() else {
            return;
        };
        let before = edited.clone();
        if edited.kind == crate::Kind::Text {
            show(ui, &mut edited, self.shell.badges);
        } else {
            let base = if edited.kind == crate::Kind::Step {
                capture_core::steps::foreground(&edited).color
            } else {
                edited.color
            };
            ui.horizontal(|ui| color_swatch(ui, &mut edited.text, base));
            crate::property_layout::divider(ui);
            outline_controls(ui, &mut edited);
        }
        if edited != before {
            self.update_text_sidebar(edited, false);
        }
        ui.push_id("text_sidebar_font", |ui| self.show_font_controls(ui, true));
        ui.horizontal(|ui| {
            ui.label("Justification");
            ui.push_id("text_sidebar_justification", |ui| {
                self.show_justification_grid(ui)
            });
        });
        // Refresh after a font action; never overwrite it with a stale copy.
        let Some(mut edited) = self.selected_annotation() else {
            return;
        };
        let before = edited.clone();
        let rotation_requested = if self.text_draft.is_none() {
            crate::property_layout::rotation_controls(ui, &mut edited)
                .is_some_and(|r| r[0].changed() || r[1].clicked())
        } else {
            false
        };
        crate::shadow_properties::show(ui, &mut edited);
        advanced(ui, &mut edited, self.shell.badges);
        if edited != before || rotation_requested {
            self.update_text_sidebar(edited, rotation_requested);
        }
    }

    fn update_text_sidebar(&mut self, edited: Annotation, rotation_requested: bool) {
        if let Some(draft) = &mut self.text_draft {
            match edited.fit_text_shape([self.session.document.width, self.session.document.height])
            {
                Ok(edited) => {
                    draft.preview = edited;
                    self.needs_render = true;
                }
                Err(error) => self.status = error.to_string(),
            }
        } else {
            self.preview_property_edit_with_rotation(edited, rotation_requested);
        }
    }
}

/// Edit the selected annotation through the caller's shared Update command.
/// Disclosure state belongs to egui, so opening it never changes the document.
pub(super) fn show(ui: &mut egui::Ui, annotation: &mut Annotation, _badges: bool) {
    let Some(style) = &mut annotation.text else {
        return;
    };
    let mut text_color = style.color.unwrap_or(annotation.color);
    if annotation.kind == crate::Kind::Text {
        ui.horizontal(|ui| {
            crate::property_layout::swatch(ui, "Text", &mut text_color);
            outline_swatch(ui, style, "Stroke");
        });
        crate::property_layout::divider(ui);
    }
    if text_color != style.color.unwrap_or(annotation.color) {
        if style.color.is_some()
            || matches!(
                annotation.kind,
                crate::Kind::Rectangle | crate::Kind::Ellipse | crate::Kind::Polygon
            )
        {
            style.color = Some(text_color);
        } else {
            annotation.color = text_color;
        }
    }
    if annotation.kind == crate::Kind::Text {
        outline_controls(ui, annotation);
    }
}

/// Shared first-row label color. Preserve independent text fill once edited.
pub(super) fn color_swatch(
    ui: &mut egui::Ui,
    text: &mut Option<capture_core::text::TextStyle>,
    base: [u8; 4],
) {
    if let Some(style) = text {
        let initial = style.color.unwrap_or(base);
        let mut color = initial;
        crate::property_layout::swatch(ui, "Text", &mut color);
        if color != initial {
            style.color = Some(color);
        }
        outline_swatch(ui, style, "Stroke");
    }
}

fn outline_swatch(ui: &mut egui::Ui, style: &mut capture_core::text::TextStyle, label: &str) {
    let mut outline = style.outline.unwrap_or_default();
    let before = outline.color;
    ui.scope(|ui| crate::property_layout::swatch(ui, label, &mut outline.color))
        .response
        .on_hover_text("Text outline color");
    if outline.color != before {
        style.outline = Some(outline);
    }
}
pub(super) fn outline_controls(ui: &mut egui::Ui, annotation: &mut Annotation) {
    let Some(style) = &mut annotation.text else {
        return;
    };
    let mut enabled = style.outline.is_some();
    let toggle = crate::controls::toggle(ui, &mut enabled, "Text Outline");
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("text-outline-toggle"), toggle.rect));
    if toggle.changed() {
        style.outline = enabled.then(|| style.outline.unwrap_or_default());
    }
    if let Some(outline) = &mut style.outline {
        let slider = crate::property_layout::slider(
            ui,
            "Text outline size",
            egui::Slider::new(&mut outline.width, 0.0..=32.0),
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("text-outline-size"), slider.rect));
        let _ = slider;
    }
}

pub(super) fn advanced(ui: &mut egui::Ui, annotation: &mut Annotation, badges: bool) {
    let Some(style) = &mut annotation.text else {
        return;
    };
    let mut text_color = style.color.unwrap_or(annotation.color);
    crate::property_layout::divider(ui);
    egui::CollapsingHeader::new("Advanced")
        .id_salt("selected_text_advanced")
        .show_unindented(ui, |ui| {
            crate::shell::badge(ui, badges, "EDIT-012");
            ui.label("Text fitting");
            egui::ComboBox::from_id_salt("selected_text_fit")
                .width(206.0)
                .selected_text(style.fit.label())
                .show_ui(ui, |ui| {
                    for fit in [TextFit::None, TextFit::Shrink, TextFit::ResizeShape] {
                        ui.selectable_value(&mut style.fit, fit, fit.label());
                    }
                });
            crate::controls::toggle(ui, &mut style.wrap, "Wrap text in shape");
            if let Some(callout) = &mut annotation.callout {
                crate::property_layout::slider(
                    ui,
                    "Padding",
                    egui::Slider::new(&mut callout.padding, 0.0..=128.0),
                );
            } else {
                crate::property_layout::slider(
                    ui,
                    "Padding",
                    egui::Slider::new(&mut style.padding, 0.0..=128.0),
                );
            }
            // Convert only after an edit; simply opening Advanced must not
            // quantize an existing RGBA alpha value or dirty the document.
            let mut opacity = f32::from(text_color[3]) * 100.0 / 255.0;
            let opacity_changed = crate::property_layout::slider(
                ui,
                "Text opacity",
                egui::Slider::new(&mut opacity, 0.0..=100.0)
                    .suffix("%")
                    .fixed_decimals(0),
            )
            .changed();
            if opacity_changed {
                text_color[3] = (opacity * 255.0 / 100.0).round() as u8;
            }
        });

    if text_color != style.color.unwrap_or(annotation.color) {
        style.color = Some(text_color);
    }
}
