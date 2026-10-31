use crate::{Annotation, Kind, egui};

pub(super) fn is_effect(kind: Option<Kind>) -> bool {
    matches!(kind, Some(Kind::Blur | Kind::Pixelate))
}

pub(super) fn show(ui: &mut egui::Ui, annotation: &mut Annotation) {
    if !is_effect(Some(annotation.kind)) {
        return;
    }
    crate::property_layout::slider(
        ui,
        "Intensity",
        egui::Slider::new(&mut annotation.width, 1..=32),
    );
}
