//! Shared video controls: fixed-height actions and crisp, font-independent marks.
use super::*;
use egui::{Color32, Rect, Stroke};

pub(crate) fn icon(p: &egui::Painter, rect: Rect, name: &str, color: Color32) {
    if name == "Sensitive search" {
        let at =
            |x: f32, y: f32| rect.min + egui::vec2(x * rect.width() / 24., y * rect.height() / 24.);
        let stroke = Stroke::new(1.5, color);
        p.circle_stroke(at(10., 10.), rect.width() * 0.34, stroke);
        p.line_segment([at(16., 16.), at(22., 22.)], stroke);
        for x in [6., 10., 14.] {
            for angle in [
                0f32,
                std::f32::consts::PI / 3.,
                std::f32::consts::PI * 2. / 3.,
            ] {
                let delta = egui::vec2(angle.cos(), angle.sin()) * rect.width() / 15.;
                p.line_segment(
                    [at(x, 10.) - delta, at(x, 10.) + delta],
                    Stroke::new(0.9, color),
                );
            }
        }
        return;
    }
    if matches!(
        name,
        "Move up" | "Move down" | "Send to top" | "Send to bottom"
    ) {
        let up = matches!(name, "Move up" | "Send to top");
        let at = |x: f32, y: f32| {
            rect.min
                + egui::vec2(
                    x * rect.width() / 24.,
                    (if up { y } else { 24. - y }) * rect.height() / 24.,
                )
        };
        let stroke = Stroke::new(1.5, color);
        p.line_segment([at(12., 20.), at(12., 6.)], stroke);
        p.add(egui::Shape::line(
            vec![at(6., 12.), at(12., 6.), at(18., 12.)],
            stroke,
        ));
        if name.starts_with("Send") {
            p.line_segment([at(4., 3.), at(20., 3.)], stroke);
        }
        return;
    }
    if name == "Copy" || name == "Lock" {
        let at =
            |x: f32, y: f32| rect.min + egui::vec2(x * rect.width() / 24., y * rect.height() / 24.);
        let stroke = Stroke::new(1.4, color);
        if name == "Copy" {
            p.rect_stroke(
                Rect::from_min_max(at(3., 3.), at(15., 16.)),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(
                Rect::from_min_max(at(8., 8.), at(21., 22.)),
                2.,
                Color32::from_rgb(42, 47, 50),
            );
            p.rect_stroke(
                Rect::from_min_max(at(8., 8.), at(21., 22.)),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
        } else {
            p.add(egui::Shape::line(
                (0..=16)
                    .map(|i| {
                        let angle = std::f32::consts::PI + i as f32 / 16. * std::f32::consts::PI;
                        at(12. + 5. * angle.cos(), 11. + 6. * angle.sin())
                    })
                    .collect(),
                stroke,
            ));
            p.rect_stroke(
                Rect::from_min_max(at(4., 10.), at(20., 22.)),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        return;
    }
    if matches!(
        name,
        "Rotate left" | "Rotate right" | "Rotation" | "Flip horizontally" | "Flip vertically"
    ) {
        crate::ribbon_settings::draw(
            p,
            rect.center(),
            match name {
                "Rotate left" => "Rotate counterclockwise",
                "Rotate right" | "Rotation" => "Rotate clockwise",
                _ => name,
            },
            color,
        );
        return;
    }
    if matches!(name, "Align top" | "Align middle" | "Align bottom") {
        let at =
            |x: f32, y: f32| rect.min + egui::vec2(x * rect.width() / 24., y * rect.height() / 24.);
        let y = match name {
            "Align top" => 3.,
            "Align bottom" => 21.,
            _ => 12.,
        };
        let stroke = Stroke::new(1.3, color);
        p.line_segment([at(2., y), at(22., y)], stroke);
        for (x, h) in [(6., 10.), (15., 15.)] {
            let top = if y == 3. {
                y + 3.
            } else if y == 21. {
                y - h - 3.
            } else {
                y - h / 2.
            };
            p.rect_stroke(
                Rect::from_min_max(at(x, top), at(x + 3., top + h)),
                0.,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        return;
    }
    if name == "Headphones" {
        let c = rect.center();
        let radius = rect.width().min(rect.height()) * 0.36;
        let points = (0..=24)
            .map(|i| {
                let angle = std::f32::consts::PI + i as f32 / 24. * std::f32::consts::PI;
                c + egui::vec2(angle.cos(), angle.sin()) * radius
            })
            .collect();
        p.add(egui::Shape::line(points, Stroke::new(1.5, color)));
        for side in [-1., 1.] {
            p.rect_stroke(
                Rect::from_center_size(
                    c + egui::vec2(side * radius, radius * 0.28),
                    egui::vec2(radius * 0.42, radius * 0.9),
                ),
                2.,
                Stroke::new(1.5, color),
                egui::StrokeKind::Inside,
            );
        }
        return;
    }
    if name == "Muted" {
        let at =
            |x: f32, y: f32| rect.min + egui::vec2(x * rect.width() / 24., y * rect.height() / 24.);
        let stroke = Stroke::new(1.5, color);
        p.add(egui::Shape::closed_line(
            vec![
                at(3., 9.),
                at(7., 9.),
                at(12., 5.),
                at(12., 19.),
                at(7., 15.),
                at(3., 15.),
            ],
            stroke,
        ));
        p.line_segment([at(16., 9.), at(22., 15.)], stroke);
        p.line_segment([at(22., 9.), at(16., 15.)], stroke);
        return;
    }
    if name == "Timing" {
        let stroke = Stroke::new(1.4, color);
        let c = rect.center();
        p.circle_stroke(c, rect.width().min(rect.height()) * 0.39, stroke);
        p.line_segment([c - egui::vec2(0., 5.), c], stroke);
        p.line_segment([c, c + egui::vec2(3., 2.)], stroke);
        return;
    }
    let glyph = match name {
        "Style" | "General" | "Properties" => "sliders-horizontal",
        "Presets" => "layout-template",
        "Crop" => "crop",
        "Palette" => "palette",
        "Image" => "image",
        "Restart" => "rotate-ccw",
        "Open" => "file",
        "Save As" => "save",
        "Position" => "move",
        "Selection" => "mouse-pointer-2",
        "Outline" => "square",
        "Layers" => "layers",
        "Capture" => "camera",
        "Tools" => "pencil-ruler",
        "Functions" | "Keybinds" => "keyboard",
        "Startup" => "power",
        "Library" => "images",
        "Align left" => "align-left",
        "Align center" => "align-center",
        "Align right" => "align-right",
        "Motion" => "sparkles",
        "Audio" | "Mute" => "volume-2",
        "Text" => "type",
        "Play" => "play",
        "Pause" => "pause",
        "Previous" => "step-back",
        "Next" => "step-forward",
        "Microphone" => "mic",
        "Recording" => "film",
        "Voiceover" | "Narration" => "mic",
        "Link" => "link",
        "Unlink" => "unlink",
        "Move left" => "arrow-left",
        "Move right" => "arrow-right",
        "Camera" => "video",
        "Audio effects" => "sliders-horizontal",
        "Analyze Voice" => "scan-line",
        "Effect power" => "power",
        "Reset effect" => "rotate-ccw",
        "Refresh" => "refresh-cw",
        "Delete effect" => "trash-2",
        "Close" => "x",
        "Parametric EQ" => "chart-no-axes-combined",
        "Compressor" => "minimize-2",
        "Vocal Clarity" => "sparkles",
        "De-esser" => "audio-lines",
        "Voice Leveler" => "equal",
        "Output Limiter" => "shield-check",
        "Tracks" => "layers",
        "Add Track" => "list-plus",
        "Up" => "chevron-up",
        "Down" => "chevron-down",
        "Expand section" => "chevron-right",
        "Split" => "scissors",
        "Trim" => "fold-horizontal",
        "Start here" => "arrow-right-to-line",
        "End here" => "arrow-left-to-line",
        "Pair" => "link",
        "Recent Clips" => "images",
        "Timeline" => "panel-bottom",
        "Expand Properties pane" => "panel-right-open",
        "Expand Timeline" => "panel-bottom-open",
        "Show annotations" => "eye",
        "Hide annotations" => "eye-off",
        "Timeline snapping" => "magnet",
        "Zoom preview in" | "Zoom" | "Zoom in" => "zoom-in",
        "Zoom preview out" => "zoom-out",
        "Fit preview" => "scan",
        "Video Setup" => "monitor-cog",
        "Plus" => "plus",
        "Minus" => "minus",
        _ => {
            crate::icons::draw_scaled(p, rect, name, color, rect.width() / 30.);
            return;
        }
    };
    let side = rect.width().min(rect.height());
    let px = (side * p.ctx().pixels_per_point()).ceil().clamp(16., 192.) as u16;
    let id = egui::Id::unique(("video-glyph", glyph, px));
    let texture = p
        .ctx()
        .data(|d| d.get_temp::<egui::TextureHandle>(id))
        .or_else(|| {
            let image = crate::ui_glyphs::render(glyph, px)?;
            let texture = p.ctx().load_texture(
                format!("video-{glyph}-{px}"),
                egui::ColorImage::from_rgba_premultiplied(
                    [px as usize, px as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            p.ctx().data_mut(|d| d.insert_temp(id, texture.clone()));
            Some(texture)
        });
    if let Some(texture) = texture {
        p.image(
            texture.id(),
            Rect::from_center_size(rect.center(), egui::vec2(side, side)),
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            color,
        );
    }
    if name == "Mute" {
        p.line_segment(
            [rect.left_top(), rect.right_bottom()],
            Stroke::new(1.5, color),
        );
    }
}

pub(crate) fn button(
    ui: &mut egui::Ui,
    glyph: &str,
    label: &str,
    width: f32,
    selected: bool,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(width, 28.), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::Role::Button,
            ui.is_enabled(),
            selected,
            if label.is_empty() { glyph } else { label },
        )
    });
    let fill = if selected {
        crate::tabs::ACTIVE_BLUE
    } else if response.hovered() || response.has_focus() {
        Color32::from_rgb(67, 77, 82)
    } else {
        Color32::from_rgb(54, 62, 66)
    };
    ui.painter().rect_filled(r, 5., fill);
    let text = ui.painter().layout_no_wrap(
        label.into(),
        egui::FontId::proportional(12.),
        ui.visuals().text_color(),
    );
    let content = if label.is_empty() {
        18.
    } else {
        25. + text.size().x
    };
    let x = r.center().x - content / 2.;
    icon(
        ui.painter(),
        Rect::from_center_size(egui::pos2(x + 9., r.center().y), egui::vec2(18., 18.)),
        glyph,
        ui.visuals().text_color(),
    );
    if !label.is_empty() {
        ui.painter().galley(
            egui::pos2(x + 25., r.center().y - text.size().y / 2.),
            text,
            Color32::WHITE,
        );
    }
    response.on_hover_text(if label.is_empty() { glyph } else { label })
}

pub(super) fn section(ui: &mut egui::Ui, name: &str) {
    let glyph = match name {
        "Clip fades" => "Motion",
        "Narration" => "Microphone",
        "Frame rate" => "Timing",
        "Framing" => "Crop",
        _ => "Position",
    };
    super::property_design::heading(ui, glyph, name);
}
