use crate::tips::Tip as _;
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
            show(ui, &mut edited);
        } else {
            let base = if edited.kind == crate::Kind::Step {
                capture_core::steps::foreground(&edited).color
            } else {
                edited.color
            };
            crate::property_layout::color_row(ui, 3, |ui| color_swatch(ui, &mut edited.text, base));
            crate::property_layout::divider(ui);
            outline_controls(ui, &mut edited);
        }
        if edited != before {
            self.update_text_sidebar(edited, false);
        }
        ui.push_id("text_sidebar_font", |ui| self.show_font_controls(ui, true));
        ui.horizontal(|ui| self.rich_text_buttons(ui));
        self.caret_formatting_toggle(ui);
        self.common_paragraph_controls(ui);
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
        advanced(ui, &mut edited);
        if edited != before || rotation_requested {
            self.update_text_sidebar(edited, rotation_requested);
        }
    }

    pub(super) fn caret_formatting_toggle(&mut self, ui: &mut egui::Ui) {
        if crate::controls::toggle(ui, &mut self.rich_text.caret_formatting, "Format typing at caret")
            .tip("Off: format the whole block unless characters are highlighted. On: a caret with no selection formats newly typed text.")
            .changed()
        {
            if let Some(draft) = &mut self.text_draft {
                draft.typing = None;
            }
            self.needs_render = true;
        }
    }

    /// While typing with text selected (or caret formatting), color edits go to
    /// that selection, not the whole object. The swatches then have to show
    /// the selection's format, or they appear to snap back (BI2). Returns the
    /// annotation as the swatches should show it, or `None` to show it as is.
    pub(super) fn selection_view(&self, a: &Annotation) -> Option<Annotation> {
        use capture_core::text::rich;
        let draft = self.text_draft.as_ref()?;
        if draft.preview.id != a.id
            || !draft.selection_active
            || (draft.selection.is_empty() && !self.rich_text.caret_formatting)
        {
            return None;
        }
        let text = a.text.as_ref()?;
        let format = if draft.selection.is_empty() {
            draft
                .typing
                .clone()
                .unwrap_or_else(|| rich::format_at(text, draft.selection.start.saturating_sub(1)))
        } else {
            rich::format_at(text, draft.selection.start)
        };
        let mut shown = a.clone();
        format.apply(shown.text.as_mut()?);
        Some(shown)
    }
    /// The Appearance group for `a`. While a text selection is being formatted
    /// it shows (and edits) that selection's format; the edited annotation is
    /// returned for [`Self::update_text_sidebar`] and `a` is left alone.
    pub(crate) fn appearance_for_selection(
        &self,
        ui: &mut egui::Ui,
        a: &mut Annotation,
    ) -> Option<(Annotation, Annotation)> {
        let Some(mut shown) = self.selection_view(a) else {
            crate::appearance::show(ui, a);
            return None;
        };
        let before = shown.clone();
        crate::appearance::show(ui, &mut shown);
        (shown != before).then_some((before, shown))
    }
    pub(super) fn update_text_sidebar(&mut self, edited: Annotation, rotation_requested: bool) {
        self.update_text_sidebar_seen(edited, rotation_requested, None);
    }
    /// Like [`Self::update_text_sidebar`]; `seen` is the annotation as the
    /// controls showed it (a selection's format, see [`Self::selection_view`]),
    /// so only the fields the user actually changed reach the selection.
    pub(super) fn update_text_sidebar_seen(
        &mut self,
        mut edited: Annotation,
        rotation_requested: bool,
        seen: Option<Annotation>,
    ) {
        if let Some(before) = self.selected_annotation()
            && let (Some(old), Some(new)) = (&before.text, &mut edited.text)
        {
            use capture_core::text::rich::{self, Format};
            let old_format = seen
                .as_ref()
                .and_then(|s| s.text.as_ref())
                .map_or_else(|| Format::from_style(old), Format::from_style);
            let object_format = Format::from_style(old);
            let mut new_format = Format::from_style(new);
            if before.color != edited.color && edited.kind == crate::Kind::Text {
                new_format.color = Some(edited.color);
            }
            if old_format != new_format {
                if let Some(draft) = &mut self.text_draft
                    && draft.selection_active
                    && (!draft.selection.is_empty() || self.rich_text.caret_formatting)
                {
                    object_format.apply(new);
                    edited.color = before.color;
                    if draft.selection.is_empty() {
                        let mut format = draft.typing.clone().unwrap_or_else(|| {
                            rich::format_at(old, draft.selection.start.saturating_sub(1))
                        });
                        format.changed_fields(&old_format, &new_format);
                        draft.typing = Some(format);
                    } else {
                        rich::format_range(new, draft.selection.clone(), |f| {
                            f.changed_fields(&old_format, &new_format)
                        });
                    }
                } else if let Some(rich) = &mut new.rich {
                    for run in &mut rich.runs {
                        run.format.changed_fields(&old_format, &new_format);
                    }
                }
            }
        }
        if let Some(draft) = &mut self.text_draft {
            match edited.fit_text_shape([self.session.document.width, self.session.document.height])
            {
                Ok(edited) => {
                    draft.preview = edited;
                    self.needs_render = true;
                }
                Err(error) => self.fail(error),
            }
        } else {
            self.preview_property_edit_with_rotation(edited, rotation_requested);
        }
    }
}

/// Edit the selected annotation through the caller's shared Update command.
/// Disclosure state belongs to egui, so opening it never changes the document.
pub(super) fn show(ui: &mut egui::Ui, annotation: &mut Annotation) {
    let Some(style) = &mut annotation.text else {
        return;
    };
    let mut text_color = style.color.unwrap_or(annotation.color);
    if annotation.kind == crate::Kind::Text {
        crate::property_layout::color_row(ui, 3, |ui| {
            crate::property_layout::swatch(ui, "Text", &mut text_color);
            outline_swatch(ui, style, "Stroke");
            highlight_swatch(ui, style);
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
        highlight_swatch(ui, style);
    }
}

pub(super) fn highlight_swatch(ui: &mut egui::Ui, style: &mut capture_core::text::TextStyle) {
    let mut color = style.typography().highlight.unwrap_or([255, 235, 0, 0]);
    let before = color;
    crate::property_layout::swatch(ui, "Highlight", &mut color);
    if color != before {
        style
            .typography
            .get_or_insert_with(Default::default)
            .highlight = (color[3] > 0).then_some(color);
    }
}

pub(super) fn outline_swatch(
    ui: &mut egui::Ui,
    style: &mut capture_core::text::TextStyle,
    label: &str,
) {
    let mut outline = style.outline.unwrap_or_default();
    let before = outline.color;
    ui.scope(|ui| crate::property_layout::swatch(ui, label, &mut outline.color))
        .response
        .tip("Text stroke color");
    if outline.color != before {
        style.outline = Some(outline);
    }
}
pub(super) fn outline_controls(ui: &mut egui::Ui, annotation: &mut Annotation) {
    let Some(style) = &mut annotation.text else {
        return;
    };
    let mut outline = style.outline.unwrap_or_default();
    if style.outline.is_none() {
        outline.width = 0.;
    }
    let slider = crate::property_layout::slider(
        ui,
        "Text stroke size",
        egui::Slider::new(&mut outline.width, 0.0..=32.0),
    );
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("text-outline-size"), slider.rect));
    if slider.changed() {
        style.outline = (outline.width > 0.).then_some(outline);
    }
}

pub(super) fn advanced(ui: &mut egui::Ui, annotation: &mut Annotation) {
    let Some(style) = &mut annotation.text else {
        return;
    };
    crate::property_layout::divider(ui);
    ui.scope(|ui| {
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
        // Text opacity is set with the alpha of the Text color (properties audit).
    });
}
