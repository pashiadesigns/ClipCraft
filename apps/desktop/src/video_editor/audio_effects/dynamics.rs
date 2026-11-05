use super::*;
use capture_core::video::effects::{Effect, Meter};
use egui::{Color32, Stroke};
pub(super) fn draw(ui: &mut egui::Ui, effect: &mut Effect, meter: Option<Meter>) -> bool {
    let (r, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 200.),
        egui::Sense::click_and_drag(),
    );
    let p = ui.painter_at(r);
    p.rect_filled(r, 6., Color32::from_rgb(27, 37, 41));
    let blue = Color32::from_rgb(73, 197, 233);
    let gray = Color32::from_rgb(125, 144, 151);
    let plot = egui::Rect::from_min_max(r.min + egui::vec2(40., 30.), r.max - egui::vec2(12., 62.));
    let leveler = effect.kind == Kind::Leveler;
    let title = if leveler {
        "LIVE SPEECH LEVEL"
    } else {
        "LIVE WAVEFORM"
    };
    p.text(
        r.min + egui::vec2(12., 8.),
        egui::Align2::LEFT_TOP,
        title,
        egui::FontId::proportional(11.),
        blue,
    );
    for (offset, label, color) in [(155., "Before", gray), (75., "After", blue)] {
        let at = egui::pos2(r.right() - offset, r.top() + 14.);
        p.circle_filled(at, 3., color);
        p.text(
            at + egui::vec2(8., 0.),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.),
            color,
        );
    }
    let history = meter.and_then(|m| m.history).unwrap_or_default();
    for (i, label) in [
        (0, "−5s"),
        (1, "−4s"),
        (2, "−3s"),
        (3, "−2s"),
        (4, "−1s"),
        (5, "Now"),
    ] {
        let x = plot.left() + i as f32 / 5. * plot.width();
        p.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            Stroke::new(0.6, Color32::from_gray(65)),
        );
        p.text(
            egui::pos2(x, r.bottom() - 3.),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(10.),
            gray,
        );
    }
    let y_db = |value: f32| plot.bottom() - ((value + 60.) / 60.).clamp(0., 1.) * plot.height();
    let cy = plot.center().y;
    let amp = |v: f32| v.clamp(0., 1.).sqrt() * plot.height() * 0.5;
    for value in [0., -12., -24.] {
        let y = if leveler {
            y_db(value)
        } else {
            cy - amp(capture_core::video::effects::gain(value as f64) as f32)
        };
        p.text(
            egui::pos2(plot.left() - 6., y),
            egui::Align2::RIGHT_CENTER,
            format!("{value:.0}"),
            egui::FontId::proportional(10.),
            gray,
        );
        p.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(0.5, Color32::from_gray(59)),
        );
    }
    let x = |i: usize| plot.left() + ((250 - history.len + i) as f32 + 0.5) / 250. * plot.width();
    if effect.kind == Kind::DeEsser {
        for (i, v) in history.points[..history.len].iter().enumerate() {
            if v.change < -0.2 {
                p.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x(i) - plot.width() / 500., plot.top()),
                        egui::pos2(x(i) + plot.width() / 500., plot.bottom()),
                    ),
                    0.,
                    Color32::from_rgba_unmultiplied(240, 174, 85, 60),
                );
            }
        }
    }
    for (output, color) in [(false, gray), (true, blue)] {
        if leveler {
            let points: Vec<_> = history.points[..history.len]
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    egui::pos2(
                        x(i),
                        y_db(capture_core::video::effects::db(if output {
                            v.output_rms
                        } else {
                            v.input_rms
                        } as f64) as f32),
                    )
                })
                .collect();
            p.add(egui::Shape::line(
                super::curve::smooth(&points),
                Stroke::new(1.5, color),
            ));
        } else {
            let upper: Vec<_> = history.points[..history.len]
                .iter()
                .enumerate()
                .map(|(i, v)| egui::pos2(x(i), cy - amp(if output { v.output } else { v.input })))
                .collect();
            let upper = super::curve::smooth(&upper);
            let lower: Vec<_> = upper
                .iter()
                .map(|p| egui::pos2(p.x, 2. * cy - p.y))
                .collect();
            let mut mesh = egui::Mesh::default();
            for (u, l) in upper.windows(2).zip(lower.windows(2)) {
                let base = mesh.vertices.len() as u32;
                for pos in [u[0], u[1], l[1], l[0]] {
                    mesh.colored_vertex(pos, color.gamma_multiply(0.25));
                }
                mesh.add_triangle(base, base + 1, base + 2);
                mesh.add_triangle(base, base + 2, base + 3);
            }
            p.add(egui::Shape::mesh(mesh));
            p.add(egui::Shape::line(upper, Stroke::new(1.2, color)));
            p.add(egui::Shape::line(lower, Stroke::new(1.2, color)));
        }
    }
    let threshold_index = match effect.kind {
        Kind::DeEsser => None,
        _ => Some(0),
    };
    let mut changed = false;
    if let Some(index) = threshold_index {
        let value = effect.values[index];
        let y = if leveler {
            y_db(value)
        } else {
            cy - amp(capture_core::video::effects::gain(value as f64) as f32)
        };
        p.line_segment(
            [egui::pos2(plot.left(), y), egui::pos2(plot.right(), y)],
            Stroke::new(1., Color32::from_rgb(245, 191, 104)),
        );
        p.text(
            egui::pos2(plot.right() - 3., y + 2.),
            egui::Align2::RIGHT_TOP,
            format!(
                "{} {value:.1} dBFS · drag",
                if leveler {
                    "Target"
                } else if effect.kind == Kind::Limiter {
                    "Ceiling"
                } else {
                    "Threshold"
                }
            ),
            egui::FontId::proportional(10.),
            Color32::from_rgb(245, 191, 104),
        );
        if response.double_clicked() {
            effect.values[index] =
                capture_core::video::effects::Effect::new(effect.id, effect.kind).values[index];
            return true;
        }
        if (response.clicked() || response.dragged())
            && let Some(pos) = response.interact_pointer_pos()
            && plot.contains(pos)
        {
            let db = if leveler {
                (plot.bottom() - pos.y) / plot.height() * 60. - 60.
            } else {
                capture_core::video::effects::db(
                    ((cy - pos.y).abs() / (plot.height() * 0.5)).powi(2) as f64,
                ) as f32
            };
            let param = effect.kind.parameters()[index];
            effect.values[index] = db.clamp(param.min, param.max);
            changed = true;
        }
    }
    let bottom = egui::Rect::from_min_max(
        egui::pos2(plot.left(), r.bottom() - 48.),
        egui::pos2(plot.right(), r.bottom() - 20.),
    );
    let zero = bottom.top() + bottom.height() / 3.;
    p.line_segment(
        [
            egui::pos2(bottom.left(), zero),
            egui::pos2(bottom.right(), zero),
        ],
        Stroke::new(0.5, gray),
    );
    let points: Vec<_> = history.points[..history.len]
        .iter()
        .enumerate()
        .map(|(i, v)| {
            egui::pos2(
                x(i),
                zero - v.change.clamp(-24., 12.) / 36. * bottom.height(),
            )
        })
        .collect();
    p.add(egui::Shape::line(
        super::curve::smooth(&points),
        Stroke::new(1.5, Color32::from_rgb(233, 166, 103)),
    ));
    let last = history
        .points
        .get(history.len.saturating_sub(1))
        .map_or(0., |p| p.change);
    p.text(
        bottom.left_top(),
        egui::Align2::LEFT_TOP,
        format!("Level change {last:+.1} dB"),
        egui::FontId::proportional(10.),
        Color32::from_rgb(233, 166, 103),
    );
    if history.len == 0 {
        p.text(
            plot.center(),
            egui::Align2::CENTER_CENTER,
            "Play audio to see its actual response",
            egui::FontId::proportional(13.),
            Color32::LIGHT_GRAY,
        );
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("dynamics-plot"), plot));
    if effect.kind == Kind::DeEsser {
        ui.label(
            egui::RichText::new("Amber marks sibilance being reduced.")
                .small()
                .color(Color32::from_rgb(240, 174, 85)),
        );
    }
    changed
}
