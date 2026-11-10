//! Properties for objects of different kinds selected together (approved mockup
//! `artifacts/bo-proposal/mixed-selection.html`): what they share, with a note
//! naming the objects a setting leaves out.
use crate::tips::Tip as _;
use crate::video_editor::property_design as design;
use crate::{Annotation, Editor, Kind, egui, property_layout as layout};
use capture_core::multi_selection as shared;

/// The name of an object's kind, as the selection chips and notes show it.
fn kind_name(a: &Annotation) -> &'static str {
    if a.focus.is_some_and(|f| f.magnify.is_some()) {
        return "Magnify";
    }
    if a.focus.is_some() {
        return "Focus";
    }
    if a.image.as_ref().is_some_and(|b| b.is_icon()) {
        return "Icon";
    }
    match a.kind {
        Kind::Step => "Step",
        Kind::Line => "Line",
        Kind::Arrow => "Arrow",
        Kind::Pen => "Pen",
        Kind::Highlighter => "Highlighter",
        Kind::Rectangle => "Rectangle",
        Kind::Ellipse => "Ellipse",
        Kind::Polygon => "Polygon",
        Kind::Text => "Text",
        Kind::Callout => "Callout",
        Kind::Image => "Image",
        Kind::Redact => "Redaction",
        Kind::Blur => "Blur",
        Kind::Pixelate => "Pixelate",
        Kind::Eraser => "Eraser",
    }
}
/// Each kind's name once, in selection order.
fn names(objects: impl Iterator<Item = impl std::borrow::Borrow<Annotation>>) -> Vec<&'static str> {
    let mut out = vec![];
    for a in objects {
        let name = kind_name(a.borrow());
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}
fn listed(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
/// Shown only when at least two kinds in the selection can use the setting.
fn shown(objects: &[Annotation], can: fn(&Annotation) -> bool) -> bool {
    names(objects.iter().filter(|a| can(a))).len() >= 2
}
/// "Applies to Step and Rectangle. Icon and Text are left out."
fn note(ui: &mut egui::Ui, objects: &[Annotation], can: &[fn(&Annotation) -> bool]) {
    let usable = |a: &Annotation| can.iter().any(|c| c(a));
    let left: Vec<_> = names(objects.iter().filter(|a| !usable(a)));
    if left.is_empty() {
        return;
    }
    let applies = names(objects.iter().filter(|a| usable(a)));
    let verb = if left.len() == 1 { "is" } else { "are" };
    ui.small(format!(
        "Applies to {}. {} {verb} left out.",
        listed(&applies),
        listed(&left)
    ));
}
fn common<T: PartialEq + Copy>(values: impl Iterator<Item = T>) -> (Option<T>, bool) {
    let mut first = None;
    let mut mixed = false;
    for v in values {
        match first {
            None => first = Some(v),
            Some(f) if f != v => mixed = true,
            _ => {}
        }
    }
    (first, mixed)
}

impl Editor {
    /// Several objects selected, not all of one kind.
    pub(crate) fn mixed_selection(&self) -> bool {
        self.selected_objects().len() > 1 && !self.selection_same_kind()
    }
    /// One chip per kind under the selection's title.
    pub(crate) fn mixed_chips(&self, ui: &mut egui::Ui) {
        if !self.mixed_selection() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
            for name in names(self.selected_objects().iter()) {
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(36, 43, 46))
                    .corner_radius(99)
                    .inner_margin(egui::Margin::symmetric(9, 1))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .size(12.)
                                .color(crate::panel_ui::MUTED),
                        );
                    });
            }
        });
    }
    fn mixed_objects(&self) -> Vec<Annotation> {
        self.held_batch().unwrap_or_else(|| self.selected_objects())
    }
    /// A live edit of every selected object, measured from the unchanged document.
    pub(crate) fn apply_mixed(&mut self, edit: impl Fn(&mut Annotation)) {
        let canvas = [self.session.document.width, self.session.document.height];
        let batch: capture_core::Result<Vec<_>> = self
            .selected_objects()
            .into_iter()
            .map(|mut a| {
                edit(&mut a);
                a.fit_text_shape(canvas)
            })
            .collect();
        match batch {
            Ok(batch) => self.preview_property_batch(batch),
            Err(error) => self.status = error.to_string(),
        }
    }
    pub(crate) fn mixed_style_groups(&mut self, ui: &mut egui::Ui) {
        let objects = self.mixed_objects();
        design::heading(ui, "Style", "Appearance");
        self.mixed_appearance(ui, &objects);
        design::end(ui);
        if shown(&objects, shared::can_fill) || shown(&objects, shared::can_outline) {
            design::heading(ui, "Outline", "Fill and outline");
            self.mixed_fill_and_outline(ui, &objects);
            design::end(ui);
        }
        if shown(&objects, shared::can_hold_text) {
            design::heading(ui, "Text", "Text");
            self.show_font_controls(ui, true);
            self.show_justification_grid(ui);
            note(ui, &objects, &[shared::can_hold_text]);
            design::end(ui);
        }
        design::heading(ui, "Position", "Position");
        self.mixed_position(ui, &objects);
        design::end(ui);
    }
    fn mixed_appearance(&mut self, ui: &mut egui::Ui, objects: &[Annotation]) {
        let (first, mixed) = common(
            objects
                .iter()
                .filter(|a| shared::can_fade(a))
                .map(shared::opacity),
        );
        if let Some(first) = first {
            let mut percent = first as f32 / 255. * 100.;
            let mut slider = egui::Slider::new(&mut percent, 0.0..=100.0)
                .suffix("%")
                .fixed_decimals(0);
            slider = if mixed {
                slider.custom_formatter(|_, _| "Mixed".into())
            } else {
                slider.custom_formatter(|v, _| format!("{v:.0}"))
            };
            if layout::slider(ui, "Opacity", slider)
                .tip("How see-through the selected objects are.")
                .changed()
            {
                let value = (percent * 2.55).round() as u8;
                self.apply_mixed(move |a| shared::set_opacity(a, value));
            }
        }
        // The shared control edits a stand-in; only a real change reaches the objects.
        if let Some(sample) = objects.iter().find(|a| shared::can_shadow(a)) {
            let mut proxy = sample.clone();
            crate::shadow_properties::show(ui, &mut proxy);
            if proxy.shadow != sample.shadow {
                let shadow = proxy.shadow;
                self.apply_mixed(move |a| {
                    if shared::can_shadow(a) {
                        a.shadow = shadow;
                    }
                });
            }
        }
    }
    fn mixed_fill_and_outline(&mut self, ui: &mut egui::Ui, objects: &[Annotation]) {
        let fills = objects.iter().filter(|a| shared::can_fill(a));
        let (fill, fill_mixed) = common(fills.map(shared::fill));
        let fill = fill.filter(|_| shown(objects, shared::can_fill));
        let lines = || objects.iter().filter(|a| shared::can_outline(a));
        let (outline, outline_mixed) = common(lines().map(shared::outline));
        let outline = outline.filter(|_| shown(objects, shared::can_outline));
        let (mut new_fill, mut new_outline) = (fill, outline);
        layout::color_row(ui, 2, |ui| {
            if let Some(color) = new_fill.as_mut() {
                layout::swatch(ui, "Fill", color);
            }
            if let Some(color) = new_outline.as_mut() {
                layout::swatch(ui, "Outline", color);
            }
        });
        match (
            fill.is_some() && fill_mixed,
            outline.is_some() && outline_mixed,
        ) {
            (true, true) => ui.small("Fill and Outline are mixed."),
            (true, false) => ui.small("Fill is mixed."),
            (false, true) => ui.small("Outline is mixed."),
            (false, false) => ui.response(),
        };
        if new_fill != fill
            && let Some(color) = new_fill
        {
            self.apply_mixed(move |a| shared::set_fill(a, color));
        }
        if new_outline != outline
            && let Some(color) = new_outline
        {
            self.apply_mixed(move |a| shared::set_outline(a, color));
        }
        if shown(objects, shared::can_outline) {
            if let (Some(first), mixed) = common(lines().map(shared::thickness)) {
                let mut value = first;
                let mut slider = egui::Slider::new(&mut value, 0.0..=32.0);
                if mixed {
                    slider = slider.custom_formatter(|_, _| "Mixed".into());
                }
                if layout::slider(ui, "Thickness", slider)
                    .tip("Width of the outline in pixels.")
                    .changed()
                {
                    self.apply_mixed(move |a| shared::set_thickness(a, value));
                }
            }
            if let Some(sample) = lines().next() {
                let mut proxy = sample.clone();
                crate::line_properties::outline(ui, &mut proxy);
                if proxy.line_style != sample.line_style {
                    let pattern = proxy
                        .line_style
                        .map_or(capture_core::line_style::Pattern::Solid, |s| s.pattern);
                    self.apply_mixed(move |a| {
                        if shared::can_outline(a) {
                            a.line_style = crate::line_properties::with_pattern(a, pattern);
                        }
                    });
                }
            }
        }
        note(ui, objects, &[shared::can_fill, shared::can_outline]);
    }
    fn mixed_position(&mut self, ui: &mut egui::Ui, objects: &[Annotation]) {
        if let Some(sample) = objects.iter().find(|a| a.can_rotate()) {
            let mut proxy = sample.clone();
            let changed = layout::rotation_controls(ui, &mut proxy)
                .is_some_and(|r| r[0].changed() || r[1].clicked());
            if changed {
                let degrees = proxy.rotation_degrees;
                self.apply_mixed(move |a| {
                    if a.can_rotate() {
                        a.rotation_degrees = degrees;
                    }
                });
            }
        }
        self.show_layout_grid(ui, false);
    }
}
