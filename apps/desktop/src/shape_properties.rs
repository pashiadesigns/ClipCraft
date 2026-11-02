use crate::{Annotation, Kind, egui};

pub(super) fn show(ui: &mut egui::Ui, annotation: &mut Annotation) {
    if !matches!(
        annotation.kind,
        Kind::Rectangle | Kind::Ellipse | Kind::Polygon
    ) {
        return;
    }
    let mut outline = annotation.color;
    let had_fill = annotation.fill.is_some();
    ui.horizontal_top(|ui| {
        let mut fill = annotation.fill.unwrap_or([
            annotation.color[0],
            annotation.color[1],
            annotation.color[2],
            0,
        ]);
        crate::property_layout::swatch(ui, "Fill", &mut fill);
        if had_fill || fill[3] != 0 {
            annotation.fill = Some(fill);
        }
        crate::property_layout::swatch(ui, "Outline", &mut outline);
        if !annotation.is_image_frame() {
            crate::text_properties::color_swatch(ui, &mut annotation.text, annotation.color);
        }
    });
    crate::property_layout::divider(ui);
    crate::property_layout::slider(
        ui,
        "Thickness",
        egui::Slider::new(&mut annotation.width, 1..=32),
    );
    crate::property_layout::slider(
        ui,
        if annotation.is_image_frame() {
            "Outline opacity"
        } else {
            "Opacity"
        },
        egui::Slider::new(&mut outline[3], 0..=255),
    );
    if let Some(fill) = &mut annotation.fill {
        crate::property_layout::slider(
            ui,
            "Fill opacity",
            egui::Slider::new(&mut fill[3], 0..=255),
        );
    }
    if annotation.kind == Kind::Polygon {
        let mut sides = annotation.points.len() as u32;
        if crate::property_layout::slider(ui, "Sides", egui::Slider::new(&mut sides, 3..=64))
            .changed()
        {
            let (min, max) = annotation.body_bounds();
            annotation.points = capture_core::polygon::outline(min, max, sides);
            annotation.start = min;
            annotation.end = max;
        }
    }
    if outline != annotation.color {
        // Older/default labels may inherit their ink from the shape outline.
        // Materialize that displayed color only on an edit, before replacing
        // the outline (including alpha); passive display remains a no-op.
        if let Some(text) = &mut annotation.text {
            text.color.get_or_insert(annotation.color);
        }
        annotation.color = outline;
    }
}
