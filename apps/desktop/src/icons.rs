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
        "Group presets" | "Save group" => {
            p.rect_stroke(
                Rect::from_center_size(at(-4., -3.), egui::vec2(14., 12.) * scale),
                2.,
                stroke,
                StrokeKind::Inside,
            );
            p.rect_stroke(
                Rect::from_center_size(at(4., 4.), egui::vec2(14., 12.) * scale),
                2.,
                stroke,
                StrokeKind::Inside,
            );
            if name == "Save group" {
                line(at(9., -11.), at(9., -3.));
                line(at(5., -7.), at(13., -7.));
            }
        }
        "Magnify" | "Magnified callout" => {
            p.rect_stroke(
                Rect::from_center_size(at(-9., 3.), egui::vec2(8., 7.) * scale),
                1.,
                stroke,
                StrokeKind::Inside,
            );
            p.circle_stroke(at(6., -2.), 8. * scale, stroke);
            line(at(-5., 0.), at(-1., -7.));
            line(at(-5., 6.), at(2., 5.));
            line(at(3., -2.), at(9., -2.));
            line(at(6., -5.), at(6., 1.));
        }
        "GIF Studio" => {
            p.rect_stroke(box_rect, 3., stroke, StrokeKind::Inside);
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "GIF",
                egui::FontId::proportional(9. * scale),
                color,
            );
        }
        "Focus" => {
            p.rect_filled(box_rect, 3., color.gamma_multiply(0.25));
            p.rect_stroke(box_rect, 3., stroke, StrokeKind::Inside);
            p.circle_stroke(c, 5.5 * scale, stroke);
            for (x, y, sx, sy) in [
                (-12., -10., 1., 1.),
                (12., -10., -1., 1.),
                (-12., 10., 1., -1.),
                (12., 10., -1., -1.),
            ] {
                line(at(x, y + sy * 4.), at(x, y));
                line(at(x, y), at(x + sx * 4., y));
            }
        }
        "Zoom" | "Zoom in" => crate::video_editor::ui::icon(p, rect, "Zoom in", color),
        "Expand canvas on" | "Expand canvas off" | "Auto-expand canvas" => {
            p.rect_stroke(
                Rect::from_center_size(c, egui::vec2(16., 14.) * scale),
                1. * scale,
                stroke,
                StrokeKind::Inside,
            );
            if name != "Expand canvas off" {
                for (x, y) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
                    line(at(x * 5., y * 4.), at(x * 13., y * 12.));
                    line(at(x * 13., y * 12.), at(x * 8., y * 12.));
                    line(at(x * 13., y * 12.), at(x * 13., y * 7.));
                }
            } else {
                for (x, y) in [(-12., -11.), (12., 11.)] {
                    line(at(x, -11.), at(x, 11.));
                    line(at(-12., y), at(12., y));
                }
            }
        }
        "Bullet list" | "Numbered list" | "No list" => {
            for (i, y) in [-8., 0., 8.].into_iter().enumerate() {
                line(at(-3., y), at(12., y));
                if name == "Bullet list" {
                    p.circle_filled(at(-10., y), 1.7 * scale, color);
                } else if name == "Numbered list" {
                    p.text(
                        at(-10., y),
                        egui::Align2::CENTER_CENTER,
                        (i + 1).to_string(),
                        egui::FontId::proportional(9. * scale),
                        color,
                    );
                }
            }
        }
        "Capture Clip" | "Capture Video" => {
            p.circle_filled(c, 11. * scale, color);
            p.circle_stroke(c, 13. * scale, Stroke::new(scale, color));
            let white = Stroke::new(1.25 * scale, Color32::WHITE);
            if name == "Capture Clip" {
                p.add(egui::Shape::closed_line(
                    vec![
                        at(-6., -3.),
                        at(-3., -3.),
                        at(-2., -5.),
                        at(2., -5.),
                        at(3., -3.),
                        at(6., -3.),
                        at(6., 5.),
                        at(-6., 5.),
                    ],
                    white,
                ));
                p.circle_stroke(at(0., 1.), 2.5 * scale, white);
            } else {
                p.rect_stroke(
                    Rect::from_min_max(at(-6., -4.), at(2., 4.)),
                    1. * scale,
                    white,
                    StrokeKind::Inside,
                );
                p.add(egui::Shape::closed_line(
                    vec![at(2., -1.5), at(6., -4.), at(6., 4.), at(2., 1.5)],
                    white,
                ));
            }
        }
        "Scrolling capture" => {
            p.rect_stroke(box_rect, 2. * scale, stroke, StrokeKind::Inside);
            line(at(0., -6.), at(0., 6.));
            for y in [-6., 6.] {
                let dy = if y < 0. { 3. } else { -3. };
                line(at(0., y), at(-3., y + dy));
                line(at(0., y), at(3., y + dy));
            }
        }
        "Show Recent Clips" | "Hide Recent Clips" => {
            let dy = if name == "Show Recent Clips" { -1. } else { 1. };
            let blue = Stroke::new(2. * scale, crate::shell::ACCENT);
            p.line_segment([at(-6., -3. * dy), at(0., 3. * dy)], blue);
            p.line_segment([at(0., 3. * dy), at(6., -3. * dy)], blue);
        }
        "Show annotations" | "Hide annotations" => {
            let points = (0..=32)
                .map(|i| {
                    let t = i as f32 * std::f32::consts::TAU / 32.;
                    at(11. * t.cos(), 6. * t.sin())
                })
                .collect();
            p.add(egui::Shape::closed_line(points, stroke));
            p.circle_stroke(c, 2.8 * scale, stroke);
            if name == "Hide annotations" {
                line(at(-11., 10.), at(11., -10.));
            }
        }
        "Penlighter" => {
            // Two original nibs: fine writing at left, broad yellow highlighting at right.
            p.add(egui::Shape::closed_line(
                vec![
                    at(-12., 5.),
                    at(-5., -11.),
                    at(0., -9.),
                    at(-7., 7.),
                    at(-12., 11.),
                    at(-12., 5.),
                ],
                stroke,
            ));
            line(at(-12., 5.), at(-7., 7.));
            p.add(egui::Shape::closed_line(
                vec![at(0., 4.), at(6., -10.), at(12., -7.), at(6., 7.)],
                stroke,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![at(0., 4.), at(6., 7.), at(3., 11.), at(-3., 11.)],
                Color32::YELLOW,
                Stroke::NONE,
            ));
            p.line_segment(
                [at(-2., 14.), at(11., 12.)],
                Stroke::new(3. * scale, Color32::YELLOW),
            );
        }
        "Region" => {
            for (x, y, dx, dy) in [
                (-11., -9., 1., 1.),
                (11., -9., -1., 1.),
                (-11., 9., 1., -1.),
                (11., 9., -1., -1.),
            ] {
                line(at(x, y), at(x + dx * 7., y));
                line(at(x, y), at(x, y + dy * 6.));
            }
        }
        "Window" => {
            p.rect_stroke(box_rect, 2. * scale, stroke, StrokeKind::Inside);
            line(at(-10., -3.), at(10., -3.));
            p.circle_filled(at(-6., -6.), scale, color);
        }
        "Fullscreen" => {
            p.rect_stroke(box_rect, 2. * scale, stroke, StrokeKind::Inside);
            line(at(0., 9.), at(0., 13.));
            line(at(-6., 13.), at(6., 13.));
            for (x, y, dx, dy) in [
                (-7., -5., 1., 1.),
                (7., -5., -1., 1.),
                (-7., 5., 1., -1.),
                (7., 5., -1., -1.),
            ] {
                line(at(x, y), at(x + dx * 3., y));
                line(at(x, y), at(x, y + dy * 3.));
            }
        }
        "Arrow" | "Lines" => {
            // Parallel arrow and dotted line represent both connector styles.
            line(at(-11.0, -5.0), at(11.0, -5.0));
            line(at(11.0, -5.0), at(5.0, -10.0));
            line(at(11.0, -5.0), at(5.0, 0.0));
            for x in [-10.0, -5.0, 0.0, 5.0, 10.0] {
                p.circle_filled(at(x, 7.0), 1.3 * scale, color);
            }
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
        "Shape" | "Callout" => {
            p.rect_stroke(
                box_rect,
                if name == "Callout" { 4.0 * scale } else { 0.0 },
                stroke,
                StrokeKind::Inside,
            );
            if name == "Callout" {
                line(at(-5.0, 8.0), at(-9.0, 13.0));
            }
        }
        "Move" => {
            p.add(egui::Shape::convex_polygon(
                vec![at(-9.0, -12.0), at(-6.0, 10.0), at(0.0, 3.0), at(10.0, 0.0)],
                color,
                Stroke::NONE,
            ));
        }
        "Icons" => {
            p.circle_stroke(at(-6., -6.), 5. * scale, stroke);
            p.rect_stroke(
                Rect::from_min_max(at(2., -11.), at(12., -1.)),
                2. * scale,
                stroke,
                StrokeKind::Inside,
            );
            line(at(-11., 7.), at(-7., 11.));
            line(at(-7., 11.), at(0., 3.));
            p.add(egui::Shape::closed_line(
                vec![at(7., 2.), at(13., 12.), at(1., 12.)],
                stroke,
            ));
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
        "Steps" | "Step" => {
            for (x, y, n) in [(-7., -6., "1"), (0., 0., "2"), (7., 6., "3")] {
                p.circle_filled(at(x, y), 7. * scale, Color32::from_rgb(37, 39, 41));
                p.circle_stroke(at(x, y), 7. * scale, stroke);
                p.text(
                    at(x, y),
                    egui::Align2::CENTER_CENTER,
                    n,
                    egui::FontId::proportional(10. * scale),
                    color,
                );
            }
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
        "Properties" => {
            // Three sliders, as on a settings panel.
            let stroke = egui::Stroke::new(1.6, color);
            for (y, knob) in [(0.25, 0.68), (0.5, 0.32), (0.75, 0.58)] {
                let at = |x: f32| {
                    egui::pos2(
                        rect.left() + rect.width() * x,
                        rect.top() + rect.height() * y,
                    )
                };
                p.line_segment([at(0.14), at(0.86)], stroke);
                p.circle_filled(at(knob), rect.height() * 0.09, color);
            }
        }
        "Obfuscate" | "Blur" => {
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

        _ => {
            p.rect_filled(box_rect, 0.0, color);
        }
    }
}
