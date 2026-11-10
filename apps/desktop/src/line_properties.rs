use crate::tips::Tip as _;
use crate::{Annotation, Kind, egui, property_layout};
use capture_core::line_style::{Curve, Endpoint, Pattern, Style};

pub(super) fn show(ui: &mut egui::Ui, a: &mut Annotation) {
    if !matches!(a.kind, Kind::Arrow | Kind::Line) {
        return;
    }
    let mut style = a.line_style.unwrap_or_else(|| Style::for_kind(a.kind));
    let before = style;
    ui.horizontal(|ui| {
        endpoint(ui, "Start", &mut style.start);
        pattern_picker(ui, &mut style.pattern);
        endpoint(ui, "End", &mut style.end);
    });
    ui.add_enabled_ui(style.start.sized(), |ui| {
        property_layout::slider(
            ui,
            "Start size",
            egui::Slider::new(&mut style.start_size, 0.5..=12.),
        );
    });
    ui.add_enabled_ui(style.end.sized(), |ui| {
        property_layout::slider(
            ui,
            "End size",
            egui::Slider::new(&mut style.end_size, 0.5..=12.),
        );
    });
    ui.horizontal(|ui|{
        property_layout::field_label(ui,"Curve",24.0);
        egui::ComboBox::from_id_salt("line-curve").width(152.).selected_text(match style.curve {Curve::Straight=>"Straight",Curve::Circular=>"Circular arc",Curve::Bezier=>"Bézier curve"}).show_ui(ui,|ui|{
            for (curve,label) in [(Curve::Straight,"Straight"),(Curve::Circular,"Circular arc"),(Curve::Bezier,"Bézier curve")] {
                let r=ui.selectable_value(&mut style.curve,curve,label);
                #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::unique(("line-curve-choice",label)),r.rect));
                if r.clicked() && curve!=Curve::Straight && style.bend==0. {style.bend=0.25;}
            }
        }).response.tip("Drag the middle node to bend the line. Circular arcs retain a true circular shape as endpoints move.");
    });
    if before != style {
        if style.curve == Curve::Circular {
            style.bend = style.bend.clamp(-0.5, 0.5);
        }
        a.line_style = Some(style);
    }
}

pub(super) fn outline(ui: &mut egui::Ui, a: &mut Annotation) {
    let mut pattern = a.line_style.map_or(Pattern::Solid, |s| s.pattern);
    let before = pattern;
    ui.horizontal(|ui| {
        property_layout::field_label(ui, "Outline style", 24.);
        pattern_picker(ui, &mut pattern);
    });
    if pattern != before {
        a.line_style = with_pattern(a, pattern);
    }
}
/// The object's line style with only the pattern changed. The curve, ends and
/// everything else stay as they were.
pub(crate) fn with_pattern(a: &Annotation, pattern: Pattern) -> Option<Style> {
    let plain = Style::for_kind(a.kind);
    let next = Style {
        pattern,
        ..a.line_style.unwrap_or(plain)
    };
    (next != plain).then_some(next)
}
fn pattern_picker(ui: &mut egui::Ui, pattern: &mut Pattern) {
    let response = egui::ComboBox::from_id_salt("line-pattern")
        .width(96.)
        .selected_text(" ")
        .show_ui(ui, |ui| {
            for choice in Pattern::ALL {
                let r =
                    ui.selectable_value(pattern, choice, format!("           {}", choice.label()));
                paint_pattern(
                    ui,
                    r.rect.with_max_x(r.rect.left() + 49.).shrink(6.),
                    choice,
                );
            }
        })
        .response
        .tip(format!("Line pattern: {}", pattern.label()));
    paint_pattern(
        ui,
        response
            .rect
            .with_max_x(response.rect.right() - 20.)
            .shrink(6.),
        *pattern,
    );
}

fn endpoint(ui: &mut egui::Ui, label: &str, value: &mut Endpoint) {
    let response = egui::ComboBox::from_id_salt(("endpoint", label))
        .width(55.)
        .selected_text(" ")
        .show_ui(ui, |ui| {
            for kind in Endpoint::ALL {
                let r = ui.selectable_value(value, kind, format!("          {}", kind.label()));
                paint_endpoint(
                    ui,
                    r.rect.with_max_x(r.rect.left() + 43.).shrink(4.),
                    kind,
                    label == "Start",
                );
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::unique(("endpoint-choice", label, kind.label())),
                        r.rect,
                    )
                });
            }
        })
        .response
        .tip(format!("{label} endpoint: {}", value.label()));
    paint_endpoint(
        ui,
        response
            .rect
            .with_max_x(response.rect.right() - 19.)
            .shrink(4.),
        *value,
        label == "Start",
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("{label} endpoint"))
    });
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("endpoint-menu", label)), response.rect));
}
pub(super) fn sample(kind: Endpoint, start: bool) -> Annotation {
    Annotation {
        id: 0,
        kind: Kind::Line,
        start: [12., 20.],
        end: [66., 20.],
        color: [255; 4],
        width: 3,
        corner_radius: 0.,

        focus: None,

        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.,
        mirror_x: false,
        fill: None,
        text: None,
        points: vec![],
        callout: None,
        image: None,
        step_outline: None,
        shadow: None,
        line_style: Some(Style {
            start: if start { kind } else { Endpoint::Flat },
            end: if start { Endpoint::Flat } else { kind },
            ..Style::for_kind(Kind::Line)
        }),
    }
}
fn paint_endpoint(ui: &egui::Ui, rect: egui::Rect, kind: Endpoint, start: bool) {
    let mut a = sample(kind, start);
    a.color = ui.visuals().text_color().to_array();
    let key = ui.make_persistent_id(("endpoint-preview", kind.label(), start));
    if let Some(texture) = preview(ui.ctx(), key, &a) {
        ui.painter().image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            egui::Color32::WHITE,
        );
    }
}

/// One cached supersampled preview per visible tile/control, replaced when its
/// style changes. The same renderer supplies filled heads, patterns and shadows.
pub(super) fn preview(
    ctx: &egui::Context,
    id: egui::Id,
    a: &Annotation,
) -> Option<egui::TextureHandle> {
    if let Some((stored, texture)) =
        ctx.data(|d| d.get_temp::<(Annotation, egui::TextureHandle)>(id))
        && stored == *a
    {
        return Some(texture);
    }
    let (mut lo, mut hi) = capture_core::line_style::bounds(a);
    if let Some((l, h)) = capture_core::shadow::bounds(a) {
        for i in 0..2 {
            lo[i] = lo[i].min(l[i]);
            hi[i] = hi[i].max(h[i]);
        }
    }
    let margin = a.width as f32 * 0.5 + 2.;
    lo = lo.map(|v| v - margin);
    hi = hi.map(|v| v + margin);
    let scale =
        (156. / f64::from(hi[0] - lo[0]).max(1.)).min(60. / f64::from(hi[1] - lo[1]).max(1.));
    let center = [0, 1].map(|i| f64::from(lo[i] + hi[i]) * 0.5);
    let image = capture_core::display::render_annotation_preview(
        a,
        capture_core::display::Viewport {
            size: [160, 64],
            origin: [center[0] - 80. / scale, center[1] - 32. / scale],
            scale,
        },
    )
    .ok()?;
    let texture = ctx.load_texture(
        "Line style",
        egui::ColorImage::from_rgba_unmultiplied([160, 64], image.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(id, (a.clone(), texture.clone())));
    Some(texture)
}
fn paint_pattern(ui: &egui::Ui, rect: egui::Rect, pattern: Pattern) {
    let stroke = egui::Stroke::new(2.0, ui.visuals().text_color());
    if pattern == Pattern::Wavy {
        let points = (0..=48)
            .map(|i| {
                let t = i as f32 / 48.;
                egui::pos2(
                    rect.left() + t * rect.width(),
                    rect.center().y + (t * std::f32::consts::TAU * 3.).sin() * 2.,
                )
            })
            .collect();
        ui.painter().add(egui::Shape::line(points, stroke));
        return;
    }
    let dash = pattern.dashes(1.5);
    if dash.is_empty() {
        ui.painter()
            .line_segment([rect.left_center(), rect.right_center()], stroke);
        return;
    }
    let mut x = rect.left();
    let mut index = 0;
    while x < rect.right() {
        let end = (x + dash[index % dash.len()] as f32).min(rect.right());
        if index % 2 == 0 {
            ui.painter().line_segment(
                [
                    egui::pos2(x, rect.center().y),
                    egui::pos2(end, rect.center().y),
                ],
                stroke,
            );
        }
        x = end;
        index += 1;
    }
}

pub(super) fn paint_selection(
    painter: &egui::Painter,
    a: &Annotation,
    transform: crate::canvas_view::Transform,
    color: egui::Color32,
) {
    if matches!(a.kind, Kind::Arrow | Kind::Line) {
        for line in
            capture_core::line_style::outlines(a, 0.25 / f64::from(transform.scale.max(0.01)))
        {
            painter.add(egui::Shape::line(
                line.into_iter()
                    .map(|p| transform.screen_point(a.rotation().point(p)))
                    .collect(),
                egui::Stroke::new(1., color),
            ));
        }
    }
}
