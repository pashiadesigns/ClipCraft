//! Non-ribbon navigation and settings styling. Ribbon defaults remain untouched.
use crate::egui;
use crate::tips::Tip as _;
use egui::{Color32, FontId, Response, RichText, Sense, Stroke};

pub(crate) const RULE: Color32 = Color32::from_rgb(81, 92, 96);
pub(crate) const MUTED: Color32 = Color32::from_rgb(177, 195, 202);
pub(crate) fn font(size: f32) -> FontId {
    FontId::new(
        size,
        crate::text_layout::family(&capture_core::text::TextStyle {
            bold: true,
            ..Default::default()
        }),
    )
}
pub(crate) fn title(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(font(size)).color(Color32::WHITE)
}
pub(crate) fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.), Sense::hover());
    ui.painter()
        .line_segment([r.left_center(), r.right_center()], Stroke::new(1., RULE));
}
pub(crate) fn rail<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(crate::tabs::BACKGROUND)
        .corner_radius(5)
        .inner_margin(3)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            body(ui)
        })
        .inner
}
pub(crate) fn tab(
    ui: &mut egui::Ui,
    label: &str,
    glyph: &str,
    selected: bool,
    width: f32,
    show_label: bool,
) -> Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(width, 30.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), selected, label)
    });
    let p = ui.painter();
    p.rect_filled(
        r,
        4.,
        if selected {
            crate::tabs::ACTIVE_BLUE
        } else if response.hovered() || response.has_focus() {
            Color32::from_rgb(54, 64, 69)
        } else {
            Color32::TRANSPARENT
        },
    );
    let text = p.layout_no_wrap(label.into(), font(12.), Color32::WHITE);
    let show_label = show_label && text.size().x + 32. <= width;
    let x = if show_label {
        r.center().x - (text.size().x + 23.) / 2. + 8.
    } else {
        r.center().x
    };
    crate::video_editor::ui::icon(
        p,
        egui::Rect::from_center_size(egui::pos2(x, r.center().y), egui::vec2(16., 16.)),
        glyph,
        Color32::WHITE,
    );
    if show_label {
        p.galley(
            egui::pos2(x + 15., r.center().y - text.size().y / 2.),
            text,
            Color32::WHITE,
        );
    }
    if response.has_focus() {
        p.rect_stroke(
            r,
            4.,
            Stroke::new(1., Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
    response.tip_title(label)
}
pub(crate) fn heading(ui: &mut egui::Ui, glyph: &str, label: &str) {
    ui.add_space(10.);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(17., 22.), Sense::hover());
        crate::video_editor::ui::icon(ui.painter(), r, glyph, MUTED);
        ui.label(title(label, 13.));
    });
    ui.add_space(4.);
}
pub(crate) fn setting(
    ui: &mut egui::Ui,
    label: &str,
    description: &str,
    value: &mut bool,
) -> Response {
    let mut response = None;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 54.).max(100.);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 42.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_min_width(width);
                ui.label(title(label, 13.));
                ui.label(RichText::new(description).size(12.).color(MUTED));
            },
        );
        response = Some(crate::controls::toggle(ui, value, ""));
    });
    response.unwrap().tip_title(label)
}
