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
                    .filter(|s| s.focus.is_some())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut chosen = None;
        ui.horizontal_wrapped(|ui| {
            for (name, blur, darken) in [
                ("Darken", 0., 0.6),
                ("Blur", 8., 0.),
                ("Blur + darken", 8., 0.5),
            ] {
                let active = state.effect.blur == blur && state.effect.darken == darken;
                let mut a = sample.clone();
                a.focus = Some(Focus {
                    blur,
                    darken,
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
        }
        design::end(ui);
        design::heading(ui, "Focus", "Focus window");
        ui.horizontal_wrapped(|ui| {
            for (i, name) in ["Rectangle", "Rounded rectangle", "Circle"]
                .iter()
                .enumerate()
            {
                ui.selectable_value(&mut state.shape, i as u8, *name);
            }
        });
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
