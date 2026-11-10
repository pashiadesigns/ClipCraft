//! The shared Appearance group (approved properties audit, `artifacts/bg-proposal`).
//! Every tool uses one layout, identical in the image and video panes:
//! color row, Thickness, Outline style, Text stroke, Opacity, Fill opacity,
//! Corner radius, Shadow. Controls a tool doesn't have are left out.
use crate::tips::Tip as _;
use crate::{Annotation, Kind, egui, property_layout as layout};

/// Edits `a` in place; the caller commits the change as one property edit.
pub(crate) fn show(ui: &mut egui::Ui, a: &mut Annotation) {
    match a.kind {
        Kind::Rectangle | Kind::Ellipse | Kind::Polygon => crate::shape_properties::show(ui, a),
        Kind::Callout => callout(ui, a),
        Kind::Step => step(ui, a),
        Kind::Text => crate::text_properties::show(ui, a),
        Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlighter => line(ui, a),
        _ => {}
    }
    if matches!(a.kind, Kind::Rectangle | Kind::Callout) {
        let max = a.max_corner_radius();
        layout::slider(
            ui,
            "Corner radius",
            egui::Slider::new(&mut a.corner_radius, 0. ..=max),
        );
    }
    if !matches!(a.kind, Kind::Eraser) {
        crate::shadow_properties::show(ui, a);
    }
}

/// Pointer style, for a selected callout and the next one. The body shape is
/// a preset (Callout, Rounded, Oval and Cloud callout).
pub(crate) fn callout_pointer(ui: &mut egui::Ui, tail: &mut capture_core::callout::CalloutTail) {
    use capture_core::callout::CalloutTail as T;
    layout::choice(
        ui,
        "Pointer",
        tail,
        &[
            (T::Wedge, "Wedge"),
            (T::Thin, "Thin"),
            (T::Bubbles, "Thought bubbles"),
        ],
        "How the callout points at something. Drag its tip on the canvas to aim it.",
    );
}

fn callout(ui: &mut egui::Ui, a: &mut Annotation) {
    let ink = a.color;
    if let Some(c) = a.callout.as_mut() {
        callout_pointer(ui, &mut c.tail);
        layout::color_row(ui, 5, |ui| {
            layout::swatch(ui, "Fill", &mut c.fill);
            layout::swatch(ui, "Outline", &mut c.outline);
            crate::text_properties::color_swatch(ui, &mut a.text, ink);
        });
    }
    layout::divider(ui);
    layout::slider(ui, "Thickness", egui::Slider::new(&mut a.width, 1..=32));
    crate::line_properties::outline(ui, a);
    crate::text_properties::outline_controls(ui, a);
    if let Some(c) = a.callout.as_mut() {
        layout::slider(ui, "Opacity", egui::Slider::new(&mut c.outline[3], 0..=255));
        layout::slider(
            ui,
            "Fill opacity",
            egui::Slider::new(&mut c.fill[3], 0..=255),
        );
    }
}

/// Steps match shapes: Thickness 0 means no outline (it replaces the old
/// Show outline switch), and the outline takes the same outline styles.
fn step(ui: &mut egui::Ui, a: &mut Annotation) {
    let mut outline = a
        .step_outline
        .clone()
        .unwrap_or(capture_core::steps::Outline {
            width: 0.,
            ..Default::default()
        });
    let before = outline.clone();
    let ink = capture_core::steps::foreground(a).color;
    layout::color_row(ui, 5, |ui| {
        layout::swatch(ui, "Fill", &mut a.color);
        layout::swatch(ui, "Outline", &mut outline.color);
        crate::text_properties::color_swatch(ui, &mut a.text, ink);
    });
    layout::divider(ui);
    // Choosing an outline color on a marker without one turns the outline on.
    if outline.color != before.color && outline.width == 0. {
        outline.width = capture_core::steps::Outline::default().width;
    }
    layout::slider(
        ui,
        "Thickness",
        egui::Slider::new(&mut outline.width, 0.0..=32.0),
    )
    .tip("0 shows no outline");
    let has_outline = outline.width > 0.;
    // Only an edit changes the marker; showing the pane never does.
    if outline != before {
        a.step_outline = has_outline.then_some(outline);
    }
    ui.add_enabled_ui(has_outline, |ui| crate::line_properties::outline(ui, a));
    crate::text_properties::outline_controls(ui, a);
    if let Some(o) = a.step_outline.as_mut() {
        layout::slider(ui, "Opacity", egui::Slider::new(&mut o.color[3], 0..=255));
    }
    layout::slider(
        ui,
        "Fill opacity",
        egui::Slider::new(&mut a.color[3], 0..=255),
    );
}

fn line(ui: &mut egui::Ui, a: &mut Annotation) {
    layout::color_row(ui, 1, |ui| layout::swatch(ui, "Color", &mut a.color));
    layout::divider(ui);
    if !a.highlighter_rect {
        layout::slider(ui, "Thickness", egui::Slider::new(&mut a.width, 1..=32));
    }
    crate::line_properties::show(ui, a);
    layout::slider(ui, "Opacity", egui::Slider::new(&mut a.color[3], 0..=255));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::PaintedOutput;
    /// Positions of the labels drawn by the Appearance group, in reading order.
    fn order(a: &mut Annotation, names: &[&str]) {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let before = a.clone();
        let out = PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(340., 1200.),
                )),
                ..Default::default()
            },
            |ui| show(ui, a),
        ));
        let at: Vec<(i32, i32)> = names
            .iter()
            .map(|n| {
                let r = crate::color_widgets::tests::text_rect(&out.shapes, n)
                    .unwrap_or_else(|| panic!("{n} missing for {:?}", a.kind));
                ((r.center().y / 6.).round() as i32, r.left() as i32)
            })
            .collect();
        assert!(
            at.windows(2).all(|w| w[0] < w[1]),
            "{names:?} out of order for {:?}: {at:?}",
            a.kind
        );
        for gone in [
            "Text opacity",
            "Show outline",
            "Outline size",
            "Outline width",
            "Width",
        ] {
            assert!(
                crate::color_widgets::tests::text_rect(&out.shapes, gone).is_none(),
                "{gone} still shown for {:?}",
                a.kind
            );
        }
        // Showing the pane never edits the object.
        assert_eq!(*a, before);
    }
    #[test]
    fn every_tool_shares_one_appearance_order_without_text_opacity() {
        let doc: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        let find = |k: Kind| doc.annotations.iter().find(|a| a.kind == k).cloned();
        let mut shape = find(Kind::Rectangle).unwrap();
        order(
            &mut shape,
            &[
                "Fill",
                "Outline",
                "Thickness",
                "Outline style",
                "Opacity",
                "Corner radius",
                "Shadow",
            ],
        );
        let mut step = find(Kind::Step).unwrap();
        order(
            &mut step,
            &[
                "Fill",
                "Outline",
                "Text",
                "Thickness",
                "Outline style",
                "Text stroke size",
                "Fill opacity",
                "Shadow",
            ],
        );
        let mut line = find(Kind::Arrow).unwrap();
        order(&mut line, &["Color", "Thickness", "Opacity", "Shadow"]);
        if let Some(mut callout) = find(Kind::Callout) {
            order(
                &mut callout,
                &[
                    "Fill",
                    "Outline",
                    "Text",
                    "Thickness",
                    "Outline style",
                    "Text stroke size",
                    "Opacity",
                    "Fill opacity",
                    "Corner radius",
                    "Shadow",
                ],
            );
        }
    }
}
