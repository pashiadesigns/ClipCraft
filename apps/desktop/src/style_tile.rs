//! Compact original vector previews shared by templates and category dropdowns.
use crate::tips::Tip as _;
use crate::{Kind, egui};
use capture_core::object_styles::ObjectStyle;

/// Keep tiles at their readable native size; use extra pane width for columns.
pub(super) fn columns(width: f32, dropdown: bool) -> usize {
    ((width + 5.) / (82. + if dropdown { 22. } else { 0. } + 5.))
        .floor()
        .clamp(1., 3.) as usize
}

pub(super) fn tile(
    ui: &mut egui::Ui,
    style: &ObjectStyle,
    label: &str,
    active: bool,
) -> egui::Response {
    paint_tile(ui, style, label, active, false)
}
pub(super) fn split_tile(
    ui: &mut egui::Ui,
    style: &ObjectStyle,
    label: &str,
    active: bool,
) -> egui::Response {
    paint_tile(ui, style, label, active, true)
}
fn paint_tile(
    ui: &mut egui::Ui,
    style: &ObjectStyle,
    label: &str,
    active: bool,
    enclosed: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(82.0, 52.0), egui::Sense::click());
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("style-tile", format!("{:?}", style.kind), label)),
            (rect, active, ui.is_enabled()),
        )
    });
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Button,
            ui.is_enabled(),
            format!("Apply {label}"),
        )
    });
    ui.painter().rect_filled(
        rect,
        3,
        if response.hovered() {
            egui::Color32::from_rgb(65, 79, 85)
        } else {
            egui::Color32::from_rgb(42, 47, 50)
        },
    );
    let c = |v: [u8; 4]| egui::Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3]);
    let color = c(style.color).gamma_multiply(if ui.is_enabled() { 1.0 } else { 0.4 });
    let r =
        egui::Rect::from_center_size(rect.center() - egui::vec2(0.0, 8.0), egui::vec2(34.0, 24.0));
    let stroke = egui::Stroke::new((style.width as f32 * 0.55).clamp(1.0, 4.5), color);
    let fill = style.fill.map(c).unwrap_or(egui::Color32::TRANSPARENT);
    let p = ui.painter();
    match style.kind {
        _ if style.focus.and_then(|f| f.magnify).is_some() => {
            crate::icons::draw_scaled(p, r, "Magnified callout", color, 1.2);
        }
        _ if style.focus.is_some() => {
            let effect = style.focus.unwrap();
            let outer = egui::Rect::from_center_size(r.center(), egui::vec2(64., 28.));
            p.rect_filled(
                outer,
                2.,
                egui::Color32::from_gray((175. * (1. - effect.darken)) as u8),
            );
            for row in 0..3 {
                p.line_segment(
                    [
                        outer.left_top() + egui::vec2(3., 5. + row as f32 * 8.),
                        outer.right_top() + egui::vec2(-3., 5. + row as f32 * 8.),
                    ],
                    egui::Stroke::new(
                        if effect.blur > 0. { 3.5 } else { 1. },
                        egui::Color32::from_gray((220. * (1. - effect.darken)) as u8),
                    ),
                );
            }
            if style.kind == Kind::Ellipse {
                p.circle_filled(r.center(), 11., egui::Color32::from_gray(235));
                p.circle_stroke(r.center(), 11., stroke);
            } else {
                p.rect(
                    r,
                    if style.corner_radius > 0. { 5 } else { 0 },
                    egui::Color32::from_gray(235),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            p.line_segment(
                [
                    r.center() - egui::vec2(8., 0.),
                    r.center() + egui::vec2(8., 0.),
                ],
                egui::Stroke::new(1., egui::Color32::from_gray(80)),
            );
        }
        Kind::Eraser => {
            let radius = (style.width as f32 * 0.375 + 2.).clamp(2.5, 15.);
            let center = r.center();
            // Clip checker squares to the circular footprint, retaining vector previews.
            for row in -4..4 {
                for col in -4..4 {
                    let lo = center + egui::vec2(col as f32 * 4., row as f32 * 4.);
                    let square = egui::Rect::from_min_size(lo, egui::vec2(4., 4.));
                    let circle = (0..64)
                        .map(|i| {
                            let angle = i as f32 * std::f32::consts::TAU / 64.;
                            center + egui::vec2(angle.cos(), angle.sin()) * radius
                        })
                        .collect();
                    p.with_clip_rect(square.intersect(ui.clip_rect())).add(
                        egui::Shape::convex_polygon(
                            circle,
                            egui::Color32::from_gray(if (row + col) % 2 == 0 { 215 } else { 150 }),
                            egui::Stroke::NONE,
                        ),
                    );
                }
            }
            p.circle_stroke(
                center,
                radius,
                egui::Stroke::new(1., egui::Color32::from_gray(235)),
            );
        }
        Kind::Blur | Kind::Pixelate => {
            let region = match style.outline_shape {
                Some(capture_core::object_styles::Outline::Region(region)) => region,
                _ => Default::default(),
            };
            if let Some(texture) = effect_thumbnail(ui.ctx(), style.kind, style.width, region) {
                p.image(
                    texture.id(),
                    egui::Rect::from_center_size(r.center(), egui::vec2(64.0, 28.0)),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE.gamma_multiply(if ui.is_enabled() { 1.0 } else { 0.4 }),
                );
            }
        }
        Kind::Rectangle => {
            p.rect(
                r,
                if style.corner_radius > 0.0 { 6 } else { 0 },
                fill,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Kind::Ellipse | Kind::Step => {
            let stroke = if style.kind == Kind::Step {
                style.step_outline.as_ref().map_or(egui::Stroke::NONE, |s| {
                    egui::Stroke::new((s.width * 0.375).min(6.0), c(s.color))
                })
            } else {
                stroke
            };
            p.circle(
                r.center(),
                12.0 - stroke.width * 0.5,
                if style.kind == Kind::Step {
                    color
                } else {
                    fill
                },
                stroke,
            );
            if style.kind == Kind::Step {
                p.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    "1",
                    egui::FontId::proportional(16.0),
                    egui::Color32::WHITE,
                );
            }
        }
        Kind::Polygon => {
            use capture_core::object_styles::Outline;
            let (lo, hi) = ([r.left(), r.top()], [r.right(), r.bottom()]);
            let points = match style.outline_shape {
                Some(Outline::Ready(shape)) => shape.outline(lo, hi),
                Some(Outline::Regular(n)) => capture_core::polygon::outline(lo, hi, n),
                _ => capture_core::polygon::outline(lo, hi, 5),
            };
            let points: Vec<_> = points.iter().map(|q| egui::pos2(q[0], q[1])).collect();
            if fill.a() > 0 {
                p.add(egui::Shape::convex_polygon(
                    points.clone(),
                    fill,
                    egui::Stroke::NONE,
                ));
            }
            p.add(egui::Shape::closed_line(points, stroke));
        }
        Kind::Pen => {
            let points = (0..=64)
                .map(|i| {
                    let t = i as f32 / 64.;
                    egui::pos2(
                        r.left() - 10. + t * 54.,
                        r.center().y + 5. * (t * std::f32::consts::TAU * 3.).sin() - t * 3.,
                    )
                })
                .collect();
            p.add(egui::Shape::line(
                points,
                egui::Stroke::new(stroke.width.min(3.), color),
            ));
        }
        Kind::Highlighter => {
            let at = |x, y| r.center() + egui::vec2(x, y);
            p.rect_filled(
                egui::Rect::from_center_size(r.center(), egui::vec2(62., 30.)),
                2.,
                egui::Color32::from_gray(245),
            );
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "Abc",
                egui::FontId::proportional(22.),
                egui::Color32::from_gray(25),
            );
            let ink = egui::Color32::from_rgba_unmultiplied(
                style.color[0],
                style.color[1],
                style.color[2],
                110,
            );
            if style.highlighter_rect {
                p.rect_filled(
                    egui::Rect::from_center_size(r.center(), egui::vec2(49., 23.)),
                    0.,
                    ink,
                );
            } else {
                for (left, right, y) in [(-27., 23., -5.), (-21., 28., 1.), (-25., 20., 7.)] {
                    p.add(egui::Shape::convex_polygon(
                        vec![
                            at(left, y - 3.),
                            at(right - 3., y - 6.),
                            at(right, y),
                            at(left + 2., y + 3.),
                        ],
                        ink,
                        egui::Stroke::NONE,
                    ));
                }
            }
        }
        Kind::Arrow | Kind::Line => {
            let mut a =
                crate::line_properties::sample(capture_core::line_style::Endpoint::Flat, false);
            a.kind = style.kind;
            a.line_style = style.line_style;
            a.shadow = style.shadow;
            a.color = style.color;
            a.width = style.width;
            a.start = [12., 26.];
            a.end = [66., 8.];
            if let Some(texture) =
                crate::line_properties::preview(ui.ctx(), response.id.with("line-style"), &a)
            {
                p.image(
                    texture.id(),
                    egui::Rect::from_center_size(r.center(), egui::vec2(74., 29.6)),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
        }
        Kind::Text => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "Aa",
                egui::FontId::new(
                    22.0,
                    crate::text_layout::family(&style.text_style().unwrap_or_default()),
                ),
                color,
            );
        }
        Kind::Callout => {
            if let Some(texture) = callout_thumbnail(ui.ctx(), style) {
                p.image(
                    texture.id(),
                    egui::Rect::from_center_size(r.center(), egui::vec2(58., 32.)),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            }
        }
        Kind::Redact => {
            p.rect_filled(
                egui::Rect::from_center_size(r.center(), egui::vec2(62., 26.)),
                0.,
                egui::Color32::BLACK,
            );
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "Redact",
                egui::FontId::proportional(13.),
                egui::Color32::from_gray(210),
            );
        }
        _ => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                style.width.to_string(),
                egui::FontId::proportional(16.0),
                ui.visuals().text_color(),
            );
        }
    }
    let short: String = label.chars().take(17).collect();
    p.text(
        rect.center_bottom() - egui::vec2(0.0, 4.0),
        egui::Align2::CENTER_BOTTOM,
        short,
        egui::FontId::proportional(9.0),
        ui.visuals().text_color(),
    );
    if active && !enclosed {
        p.rect_stroke(
            rect,
            3,
            egui::Stroke::new(2., egui::Color32::from_rgb(38, 155, 194)),
            egui::StrokeKind::Inside,
        );
    }
    response.tip(format!(
        "{label}\nClick to apply to the selection and use for new objects."
    ))
}

// Use the production callout renderer, including its triangular tail and rounding.
fn callout_thumbnail(ctx: &egui::Context, style: &ObjectStyle) -> Option<egui::TextureHandle> {
    let key = egui::Id::unique(("callout-style-preview", serde_json::to_string(style).ok()?));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(t);
    }
    let mut a = crate::line_properties::sample(capture_core::line_style::Endpoint::Flat, false);
    a.kind = Kind::Callout;
    a.start = [12., 6.];
    a.end = [104., 46.];
    a.width = style.width.min(5);
    a.color = style.color;
    a.line_style = style.line_style;
    a.text = Some(capture_core::text::TextStyle {
        content: "Text".into(),
        ..Default::default()
    });
    a.corner_radius = if style.corner_radius > 0. { 10. } else { 0. };
    a.shadow = None;
    let (fill, outline) = style.callout_colors()?;
    a.callout = Some(capture_core::callout::CalloutStyle {
        shape: match style.outline_shape {
            Some(capture_core::object_styles::Outline::Callout(shape)) => shape,
            _ => Default::default(),
        },
        tail: Default::default(),
        fill,
        outline,
        pointer: [36., 62.],
        padding: 0.,
    });
    let mut doc = capture_core::Document::canvas(116, 64, [0; 4]).ok()?;
    doc.annotations.push(a);
    let pixels = doc.render().ok()?;
    let t = ctx.load_texture(
        "Callout style",
        egui::ColorImage::from_rgba_unmultiplied([116, 64], pixels.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(key, t.clone()));
    Some(t)
}

// Fixed synthetic content only, shared with the document effect renderer. Cache
// by the bounded intensity so repainting never reruns filtering or uploads.
fn effect_thumbnail(
    ctx: &egui::Context,
    kind: Kind,
    width: u32,
    region: capture_core::obscure::Region,
) -> Option<egui::TextureHandle> {
    let width = width.clamp(1, 32);
    let key = egui::Id::unique((
        "effect-style-thumbnail",
        kind == Kind::Pixelate,
        width,
        region as u8,
    ));
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(texture);
    }
    let image = effect_sample_shaped(kind, width, region).ok()?;
    let texture = ctx.load_texture(
        "Synthetic effect style",
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions::NEAREST,
    );
    ctx.data_mut(|d| d.insert_temp(key, texture.clone()));
    Some(texture)
}

fn effect_sample_shaped(
    kind: Kind,
    width: u32,
    region: capture_core::obscure::Region,
) -> capture_core::Result<image::RgbaImage> {
    let mut document = capture_core::Document::blank(128, 56)?;
    document.annotations.push(crate::Annotation {
        id: 1,
        kind: Kind::Text,
        start: [0.0, 0.0],
        end: [128.0, 56.0],
        width: 1,
        color: [0, 0, 0, 255],
        text: Some(capture_core::text::TextStyle {
            content: "Abc".into(),
            size_points: 34.0,
            horizontal: capture_core::text::HorizontalAlignment::Center,
            vertical: capture_core::text::VerticalAlignment::Middle,
            fit: capture_core::text::TextFit::Shrink,
            ..Default::default()
        }),
        callout: None,
        points: vec![],
        corner_radius: 0.0,
        fill: None,

        focus: None,

        highlighter_rect: false,
        hard_edge: false,
        group: None,
        rotation_degrees: 0.0,
        mirror_x: false,
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
    });
    use capture_core::obscure::Region;
    // Shaped regions sit over crisp bars so the blurred shape itself shows:
    // a circle for Ellipse and a squiggle for Freehand (BT, user review BS16).
    if region != Region::Rectangle {
        document.annotations[0].text = None;
        document.annotations[0].kind = Kind::Rectangle;
        document.annotations[0].fill = Some([255, 255, 255, 255]);
        document.annotations[0].color = [255, 255, 255, 255];
        for i in 0..16 {
            let mut bar = document.annotations[0].clone();
            bar.id = 10 + i;
            let x = 2. + i as f32 * 8.;
            bar.start = [x, 0.];
            bar.end = [x + 4., 56.];
            bar.fill = Some([0, 0, 0, 255]);
            bar.color = [0, 0, 0, 255];
            document.annotations.push(bar);
        }
    }
    let mut effect = document.annotations[0].clone();
    effect.id = 2;
    effect.kind = kind;
    effect.width = width;
    effect.text = None;
    effect.fill = None;
    effect.start = [0., 0.];
    effect.end = [128., 56.];
    effect.points = match region {
        Region::Rectangle => vec![],
        Region::Ellipse => capture_core::obscure::ellipse_region([40., 4.], [88., 52.]),
        Region::Freehand => {
            // A wavy band: the top edge left to right, the bottom edge back.
            let wave = |x: f32| 28. + 13. * (x / 128. * std::f32::consts::TAU * 1.5).sin();
            let xs: Vec<f32> = (0..=24).map(|i| 10. + i as f32 * 4.5).collect();
            xs.iter()
                .map(|x| [*x, wave(*x) - 8.])
                .chain(xs.iter().rev().map(|x| [*x, wave(*x) + 8.]))
                .collect()
        }
    };
    document.annotations.push(effect);
    document.render()
}

/// The dashed "Save current" tile that ends the saved presets (approved option B).
pub(super) fn save_tile(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(82.0, 52.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Save current")
    });
    let hover = response.hovered() && ui.is_enabled();
    let p = ui.painter();
    if hover {
        p.rect_filled(
            rect,
            6.,
            egui::Color32::from_rgba_unmultiplied(24, 145, 193, 20),
        );
    }
    let color = if hover {
        crate::shell::ACCENT
    } else {
        egui::Color32::from_rgb(86, 102, 107)
    };
    let corners = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    for edge in corners.windows(2) {
        p.add(egui::Shape::dashed_line(
            &[edge[0], edge[1]],
            egui::Stroke::new(1., color),
            4.,
            3.,
        ));
    }
    let ink = if ui.is_enabled() {
        egui::Color32::from_rgb(125, 214, 237)
    } else {
        egui::Color32::from_gray(110)
    };
    let c = rect.center() - egui::vec2(0., 8.);
    let plus = egui::Stroke::new(1.6, ink);
    p.line_segment([c - egui::vec2(6., 0.), c + egui::vec2(6., 0.)], plus);
    p.line_segment([c - egui::vec2(0., 6.), c + egui::vec2(0., 6.)], plus);
    p.text(
        rect.center() + egui::vec2(0., 12.),
        egui::Align2::CENTER_CENTER,
        "Save current",
        egui::FontId::proportional(10.5),
        ink,
    );
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("preset-save-tile"), rect));
    response.tip("Save the selected object's formatting as a preset")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shaped_effect_tiles_show_a_blurred_circle_and_squiggle_over_crisp_bars() {
        use capture_core::obscure::Region;
        let crisp = |p: &image::Rgba<u8>| p[0] == 0 || p[0] == 255;
        for kind in [Kind::Blur, Kind::Pixelate] {
            let circle = effect_sample_shaped(kind, 8, Region::Ellipse).unwrap();
            let squiggle = effect_sample_shaped(kind, 8, Region::Freehand).unwrap();
            if let Ok(dir) = std::env::var("CLIPCRAFT_EFFECT_TILES_DIR") {
                circle.save(format!("{dir}/{kind:?}-ellipse.png")).unwrap();
                squiggle
                    .save(format!("{dir}/{kind:?}-freehand.png"))
                    .unwrap();
            }
            // Inside the circle the bars soften; outside it they stay sharp.
            assert!(!crisp(circle.get_pixel(64, 28)), "{kind:?} circle centre");
            assert!(
                crisp(circle.get_pixel(8, 28)),
                "{kind:?} left of the circle"
            );
            assert!(
                crisp(circle.get_pixel(120, 28)),
                "{kind:?} right of the circle"
            );
            // The squiggle dips at x=64 (y 7..23) and rises near x=24.
            assert!(
                !crisp(squiggle.get_pixel(64, 15)),
                "{kind:?} on the squiggle"
            );
            assert!(
                crisp(squiggle.get_pixel(64, 48)),
                "{kind:?} below the squiggle"
            );
            assert!(
                crisp(squiggle.get_pixel(64, 2)),
                "{kind:?} above the squiggle"
            );
        }
    }
    #[test]
    fn callout_previews_render_valid_callouts_and_reuse_textures() {
        let ctx = egui::Context::default();
        for group in capture_core::object_styles::groups(Kind::Callout)
            .into_iter()
            .filter(|g| g.styles[0].kind == Kind::Callout)
        {
            for style in group.styles {
                let t = callout_thumbnail(&ctx, &style)
                    .expect("callout must render, including its tail");
                assert_eq!(t.id(), callout_thumbnail(&ctx, &style).unwrap().id());
            }
        }
    }
    #[test]
    fn synthetic_intensity_previews_are_distinct_cached_and_rendered_from_real_effects() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut all = vec![];
        for kind in [Kind::Blur, Kind::Pixelate] {
            for style in capture_core::object_styles::presets(kind) {
                let region = match style.outline_shape {
                    Some(capture_core::object_styles::Outline::Region(r)) => r,
                    _ => Default::default(),
                };
                let sample = effect_sample_shaped(kind, style.width, region).unwrap();
                assert_eq!(sample.dimensions(), (128, 56));
                assert!(
                    sample
                        .pixels()
                        .all(|p| p[3] == 255 && p[0] == p[1] && p[1] == p[2])
                );
                assert!(!all.contains(&sample));
                all.push(sample);
                let first = effect_thumbnail(&ctx, kind, style.width, region).unwrap();
                let again = effect_thumbnail(&ctx, kind, style.width, region).unwrap();
                assert_eq!(first.id(), again.id());
                assert_eq!(first.size(), [128, 56]);
            }
        }
        let frame = || {
            crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(540.0, 150.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    for kind in [Kind::Blur, Kind::Pixelate] {
                        ui.horizontal(|ui| {
                            for style in capture_core::object_styles::presets(kind) {
                                tile(ui, &style, &style.name, style.width == 8);
                            }
                        });
                    }
                },
            ))
        };
        let first = frame();
        let last = frame();
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_EFFECT_DIR") {
            crate::property_layout::tests::dump_meshes(&ctx, &root, "ObfuscateStyles", first, last);
        }
    }
}
