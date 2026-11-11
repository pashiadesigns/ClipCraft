//! Original ribbon implementation based on user-provided visual references.
use crate::tips::Tip as _;
use crate::{Command, Editor, egui};
use egui::{Color32, Response, Stroke};

#[cfg(test)]
mod blue_surround_preview;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RibbonTab {
    Insert,
    Formatting,
    Advanced,
    Export,
}

pub(super) fn icon(
    ui: &mut egui::Ui,
    name: &str,
    tooltip: &str,
    enabled: bool,
    selected: bool,
    size: egui::Vec2,
) -> Response {
    let r = ui
        .push_id(name, |ui| {
            ui.add_enabled(
                enabled,
                egui::Button::new("").min_size(size).selected(selected),
            )
        })
        .inner;
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, tooltip));
    let color = if enabled && matches!(name, "Record" | "Capture Clip" | "Capture Video") {
        Color32::from_rgb(235, 65, 76)
    } else if enabled {
        match name {
            "Save As" | "Copy" => Color32::from_rgb(87, 162, 239),
            "Capture" => Color32::from_rgb(107, 205, 161),
            "Paste" => Color32::from_rgb(238, 202, 98),
            _ => Color32::from_gray(230),
        }
    } else {
        Color32::from_gray(110)
    };
    draw_icon(ui.painter(), r.rect.center(), name, color);
    r.tip_title(name).tip(tooltip)
}
pub(super) fn draw_icon(p: &egui::Painter, c: egui::Pos2, name: &str, color: Color32) {
    let at = |x, y| c + egui::vec2(x, y);
    let stroke = Stroke::new(1.4, color);
    let rect = |x, y, w, h| egui::Rect::from_center_size(at(x, y), egui::vec2(w, h));
    match name {
        "GIF Studio" | "Group presets" | "Save group" => {
            crate::icons::draw(p, rect(0., 0., 28., 28.), name, color)
        }
        "Perspective" => {
            // A screen leaning back: a trapezoid with a top bar.
            let pts = vec![at(-9., 8.), at(9., 8.), at(6., -8.), at(-6., -8.)];
            p.add(egui::Shape::closed_line(pts, stroke));
            p.line_segment([at(-7., -4.), at(7., -4.)], stroke);
        }
        "Expand canvas on" | "Expand canvas off" => {
            p.rect_stroke(rect(0., 0., 12., 10.), 2., stroke, egui::StrokeKind::Inside);
            let direction = if name == "Expand canvas on" { 1. } else { -1. };
            for sign in [-1., 1.] {
                let outer = at(sign * 11., sign * 9.);
                let inner = at(sign * 7., sign * 5.);
                if direction > 0. {
                    p.arrow(inner, outer - inner, stroke);
                } else {
                    p.arrow(outer, inner - outer, stroke);
                }
            }
        }
        "Recording" | "Voiceover" | "Narration" | "Audio effects" => crate::video_editor::ui::icon(
            p,
            egui::Rect::from_center_size(c, egui::vec2(24., 24.)),
            name,
            color,
        ),
        "Capture Clip" | "Capture Video" => {
            crate::icons::draw_scaled(p, rect(0., 0., 24., 24.), name, color, 0.65)
        }
        "Video setup" => {
            p.rect_stroke(
                rect(0., -1., 20., 14.),
                2.,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(-4., 9.), at(4., 9.)], stroke);
            p.line_segment([at(0., 6.), at(0., 9.)], stroke);
            p.arrow(at(-4., 2.), egui::vec2(8., -6.), stroke);
        }
        "Tags" => {
            p.add(egui::Shape::closed_line(
                vec![
                    at(-8., -7.),
                    at(1., -7.),
                    at(9., 1.),
                    at(1., 9.),
                    at(-8., 0.),
                ],
                stroke,
            ));
            p.circle_stroke(at(-3., -2.), 1.8, stroke);
        }
        "Dates" => {
            p.rect_stroke(rect(0., 1., 18., 16.), 2., stroke, egui::StrokeKind::Inside);
            for x in [-4., 4.] {
                p.line_segment([at(x, -9.), at(x, -4.)], stroke);
            }
            p.line_segment([at(-8., -2.), at(8., -2.)], stroke);
            for x in [-4., 1., 5.] {
                p.circle_filled(at(x, 3.), 1., color);
            }
        }
        "Applications" => {
            p.rect_stroke(rect(0., 0., 18., 16.), 2., stroke, egui::StrokeKind::Inside);
            p.line_segment([at(-8., -3.), at(8., -3.)], stroke);
            p.circle_filled(at(-5., -5.5), 0.8, color);
        }
        "Websites" => {
            p.circle_stroke(c, 9., stroke);
            p.add(egui::Shape::ellipse_stroke(c, egui::vec2(4., 9.), stroke));
            p.line_segment([at(-8., 0.), at(8., 0.)], stroke);
        }
        "Pin" => {
            p.add(egui::Shape::closed_line(
                vec![
                    at(-4., -8.),
                    at(4., -8.),
                    at(3., -2.),
                    at(6., 2.),
                    at(-6., 2.),
                    at(-3., -2.),
                ],
                stroke,
            ));
            p.line_segment([at(0., 2.), at(0., 9.)], stroke);
        }
        "Search" => {
            p.circle_stroke(at(-2., -2.), 6., stroke);
            p.line_segment([at(3., 3.), at(9., 9.)], stroke);
        }
        "Image" => {
            p.rect_stroke(rect(0., 0., 18., 15.), 2., stroke, egui::StrokeKind::Inside);
            p.circle_filled(at(4., -3.), 1.5, color);
            p.add(egui::Shape::line(
                vec![
                    at(-7., 5.),
                    at(-2., -1.),
                    at(2., 3.),
                    at(5., 0.),
                    at(8., 5.),
                ],
                stroke,
            ));
        }
        "Steps" => {
            for (y, n) in [(-5., "1"), (5., "2")] {
                p.circle_stroke(at(-5., y), 4., stroke);
                p.text(
                    at(-5., y),
                    egui::Align2::CENTER_CENTER,
                    n,
                    egui::FontId::proportional(6.),
                    color,
                );
                p.line_segment([at(2., y), at(9., y)], stroke);
            }
        }
        "Tools" => {
            p.rect_stroke(rect(0., 2., 16., 11.), 2., stroke, egui::StrokeKind::Inside);
            p.add(egui::Shape::line(
                vec![at(-4., -3.), at(-4., -7.), at(4., -7.), at(4., -3.)],
                stroke,
            ));
            p.line_segment([at(-7., 1.), at(7., 1.)], stroke);
            p.line_segment([at(0., -1.), at(0., 4.)], Stroke::new(2., color));
        }
        "Library" => {
            p.line_segment([at(-8., 4.), at(-8., -8.)], stroke);
            p.line_segment([at(-8., -8.), at(5., -8.)], stroke);
            p.rect_stroke(rect(1., 1., 16., 13.), 2., stroke, egui::StrokeKind::Inside);
            let blue = Color32::from_rgb(65, 185, 230);
            p.circle_filled(at(5., -2.), 1.6, Color32::from_rgb(245, 192, 73));
            p.add(egui::Shape::line(
                vec![at(-5., 5.), at(-1., 0.), at(2., 3.), at(4., 1.), at(7., 5.)],
                Stroke::new(1.5, blue),
            ));
        }
        "Editor" => {
            p.rect_stroke(
                rect(0.0, 0.0, 17.0, 14.0),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(-5.0, 4.0), at(3.0, -4.0)], stroke);
        }
        "Template" => {
            p.rect_stroke(rect(0., 0., 18., 21.), 2., stroke, egui::StrokeKind::Inside);
            p.line_segment([at(-5., -6.), at(5., -6.)], stroke);
            p.rect_stroke(rect(-3., 0., 5., 5.), 0., stroke, egui::StrokeKind::Inside);
            p.line_segment([at(2., 0.), at(6., 0.)], stroke);
            p.line_segment([at(-5., 6.), at(5., 6.)], stroke);
        }
        "Save As" => {
            p.rect_stroke(
                rect(0.0, 0.0, 16.0, 17.0),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                rect(0.0, -4.0, 8.0, 6.0),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_stroke(
                rect(0.0, 5.0, 9.0, 6.0),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        "Capture" => {
            p.rect_stroke(
                rect(0.0, 1.0, 19.0, 13.0),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.circle_stroke(at(0.0, 1.0), 4.0, stroke);
            p.line_segment([at(-4.0, -7.0), at(3.0, -7.0)], stroke);
        }
        "Record" => {
            p.circle_filled(c, 6.0, color);
        }
        "Details" => {
            p.circle_stroke(c, 8., stroke);
            p.circle_filled(at(0., -4.), 1.2, color);
            p.line_segment([at(0., -1.), at(0., 5.)], Stroke::new(2., color));
        }
        "Refresh" => {
            let points = (0..25)
                .map(|i| {
                    let a = i as f32 * 0.21;
                    at(a.cos() * 7., a.sin() * 7.)
                })
                .collect();
            p.add(egui::Shape::line(points, stroke));
            p.add(egui::Shape::line(
                vec![at(1., -9.), at(5., -6.), at(1., -3.)],
                stroke,
            ));
        }
        "Undo" | "Redo" | "Revert" => {
            let direction = if name != "Redo" { -1.0 } else { 1.0 };
            p.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    at(-direction * 6.0, 6.0),
                    at(-direction * 11.0, -5.0),
                    at(-direction * 2.0, -7.0),
                    at(direction * 7.0, -4.0),
                ],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
            p.line_segment(
                [at(direction * 7.0, -4.0), at(direction * 2.0, -8.0)],
                stroke,
            );
            p.line_segment(
                [at(direction * 7.0, -4.0), at(direction * 2.0, 0.0)],
                stroke,
            );
        }
        "Paste" => {
            p.rect_stroke(
                rect(0.0, 1.0, 15.0, 18.0),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(rect(0.0, -8.0, 8.0, 4.0), 1.0, color);
        }
        "Copy" => {
            p.rect_stroke(
                rect(-3.0, -3.0, 12.0, 13.0),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.rect_filled(rect(3.0, 3.0, 12.0, 13.0), 1.0, Color32::from_gray(48));
            p.rect_stroke(
                rect(3.0, 3.0, 12.0, 13.0),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        "Format" => {
            p.rect_filled(rect(0.0, -4.0, 15.0, 7.0), 2.0, color);
            p.add(egui::Shape::line(
                vec![
                    at(7.0, -4.0),
                    at(9.0, -4.0),
                    at(9.0, 2.0),
                    at(0.0, 2.0),
                    at(0.0, 7.0),
                ],
                stroke,
            ));
            p.rect_filled(rect(0.0, 7.0, 4.0, 7.0), 1.0, color);
        }
        "Export PNG" | "Export image" | "Formats" => {
            p.rect_stroke(
                rect(-1.0, -1.0, 19.0, 15.0),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.circle_filled(at(-5.0, -4.0), 2.0, color);
            p.add(egui::Shape::line(
                vec![at(-8.0, 4.0), at(-2.0, -1.0), at(2.0, 3.0), at(5.0, 0.0)],
                stroke,
            ));
            p.arrow(at(5.0, 2.0), egui::vec2(0.0, 9.0), stroke);
        }
        "Grab Text" => {
            p.rect_stroke(
                rect(0.0, 0.0, 20.0, 19.0),
                2.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment([at(-5.0, -5.0), at(5.0, -5.0)], stroke);
            p.line_segment([at(0.0, -5.0), at(0.0, 6.0)], stroke);
            p.line_segment([at(-3.0, 6.0), at(3.0, 6.0)], stroke);
        }
        "Switch tools" => {
            p.arrow(at(-8.0, -4.0), egui::vec2(16.0, 0.0), stroke);
            p.arrow(at(8.0, 4.0), egui::vec2(-16.0, 0.0), stroke);
        }
        "Export" | "Exit" => {
            p.arrow(at(-7.0, 0.0), egui::vec2(14.0, 0.0), stroke);
        }
        "Options" | "Preferences" => {
            p.circle_stroke(c, 6.0, stroke);
            p.circle_stroke(c, 2.0, stroke);
        }
        "Help" => {
            p.circle_stroke(c, 8.0, stroke);
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(13.0),
                color,
            );
        }
        "Cut" => {
            p.circle_stroke(at(-5.0, 5.0), 3.0, stroke);
            p.circle_stroke(at(5.0, 5.0), 3.0, stroke);
            p.line_segment([at(-4.0, 3.0), at(7.0, -7.0)], stroke);
            p.line_segment([at(4.0, 3.0), at(-7.0, -7.0)], stroke);
        }
        _ => {}
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn command_tile(
    ui: &mut egui::Ui,
    glyph: &str,
    label: &str,
    tip: &str,
    enabled: bool,
    selected: bool,
    width: f32,
    height: f32,
) -> Response {
    let r = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .min_size(egui::vec2(width, height))
            .selected(selected),
    );
    let color = match glyph {
        "Export PNG" | "Export image" => Color32::from_rgb(107, 205, 161),
        "Copy" | "Save As" => Color32::from_rgb(87, 162, 239),
        "Revert" => Color32::from_rgb(238, 172, 87),
        "Switch tools" => Color32::from_rgb(182, 151, 242),
        "Capture Clip" | "Capture Video" => Color32::from_rgb(235, 65, 76),
        _ => Color32::from_rgb(133, 181, 198),
    };
    if matches!(glyph, "Capture Clip" | "Capture Video") {
        crate::icons::draw_scaled(
            ui.painter(),
            egui::Rect::from_center_size(
                r.rect.center_top() + egui::vec2(0., height * 0.36),
                egui::vec2(48., 48.),
            ),
            glyph,
            if enabled {
                color
            } else {
                color.gamma_multiply(0.5)
            },
            1.6,
        );
    } else {
        draw_icon(
            ui.painter(),
            r.rect.center_top() + egui::vec2(0.0, height * 0.34),
            glyph,
            if enabled {
                color
            } else {
                color.gamma_multiply(0.5)
            },
        );
    }
    let ink = if enabled {
        Color32::from_gray(235)
    } else {
        Color32::from_gray(135)
    };
    // A label wider than its tile wraps onto centered lines (approved
    // formatting-merge mockup: "Revert to / Original" in a 72 px tile).
    let room = width - 8.0;
    let fits = |text: &str, size: f32| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(size), ink)
            .size()
            .x
            <= room
    };
    let (lines, size) = if fits(label, 12.0) {
        (vec![label.to_owned()], 12.0)
    } else {
        let mut lines: Vec<String> = vec![];
        for word in label.split(' ') {
            match lines.last_mut() {
                Some(line) if fits(&format!("{line} {word}"), 11.0) => {
                    line.push(' ');
                    line.push_str(word);
                }
                _ => lines.push(word.to_owned()),
            }
        }
        (lines, 11.0)
    };
    let mut bottom = r.rect.center_bottom() - egui::vec2(0.0, 9.0);
    for line in lines.iter().rev() {
        let rect = ui.painter().text(
            bottom,
            egui::Align2::CENTER_BOTTOM,
            line,
            egui::FontId::proportional(size),
            ink,
        );
        bottom.y = rect.top() + 1.0;
    }
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, label));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("command-tile", label)), r.rect));
    r.tip_title(label).tip(tip)
}

fn labeled_icon(
    ui: &mut egui::Ui,
    name: &str,
    tip: &str,
    enabled: bool,
    selected: bool,
) -> Response {
    let r = ui.add_enabled(
        enabled,
        egui::Button::new("")
            .min_size(egui::vec2(84.0, 18.0))
            .selected(selected),
    );
    let color = if enabled {
        match name {
            "Cut" => Color32::from_rgb(238, 155, 89),
            "Copy" => Color32::from_rgb(95, 166, 235),
            _ => Color32::from_rgb(238, 199, 95),
        }
    } else {
        Color32::from_gray(110)
    };
    draw_icon(
        ui.painter(),
        r.rect.left_center() + egui::vec2(13.0, 0.0),
        name,
        color,
    );
    ui.painter().text(
        r.rect.left_center() + egui::vec2(29.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::proportional(12.0),
        if enabled {
            Color32::from_gray(240)
        } else {
            Color32::from_gray(120)
        },
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    r.tip_title(name).tip(tip)
}
pub(super) fn group(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    height: f32,
    body: impl FnOnce(&mut egui::Ui),
) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.allocate_ui_with_layout(
            egui::vec2(width, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_height(height);
                body(ui);
            },
        );
        let (r, response) = ui.allocate_exact_size(egui::vec2(width, 14.0), egui::Sense::hover());
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(10.0),
            ui.visuals().text_color(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, label));
    });
}
pub(super) fn divider(ui: &mut egui::Ui, height: f32) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, height + 14.0), egui::Sense::hover());
    ui.painter().line_segment(
        [
            r.center_top() + egui::vec2(0.0, 4.0),
            r.center_bottom() - egui::vec2(0.0, 4.0),
        ],
        Stroke::new(1.0, Color32::from_gray(74)),
    );
}

impl Editor {
    pub(super) fn ribbon_tabs(&mut self, ui: &mut egui::Ui) -> [Response; 4] {
        crate::tabs::group(ui, |ui| {
            ui.horizontal(|ui| {
                if crate::tabs::tab(ui, "File", false, 76.0).clicked() {
                    self.open_file_workspace();
                }
                let library = crate::tabs::tab(ui, "Library", self.shell.library, 102.);
                self.shell.library_tab_rect = Some(library.rect);
                if library.clicked() && self.commit_text() {
                    self.commit_property_preview();
                    self.cancel_gesture();
                    self.shell.backstage = None;
                    self.shell.preview_ui = false;
                    self.shell.library = true;
                    self.history.refresh();
                }

                [
                    (RibbonTab::Insert, "Insert", 88.0),
                    (RibbonTab::Formatting, "Formatting", 118.0),
                    (RibbonTab::Advanced, "Advanced", 112.0),
                    (RibbonTab::Export, "Export", 90.0),
                ]
                .map(|(tab, label, width)| {
                    // Advanced folded into Formatting (approved mockup). The empty
                    // slot keeps each tab at its usual index.
                    if tab == RibbonTab::Advanced {
                        let r = ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover());
                        ui.add_space(-ui.spacing().item_spacing.x);
                        return r;
                    }
                    let r = crate::tabs::tab(
                        ui,
                        label,
                        !self.shell.library && self.shell.ribbon_tab == tab,
                        width,
                    );
                    if r.clicked() {
                        self.shell.return_to_editor();
                        self.shell.ribbon_tab = tab;
                    }
                    r
                })
            })
            .inner
        })
    }

    pub(super) fn show_quick_access(&mut self, ui: &mut egui::Ui) -> Vec<(&'static str, Response)> {
        let ctx = ui.ctx().clone();
        let mut buttons = vec![];
        egui::Panel::top("quick_access_header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(33, 34, 37))
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let (mark, response) =
                        ui.allocate_exact_size(egui::vec2(26., 26.), egui::Sense::hover());
                    crate::brand::paint(
                        ui,
                        egui::Rect::from_center_size(mark.center(), egui::vec2(20., 20.)),
                    );
                    response.tip("ClipDesigner");
                    let r = icon(
                        ui,
                        "Save As",
                        "Save As editable project… · Ctrl+Shift+S\nUse Export for a PNG image.",
                        true,
                        false,
                        egui::vec2(28.0, 26.0),
                    );
                    if r.clicked() {
                        self.save(true);
                    }
                    buttons.push(("Save As", r));
                    let r = icon(
                        ui,
                        "Capture Clip",
                        "Capture Clip",
                        crate::CAPTURE_ENABLED
                            && self.capture_result.is_none()
                            && !self.capture_requested,
                        false,
                        egui::vec2(28.0, 26.0),
                    );
                    if r.clicked() {
                        self.capture_clip_clicked(&ctx);
                    }
                    buttons.push(("Capture", r));
                    let r = icon(
                        ui,
                        "Undo",
                        "Undo · Ctrl+Z",
                        !self.shell.library
                            && self.shell.backstage.is_none()
                            && (self.session.can_undo() || self.text_draft.is_some()),
                        false,
                        egui::vec2(28.0, 26.0),
                    );
                    if r.clicked() && self.commit_text() {
                        self.execute(Command::Undo);
                    }
                    buttons.push(("Undo", r));
                    let r = icon(
                        ui,
                        "Redo",
                        "Redo · Ctrl+Y",
                        !self.shell.library
                            && self.shell.backstage.is_none()
                            && self.text_draft.is_none()
                            && self.session.can_redo(),
                        false,
                        egui::vec2(28.0, 26.0),
                    );
                    if r.clicked() {
                        self.execute(Command::Redo);
                    }
                    buttons.push(("Redo", r));
                    if crate::chrome::title_and_controls(ui, &self.window_title()) {
                        self.close_editor_window(&ctx);
                    }
                });
            });
        buttons
    }
    pub(super) fn show_ribbon(&mut self, ui: &mut egui::Ui) -> [Response; 4] {
        if self.shell.ribbon_tab == RibbonTab::Advanced {
            self.shell.ribbon_tab = RibbonTab::Formatting;
        }
        egui::Panel::top("workspace_ribbon")
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(crate::shell::SURROUND))
            .show(ui, |ui| {
                // One panel avoids a separator through the active tab's join.
                ui.spacing_mut().item_spacing.y = 0.0;
                let tabs = egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(8, 2))
                    .show(ui, |ui| ui.horizontal(|ui| self.ribbon_tabs(ui)).inner)
                    .inner;
                self.capture_pills(ui, tabs[3].rect.right());
                if self.shell.preview_ui {
                    return tabs;
                }
                if self.shell.library {
                    let active = self.shell.library_tab_rect.unwrap();
                    ui.add_space(3.);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(active.left(), active.bottom() - 4.),
                            egui::pos2(active.right(), ui.min_rect().bottom() + 1.),
                        ),
                        0.,
                        crate::tabs::ACTIVE_BLUE,
                    );
                    return tabs;
                }
                let ribbon = egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(5, 3))
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .fill(Color32::from_rgb(50, 53, 55))
                            .corner_radius(10)
                            .stroke(Stroke::new(2.0, crate::tabs::ACTIVE_BLUE))
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                let height = 70.;
                                let bounds = ui.available_rect_before_wrap();
                                let setup = self.shell.ribbon_tab == RibbonTab::Formatting
                                    && !self.is_video();
                                let mut setup_rect = egui::Rect::from_min_max(
                                    egui::pos2(bounds.left(), bounds.top()),
                                    egui::pos2(bounds.left() + 108., bounds.top() + height),
                                );
                                let scroll_width = bounds.width() - if setup { 112. } else { 0. };
                                let scrolled = ui.scope_builder(
                                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                                        bounds.min + egui::vec2(if setup { 112. } else { 0. }, 0.),
                                        egui::vec2(scroll_width, height + 27.),
                                    )),
                                    |ui| {
                                        ui.set_max_width(scroll_width);
                                        egui::ScrollArea::horizontal()
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .max_width(scroll_width)
                                    .id_salt("ribbon_overflow")
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 3.0;
                                            let height =
                                                70.0;
                                            // Tabs build different widgets. Separate id scopes stop a
                                            // tile taking the state of the last tab's widget for a frame.
                                            let tab = self.shell.ribbon_tab;
                                            ui.push_id(("ribbon-tab", tab as u8), |ui| {
                                            match self.shell.ribbon_tab {
                                                RibbonTab::Insert => self.insert_ribbon(ui, height),
                                                RibbonTab::Formatting | RibbonTab::Advanced => {
                                                    ui.add_enabled_ui(!self.shell.library, |ui| {
                                                        self.formatting_ribbon(ui, height)
                                                    });
                                                }
                                                RibbonTab::Export => self.export_ribbon(ui, height),
                                            }
                                            });
                                        });
                                    })
                                    },
                                );
                                setup_rect.max.y = scrolled.response.rect.bottom();
                                let _scroll = scrolled.inner;
                                if setup {
                                    ui.scope_builder(
                                        egui::UiBuilder::new().max_rect(setup_rect),
                                        |ui| self.page_setup_ribbon(ui, setup_rect.height()),
                                    );
                                }
                                #[cfg(test)]
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(
                                        egui::Id::unique("ribbon-overflow-geometry"),
                                        (
                                            _scroll.content_size,
                                            _scroll.inner_rect,
                                            scrolled.response.rect,
                                        ),
                                    );
                                    d.insert_temp(
                                        egui::Id::unique("ribbon-scroll-content"),
                                        _scroll.inner_rect,
                                    )
                                });
                            })
                            .response
                            .rect
                    })
                    .inner;
                let active = tabs[match self.shell.ribbon_tab {
                    RibbonTab::Insert => 0,
                    RibbonTab::Formatting => 1,
                    RibbonTab::Advanced => 2,
                    RibbonTab::Export => 3,
                }]
                .rect;
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(active.left(), active.bottom() - 4.0),
                        egui::pos2(active.right(), ribbon.top() + 2.0),
                    ),
                    0.0,
                    crate::tabs::ACTIVE_BLUE,
                );
                tabs
            })
            .inner
    }
    fn insert_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        if self.is_video() {
            self.video_insert_ribbon(ui, height);
            return;
        }
        let ctx = ui.ctx().clone();
        let available = ui.available_width();
        group(ui, "Clipboard", 151.0, height, |ui| {
            crate::split_button::show(ui, |ui| {
                let main = icon(
                    ui,
                    "Paste",
                    "Paste image or editable objects into this canvas · Ctrl+V",
                    true,
                    false,
                    egui::vec2(38.0, height - 2.0),
                );
                ui.painter().text(
                    main.rect.center_bottom() - egui::vec2(0.0, 4.0),
                    egui::Align2::CENTER_BOTTOM,
                    "Paste",
                    egui::FontId::proportional(10.0),
                    Color32::from_gray(125),
                );
                if main.clicked() {
                    self.paste_canvas();
                }
                crate::split_button::dropdown_for(ui, "Paste options", &main, |ui| {
                    if ui.button("New image from clipboard").clicked() {
                        self.new_from_clipboard(&ctx);
                        ui.close();
                    }
                    let guide = self
                        .session
                        .document
                        .guide
                        .as_ref()
                        .is_some_and(|g| g.pages.is_some());
                    if ui
                        .add_enabled(guide, egui::Button::new("PDF pages…"))
                        .tip(if guide {
                            "PDF pages · Add pages from a PDF to this guide."
                        } else {
                            "PDF pages · Works in a guide with pages. Use File › New › From PDF for a new guide."
                        })
                        .clicked()
                    {
                        ui.close();
                        self.insert_pdf_pages();
                    }
                    if ui
                        .button("Image from file…")
                        .tip("Insert a PNG, JPEG, WebP, BMP, GIF, SVG or EPS file as an image object. SVG stays sharp at any size.")
                        .clicked()
                    {
                        ui.close();
                        self.suspend_global_capture();
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter(
                                "Images",
                                &["png", "jpg", "jpeg", "webp", "bmp", "gif", "svg", "eps"],
                            )
                            .pick_file()
                        {
                            self.insert_image_file(&path);
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.spacing_mut().interact_size.y = 18.0;
                ui.spacing_mut().button_padding.y = 1.0;
                if labeled_icon(
                    ui,
                    "Cut",
                    "Cut selected objects or pixels · Ctrl+X",
                    !self.selected_objects().is_empty(),
                    false,
                )
                .clicked()
                {
                    self.copy_canvas_selection(true);
                }
                if labeled_icon(
                    ui,
                    "Copy",
                    "Copy selected objects or pixels; otherwise copy the complete image · Ctrl+C",
                    true,
                    false,
                )
                .clicked()
                {
                    self.copy_canvas_selection(false);
                }
                let format = labeled_icon(
                    ui,
                    "Format",
                    "Format Painter: click for one use; double-click to keep painting. Click again to stop.",
                    self.selected_objects().len() == 1
                        || self.quick_controls.format_source.is_some(),
                    self.quick_controls.format_source.is_some(),
                );
                if format.double_clicked() {
                    self.lock_format_painter();
                } else if format.clicked() {
                    self.arm_format_painter();
                }
                #[cfg(test)]
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique("format-painter-button"), format.rect));
            });
        });
        divider(ui, height);
        let tools_width = (available - 166.0 - Self::GROUP_PRESET_WIDTH).max(102.0);
        self.ribbon_tools(ui, tools_width, height);
        self.group_preset_ribbon(ui, height);
    }
    fn tool_sections(
        &self,
        visible: usize,
    ) -> Vec<(capture_core::toolbar::Group, std::ops::Range<usize>)> {
        use capture_core::toolbar::{GROUPS, Group};
        let order = self.quick_controls.toolbar_order();
        let mut start = 0;
        let mut result = vec![];
        for g in GROUPS {
            let count = order
                .iter()
                .take(visible)
                .filter(|t| self.quick_controls.tool_group(**t) == g)
                .count();
            if count > 0 || g == Group::Favorites {
                result.push((g, start..start + count));
            }
            start += count;
        }
        result
    }
    fn tool_sections_width(&self, visible: usize, gap: f32) -> f32 {
        let sections = self.tool_sections(visible);
        sections
            .iter()
            .map(|(_, range)| {
                if range.is_empty() {
                    84.0
                } else {
                    range.len() as f32 * 72.0 + range.len().saturating_sub(1) as f32 * gap
                }
            })
            .sum::<f32>()
            + sections.len().saturating_sub(1) as f32 * (9.0 + 2.0 * gap)
    }
    fn ribbon_tools(
        &mut self,
        ui: &mut egui::Ui,
        width: f32,
        height: f32,
    ) -> (usize, egui::Rect, Option<Response>) {
        let total = self.quick_controls.toolbar_order().len();
        let gap = ui.spacing().item_spacing.x;
        let all_width = self.tool_sections_width(total, gap);
        let visible = if all_width <= width {
            total
        } else {
            (0..total)
                .rev()
                .find(|n| self.tool_sections_width(*n, gap) + gap + 24.0 <= width)
                .unwrap_or(0)
        }
        .min(total);
        let mut last = egui::Rect::NOTHING;
        for (index, (section, range)) in self.tool_sections(visible).into_iter().enumerate() {
            if index > 0 {
                divider(ui, height);
            }
            let w = if range.is_empty() {
                84.0
            } else {
                range.len() as f32 * 72.0 + range.len().saturating_sub(1) as f32 * gap
            };
            group(ui, section.label(), w, height, |ui| {
                if range.is_empty() {
                    let (r, response) =
                        ui.allocate_exact_size(egui::vec2(84.0, 58.0), egui::Sense::hover());
                    crate::icons::draw(ui.painter(), r, "Favorites", Color32::WHITE);
                    response.tip("Right-click any tool and choose Add to Favorites.");
                } else {
                    let buttons = self.toolbar_range(ui, range.start, range.end);
                    if let Some((_, r)) = buttons.last() {
                        last = r.rect;
                    }
                }
            });
        }
        let more = (visible < total).then(|| {
            let mut response = None;
            group(ui, "", 24.0, height, |ui| {
                response = Some(
                    crate::split_button::dropdown_sized(
                        ui,
                        "More tools",
                        egui::vec2(24.0, 58.0),
                        |ui| self.draggable_tools_menu(ui, visible),
                    )
                    .response
                    .tip("More tools · right-click to favorite, or drag within its section"),
                );
            });
            response.unwrap()
        });
        // A resize can remove the anchor or change its automatic widget ID.
        // Retire only this popup, never a separate palette/menu elsewhere.
        let key = ui.make_persistent_id("last-more-tools-popup");
        let current = more.as_ref().map(egui::Popup::default_response_id);
        let old = ui.ctx().data_mut(|d| {
            let old = d.get_temp::<egui::Id>(key);
            if let Some(id) = current {
                d.insert_temp(key, id);
            } else {
                d.remove::<egui::Id>(key);
            }
            old
        });
        if let Some(old) = old.filter(|id| Some(*id) != current) {
            egui::Popup::close_id(ui.ctx(), old);
        }
        (visible, last, more)
    }
    fn export_ribbon(&mut self, ui: &mut egui::Ui, height: f32) {
        if self.is_video() {
            self.video_export_ribbon(ui, height);
            return;
        }
        group(ui, "Image", 204.0, height, |ui| {
            if command_tile(
                ui,
                "Export image",
                "Export image",
                "Export PNG, JPEG or PDF with a preview",
                true,
                false,
                100.0,
                height,
            )
            .clicked()
            {
                self.export();
            }
            if command_tile(
                ui,
                "Copy",
                "Copy All",
                "Copy the complete flattened image · Ctrl+Shift+C",
                true,
                false,
                100.0,
                height,
            )
            .clicked()
            {
                self.copy_all();
            }
        });
        divider(ui, height);
        group(ui, "Editable project", 256.0, height, |ui| {
            if command_tile(
                ui,
                "Save As",
                "Save As project",
                "Save an editable ClipDesigner project",
                true,
                false,
                126.0,
                height,
            )
            .clicked()
            {
                self.save(true);
            }
            if command_tile(
                ui,
                "Template",
                "Save as Template",
                "Save current layout as a reusable local template",
                true,
                false,
                126.0,
                height,
            )
            .clicked()
            {
                self.begin_save_template();
            }
        });
        divider(ui, height);
        group(ui, "Printing", 144.0, height, |ui| {
            if command_tile(
                ui,
                "Formats",
                "Print",
                "Print preview and printer selection",
                true,
                false,
                144.0,
                height,
            )
            .clicked()
            {
                self.begin_export(true);
            }
        });
    }
    pub(super) fn restore_ribbon(
        &mut self,
        ui: &mut egui::Ui,
        height: f32,
    ) -> Vec<(&'static str, Response)> {
        let mut responses = vec![];
        {
            let r = command_tile(
                ui,
                "Revert",
                "Revert to Original",
                "Restore the original screenshot; Undo restores your edits",
                !self.shell.library && self.session.document.can_revert(),
                false,
                72.0,
                height,
            );
            if r.clicked() {
                self.execute(Command::RevertOriginal);
                self.status =
                    "Original image restored. Undo restores the previous image state.".into();
            }
            responses.push(("Revert", r));
        }
        responses
    }
}

#[cfg(test)]
mod tests;
