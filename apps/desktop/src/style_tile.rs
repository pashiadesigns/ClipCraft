//! Compact original vector previews shared by templates and category dropdowns.
use crate::{Kind, egui};
use capture_core::object_styles::ObjectStyle;

pub(super) fn tile(
    ui: &mut egui::Ui,
    style: &ObjectStyle,
    label: &str,
    active: bool,
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
    if active {
        ui.painter().rect_stroke(
            rect,
            3,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(38, 155, 194)),
            egui::StrokeKind::Inside,
        );
    }
    let c = |v: [u8; 4]| egui::Color32::from_rgba_unmultiplied(v[0], v[1], v[2], v[3]);
    let color = c(style.color).gamma_multiply(if ui.is_enabled() { 1.0 } else { 0.4 });
    let r =
        egui::Rect::from_center_size(rect.center() - egui::vec2(0.0, 8.0), egui::vec2(34.0, 24.0));
    let stroke = egui::Stroke::new((style.width as f32 * 0.55).clamp(1.0, 4.5), color);
    let fill = style.fill.map(c).unwrap_or(egui::Color32::TRANSPARENT);
    let p = ui.painter();
    match style.kind {
        Kind::Simplify => {
            p.rect_filled(
                r,
                if style.corner_radius > 0. { 6 } else { 0 },
                c(style.color),
            );
        }
        Kind::Blur | Kind::Pixelate => {
            if let Some(texture) = effect_thumbnail(ui.ctx(), style.kind, style.width) {
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
            p.add(egui::Shape::convex_polygon(
                vec![
                    r.center_top(),
                    r.right_center(),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_center(),
                ],
                fill,
                stroke,
            ));
        }
        Kind::Highlighter if style.highlighter_rect => {
            p.rect_filled(r.shrink2(egui::vec2(0., 6.)), 0, color);
        }
        Kind::Pen | Kind::Highlighter => {
            p.line_segment([r.left_bottom(), r.right_top()], stroke);
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
            let (body, outline) = style.callout_colors().unwrap_or(([255; 4], style.color));
            let stroke = egui::Stroke::new(stroke.width, c(outline));
            p.rect(
                r.shrink2(egui::vec2(0.0, 3.0)),
                if style.corner_radius > 0.0 { 5 } else { 0 },
                c(body),
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment(
                [
                    r.center_bottom() - egui::vec2(0.0, 3.0),
                    r.left_bottom() + egui::vec2(7.0, 3.0),
                ],
                stroke,
            );
        }
        _ => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                if style.kind == Kind::Redact {
                    "Cover".into()
                } else {
                    style.width.to_string()
                },
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
    response.on_hover_text(format!(
        "{label}\nClick to apply to the selection and use for new objects."
    ))
}

// Fixed synthetic content only, shared with the document effect renderer. Cache
// by the bounded intensity so repainting never reruns filtering or uploads.
fn effect_thumbnail(ctx: &egui::Context, kind: Kind, width: u32) -> Option<egui::TextureHandle> {
    let width = width.clamp(1, 32);
    let key = egui::Id::unique(("effect-style-thumbnail", kind == Kind::Pixelate, width));
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(texture);
    }
    let image = effect_sample(kind, width).ok()?;
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

fn effect_sample(kind: Kind, width: u32) -> capture_core::Result<image::RgbaImage> {
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
        analysis: None,
        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
    });
    let mut effect = document.annotations[0].clone();
    effect.id = 2;
    effect.kind = kind;
    effect.width = width;
    effect.text = None;
    document.annotations.push(effect);
    document.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_intensity_previews_are_distinct_cached_and_rendered_from_real_effects() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut all = vec![];
        for kind in [Kind::Blur, Kind::Pixelate] {
            for style in capture_core::object_styles::presets(kind) {
                let sample = effect_sample(kind, style.width).unwrap();
                assert_eq!(sample.dimensions(), (128, 56));
                assert!(
                    sample
                        .pixels()
                        .all(|p| p[3] == 255 && p[0] == p[1] && p[1] == p[2])
                );
                assert!(!all.contains(&sample));
                all.push(sample);
                let first = effect_thumbnail(&ctx, kind, style.width).unwrap();
                let again = effect_thumbnail(&ctx, kind, style.width).unwrap();
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
