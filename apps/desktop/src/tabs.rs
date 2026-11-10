//! Shared compact tab treatment: dark parent, icon and a filled active tab.
use crate::egui;
use crate::tips::Tip as _;
use egui::{Color32, Response, Stroke};

pub(super) const ACTIVE_BLUE: Color32 = Color32::from_rgb(24, 145, 193);
pub(super) const BACKGROUND: Color32 = Color32::from_rgb(20, 23, 25);

pub(super) fn group<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(BACKGROUND)
        .corner_radius(5)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            body(ui)
        })
        .inner
}

pub(super) fn tab(ui: &mut egui::Ui, label: &str, selected: bool, width: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), selected, label)
    });
    let fill = if selected {
        ACTIVE_BLUE
    } else if response.hovered() || response.has_focus() {
        Color32::from_rgb(62, 70, 74)
    } else {
        Color32::TRANSPARENT
    };
    let p = ui.painter();
    p.rect_filled(rect, 4.0, fill);
    let title = p.layout_no_wrap(
        label.into(),
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
    let c = rect.center() - egui::vec2((title.size().x + 24.) * 0.5 - 8., 0.);
    let stroke = Stroke::new(1.4, Color32::WHITE);
    let at = |x, y| c + egui::vec2(x, y);
    match label {
        "Keybinds" => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(16.0, 11.0)),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for y in [-2.0, 1.0] {
                for x in [-5.0, -2.0, 1.0, 4.0] {
                    p.circle_filled(at(x, y), 0.7, Color32::WHITE);
                }
            }
            p.line_segment(
                [at(-3.0, 3.5), at(3.0, 3.5)],
                Stroke::new(1.0, Color32::WHITE),
            );
        }
        "File" => {
            p.add(egui::Shape::line(
                vec![
                    at(1.0, -7.0),
                    at(-5.0, -7.0),
                    at(-5.0, 7.0),
                    at(5.0, 7.0),
                    at(5.0, -3.0),
                    at(1.0, -7.0),
                    at(1.0, -3.0),
                    at(5.0, -3.0),
                ],
                stroke,
            ));
            p.line_segment([at(-2.0, 1.0), at(2.0, 1.0)], stroke);
            p.line_segment([at(-2.0, 4.0), at(2.0, 4.0)], stroke);
        }
        "Library" => crate::ribbon::draw_icon(p, c, "Library", Color32::WHITE),
        "Insert" => crate::ribbon::draw_icon(p, c, "Tools", Color32::WHITE),

        "Layers" => {
            for offset in [-3.0, 2.0] {
                p.rect_stroke(
                    egui::Rect::from_center_size(at(offset, offset), egui::vec2(10.0, 10.0)),
                    1.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }

        "Clip" => crate::ribbon::draw_icon(p, c, "Capture", Color32::WHITE),
        "Formatting" => {
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(18.),
                Color32::WHITE,
            );
            p.line_segment([at(-6., 7.), at(6., 7.)], stroke);
        }
        "Export" => crate::ribbon::draw_icon(p, c, "Save As", Color32::WHITE),
        _ => {
            p.circle_stroke(c, 4.5, stroke);
            p.circle_stroke(c, 1.3, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let d = egui::vec2(a.cos(), a.sin());
                p.line_segment([c + d * 4.5, c + d * 7.0], Stroke::new(2.2, Color32::WHITE));
            }
        }
    }
    p.text(
        c + egui::vec2(16.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
    response.tip_title(label)
}
