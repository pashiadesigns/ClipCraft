use crate::tips::Tip as _;
use crate::{Annotation, Editor, Kind, egui};
use capture_core::focus::Focus;

#[derive(Clone)]
pub(super) struct State {
    pub active: bool,
    pub shape: u8,
    pub radius: f32,
    pub effect: Focus,
    pub color: [u8; 4],
    pub width: u32,
    /// Outline pattern (solid, dashed, dotted, wavy, …); presets keep it (AX4).
    pub line_style: Option<capture_core::line_style::Style>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            active: false,
            shape: 1,
            radius: 20.,
            effect: Default::default(),
            color: [24, 145, 193, 255],
            width: 2,
            line_style: None,
        }
    }
}
impl State {
    pub fn remember(&mut self, a: &Annotation) {
        let Some(effect) = a.focus else { return };
        self.effect = effect;
        self.shape = if a.kind == Kind::Ellipse {
            2
        } else if a.corner_radius > 0. {
            1
        } else {
            0
        };
        self.radius = a.corner_radius;
        self.color = a.color;
        self.width = a.width;
        self.line_style = a.line_style;
    }
    pub fn apply(&self, a: &mut Annotation) {
        a.focus = Some(self.effect);
        a.kind = if self.shape == 2 {
            Kind::Ellipse
        } else {
            Kind::Rectangle
        };
        a.corner_radius = if self.shape == 1 { self.radius } else { 0. };
        if self.shape == 2 {
            let side = (a.end[0] - a.start[0])
                .abs()
                .min((a.end[1] - a.start[1]).abs());
            a.end = [
                a.start[0] + side * (a.end[0] - a.start[0]).signum(),
                a.start[1] + side * (a.end[1] - a.start[1]).signum(),
            ];
        }
        a.fill = None;
        a.text = None;
        a.image = None;
        a.shadow = None;
        a.color = self.color;
        a.width = self.width;
        a.line_style = self.line_style;
        a.points.clear();
    }
}
impl Editor {
    pub(super) fn focus_properties(&mut self, ui: &mut egui::Ui, selected: Option<Annotation>) {
        use crate::video_editor::property_design as design;
        let original = selected.clone();
        let mut state = self.shell.focus.clone();
        if let Some(a) = &selected {
            state.effect = a.focus.unwrap_or_default();
            state.shape = if a.kind == Kind::Ellipse {
                2
            } else if a.corner_radius > 0. {
                1
            } else {
                0
            };
            state.color = a.color;
            state.width = a.width;
            state.radius = a.corner_radius;
            state.line_style = a.line_style;
        }
        self.presets_header(ui, self.tool);
        let mut sample = selected
            .clone()
            .unwrap_or_else(|| self.new_annotation(Kind::Rectangle, [0., 0.], [80., 60.]));
        state.apply(&mut sample);
        let saved: Vec<_> = self
            .presets
            .templates
            .as_ref()
            .map(|s| {
                s.entries()
                    .iter()
                    .filter(|s| {
                        s.focus.is_some()
                            && s.focus.and_then(|f| f.magnify).is_some()
                                == state.effect.magnify.is_some()
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            for (name, blur, darken) in [
                ("Clear", 0., 0.),
                ("Darken", 0., 0.6),
                ("Blur", 8., 0.),
                ("Blur + darken", 8., 0.5),
            ] {
                let active = state.effect.blur == blur && state.effect.darken == darken;
                let mut a = sample.clone();
                a.focus = Some(Focus {
                    blur,
                    darken,
                    magnify: state.effect.magnify,
                    ..Default::default()
                });
                let preset =
                    capture_core::object_styles::ObjectStyle::from_annotation(name.into(), &a)
                        .unwrap();
                if crate::style_tile::tile(ui, &preset, name, active).clicked() {
                    state.effect.blur = blur;
                    state.effect.darken = darken;
                }
            }
            for preset in &saved {
                if crate::style_tile::tile(
                    ui,
                    preset,
                    &preset.name,
                    preset.matches(std::slice::from_ref(&sample)),
                )
                .clicked()
                {
                    chosen = Some(preset.clone());
                }
            }
        });
        if let Some(preset) = chosen
            && let Ok(a) = preset.apply_compatible(&sample)
        {
            state.effect = a.focus.unwrap();
            state.shape = if a.kind == Kind::Ellipse {
                2
            } else if a.corner_radius > 0. {
                1
            } else {
                0
            };
            state.color = a.color;
            state.width = a.width;
            state.radius = a.corner_radius;
            state.line_style = a.line_style;
        }
        design::end(ui);
        let magnified = state.effect.magnify.is_some();
        design::heading(
            ui,
            if magnified { "Magnify" } else { "Focus" },
            if magnified { "Magnify" } else { "Focus window" },
        );
        if let Some(m) = state.effect.magnify.as_mut() {
            crate::property_layout::slider(
                ui,
                "Magnification",
                egui::Slider::new(&mut m.scale, 1. ..=8.).suffix("×"),
            );
            crate::controls::toggle(ui, &mut m.connectors, "Connector lines");
            crate::property_layout::slider(
                ui,
                "Horizontal offset",
                egui::Slider::new(&mut m.offset[0], -10. ..=10.),
            );
            crate::property_layout::slider(
                ui,
                "Vertical offset",
                egui::Slider::new(&mut m.offset[1], -10. ..=10.),
            );
            ui.weak("Drag the enlarged view's center to move it, or its corner to resize it.");
            crate::property_layout::divider(ui);
        }
        let shape_before = state.shape;
        // Shape buttons show their shape before the name (AV1), in image and video.
        ui.horizontal(|ui| {
            let gap = ui.spacing().item_spacing.x;
            let names = [
                ("Shape rectangle", "Rectangle"),
                ("Shape rounded", "Rounded"),
                ("Shape circle", "Circle"),
            ];
            let w = ((ui.available_width() - gap * 2.) / 3.).max(60.);
            for (i, (icon, name)) in names.iter().enumerate() {
                let active = state.shape == i as u8;
                if crate::video_editor::ui::button(ui, icon, name, w, active)
                    .tip(match i {
                        0 => "Rectangle window",
                        1 => "Rounded rectangle window; set the corner radius below",
                        _ => "Circular window",
                    })
                    .clicked()
                {
                    state.shape = i as u8;
                }
            }
        });
        // A square-cornered window has no radius to keep; choosing Rounded
        // must visibly round it, or it reads back as a plain rectangle.
        if state.shape == 1 && shape_before != 1 && state.radius <= 0. {
            let side = selected.as_ref().map_or(80., |a| {
                let (lo, hi) = a.body_bounds();
                (hi[0] - lo[0]).abs().min((hi[1] - lo[1]).abs())
            });
            state.radius = (side * 0.2).clamp(4., 40.);
        }
        ui.weak("The center stays clear. Only layers below this object are affected.");
        if state.shape == 1 {
            crate::property_layout::slider(
                ui,
                "Corner radius",
                egui::Slider::new(&mut state.radius, 0. ..=200.),
            );
        }
        crate::property_layout::slider(
            ui,
            "Darken outside",
            egui::Slider::new(&mut state.effect.darken, 0. ..=1.)
                .custom_formatter(|n, _| format!("{:.0}%", n * 100.)),
        );
        crate::property_layout::slider(
            ui,
            "Blur outside",
            egui::Slider::new(&mut state.effect.blur, 0. ..=32.),
        );
        crate::property_layout::divider(ui);
        ui.horizontal(|ui| {
            crate::property_layout::swatch_preview(
                ui,
                "Stroke",
                &mut state.color,
                crate::color_widgets::Preview::Outline,
            );
        });
        crate::property_layout::slider(
            ui,
            "Stroke width",
            egui::Slider::new(&mut state.width, 0..=32),
        );
        let mut updated = selected;
        if let Some(a) = updated.as_mut() {
            // Preserve geometry during ordinary parameter edits; shape changes
            // alone reshape the transparent window.
            let old_kind = a.kind;
            let old_radius = a.corner_radius;
            let end = a.end;
            state.apply(a);
            if a.kind == old_kind && a.corner_radius == old_radius {
                a.end = end;
            }
            crate::line_properties::outline(ui, a);
            state.line_style = a.line_style;
        }
        design::end(ui);
        state.active = self.shell.focus.active;
        self.shell.focus = state;
        if let (Some(before), Some(after)) = (original, updated)
            && before != after
        {
            self.execute(capture_core::Command::Update { annotation: after });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_state_keeps_the_outline_pattern() {
        for pattern in [
            capture_core::line_style::Pattern::Dashed,
            capture_core::line_style::Pattern::Dotted,
            capture_core::line_style::Pattern::Wavy,
        ] {
            let mut source =
                capture_core::steps::marker(1, [60., 60.], 40, [0; 4], (1, false), [320, 180])
                    .unwrap();
            source.kind = Kind::Rectangle;
            source.text = None;
            source.focus = Some(Focus {
                magnify: Some(Default::default()),
                ..Default::default()
            });
            source.line_style = Some(capture_core::line_style::Style {
                pattern,
                ..capture_core::line_style::Style::for_kind(Kind::Rectangle)
            });
            let mut state = State::default();
            state.remember(&source);
            let mut next = source.clone();
            next.line_style = None;
            state.apply(&mut next);
            assert_eq!(next.line_style.map(|s| s.pattern), Some(pattern));
        }
    }
}
