use super::*;
use capture_core::video::effects::Effect;
use egui::{Color32, Pos2, Rect, Stroke};
const BLUE: Color32 = Color32::from_rgb(73, 197, 233);
pub(super) fn draw(
    ui: &mut egui::Ui,
    effect: &mut Effect,
    band: &mut usize,
    meter: Option<capture_core::video::effects::Meter>,
) -> bool {
    if matches!(
        effect.kind,
        Kind::Compressor | Kind::DeEsser | Kind::Leveler | Kind::Limiter
    ) {
        return super::dynamics::draw(ui, effect, meter);
    }
    let spectrum = meter.and_then(|m| m.spectrum);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 200.),
        egui::Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6., Color32::from_rgb(32, 42, 46));
    painter.rect_stroke(
        rect,
        6.,
        Stroke::new(1., Color32::from_rgb(82, 98, 105)),
        egui::StrokeKind::Inside,
    );
    let plot = rect.shrink2(egui::vec2(34., 22.));
    let x = |f: f64| plot.left() + ((f / 20.).ln() / 1000f64.ln()) as f32 * plot.width();
    let y = |db: f64| plot.top() + ((12. - db) / 24.) as f32 * plot.height();
    let text = |p: Pos2, a: egui::Align2, t: String| {
        painter.text(
            p,
            a,
            t,
            egui::FontId::proportional(11.),
            Color32::from_rgb(180, 193, 198),
        );
    };
    {
        if matches!(effect.kind, Kind::Equalizer | Kind::Clarity) {
            if let Some(spectrum) = spectrum {
                for (values, color) in [
                    (spectrum.input, Color32::from_rgb(112, 139, 148)),
                    (spectrum.output, Color32::from_rgb(64, 151, 162)),
                ] {
                    let points: Vec<_> = values
                        .iter()
                        .enumerate()
                        .map(|(i, value)| {
                            egui::pos2(
                                x(capture_core::video::effects::Spectrum::frequency(i)),
                                plot.bottom()
                                    - ((*value + 90.) / 90.).clamp(0., 1.) * plot.height(),
                            )
                        })
                        .collect();
                    let points = super::curve::smooth(&points);
                    let mut mesh = egui::Mesh::default();
                    for pair in points.windows(2) {
                        let base = mesh.vertices.len() as u32;
                        for pos in [
                            egui::pos2(pair[0].x, plot.bottom()),
                            pair[0],
                            pair[1],
                            egui::pos2(pair[1].x, plot.bottom()),
                        ] {
                            mesh.colored_vertex(pos, color.gamma_multiply(0.18));
                        }
                        mesh.add_triangle(base, base + 1, base + 2);
                        mesh.add_triangle(base, base + 2, base + 3);
                    }
                    painter.add(egui::Shape::mesh(mesh));
                    painter.add(egui::Shape::line(points, Stroke::new(1., color)));
                }
                text(
                    plot.right_top(),
                    egui::Align2::RIGHT_TOP,
                    "Spectrum 0…−90 dBFS".into(),
                );
            } else {
                text(
                    plot.right_top(),
                    egui::Align2::RIGHT_TOP,
                    "Play to view live spectrum".into(),
                );
            }
        }
        for freq in [40., 100., 500., 2000., 10000.] {
            let px = x(freq);
            painter.line_segment(
                [egui::pos2(px, plot.top()), egui::pos2(px, plot.bottom())],
                Stroke::new(0.6, Color32::from_gray(75)),
            );
            text(
                egui::pos2(px, rect.bottom() - 5.),
                egui::Align2::CENTER_BOTTOM,
                if freq >= 1000. {
                    format!("{}k", freq / 1000.)
                } else {
                    format!("{freq}")
                },
            );
        }
        for db in [-12., -6., 0., 6., 12.] {
            painter.line_segment(
                [
                    egui::pos2(plot.left(), y(db)),
                    egui::pos2(plot.right(), y(db)),
                ],
                Stroke::new(if db == 0. { 1. } else { 0.6 }, Color32::from_gray(75)),
            );
            text(
                egui::pos2(plot.left() - 6., y(db)),
                egui::Align2::RIGHT_CENTER,
                format!("{db:+}"),
            );
        }
        let points = (0..=180)
            .map(|i| {
                let f = 20. * 1000f64.powf(i as f64 / 180.);
                egui::pos2(
                    x(f),
                    y((if effect.kind == Kind::DeEsser {
                        -effect.values[2] as f64
                            * (-(f / effect.values[0] as f64).ln().powi(2) * 6.).exp()
                    } else {
                        effect.response_db(f)
                    })
                    .clamp(-12., 12.)),
                )
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(2., BLUE)));
        if effect.kind == Kind::DeEsser {
            text(
                plot.right_top(),
                egui::Align2::RIGHT_TOP,
                "Maximum reduction envelope".into(),
            );
        }
        if effect.kind == Kind::Equalizer {
            let to_frequency = |p: Pos2| {
                (20. * 1000f64.powf(((p.x - plot.left()) / plot.width()).clamp(0., 1.) as f64))
                    as f32
            };
            let gain_at =
                |p: Pos2| (12. - (p.y - plot.top()) / plot.height() * 24.).clamp(-12., 12.);
            let node_pos = |b: &capture_core::video::effects::Band| {
                egui::pos2(
                    x(b.frequency as f64),
                    y(effect.response_db(b.frequency as f64).clamp(-12., 12.)),
                )
            };
            let nearest = |p: Pos2| {
                effect
                    .bands
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| node_pos(b).distance(p) <= 12.)
                    .min_by(|(_, a), (_, b)| {
                        node_pos(a)
                            .distance_sq(p)
                            .total_cmp(&node_pos(b).distance_sq(p))
                    })
                    .map(|(i, _)| i)
            };
            let mut changed = false;
            let drag_id = response.id.with("eq-node-drag");
            if response.double_clicked()
                && let Some(pos) = response.interact_pointer_pos()
                && let Some(index) = nearest(pos)
            {
                let defaults = capture_core::video::effects::Effect::new(effect.id, effect.kind);
                let old = effect.bands[index];
                effect.bands[index] = defaults
                    .bands
                    .get(index)
                    .copied()
                    .filter(|b| b.kind == old.kind)
                    .unwrap_or(capture_core::video::effects::Band {
                        frequency: match old.kind {
                            capture_core::video::effects::BandKind::Bell => 1000.,
                            capture_core::video::effects::BandKind::HighPass => 80.,
                            capture_core::video::effects::BandKind::LowPass => 12000.,
                        },
                        gain: 0.,
                        q: 1.,
                        ..old
                    });
                *band = index;
                return true;
            }
            if (response.drag_started() || response.clicked())
                && let Some(p) = ui
                    .input(|i| i.pointer.press_origin())
                    .or(response.interact_pointer_pos())
            {
                ui.ctx().data_mut(|d| d.insert_temp(drag_id, false));
                if let Some(i) = nearest(p) {
                    *band = i;
                    ui.ctx().data_mut(|d| d.insert_temp(drag_id, true));
                } else if plot.contains(p)
                    && effect.bands.len() < capture_core::video::effects::MAX_EQ_BANDS
                    && (p.y - y(effect.response_db(to_frequency(p) as f64).clamp(-12., 12.))).abs()
                        < 12.
                {
                    effect.bands.push(capture_core::video::effects::Band::bell(
                        to_frequency(p),
                        1.,
                    ));
                    *band = effect.bands.len() - 1;
                    changed = true;
                    ui.ctx().data_mut(|d| d.insert_temp(drag_id, true));
                }
            }
            if response.dragged()
                && ui
                    .ctx()
                    .data(|d| d.get_temp::<bool>(drag_id).unwrap_or(false))
                && *band < effect.bands.len()
                && let Some(p) = response.interact_pointer_pos()
            {
                let f = to_frequency(p);
                let old = effect.bands[*band].gain;
                effect.bands[*band].frequency = f;
                let contribution = effect.response_db(f as f64) as f32 - old;
                if effect.bands[*band].kind == capture_core::video::effects::BandKind::Bell {
                    effect.bands[*band].gain = (gain_at(p) - contribution).clamp(-12., 12.);
                }
                changed = true;
            }
            response.clone().on_hover_text("Click the EQ curve to add a band. Drag a node for frequency and gain. Use Width · Q below to set bandwidth.");
            for (i, b) in effect.bands.iter().enumerate() {
                let pos = egui::pos2(
                    x(b.frequency as f64),
                    y(effect.response_db(b.frequency as f64).clamp(-12., 12.)),
                );
                painter.circle_filled(pos, if i == *band { 6. } else { 4. }, BLUE);
                if i == *band {
                    painter.circle_stroke(pos, 6., Stroke::new(1., Color32::WHITE));
                }
            }
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("effects-plot"), plot));
            if changed {
                return true;
            }
        }
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("effects-graph"), rect));
    false
}
pub(super) fn meters(ui: &mut egui::Ui, meter: Option<capture_core::video::effects::Meter>) {
    ui.label(egui::RichText::new("Signal").strong());
    let m = meter.unwrap_or_default();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 207.), egui::Sense::hover());
    for (i, label, value) in [
        (0, "In", m.input),
        (1, "GR", m.reduction),
        (2, "Out", m.output),
    ] {
        let cx = rect.left() + 14. + i as f32 * (rect.width() - 28.) / 2.;
        let bar = Rect::from_min_size(egui::pos2(cx - 7., rect.top() + 24.), egui::vec2(14., 145.));
        ui.painter().text(
            egui::pos2(cx, rect.top()),
            egui::Align2::CENTER_TOP,
            label,
            egui::FontId::proportional(11.),
            Color32::LIGHT_GRAY,
        );
        ui.painter()
            .rect_filled(bar, 2., Color32::from_rgb(32, 41, 45));
        let h = if i == 1 {
            value / 12.
        } else {
            ((capture_core::video::effects::db(value as f64) + 60.) / 60.) as f32
        }
        .clamp(0., 1.)
            * bar.height();
        let fill = if i == 1 {
            Rect::from_min_size(bar.min, egui::vec2(14., h))
        } else {
            Rect::from_min_max(bar.left_bottom() - egui::vec2(0., h), bar.right_bottom())
        };
        ui.painter().rect_filled(
            fill,
            2.,
            if i == 1 {
                Color32::from_rgb(232, 196, 110)
            } else {
                Color32::from_rgb(112, 197, 174)
            },
        );
        ui.painter().text(
            egui::pos2(cx, bar.bottom() + 6.),
            egui::Align2::CENTER_TOP,
            if i == 1 {
                format!("−{value:.1}")
            } else {
                format!("{:.0}", capture_core::video::effects::db(value as f64))
            },
            egui::FontId::proportional(11.),
            Color32::LIGHT_GRAY,
        );
    }
    ui.small("Selected effect");
    ui.small(if meter.is_some() {
        "Live playback levels"
    } else {
        "Play to meter audio"
    });
}
