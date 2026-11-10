//! Filters, color, border and edge controls (FX-001, FX-002, approved mockup
//! `artifacts/br-proposal/roadmap-ui.html`). One set of controls serves the
//! Image tab and video clips.
use crate::tips::Tip as _;
use crate::video_editor::property_design as design;
use crate::{egui, property_layout as layout};
use capture_core::picture_finish::{ColorAdjust, EdgeStyle, Finish};

/// Where the groups sit, which decides how their headings look.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Pane,
}
fn begin(ui: &mut egui::Ui, place: Place, title: &str) {
    match place {
        Place::Pane => design::heading(ui, "Image", title),
    }
}
fn end(ui: &mut egui::Ui, place: Place) {
    if place == Place::Pane {
        design::end(ui);
    }
}

/// Filters are starting points that set the sliders below them.
pub(crate) fn color_controls(ui: &mut egui::Ui, color: &mut ColorAdjust, place: Place) {
    begin(ui, place, "Filters");
    ui.horizontal_wrapped(|ui| {
        for (name, look) in ColorAdjust::LOOKS {
            if ui
                .selectable_label(*color == look, name)
                .tip_title(name)
                .tip("A starting point. It sets the Color sliders, so you can adjust it after.")
                .clicked()
            {
                *color = look;
            }
        }
    });
    end(ui, place);
    begin(ui, place, "Color");
    for (name, value, tip) in [
        (
            "Brightness",
            &mut color.brightness,
            "Make the picture lighter or darker.",
        ),
        (
            "Contrast",
            &mut color.contrast,
            "Spread or narrow the difference between light and dark.",
        ),
        (
            "Saturation",
            &mut color.saturation,
            "Make colors stronger or closer to gray.",
        ),
        (
            "Warmth",
            &mut color.warmth,
            "Shift colors toward orange or blue.",
        ),
    ] {
        layout::slider(ui, name, egui::Slider::new(value, -100..=100)).tip(tip);
    }
    if crate::video_editor::ui::button(ui, "Clear", "Reset color", ui.available_width(), false)
        .tip("Return to the original colors.")
        .clicked()
    {
        *color = ColorAdjust::default();
    }
    end(ui, place);
}

pub(crate) fn finish_controls(ui: &mut egui::Ui, f: &mut Finish, place: Place) {
    color_controls(ui, &mut f.color, place);
    begin(ui, place, "Border");
    layout::color_row(ui, 1, |ui| layout::swatch(ui, "Color", &mut f.border.color));
    layout::slider(
        ui,
        "Width",
        egui::Slider::new(&mut f.border.width, 0. ..=64.)
            .suffix(" px")
            .fixed_decimals(0),
    )
    .tip("How thick the border is. 0 removes it.");
    layout::slider(
        ui,
        "Corners",
        egui::Slider::new(&mut f.border.corners, 0. ..=200.)
            .suffix(" px")
            .fixed_decimals(0),
    )
    .tip("How round the picture's corners are.");
    end(ui, place);
    begin(ui, place, "Edge");
    ui.horizontal_wrapped(|ui| {
        for (style, name) in EdgeStyle::ALL {
            ui.selectable_value(&mut f.edge.style, style, name)
                .tip_title(name)
                .tip("The shape of the picture's edge. Torn, Wave and Zigzag match Cut Out.");
        }
    });
    if f.edge.style != EdgeStyle::None {
        layout::slider(
            ui,
            "Strength",
            egui::Slider::new(&mut f.edge.strength, 0..=100).suffix("%"),
        )
        .tip("How far the edge effect reaches into the picture.");
        let mut all = f.edge.sides == [true; 4];
        if crate::controls::toggle(ui, &mut all, "All sides").changed() {
            f.edge.sides = if all {
                [true; 4]
            } else {
                [true, false, false, false]
            };
        }
        if !all {
            ui.columns(2, |col| {
                for (i, name) in ["Top", "Right", "Bottom", "Left"].into_iter().enumerate() {
                    crate::controls::toggle(&mut col[i % 2], &mut f.edge.sides[i], name);
                }
            });
        }
    }
    end(ui, place);
}
