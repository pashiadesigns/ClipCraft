use crate::{Annotation, egui, property_layout};
use capture_core::shadow::Shadow;

pub(super) fn show(ui: &mut egui::Ui, a: &mut Annotation) {
    ui.push_id("object-shadow", |ui| {
        ui.horizontal(|ui| {
            property_layout::field_label(ui, "Shadow", 54.0);
            let angles = [225., 270., 315., 180., 0., 0., 135., 90., 45.];
            let names = [
                "Top left",
                "Top",
                "Top right",
                "Left",
                "Off",
                "Right",
                "Bottom left",
                "Bottom",
                "Bottom right",
            ];
            let active = a.shadow.map(|s| (s.angle / 45.).round() as i32 % 8);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(54., 54.), egui::Sense::hover());
            for i in 0..9 {
                let cell = egui::Rect::from_min_size(
                    rect.min + egui::vec2((i % 3) as f32 * 18., (i / 3) as f32 * 18.),
                    egui::vec2(18., 18.),
                );
                let selected = if i == 4 {
                    a.shadow.is_none()
                } else {
                    active == Some((angles[i] / 45.) as i32 % 8)
                };
                let r = ui
                    .put(
                        cell,
                        egui::Button::new(if i == 4 { "×" } else { "" })
                            .fill(if selected {
                                egui::Color32::from_rgb(18, 151, 194)
                            } else {
                                egui::Color32::from_rgb(52, 59, 61)
                            })
                            .stroke(egui::Stroke::new(0.7, egui::Color32::from_gray(95)))
                            .corner_radius(0),
                    )
                    .on_hover_text(format!("Shadow: {}", names[i]));
                r.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        true,
                        format!("Shadow {}", names[i]),
                    )
                });
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique(("shadow-direction", names[i])), r.rect)
                });
                if i != 4 && selected {
                    ui.painter()
                        .circle_filled(cell.center(), 3., egui::Color32::WHITE);
                }
                if r.clicked() {
                    a.shadow = if i == 4 {
                        None
                    } else {
                        Some(Shadow {
                            angle: angles[i],
                            ..a.shadow.unwrap_or_default()
                        })
                    };
                }
            }
            let r = ui
                .add_sized(
                    [90., 54.],
                    egui::Button::new("Settings   ")
                        .fill(egui::Color32::from_rgb(52, 59, 61))
                        .stroke(egui::Stroke::new(0.7, egui::Color32::from_gray(95))),
                )
                .on_hover_text("Shadow color, angle, distance, opacity and blur");
            let c = r.rect.right_center() - egui::vec2(10., 0.);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    c + egui::vec2(-3., -2.),
                    c + egui::vec2(3., -2.),
                    c + egui::vec2(0., 2.),
                ],
                egui::Color32::WHITE,
                egui::Stroke::NONE,
            ));
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("shadow-settings"), r.rect));
            // The color picker owns egui's single memory popup slot. Keep the
            // parent alive independently while its nested picker is open.
            let popup_id = r.id.with("shadow-popup");
            let state_id = r.id.with("shadow-open");
            let mut open = ui
                .ctx()
                .data_mut(|d| d.get_temp::<bool>(state_id).unwrap_or(false));
            if r.clicked() {
                open = !open;
                if open {
                    egui::Popup::open_id(ui.ctx(), popup_id);
                }
            }
            let child_open =
                egui::Popup::is_any_open(ui.ctx()) && !egui::Popup::is_id_open(ui.ctx(), popup_id);
            egui::Popup::from_response(&r)
                .id(popup_id)
                .open_bool(&mut open)
                .frame(egui::Frame::popup(ui.style()).inner_margin(12))
                .close_behavior(if child_open {
                    egui::PopupCloseBehavior::IgnoreClicks
                } else {
                    egui::PopupCloseBehavior::CloseOnClickOutside
                })
                .show(|ui| {
                    ui.set_width(248.);
                    let mut shadow = a.shadow.unwrap_or_default();
                    let before = shadow;
                    let mut enabled = a.shadow.is_some();
                    let was_enabled = enabled;
                    ui.horizontal(|ui| {
                        ui.label("Color");
                        let color = crate::color_widgets::property_rgba(
                            ui,
                            &mut shadow.color,
                            crate::color_widgets::Preview::Fill,
                        );
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::unique("shadow-color"), color.rect)
                        });
                        #[cfg(not(test))]
                        let _ = color;
                        crate::controls::toggle(ui, &mut enabled, "Enabled");
                    });
                    property_layout::slider(
                        ui,
                        "Angle",
                        egui::Slider::new(&mut shadow.angle, 0.0..=360.0).suffix("°"),
                    );
                    property_layout::slider(
                        ui,
                        "Distance",
                        egui::Slider::new(&mut shadow.distance, 0.0..=256.0),
                    );
                    let mut opacity = f32::from(shadow.color[3]) / 2.55;
                    if property_layout::slider(
                        ui,
                        "Opacity",
                        egui::Slider::new(&mut opacity, 0.0..=100.0).suffix("%"),
                    )
                    .changed()
                    {
                        shadow.color[3] = (opacity * 2.55).round() as u8;
                    }
                    property_layout::slider(
                        ui,
                        "Blur",
                        egui::Slider::new(&mut shadow.blur, 0.0..=64.0),
                    );
                    if shadow != before {
                        enabled = true;
                    }
                    if shadow != before || enabled != was_enabled {
                        a.shadow = enabled.then_some(shadow);
                    }
                });
            if open && !egui::Popup::is_any_open(ui.ctx()) {
                egui::Popup::open_id(ui.ctx(), popup_id);
            } else if !open {
                egui::Popup::close_id(ui.ctx(), popup_id);
            }
            ui.ctx().data_mut(|d| d.insert_temp(state_id, open));
        });
    });
}

#[cfg(test)]
mod tests;
