//! Shared enclosing frame for a repeat action and its internal dropdown.
use crate::egui;

/// One continuous preset surface. The selection outline is painted after both
/// hit targets, so neither button's hover fill can cover any part of it.
pub(super) fn preset<R>(
    ui: &mut egui::Ui,
    active: bool,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let result = egui::Frame::new()
        .fill(egui::Color32::from_rgb(42, 47, 50))
        .corner_radius(3)
        .inner_margin(0)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            let widgets = &mut ui.visuals_mut().widgets;
            for widget in [
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
                &mut widgets.open,
            ] {
                widget.bg_stroke = egui::Stroke::NONE;
                widget.corner_radius = egui::CornerRadius {
                    nw: 0,
                    sw: 0,
                    ne: 3,
                    se: 3,
                };
                widget.expansion = 0.;
            }
            widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(42, 47, 50);
            widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(65, 79, 85);
            widgets.active.weak_bg_fill = widgets.hovered.weak_bg_fill;
            widgets.open.weak_bg_fill = widgets.hovered.weak_bg_fill;
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), content)
                .inner
        });
    if active {
        ui.painter().rect_stroke(
            result.response.rect,
            3,
            egui::Stroke::new(2., egui::Color32::from_rgb(38, 155, 194)),
            egui::StrokeKind::Inside,
        );
    }
    result.inner
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_preset_outline_encloses_both_targets_above_their_hover_fills() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let style = capture_core::object_styles::presets(crate::Kind::Rectangle).remove(0);
        let mut main = egui::Rect::NOTHING;
        let mut arrow = egui::Rect::NOTHING;
        let mut paint = |pointer: Option<egui::Pos2>| {
            ctx.run_ui(
                egui::RawInput {
                    events: pointer
                        .map(|p| vec![egui::Event::PointerMoved(p)])
                        .unwrap_or_default(),
                    ..Default::default()
                },
                |ui| {
                    preset(ui, true, |ui| {
                        let r = crate::style_tile::split_tile(ui, &style, "Rectangle", true);
                        main = r.rect;
                        arrow = dropdown_for(ui, "Rectangle variations", &r, |ui| {
                            ui.label("Variations");
                        })
                        .response
                        .rect;
                    });
                },
            )
        };
        let mut first = paint(None::<egui::Pos2>);
        first.textures_delta.clear();
        drop(paint);
        assert!((main.right() - arrow.left()).abs() < 0.01);
        assert_eq!(main.height(), arrow.height());
        let joined = main.union(arrow);
        let check = |out: &egui::FullOutput| {
            let index = out.shapes.iter().position(|s| matches!(&s.shape,egui::Shape::Rect(r) if r.stroke.width==2. && r.stroke.color==egui::Color32::from_rgb(38,155,194) && r.rect==joined)).unwrap();
            assert!(out.shapes.iter().enumerate().filter(|(_,s)| matches!(&s.shape,egui::Shape::Rect(r) if r.fill.a()>0 && r.rect.intersects(joined))).all(|(i,_)| i<index));
        };
        check(&first);
        for pointer in [main.center(), arrow.center()] {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerMoved(pointer)],
                    ..Default::default()
                },
                |ui| {
                    preset(ui, true, |ui| {
                        let r = crate::style_tile::split_tile(ui, &style, "Rectangle", true);
                        dropdown_for(ui, "Rectangle variations", &r, |ui| {
                            ui.label("Variations");
                        });
                    });
                },
            );
            out.textures_delta.clear();
            check(&out);
            assert!(out.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Rect(r) if r.fill==egui::Color32::from_rgb(65,79,85) && r.rect.contains(pointer))));
        }
    }
}
