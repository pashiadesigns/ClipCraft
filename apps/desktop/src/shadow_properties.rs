use crate::tips::Tip as _;
use crate::{Annotation, egui, property_layout};
use capture_core::shadow::Shadow;

pub(super) fn show(ui: &mut egui::Ui, a: &mut Annotation) {
    if matches!(
        a.kind,
        crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate
    ) {
        return;
    }
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
                    .interact(cell, ui.scope_id().with(i), egui::Sense::click())
                    .tip(format!("Shadow: {}", names[i]));
                ui.painter().rect(
                    cell.shrink(0.5),
                    1.,
                    if selected {
                        crate::shell::ACCENT
                    } else if r.hovered() {
                        egui::Color32::from_gray(80)
                    } else {
                        egui::Color32::from_rgb(52, 59, 61)
                    },
                    egui::Stroke::new(0.7, egui::Color32::from_gray(95)),
                    egui::StrokeKind::Inside,
                );
                if i == 4 {
                    let c = cell.center();
                    let s = egui::Stroke::new(1.2, egui::Color32::WHITE);
                    ui.painter()
                        .line_segment([c - egui::vec2(3., 3.), c + egui::vec2(3., 3.)], s);
                    ui.painter()
                        .line_segment([c - egui::vec2(3., -3.), c + egui::vec2(3., -3.)], s);
                }
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
            if let Some(shadow) = &mut a.shadow {
                let response = crate::color_widgets::property_rgba(
                    ui,
                    &mut shadow.color,
                    crate::color_widgets::Preview::Fill,
                );
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::unique("shadow-color"), response.rect));
                let _ = response;
            }
        });
        let Some(mut shadow) = a.shadow else {
            return;
        };
        property_layout::slider(
            ui,
            "Shadow angle",
            egui::Slider::new(&mut shadow.angle, 0. ..=360.).suffix("°"),
        );
        property_layout::slider(
            ui,
            "Shadow distance",
            egui::Slider::new(&mut shadow.distance, 0. ..=256.),
        );
        let mut opacity = f32::from(shadow.color[3]) / 2.55;
        if property_layout::slider(
            ui,
            "Shadow opacity",
            egui::Slider::new(&mut opacity, 0. ..=100.).suffix("%"),
        )
        .changed()
        {
            shadow.color[3] = (opacity * 2.55).round() as u8;
        }
        property_layout::slider(
            ui,
            "Shadow blur",
            egui::Slider::new(&mut shadow.blur, 0. ..=64.),
        );
        a.shadow = Some(shadow);
    });
}

#[cfg(test)]
mod tests;
