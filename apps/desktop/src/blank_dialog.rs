//! Shared parts of the New Image and New Video dialogs (approved bk-proposal):
//! size presets as small pictures named for their main use, with the pixels on
//! hover, and a small preview shaped like the chosen size.
use crate::egui;
use crate::tips::Tip as _;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    Slide,
    Phone,
    Post,
    Thumbnail,
    Page,
}
pub(crate) struct Preset {
    pub name: &'static str,
    pub size: [u32; 2],
    pub hint: &'static str,
    pub shape: Shape,
}
pub(crate) const IMAGE_PRESETS: [Preset; 5] = [
    Preset {
        name: "Slide",
        size: [1920, 1080],
        hint: "16:9",
        shape: Shape::Slide,
    },
    Preset {
        name: "Phone",
        size: [1080, 1920],
        hint: "9:16",
        shape: Shape::Phone,
    },
    Preset {
        name: "Post",
        size: [1080, 1080],
        hint: "1:1",
        shape: Shape::Post,
    },
    Preset {
        name: "Thumbnail",
        size: [1280, 720],
        hint: "16:9",
        shape: Shape::Thumbnail,
    },
    Preset {
        name: "Page",
        size: [1275, 1650],
        hint: "Letter at 150 ppi",
        shape: Shape::Page,
    },
];
pub(crate) const VIDEO_PRESETS: [Preset; 4] = [
    Preset {
        name: "Screen",
        size: [1920, 1080],
        hint: "16:9",
        shape: Shape::Slide,
    },
    Preset {
        name: "Phone",
        size: [1080, 1920],
        hint: "9:16",
        shape: Shape::Phone,
    },
    Preset {
        name: "Post",
        size: [1080, 1080],
        hint: "1:1",
        shape: Shape::Post,
    },
    Preset {
        name: "Web",
        size: [1280, 720],
        hint: "16:9, smaller files",
        shape: Shape::Thumbnail,
    },
];

fn draw_shape(p: &egui::Painter, center: egui::Pos2, shape: Shape, ink: egui::Color32) {
    let stroke = egui::Stroke::new(1.5, ink);
    let soft = ink.gamma_multiply(0.55);
    let frame = |w: f32, h: f32, radius: f32| {
        let r = egui::Rect::from_center_size(center, egui::vec2(w, h));
        p.rect_stroke(r, radius, stroke, egui::StrokeKind::Inside);
        r
    };
    match shape {
        Shape::Slide => {
            let r = frame(30., 17., 2.);
            for (y, w) in [(4.5, 15.), (9., 10.)] {
                p.line_segment(
                    [r.min + egui::vec2(4., y), r.min + egui::vec2(4. + w, y)],
                    egui::Stroke::new(1.5, soft),
                );
            }
        }
        Shape::Phone => {
            let r = frame(14., 25., 3.);
            p.line_segment(
                [
                    r.center_top() + egui::vec2(-2., 3.),
                    r.center_top() + egui::vec2(2., 3.),
                ],
                egui::Stroke::new(1.5, soft),
            );
        }
        Shape::Post => {
            let r = frame(21., 21., 2.);
            p.add(egui::Shape::line(
                vec![
                    r.left_bottom() + egui::vec2(3., -3.),
                    r.left_bottom() + egui::vec2(8., -10.),
                    r.left_bottom() + egui::vec2(12., -6.),
                    r.left_bottom() + egui::vec2(15., -9.),
                    r.left_bottom() + egui::vec2(18., -3.),
                ],
                egui::Stroke::new(1.3, soft),
            ));
        }
        Shape::Thumbnail => {
            frame(26., 15., 2.);
            p.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-3., -4.),
                    center + egui::vec2(4., 0.),
                    center + egui::vec2(-3., 4.),
                ],
                soft,
                egui::Stroke::NONE,
            ));
        }
        Shape::Page => {
            let r = egui::Rect::from_center_size(center, egui::vec2(18., 24.));
            let corner = 5.;
            p.add(egui::Shape::closed_line(
                vec![
                    r.min,
                    r.right_top() - egui::vec2(corner, 0.),
                    r.right_top() + egui::vec2(0., corner),
                    r.max,
                    r.left_bottom(),
                ],
                stroke,
            ));
            for y in [9., 13., 17.] {
                p.line_segment(
                    [r.min + egui::vec2(4., y), r.min + egui::vec2(14., y)],
                    egui::Stroke::new(1.2, soft),
                );
            }
        }
    }
}

/// The preset row. Returns the size of the preset just clicked.
pub(crate) fn presets(ui: &mut egui::Ui, list: &[Preset], current: [u32; 2]) -> Option<[u32; 2]> {
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.;
        for preset in list {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(66., 56.), egui::Sense::click());
            let on = preset.size == current;
            let p = ui.painter();
            p.rect_filled(
                rect,
                7.,
                if on {
                    egui::Color32::from_rgb(35, 65, 74)
                } else if response.hovered() {
                    egui::Color32::from_rgb(52, 61, 64)
                } else {
                    egui::Color32::from_rgb(38, 45, 48)
                },
            );
            if on {
                p.rect_stroke(
                    rect,
                    7.,
                    egui::Stroke::new(1., crate::shell::ACCENT),
                    egui::StrokeKind::Inside,
                );
            }
            let ink = if on {
                egui::Color32::from_rgb(125, 214, 237)
            } else {
                egui::Color32::from_rgb(207, 230, 238)
            };
            draw_shape(
                p,
                rect.center_top() + egui::vec2(0., 20.),
                preset.shape,
                ink,
            );
            p.text(
                rect.center_bottom() - egui::vec2(0., 9.),
                egui::Align2::CENTER_CENTER,
                preset.name,
                egui::FontId::proportional(10.5),
                ui.visuals().text_color(),
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, preset.name)
            });
            let response = response.tip(format!(
                "{} × {} px · {}",
                preset.size[0], preset.size[1], preset.hint
            ));
            if response.clicked() {
                picked = Some(preset.size);
            }
        }
    });
    picked
}

/// A small picture of the new canvas: its shape, and color or checkerboard.
pub(crate) fn preview(ui: &mut egui::Ui, size: Option<[u32; 2]>, color: Option<[u8; 4]>) {
    let (area, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 74.), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(area, 6., egui::Color32::from_rgb(21, 26, 28));
    let Some([w, h]) = size else {
        return;
    };
    let scale = (112. / w as f32).min(63. / h as f32);
    let rect = egui::Rect::from_center_size(
        area.center(),
        egui::vec2(w as f32 * scale, h as f32 * scale),
    );
    match color {
        Some(c) => {
            p.rect_filled(
                rect,
                0.,
                egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]),
            );
        }
        None => {
            p.rect_filled(rect, 0., egui::Color32::from_gray(242));
            let clip = p.with_clip_rect(rect);
            let cell = 5.;
            let (cols, rows) = (
                (rect.width() / cell).ceil() as i32,
                (rect.height() / cell).ceil() as i32,
            );
            for y in 0..rows {
                for x in 0..cols {
                    if (x + y) % 2 == 0 {
                        clip.rect_filled(
                            egui::Rect::from_min_size(
                                rect.min + egui::vec2(x as f32 * cell, y as f32 * cell),
                                egui::vec2(cell, cell),
                            ),
                            0.,
                            egui::Color32::from_gray(207),
                        );
                    }
                }
            }
        }
    }
    p.rect_stroke(
        rect,
        0.,
        egui::Stroke::new(1., egui::Color32::from_rgb(86, 102, 107)),
        egui::StrokeKind::Outside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_preset_is_a_valid_canvas_and_video_size() {
        for p in IMAGE_PRESETS.iter().chain(&VIDEO_PRESETS) {
            capture_core::validate_dimensions(p.size[0], p.size[1]).unwrap();
        }
        for p in &VIDEO_PRESETS {
            let mut doc = capture_core::Document::blank(p.size[0], p.size[1]).unwrap();
            let project =
                capture_core::video::editor::Project::new("synthetic.mp4".into(), p.size, 4000, 30);
            doc.video = Some(project.clone());
            project.validate(&doc).unwrap();
        }
    }
    #[test]
    fn clicking_a_tile_picks_its_size() {
        let ctx = egui::Context::default();
        let mut picked = None;
        let mut run = |events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 120.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    if let Some(size) = presets(ui, &IMAGE_PRESETS, [1920, 1080]) {
                        picked = Some(size);
                    }
                },
            );
            output.textures_delta.clear();
        };
        run(vec![]);
        run(vec![]);
        // The second tile: Phone.
        let at = egui::pos2(49. + 4. + 24., 27.);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        run(vec![egui::Event::PointerMoved(at), press(true)]);
        run(vec![press(false)]);
        assert_eq!(picked, Some([1080, 1920]));
    }
}
