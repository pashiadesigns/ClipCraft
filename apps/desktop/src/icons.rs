//! Original vector marks: no external icon assets or font-symbol dependency.
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, StrokeKind};

pub fn draw(p: &Painter, rect: Rect, name: &str, color: Color32) {
    draw_scaled(p, rect, name, color, 1.0);
}

pub(super) fn draw_scaled(p: &Painter, rect: Rect, name: &str, color: Color32, scale: f32) {
    let c = rect.center();
    let at = |x: f32, y: f32| c + egui::vec2(x, y) * scale;
    let stroke = Stroke::new(2.0 * scale, color);
    let line = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], stroke);
    };
    let box_rect = Rect::from_center_size(c, egui::vec2(22.0, 18.0) * scale);
    match name {
        "Arrow" => {
            line(at(-10.0, 10.0), at(9.0, -9.0));
            line(at(9.0, -9.0), at(0.0, -9.0));
            line(at(9.0, -9.0), at(9.0, 0.0));
        }
        "Ellipse" => {
            p.circle_stroke(c, 11.0 * scale, stroke);
        }
        "Polygon" => {
            p.add(egui::Shape::closed_line(
                vec![at(0.0, -12.0), at(12.0, 10.0), at(-12.0, 10.0)],
                stroke,
            ));
        }
        "Redact" => {
            p.rect_filled(box_rect, 2.0 * scale, Color32::from_gray(238));
            p.rect_filled(
                Rect::from_min_max(at(-8.0, -3.0), at(8.0, 3.0)),
                0.0,
                Color32::from_gray(18),
            );
        }
        "Pixelate" => {
            for (x, y) in [(-7.0, -7.0), (7.0, -7.0), (-7.0, 7.0), (7.0, 7.0)] {
                p.rect_filled(
                    Rect::from_center_size(at(x, y), egui::vec2(10.0, 10.0) * scale),
                    0.0,
                    color,
                );
            }
        }
        "Text" => {
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "A",
                egui::FontId::proportional(28.0 * scale),
                color,
            );
        }
        "Shape" | "Selection" | "Callout" => {
            p.rect_stroke(
                box_rect,
                if name == "Callout" { 4.0 * scale } else { 0.0 },
                stroke,
                StrokeKind::Inside,
            );
            if name == "Callout" {
                line(at(-5.0, 8.0), at(-9.0, 13.0));
            }
            if name == "Selection" {
                for corner in [
                    box_rect.min,
                    box_rect.max,
                    box_rect.left_bottom(),
                    box_rect.right_top(),
                ] {
                    p.rect_filled(
                        Rect::from_center_size(corner, egui::vec2(4.0, 4.0) * scale),
                        0.0,
                        color,
                    );
                }
            }
        }
        "Move" => {
            p.add(egui::Shape::convex_polygon(
                vec![at(-9.0, -12.0), at(-6.0, 10.0), at(0.0, 3.0), at(10.0, 0.0)],
                color,
                Stroke::NONE,
            ));
        }
        "Stamp" => {
            let handle = Color32::from_rgb(214, 157, 99);
            p.rect_filled(
                Rect::from_min_max(at(-5.0, -12.0), at(5.0, -4.0)),
                4.0 * scale,
                handle,
            );
            p.rect_filled(
                Rect::from_min_max(at(-2.5, -5.0), at(2.5, 3.0)),
                1.0 * scale,
                handle,
            );
            p.add(egui::Shape::convex_polygon(
                vec![at(-7.0, 3.0), at(7.0, 3.0), at(11.0, 8.0), at(-11.0, 8.0)],
                color,
                Stroke::NONE,
            ));
            p.rect_filled(
                Rect::from_min_max(at(-11.0, 9.0), at(11.0, 12.0)),
                1.0 * scale,
                color,
            );
        }
        "Favorites" => {
            let points: Vec<_> = (0..10)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                    let radius = if i % 2 == 0 { 12.0 } else { 5.0 };
                    at(angle.cos() * radius, angle.sin() * radius)
                })
                .collect();
            p.add(egui::Shape::closed_line(
                points,
                Stroke::new(2.0 * scale, Color32::from_rgb(250, 204, 77)),
            ));
        }
        "Magnify" | "Step" => {
            p.circle_stroke(c, 10.0 * scale, stroke);
            if name == "Magnify" {
                line(at(7.0, 7.0), at(14.0, 14.0));
                line(at(-5.0, 0.0), at(5.0, 0.0));
                line(at(0.0, -5.0), at(0.0, 5.0));
            } else {
                p.text(
                    c,
                    egui::Align2::CENTER_CENTER,
                    "1",
                    egui::FontId::proportional(16.0 * scale),
                    color,
                );
            }
        }
        "Magic Wand" => {
            // A rounded baton and compact four-point sparkles stay legible
            // inside the same 28px footprint as the other tool marks.
            let tip = Color32::from_rgb(250, 211, 115);
            p.line_segment(
                [at(-9.0, 9.0), at(1.5, -1.5)],
                Stroke::new(3.6 * scale, color),
            );
            p.circle_filled(at(-9.0, 9.0), 1.8 * scale, color);
            p.line_segment(
                [at(1.5, -1.5), at(4.0, -4.0)],
                Stroke::new(3.6 * scale, tip),
            );
            let sparkle = |x: f32, y: f32, radius: f32| {
                let waist = radius * 0.28;
                for points in [
                    vec![
                        at(x, y - radius),
                        at(x + waist, y),
                        at(x, y + radius),
                        at(x - waist, y),
                    ],
                    vec![
                        at(x - radius, y),
                        at(x, y - waist),
                        at(x + radius, y),
                        at(x, y + waist),
                    ],
                ] {
                    p.add(egui::Shape::convex_polygon(points, tip, Stroke::NONE));
                }
            };
            sparkle(8.0, -8.0, 4.0);
            sparkle(-2.0, -9.0, 2.3);
            sparkle(10.0, 2.0, 2.3);
        }
        "Line" => {
            line(at(-9.0, 10.0), at(9.0, -10.0));
        }
        "Pen" => {
            p.add(egui::Shape::convex_polygon(
                vec![at(-7.0, 3.0), at(5.0, -9.0), at(10.0, -4.0), at(-2.0, 8.0)],
                Color32::TRANSPARENT,
                stroke,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![at(-7.0, 3.0), at(-10.0, 11.0), at(-2.0, 8.0)],
                color,
                Stroke::NONE,
            ));
            line(at(5.0, -9.0), at(7.0, -11.0));
            line(at(7.0, -11.0), at(12.0, -6.0));
            line(at(12.0, -6.0), at(8.0, -2.0));
            line(at(-5.0, 12.0), at(5.0, 12.0));
        }
        "Highlighter" => {
            let ink = Color32::from_rgb(247, 218, 84);
            p.add(egui::Shape::convex_polygon(
                vec![at(-5.0, 0.0), at(4.0, -11.0), at(12.0, -4.0), at(3.0, 7.0)],
                Color32::TRANSPARENT,
                stroke,
            ));
            line(at(-5.0, 0.0), at(3.0, 7.0));
            p.add(egui::Shape::convex_polygon(
                vec![at(-5.0, 2.0), at(0.0, 6.0), at(-4.0, 10.0), at(-10.0, 10.0)],
                ink,
                Stroke::NONE,
            ));
            p.rect_filled(
                Rect::from_min_max(at(-12.0, 12.0), at(9.0, 14.0)),
                1.0 * scale,
                ink,
            );
        }
        "Cut Out" => {
            // A craft knife, distinct from Crop's intersecting corner marks.
            p.add(egui::Shape::convex_polygon(
                vec![at(-3.0, 1.0), at(7.0, -11.0), at(12.0, -7.0), at(2.0, 5.0)],
                Color32::TRANSPARENT,
                stroke,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![at(-3.0, 2.0), at(1.0, 6.0), at(-11.0, 12.0), at(-8.0, 6.0)],
                Color32::from_rgb(167, 201, 218),
                Stroke::NONE,
            ));
            line(at(4.0, -4.0), at(7.0, -7.0));
        }
        "Crop" => {
            line(at(-6.0, -13.0), at(-6.0, 6.0));
            line(at(-6.0, 6.0), at(13.0, 6.0));
            line(at(-13.0, -6.0), at(6.0, -6.0));
            line(at(6.0, -6.0), at(6.0, 13.0));
        }
        "Blur" => {
            for x in -1..=1 {
                for y in -1..=1 {
                    p.rect_filled(
                        Rect::from_center_size(
                            at(x as f32 * 8.0, y as f32 * 8.0),
                            egui::vec2(6.0, 6.0) * scale,
                        ),
                        0.0,
                        color.gamma_multiply(0.4 + (x + y + 2) as f32 * 0.15),
                    );
                }
            }
        }
        "Fill" => {
            // Neutral tipped bucket; only the pouring paint receives color.
            p.add(egui::Shape::convex_polygon(
                vec![
                    at(-11.0, -1.0),
                    at(-2.0, -10.0),
                    at(8.0, 0.0),
                    at(-1.0, 9.0),
                ],
                Color32::TRANSPARENT,
                stroke,
            ));
            line(at(-11.0, -1.0), at(8.0, 0.0));
            line(at(-7.0, -4.0), at(-7.0, -11.0));
            line(at(-7.0, -11.0), at(-3.0, -13.0));
            let paint = Color32::from_rgb(91, 189, 234);
            p.add(egui::Shape::convex_polygon(
                vec![at(8.0, 1.0), at(11.0, 5.0), at(13.0, 9.0), at(7.0, 9.0)],
                paint,
                Stroke::NONE,
            ));
            p.circle_filled(at(10.0, 9.0), 3.0 * scale, paint);
        }
        "Eraser" => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    at(-10.0, 3.0),
                    at(3.0, -10.0),
                    at(12.0, -1.0),
                    at(-1.0, 12.0),
                ],
                Color32::from_rgb(238, 147, 180),
                Stroke::NONE,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![at(-10.0, 3.0), at(-5.0, -2.0), at(4.0, 7.0), at(-1.0, 12.0)],
                color,
                Stroke::NONE,
            ));
            line(at(-1.0, 13.0), at(11.0, 13.0));
        }
        "Simplify" => {
            // A miniature UI with detail replaced by clean colored blocks.
            p.rect_stroke(
                Rect::from_min_max(at(-12.0, -11.0), at(12.0, 11.0)),
                2.0 * scale,
                Stroke::new(1.5 * scale, color),
                StrokeKind::Inside,
            );
            line(at(-8.0, -6.0), at(2.0, -6.0));
            p.rect_filled(
                Rect::from_min_max(at(-8.0, -1.0), at(-2.0, 7.0)),
                1.0 * scale,
                Color32::from_rgb(106, 198, 184),
            );
            p.rect_filled(
                Rect::from_min_max(at(1.0, -1.0), at(8.0, 2.0)),
                1.0 * scale,
                color,
            );
            p.rect_filled(
                Rect::from_min_max(at(1.0, 5.0), at(6.0, 7.0)),
                1.0 * scale,
                color,
            );
        }
        _ => {
            p.rect_filled(box_rect, 0.0, color);
        }
    }
}
