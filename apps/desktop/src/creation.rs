//! One annotation drives creation preview and release. No document mutation
//! or Undo entry occurs until the pointer is released.
use crate::canvas_view::Transform;
use crate::{Annotation, Editor, Kind, egui};
use capture_core::display::Viewport;

#[derive(Default)]
pub(super) struct Creation {
    pub annotation: Option<Annotation>,
    pub simplify_candidates: Option<Vec<Annotation>>,
    pub simplified: Vec<Annotation>,
    box_end: Option<[f32; 2]>,
    tail: Option<[f32; 2]>,
    texture: Option<egui::TextureHandle>,
    key: Option<(Annotation, Viewport)>,
}
// Bound uploads to the backend limit and a modest interactive pixel budget.
// The screen rectangle stays exact; very large previews temporarily use a
// lower-resolution texture. The committed document/export never does.
fn preview_resolution(size: egui::Vec2, max_side: usize) -> (egui::Vec2, f32) {
    let side = max_side.clamp(1, 8192) as f32;
    let factor = (side / size.x.max(size.y))
        .min((4_194_304.0 / (size.x * size.y)).sqrt())
        .min(1.0);
    ((size * factor).ceil().max(egui::Vec2::splat(1.0)), factor)
}
impl Editor {
    pub(super) fn new_annotation(&self, kind: Kind, start: [f32; 2], end: [f32; 2]) -> Annotation {
        let rectangle = kind == Kind::Highlighter && self.drawing_rectangle_highlighter();
        let points = if !rectangle && matches!(kind, Kind::Pen | Kind::Highlighter | Kind::Eraser) {
            self.stroke_points.clone()
        } else if kind == Kind::Polygon {
            capture_core::polygon::outline(start, end, self.shape_sides)
        } else {
            vec![]
        };
        let mut annotation = self.styled_new_annotation(Annotation {
            id: self.session.next_id(),
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: rectangle,
            group: None,
            rotation_degrees: 0.0,
            kind,
            start,
            end,
            color: if kind == Kind::Callout {
                [35, 35, 35, 255]
            } else {
                [
                    self.color[0],
                    self.color[1],
                    self.color[2],
                    self.quick_styles.alpha,
                ]
            },
            width: self.width,
            points,
            corner_radius: 0.0,
            fill: None,
            text: matches!(kind, Kind::Text | Kind::Callout).then(|| {
                capture_core::text::TextStyle {
                    content: String::new(),
                    ..Default::default()
                }
            }),
            callout: (kind == Kind::Callout).then(|| {
                capture_core::callout::CalloutStyle::for_box(
                    start,
                    end,
                    [self.session.document.width, self.session.document.height],
                )
            }),
        });
        if annotation.highlighter_rect {
            annotation.points.clear();
        }
        annotation
    }
    pub(super) fn update_creation(
        &mut self,
        start: [f32; 2],
        pointer: [f32; 2],
        kind: Kind,
        shift: bool,
        alt: bool,
    ) {
        let canvas = [self.session.document.width, self.session.document.height];
        let mut end = capture_core::angle_snap::endpoint(
            start,
            pointer,
            canvas,
            matches!(kind, Kind::Line | Kind::Arrow) && shift,
        );
        if shift
            && (self.drawing_rectangle_highlighter() && kind == Kind::Highlighter
                || matches!(
                    kind,
                    Kind::Rectangle
                        | Kind::Ellipse
                        | Kind::Polygon
                        | Kind::Callout
                        | Kind::Simplify
                ))
        {
            end = capture_core::angle_snap::square_endpoint(start, pointer, canvas);
        }
        if kind == Kind::Callout {
            if alt {
                end = self.creation.box_end.unwrap_or(end);
                self.creation.tail = Some([
                    pointer[0].clamp(0.0, canvas[0] as f32),
                    pointer[1].clamp(0.0, canvas[1] as f32),
                ]);
            } else {
                self.creation.box_end = Some(end);
            }
        }
        let mut annotation = self.new_annotation(kind, start, end);
        if let (Some(callout), Some(tail)) = (&mut annotation.callout, self.creation.tail) {
            callout.pointer = tail;
        }
        if kind == Kind::Simplify && self.simplify_auto {
            if self.creation.simplify_candidates.is_none() {
                self.creation.simplify_candidates = Some(self.analyze_simplify());
            }
            let (lo, hi) = annotation.body_bounds();
            self.creation.simplified = self
                .creation
                .simplify_candidates
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(|a| {
                    let mut a = a.clone();
                    for i in 0..2 {
                        a.start[i] = a.start[i].max(lo[i]);
                        a.end[i] = a.end[i].min(hi[i]);
                    }
                    ((a.end[0] - a.start[0]) > 1. && (a.end[1] - a.start[1]) > 1.).then_some(a)
                })
                .collect();
        }
        self.creation.annotation = Some(annotation);
    }
    pub(super) fn paint_creation(&mut self, ui: &egui::Ui, transform: Transform) {
        let Some(annotation) = self.creation.annotation.as_ref() else {
            return;
        };
        let p = ui
            .painter()
            .with_clip_rect(transform.viewport.intersect(transform.image));
        let (min, max) = annotation.bounds();
        let bounds =
            egui::Rect::from_min_max(transform.screen_point(min), transform.screen_point(max));
        if self.creation.simplify_candidates.is_some() {
            for block in &self.creation.simplified {
                p.rect_filled(
                    egui::Rect::from_min_max(
                        transform.screen_point(block.start),
                        transform.screen_point(block.end),
                    ),
                    0.,
                    egui::Color32::from_rgba_unmultiplied(
                        block.color[0],
                        block.color[1],
                        block.color[2],
                        block.color[3],
                    ),
                );
            }
        }
        if self.creation.simplify_candidates.is_some()
            || matches!(annotation.kind, Kind::Text | Kind::Blur | Kind::Pixelate)
        {
            // A neutral guide is editor UI, never a promised formatted border.
            let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 191, 224));
            for edge in [
                [bounds.left_top(), bounds.right_top()],
                [bounds.right_top(), bounds.right_bottom()],
                [bounds.right_bottom(), bounds.left_bottom()],
                [bounds.left_bottom(), bounds.left_top()],
            ] {
                p.extend(egui::Shape::dashed_line(&edge, stroke, 4.0, 3.0));
            }
            return;
        }
        if annotation.kind == Kind::Eraser {
            return;
        }
        let visible = bounds
            .expand(32.0 * transform.scale)
            .intersect(transform.viewport)
            .intersect(transform.image);
        if !visible.is_positive() {
            return;
        }
        let ppp = ui.ctx().pixels_per_point();
        let low = (visible.min.to_vec2() * ppp).floor().to_pos2() / ppp;
        let size = ((visible.max.to_vec2() * ppp).ceil() - low.to_vec2() * ppp)
            .max(egui::Vec2::splat(1.0));
        let (raster_size, quality) = preview_resolution(size, ui.input(|i| i.max_texture_side));
        let key = Viewport {
            size: [raster_size.x as u32, raster_size.y as u32],
            origin: transform.image_point(low).map(f64::from),
            scale: f64::from(transform.scale * ppp * quality),
        };
        if !self
            .creation
            .key
            .as_ref()
            .is_some_and(|(a, k)| a == annotation && *k == key)
        {
            if let Ok(image) = capture_core::display::render_annotation_preview(annotation, key) {
                let pixels = egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                );
                if let Some(texture) = self.creation.texture.as_mut() {
                    texture.set(pixels, egui::TextureOptions::LINEAR);
                } else {
                    self.creation.texture = Some(ui.ctx().load_texture(
                        "creation-preview",
                        pixels,
                        egui::TextureOptions::LINEAR,
                    ));
                }
                self.creation.key = Some((annotation.clone(), key));
            } else {
                return;
            }
        }
        if let Some(texture) = &self.creation.texture {
            p.image(
                texture.id(),
                egui::Rect::from_min_size(low, raster_size / (ppp * quality)),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headless_creation_fixtures_show_final_style_and_neutral_text_guide() {
        use crate::property_layout::tests::{PaintedOutput, dump_meshes};
        for (kind, filled, name) in [
            (Kind::Rectangle, true, "RoundedFilled"),
            (Kind::Rectangle, false, "RoundedOutline"),
            (Kind::Ellipse, true, "Ellipse"),
            (Kind::Callout, false, "CalloutTail"),
            (Kind::Text, false, "TextGuide"),
        ] {
            let ctx = egui::Context::default();
            let mut e = crate::canvas::tests::editor(&ctx);
            let mut a = e.new_annotation(kind, [60.0, 50.0], [350.0, 200.0]);
            a.color = [30, 100, 180, 255];
            a.width = 8;
            a.fill = filled.then_some([60, 190, 110, 180]);
            if matches!(kind, Kind::Rectangle | Kind::Callout) {
                a.corner_radius = 30.0;
            }
            if let Some(c) = &mut a.callout {
                c.pointer = [450.0, 260.0];
            }
            e.creation.annotation = Some(a);
            let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 320.0));
            let transform = Transform {
                image: area,
                viewport: area,
                scale: 1.0,
                zoom: 1.0,
            };
            let mut render = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(area),
                        ..Default::default()
                    },
                    |ui| {
                        ui.painter().rect_filled(area, 0, egui::Color32::WHITE);
                        e.paint_creation(ui, transform);
                    },
                ))
            };
            let first = render();
            let output = render();
            if kind == Kind::Text {
                assert!(
                    !output
                        .shapes
                        .iter()
                        .any(|s| matches!(s.shape, egui::Shape::Mesh(_)))
                );
                assert!(
                    output
                        .shapes
                        .iter()
                        .filter(|s| matches!(s.shape, egui::Shape::LineSegment { .. }))
                        .count()
                        > 20
                );
            } else {
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|s| matches!(s.shape, egui::Shape::Mesh(_)))
                );
            }
            if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_CREATION_DIR") {
                dump_meshes(&ctx, &root, name, first, output);
            }
        }
    }
    #[test]
    fn preview_uploads_fit_backend_limits_without_reducing_normal_sizes() {
        let ordinary = egui::vec2(1200.0, 900.0);
        assert_eq!(preview_resolution(ordinary, 8192), (ordinary, 1.0));
        for size in [egui::vec2(20000.0, 1.0), egui::vec2(8000.0, 8000.0)] {
            for limit in [1024, 2048, 8192] {
                let (raster, quality) = preview_resolution(size, limit);
                assert!(raster.x <= limit as f32 && raster.y <= limit as f32);
                assert!(raster.x * raster.y <= 4_194_304.0 + 4096.0);
                assert!((0.0..=1.0).contains(&quality));
            }
        }
    }
}
