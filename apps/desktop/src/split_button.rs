//! Shared enclosing frame for a repeat action and its internal dropdown.
use crate::egui;

/// Pair the dropdown's complete hit target with the actual rendered button.
pub(super) fn dropdown_for<R>(
    ui: &mut egui::Ui,
    label: &str,
    button: &egui::Response,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    dropdown_sized(ui, label, egui::vec2(22.0, button.rect.height()), content)
}

pub(super) fn dropdown_sized<R>(
    ui: &mut egui::Ui,
    label: &str,
    size: egui::Vec2,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let response = ui
        .push_id(label, |ui| ui.add(egui::Button::new("").min_size(size)))
        .inner;
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("split-dropdown", label)), response.rect));
    let inner = egui::Popup::menu(&response).show(content).map(|r| r.inner);
    let result = egui::InnerResponse { response, inner };
    result
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let center = result.response.rect.center();
    // Draw geometry: the bundled UI fonts do not all contain U+25BC.
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            center + egui::vec2(-4.0, -2.0),
            center + egui::vec2(4.0, -2.0),
            center + egui::vec2(0.0, 3.0),
        ],
        ui.visuals().text_color(),
        egui::Stroke::NONE,
    ));
    result
}

pub(super) fn show<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let style = ui.visuals().widgets.inactive;
    egui::Frame::new()
        .fill(style.weak_bg_fill)
        .stroke(egui::Stroke::new(
            1.0,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ))
        .corner_radius(4)
        .inner_margin(0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let widgets = &mut ui.visuals_mut().widgets;
            for widget in [
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
                &mut widgets.open,
            ] {
                widget.bg_stroke = egui::Stroke::NONE;
                widget.corner_radius = egui::CornerRadius::ZERO;
                widget.expansion = 0.0;
            }
            // The parent may be a tall ribbon row. Do not center a guessed
            // single-line row first and then grow it after placing the tiles.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), content)
                .inner
        })
        .inner
}
