//! Shared video controls: fixed-height actions and crisp, font-independent marks.
use super::*;
use crate::tips::Tip as _;
use egui::{Color32, Rect, Stroke};

/// Line icons drawn with the painter on a 24-unit grid, for glyphs the bundled
/// icon set does not include. Returns false for names it does not draw.
fn drawn(p: &egui::Painter, rect: Rect, name: &str, color: Color32) -> bool {
    let at =
        |x: f32, y: f32| rect.min + egui::vec2(x * rect.width() / 24., y * rect.height() / 24.);
    let s = rect.width() / 24.;
    let stroke = Stroke::new(1.6 * s.max(0.75), color);
    let line = |pts: &[(f32, f32)]| {
        egui::Shape::line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), stroke)
    };
    let rr = |x: f32, y: f32, w: f32, h: f32, r: f32| {
        p.rect_stroke(
            Rect::from_min_max(at(x, y), at(x + w, y + h)),
            r * s,
            stroke,
            egui::StrokeKind::Middle,
        )
    };
    match name {
        "Duplicate" => {
            rr(8.5, 8.5, 12., 12., 2.);
            p.add(line(&[
                (15.5, 5.5),
                (15.5, 4.5),
                (14., 3.5),
                (5., 3.5),
                (3.5, 5.),
                (3.5, 14.),
                (5., 15.5),
                (5.5, 15.5),
            ]));
        }
        "Chevron left" => {
            p.add(line(&[(15., 5.), (8., 12.), (15., 19.)]));
        }
        "Chevron right" => {
            p.add(line(&[(9., 5.), (16., 12.), (9., 19.)]));
        }
        "More" => {
            for x in [5., 12., 19.] {
                p.circle_filled(at(x, 12.), 1.8 * s, color);
            }
        }
        "Chain" => {
            p.add(line(&[(6., 4.), (6., 13.), (8., 16.), (19., 16.)]));
            p.add(line(&[(15., 12.), (19., 16.), (15., 20.)]));
        }
        "Folder" => {
            p.add(line(&[
                (3., 7.),
                (9., 7.),
                (11., 9.),
                (21., 9.),
                (21., 19.),
                (3., 19.),
                (3., 7.),
            ]));
        }
        "Shape rectangle" => {
            rr(4., 6.5, 16., 11., 0.);
        }
        "Shape rounded" => {
            rr(4., 6.5, 16., 11., 3.5);
        }
        "Shape circle" => {
            p.circle_stroke(at(12., 12.), 7.5 * s, stroke);
        }
        "Keyframe" | "Keyframe filled" => {
            let pts = vec![at(12., 4.), at(20., 12.), at(12., 20.), at(4., 12.)];
            p.add(egui::Shape::convex_polygon(
                pts,
                if name == "Keyframe filled" {
                    color
                } else {
                    Color32::TRANSPARENT
                },
                stroke,
            ));
        }
        "Region" => {
            let dashed = Stroke::new(stroke.width, color);
            for (a, b) in [
                ((4., 5.), (20., 5.)),
                ((20., 5.), (20., 19.)),
                ((20., 19.), (4., 19.)),
                ((4., 19.), (4., 5.)),
            ] {
                p.extend(egui::Shape::dashed_line(
                    &[at(a.0, a.1), at(b.0, b.1)],
                    dashed,
                    3. * s,
                    2. * s,
                ));
            }
        }
        "Window" => {
            rr(3., 4.5, 18., 15., 2.);
            p.add(line(&[(3., 8.5), (21., 8.5)]));
        }
        "Fullscreen" => {
            for pts in [
                [(4., 9.), (4., 4.), (9., 4.)],
                [(15., 4.), (20., 4.), (20., 9.)],
                [(20., 15.), (20., 20.), (15., 20.)],
                [(9., 20.), (4., 20.), (4., 15.)],
            ] {
                p.add(line(&pts));
            }
        }
        "Open editor" => {
            rr(3., 4.5, 18., 15., 2.);
            p.add(line(&[(3., 8.5), (21., 8.5)]));
            p.add(line(&[(9., 13.5), (15., 13.5)]));
        }
        "System audio" => {
            p.add(line(&[
                (4., 10.),
                (4., 14.),
                (8., 14.),
                (13., 18.),
                (13., 6.),
                (8., 10.),
                (4., 10.),
            ]));
            p.add(line(&[(16., 9.), (17.5, 12.), (16., 15.)]));
            p.add(line(&[(18.5, 6.5), (21., 12.), (18.5, 17.5)]));
        }
        "Video file" => {
            p.add(line(&[
                (3., 7.),
                (9., 7.),
                (11., 9.),
                (21., 9.),
                (21., 19.),
                (3., 19.),
                (3., 7.),
            ]));
            p.add(line(&[
                (10.5, 11.5),
                (14.5, 14.),
                (10.5, 16.5),
                (10.5, 11.5),
            ]));
        }
        "Video template" => {
            rr(3., 4., 18., 16., 2.);
            p.add(line(&[(3., 9.), (21., 9.)]));
            p.add(line(&[(10.5, 12.), (14.5, 14.5), (10.5, 17.), (10.5, 12.)]));
        }
        "Tutorial" => {
            p.add(line(&[
                (3., 9.),
                (12., 4.5),
                (21., 9.),
                (12., 13.5),
                (3., 9.),
            ]));
            p.add(line(&[
                (7., 11.),
                (7., 16.),
                (12., 18.5),
                (17., 16.),
                (17., 11.),
            ]));
        }
        "Product walkthrough" => {
            p.add(line(&[(4., 18.), (8., 11.), (12., 14.), (17., 6.)]));
            p.circle_stroke(at(18., 5.5), 1.8 * s, stroke);
            p.add(line(&[(3., 20.), (21., 20.)]));
        }
        "Announcement" => {
            p.add(line(&[
                (4., 10.),
                (4., 14.),
                (8., 14.),
                (15., 19.),
                (15., 5.),
                (8., 10.),
                (4., 10.),
            ]));
            p.add(line(&[(18., 9.), (19.5, 12.), (18., 15.)]));
        }
        "Blank page" => {
            p.add(line(&[
                (6., 3.),
                (14., 3.),
                (19., 8.),
                (19., 21.),
                (6., 21.),
                (6., 3.),
            ]));
            p.add(line(&[(14., 3.), (14., 8.), (19., 8.)]));
        }
        "New transparent" => {
            for (x, y) in [(4., 4.), (15., 4.), (9.5, 9.5), (4., 15.), (15., 15.)] {
                p.rect_filled(Rect::from_min_max(at(x, y), at(x + 5., y + 5.)), 0., color);
            }
        }
        "New custom size" => {
            rr(3.5, 6., 17., 12., 1.);
            p.add(line(&[(7.5, 12.), (16.5, 12.)]));
            p.add(line(&[(10., 9.5), (7.5, 12.), (10., 14.5)]));
            p.add(line(&[(14., 9.5), (16.5, 12.), (14., 14.5)]));
        }
        "New clipboard" => {
            rr(5.5, 5., 13., 16., 1.8);
            p.add(line(&[(9., 5.), (9., 3.), (15., 3.), (15., 5.)]));
        }
        "Template steps" => {
            for y in [3.5, 9.75, 16.] {
                rr(4., y, 16., 4.5, 1.);
            }
        }
        "Template list" => {
            for y in [6., 12., 18.] {
                p.circle_filled(at(5., y), 1.2 * s, color);
                p.add(line(&[(9.5, y), (20., y)]));
            }
        }
        "Template comparison" => {
            rr(3., 5., 18., 14., 1.5);
            p.add(line(&[(12., 5.), (12., 19.)]));
        }
        "Template overview" | "Template saved" => {
            rr(3., 3.5, 18., 17., 1.8);
            p.add(line(&[(3., 9.), (21., 9.)]));
            p.add(line(&[(9.5, 9.), (9.5, 20.5)]));
        }
        "Template timeline" => {
            p.add(line(&[(3., 12.), (21., 12.)]));
            for x in [6., 12., 18.] {
                p.circle_filled(at(x, 12.), 2.2 * s, color);
            }
        }
        "Template document" => {
            rr(5., 3., 14., 18., 1.);
            for (y, end) in [(8., 16.), (12., 16.), (16., 13.)] {
                p.add(line(&[(8., y), (end, y)]));
            }
        }
        "Template wrench" => {
            p.circle_stroke(at(15.5, 8.5), 4.5 * s, stroke);
            p.add(line(&[(12.3, 11.7), (4.5, 19.5)]));
            p.add(line(&[(4.5, 19.5), (5.5, 20.5)]));
        }
        "Template people" => {
            p.circle_stroke(at(9., 8.5), 3. * s, stroke);
            p.add(line(&[
                (3., 19.5),
                (4., 16.5),
                (6.5, 14.5),
                (9., 14.),
                (11.5, 14.5),
                (14., 16.5),
                (15., 19.5),
            ]));
            p.circle_stroke(at(17., 9.), 2.4 * s, stroke);
            p.add(line(&[(16.5, 14.), (19., 14.7), (20.5, 16.5), (21., 18.5)]));
        }
        _ => return false,
    }
    true
}

pub(crate) fn icon(p: &egui::Painter, rect: Rect, name: &str, color: Color32) {
    if crate::help::draw_glyph(p, rect, name, color) || drawn(p, rect, name, color) {
        return;
    }
    if name == "Keyframes" {
        let c = rect.center();
        let d = rect.width() * 0.36;
        p.add(egui::Shape::convex_polygon(
            vec![
                c + egui::vec2(0., -d),
                c + egui::vec2(d, 0.),
                c + egui::vec2(0., d),
                c + egui::vec2(-d, 0.),
            ],
            Color32::TRANSPARENT,
            Stroke::new(1.7, color),
        ));
        return;
    }
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
    if name == "Jump to Object" {
        // Locate: a ring with four ticks and a center dot.
        let stroke = Stroke::new(1.4, color);
        let c = rect.center();
        let r = rect.width().min(rect.height()) * 0.3;
        p.circle_stroke(c, r, stroke);
        p.circle_filled(c, 1.6, color);
        for d in [
            egui::vec2(1., 0.),
            egui::vec2(-1., 0.),
            egui::vec2(0., 1.),
            egui::vec2(0., -1.),
        ] {
            p.line_segment([c + d * (r - 2.), c + d * (r + 4.)], stroke);
        }
        return;
    }
    if name == "Follow playhead" {
        let stroke = Stroke::new(1.4, color);
        let c = rect.center();
        let h = rect.height() * 0.42;
        p.line_segment([c + egui::vec2(-3., -h), c + egui::vec2(-3., h)], stroke);
        p.line_segment([c + egui::vec2(-6., -h), c + egui::vec2(0., -h)], stroke);
        for x in [2., 6.] {
            p.line_segment([c + egui::vec2(x, -3.), c + egui::vec2(x + 3., 0.)], stroke);
            p.line_segment([c + egui::vec2(x + 3., 0.), c + egui::vec2(x, 3.)], stroke);
        }
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
        "Open" | "PDF" => "file",
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
        "Steps tab" => "list-ordered",
        "Open editor" => "panels-top-left",
        "Folder" => "external-link",
        "Clipboard" => "file",
        "Auto-scroll" => "chevrons-up-down",
        "Webcam" => "camera",
        "Hide tracks" => "eye-off",
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
    response.tip(if label.is_empty() { glyph } else { label })
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
