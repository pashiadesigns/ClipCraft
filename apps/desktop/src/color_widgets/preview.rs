//! Semantic previews share the actual text renderer and live RGBA picker.
use crate::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Preview {
    Fill,
    Outline,
    Text,
    Stroke,
}
impl Preview {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Fill => "Fill color",
            Self::Outline => "Outline color",
            Self::Text => "Text color",
            Self::Stroke => "Text stroke color",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Layout {
    Property,
    Inline,
    Canvas,
}

pub(super) fn paint(
    ui: &egui::Ui,
    response: &egui::Response,
    color: [u8; 4],
    preview: Preview,
    layout: Layout,
    open: bool,
) {
    let rect = response.rect;
    let visuals = ui.style().interact(response);
    let p = ui.painter();
    p.rect_filled(
        rect,
        4,
        if response.hovered() || response.is_pointer_button_down_on() {
            visuals.bg_fill
        } else {
            ui.visuals().window_fill
        },
    );
    p.rect_stroke(
        rect,
        4,
        egui::Stroke::new(
            if open { 2. } else { 1. },
            if open {
                crate::shell::ACCENT
            } else {
                if response.hovered() || response.has_focus() {
                    visuals.fg_stroke.color
                } else {
                    egui::Color32::from_gray(104)
                }
            },
        ),
        egui::StrokeKind::Inside,
    );
    let (art, arrow) = if layout == Layout::Property {
        (
            egui::Rect::from_center_size(
                rect.center_top() + egui::vec2(0., 25.),
                egui::vec2(46., 42.),
            ),
            rect.center_bottom() - egui::vec2(0., 8.),
        )
    } else {
        (
            egui::Rect::from_min_max(
                rect.min + egui::vec2(3., 3.),
                rect.max - egui::vec2(13., 3.),
            ),
            rect.right_center() - egui::vec2(7., 0.),
        )
    };
    let backdrop = Backdrop::for_color(color);
    if preview != Preview::Text || color[3] < 255 || backdrop != Backdrop::Neutral {
        checker(p, art, backdrop);
    }
    let ink = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    match preview {
        Preview::Fill | Preview::Outline => {
            let side = 30f32.min(art.width()).min(art.height());
            let square = egui::Rect::from_center_size(art.center(), egui::vec2(side, side));
            if preview == Preview::Fill {
                p.rect_filled(square, 0, ink);
            } else {
                p.rect_stroke(
                    square,
                    0,
                    egui::Stroke::new(side / 6., ink),
                    egui::StrokeKind::Inside,
                );
            }
        }
        Preview::Text | Preview::Stroke => {
            if let Some(texture) = text_texture(ui.ctx(), preview) {
                let scale = (art.width() / 46.).min(art.height() / 28.);
                let target =
                    egui::Rect::from_center_size(art.center(), egui::vec2(46., 28.) * scale);
                p.image(
                    texture.id(),
                    target,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    ink,
                );
            }
        }
    }
    p.add(egui::Shape::line(
        vec![
            arrow + egui::vec2(-3., -1.5),
            arrow + egui::vec2(0., 1.5),
            arrow + egui::vec2(3., -1.5),
        ],
        egui::Stroke::new(1.3, visuals.fg_stroke.color),
    ));
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("color-preview", preview.label())),
            (response.id, rect, art, color, open),
        )
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backdrop {
    Light,
    Dark,
    Neutral,
}
impl Backdrop {
    fn for_color(color: [u8; 4]) -> Self {
        if color[3] == 0 {
            return Self::Neutral;
        }
        let linear = |v: u8| {
            let v = f32::from(v) / 255.;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let luminance =
            0.2126 * linear(color[0]) + 0.7152 * linear(color[1]) + 0.0722 * linear(color[2]);
        if luminance < 0.12 {
            Self::Light
        } else if luminance > 0.60 {
            Self::Dark
        } else {
            Self::Neutral
        }
    }
    fn colors(self) -> [egui::Color32; 2] {
        match self {
            Self::Light => [egui::Color32::WHITE, egui::Color32::from_gray(232)],
            Self::Dark => [crate::shell::CANVAS, egui::Color32::from_rgb(30, 34, 36)],
            Self::Neutral => [egui::Color32::from_gray(110), egui::Color32::from_gray(79)],
        }
    }
}

fn checker(p: &egui::Painter, rect: egui::Rect, backdrop: Backdrop) {
    let colors = backdrop.colors();
    let cell = 5.;
    for y in 0..(rect.height() / cell).ceil() as i32 {
        for x in 0..(rect.width() / cell).ceil() as i32 {
            let tile = egui::Rect::from_min_size(
                rect.min + egui::vec2(x as f32 * cell, y as f32 * cell),
                egui::vec2(cell, cell),
            )
            .intersect(rect);
            p.rect_filled(tile, 0, colors[((x + y) % 2) as usize]);
        }
    }
}

fn text_texture(ctx: &egui::Context, preview: Preview) -> Option<egui::TextureHandle> {
    // White masks are cached independently of color/alpha, so dragging a hue
    // does not allocate textures or rasterize glyphs on each frame.
    let scale = (ctx.pixels_per_point().ceil() as u32).clamp(2, 4);
    let key = egui::Id::unique(("color-preview-text-mask", preview, scale));
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(texture);
    }
    let image = text_mask(preview, scale).ok()?;
    let texture = ctx.load_texture(
        "Color preview Abc",
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(key, texture.clone()));
    Some(texture)
}

fn text_mask(preview: Preview, scale: u32) -> capture_core::Result<image::RgbaImage> {
    use capture_core::{
        Annotation, Kind,
        text::{TextFit, TextStyle, outline::Outline},
    };
    let stroke = preview == Preview::Stroke;
    let annotation = Annotation {
        id: 1,
        kind: Kind::Text,
        start: [0., 0.],
        end: [46., 28.],
        color: [255, 255, 255, if stroke { 0 } else { 255 }],
        width: 1,
        text: Some(TextStyle {
            content: "Abc".into(),
            size_points: 16.5,
            bold: true,
            fit: TextFit::None,
            wrap: false,
            outline: stroke.then_some(Outline {
                color: [255; 4],
                width: 1.35,
            }),
            ..Default::default()
        }),
        corner_radius: 0.,
        fill: None,
        rotation_degrees: 0.,
        callout: None,
        points: vec![],
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,
        analysis: None,
        highlighter_rect: false,
        group: None,
    };
    capture_core::display::render_annotation_preview(
        &annotation,
        capture_core::display::Viewport {
            size: [46 * scale, 28 * scale],
            origin: [0., 0.],
            scale: scale.into(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn text_stroke_preview_has_true_hollow_interiors_and_cached_antialiasing() {
        for scale in [2, 3, 4] {
            let fill = text_mask(Preview::Text, scale).unwrap();
            let stroke = text_mask(Preview::Stroke, scale).unwrap();
            assert!(fill.pixels().filter(|p| p[3] == 255).count() > 40);
            assert!(stroke.pixels().any(|p| p[3] > 0 && p[3] < 255));
            for (a, b) in fill.pixels().zip(stroke.pixels()) {
                if a[3] == 255 {
                    assert_eq!(b[3], 0, "opaque letter interiors must stay transparent");
                }
            }
        }
        let ctx = egui::Context::default();
        assert_eq!(
            text_texture(&ctx, Preview::Stroke).unwrap().id(),
            text_texture(&ctx, Preview::Stroke).unwrap().id()
        );
    }

    #[test]
    fn production_color_previews_fit_sidebars_update_live_and_keep_palette_controls() {
        use super::super::{ColorDraft, property_rgba};
        for scale in [1., 2.] {
            for (variant, mut colors) in [
                (
                    "colors",
                    [
                        [148, 201, 231, 255],
                        [120, 202, 92, 255],
                        [88, 184, 235, 255],
                        [241, 196, 73, 255],
                    ],
                ),
                ("black", [[0, 0, 0, 255]; 4]),
                ("white", [[255, 255, 255, 255]; 4]),
                (
                    "alpha",
                    [
                        [0, 0, 0, 96],
                        [255, 255, 255, 96],
                        [0, 0, 0, 96],
                        [255, 255, 255, 96],
                    ],
                ),
                ("transparent", [[223, 44, 125, 0]; 4]),
            ] {
                // Each snapshot includes its initial texture uploads.
                let ctx = egui::Context::default();
                crate::shell::configure(&ctx);
                ctx.set_pixels_per_point(scale);
                ctx.all_styles_mut(|s| s.animation_time = 0.);
                let original = colors;
                let frame = |colors: &mut [[u8; 4]; 4]| {
                    PaintedOutput(ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(262., 630.),
                            )),
                            focused: true,
                            ..Default::default()
                        },
                        |ui| {
                            ui.heading("Tool Properties");
                            ui.horizontal(|ui| {
                                for (color, role) in colors.iter_mut().zip([
                                    Preview::Fill,
                                    Preview::Outline,
                                    Preview::Text,
                                    Preview::Stroke,
                                ]) {
                                    ui.vertical(|ui| {
                                        ui.set_width(56.);
                                        ui.with_layout(
                                            egui::Layout::top_down(egui::Align::Center),
                                            |ui| {
                                                ui.label(match role {
                                                    Preview::Fill => "Fill",
                                                    Preview::Outline => "Outline",
                                                    Preview::Text => "Text",
                                                    Preview::Stroke => "Stroke",
                                                });
                                                property_rgba(ui, color, role);
                                            },
                                        );
                                    });
                                }
                            });
                        },
                    ))
                };
                let first = frame(&mut colors);
                let second = frame(&mut colors);
                if let Ok(dir) = std::env::var("CLIPCRAFT_COLOR_PREVIEW_DIR") {
                    dump_meshes(
                        &ctx,
                        &dir,
                        &format!("selectors-{variant}-{scale}"),
                        first,
                        second,
                    );
                }
                assert_eq!(
                    colors, original,
                    "contrast backgrounds must never change RGBA"
                );
                for color in colors {
                    let background = Backdrop::for_color(color).colors()[0];
                    if color[3] == 0 {
                        assert_eq!(Backdrop::for_color(color), Backdrop::Neutral);
                    } else if color[..3] == [0, 0, 0] {
                        assert!(
                            background.r() >= 240 && background.g() >= 240 && background.b() >= 240
                        );
                    } else if color[..3] == [255, 255, 255] {
                        assert!(background.r() < 40 && background.g() < 40 && background.b() < 40);
                    }
                }
                let mut rects = Vec::new();
                for role in [
                    Preview::Fill,
                    Preview::Outline,
                    Preview::Text,
                    Preview::Stroke,
                ] {
                    let (_, rect, _, _, _) = ctx.data(|d| {
                        d.get_temp::<(egui::Id, egui::Rect, egui::Rect, [u8; 4], bool)>(
                            egui::Id::unique(("color-preview", role.label())),
                        )
                        .unwrap()
                    });
                    assert!(rect.right() <= 262.);
                    assert_eq!(rect.size(), egui::vec2(56., 64.));
                    rects.push(rect);
                }
                assert!(
                    rects
                        .windows(2)
                        .all(|r| r[0].right() < r[1].left() && r[0].top() == r[1].top())
                );
                let (id, _, _, _, _) = ctx.data(|d| {
                    d.get_temp::<(egui::Id, egui::Rect, egui::Rect, [u8; 4], bool)>(
                        egui::Id::unique(("color-preview", Preview::Stroke.label())),
                    )
                    .unwrap()
                });
                let chosen = [223, 44, 125, 96];
                let mut draft = ColorDraft::new(colors[3]);
                draft.set_color(chosen);
                ctx.data_mut(|d| d.insert_temp(id.with("rgba-draft"), draft));
                egui::Popup::open_id(&ctx, id.with("rgba-popup"));
                let _output = frame(&mut colors);
                assert_eq!(colors[3], chosen);
                let (_, _, _, painted, open) = ctx.data(|d| {
                    d.get_temp::<(egui::Id, egui::Rect, egui::Rect, [u8; 4], bool)>(
                        egui::Id::unique(("color-preview", Preview::Stroke.label())),
                    )
                    .unwrap()
                });
                assert_eq!(painted, chosen);
                assert!(open, "active border follows real popup");
            }
        }
    }
}
