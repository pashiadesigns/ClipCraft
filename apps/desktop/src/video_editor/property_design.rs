//! Shared spacing and disclosures for the approved Properties layout.
use super::*;
use crate::tips::Tip as _;
use egui::{Color32, FontId, Rect, Sense, Stroke};
pub(crate) fn begin(ui: &egui::Ui, key: impl std::hash::Hash + std::fmt::Debug) {
    let id = egui::Id::unique((
        "properties-auto-fit",
        key,
        ui.available_width().round() as i32,
    ));
    ui.ctx().data_mut(|d| {
        let expanded = d.get_temp::<bool>(id).unwrap_or(true);
        d.insert_temp(egui::Id::unique("properties-auto-key"), id);
        d.insert_temp(egui::Id::unique("properties-auto-open"), expanded);
        d.insert_temp(egui::Id::unique("properties-extra-height"), 0f32);
    });
}
pub(crate) fn finish(ui: &egui::Ui, content: f32, available: f32) {
    let changed = ui.ctx().data_mut(|d| {
        let extra = d
            .get_temp::<f32>(egui::Id::unique("properties-extra-height"))
            .unwrap_or(0.);
        let Some(id) = d.get_temp::<egui::Id>(egui::Id::unique("properties-auto-key")) else {
            return false;
        };
        let expanded = content + extra <= available - 2.;
        let old = d.get_temp::<bool>(id).unwrap_or(true);
        d.insert_temp(id, expanded);
        old != expanded
    });
    if changed {
        ui.ctx().request_repaint();
        if !ui.input(|i| i.pointer.any_pressed() || i.pointer.any_released()) {
            ui.ctx()
                .request_discard("Fit Properties sections to available space");
        }
    }
}
pub(crate) fn heading(ui: &mut egui::Ui, glyph: &str, title: &str) {
    ui.add_space(10.);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(17., 22.), Sense::hover());
        super::ui::icon(ui.painter(), r, glyph, Color32::from_rgb(169, 196, 207));
        ui.add_space(3.);
        ui.label(crate::panel_ui::title(title, 13.));
    });
    ui.add_space(4.);
}
/// The Presets heading with Manage presets at the right end of its row
/// (BY11). True when the link was clicked.
pub(crate) fn presets_heading(ui: &mut egui::Ui) -> bool {
    ui.add_space(10.);
    let row = ui
        .horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(17., 22.), Sense::hover());
            super::ui::icon(ui.painter(), r, "Presets", Color32::from_rgb(169, 196, 207));
            ui.add_space(3.);
            ui.label(crate::panel_ui::title("Presets", 13.));
        })
        .response
        .rect;
    ui.add_space(4.);
    let row = Rect::from_min_max(row.min, egui::pos2(ui.max_rect().right(), row.max.y));
    crate::manage_presets::header_link(ui, row, 0.)
}
/// The collapsible Presets group with Manage presets on its header row,
/// left of the chevron (BY11). True when the link was clicked.
pub(crate) fn presets_group(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) -> bool {
    group_with_link(ui, "Presets", "Presets", "", false, true, body).1
}
pub(crate) fn end(ui: &mut egui::Ui) {
    ui.add_space(8.);
    crate::panel_ui::divider(ui);
}
pub(crate) fn group(
    ui: &mut egui::Ui,
    glyph: &str,
    title: &str,
    summary: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    group_forced(ui, glyph, title, summary, false, body);
}
/// A smaller collapsible section nested inside a [`group`] (Entrance, Exit and
/// Motion curve inside Motion). Collapsed by default; manual choices last the session.
pub(crate) fn subgroup(
    ui: &mut egui::Ui,
    title: &str,
    summary: &str,
    body: impl FnOnce(&mut egui::Ui),
) {
    // Session-wide per title, so a manual choice carries across objects.
    let id = egui::Id::unique(("property-subgroup", title));
    // BA9: Entrance, Exit and Motion curve start collapsed; their summaries show the values.
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    egui::Frame::new()
        .stroke(Stroke::new(1., Color32::from_rgb(52, 61, 64)))
        .corner_radius(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let (r, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.), Sense::click());
            if response.clicked() {
                open = !open;
                ui.ctx().data_mut(|d| d.insert_temp(id, open));
            }
            let p = ui.painter();
            if response.hovered() {
                p.rect_filled(r, 7., Color32::from_white_alpha(5));
            }
            super::ui::icon(
                p,
                Rect::from_center_size(
                    egui::pos2(r.left() + 14., r.center().y),
                    egui::vec2(12., 12.),
                ),
                if open { "Down" } else { "Expand section" },
                Color32::LIGHT_GRAY,
            );
            p.text(
                egui::pos2(r.left() + 26., r.center().y),
                egui::Align2::LEFT_CENTER,
                title,
                crate::panel_ui::font(12.5),
                Color32::WHITE,
            );
            let galley = p.layout_no_wrap(
                summary.into(),
                FontId::proportional(11.),
                Color32::from_rgb(181, 194, 199),
            );
            if galley.size().x + 110. < r.width() {
                p.galley(
                    egui::pos2(
                        r.right() - 10. - galley.size().x,
                        r.center().y - galley.size().y / 2.,
                    ),
                    galley,
                    Color32::LIGHT_GRAY,
                );
            }
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    egui::Id::unique(("property-subgroup-state", title)),
                    (r, open),
                )
            });
            if open {
                egui::Frame::new()
                    .inner_margin(egui::Margin {
                        left: 10,
                        right: 10,
                        top: 0,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.push_id(id, |ui| body(ui));
                    });
            }
        });
}
pub(super) fn group_forced(
    ui: &mut egui::Ui,
    glyph: &str,
    title: &str,
    summary: &str,
    force: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    group_with_link(ui, glyph, title, summary, force, false, body).0
}
fn group_with_link(
    ui: &mut egui::Ui,
    glyph: &str,
    title: &str,
    summary: &str,
    force: bool,
    link: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> (bool, bool) {
    let id = ui.make_persistent_id(("property-group", title));
    let (mut manual, automatic, height) = ui.ctx().data(|d| {
        (
            d.get_temp::<bool>(id),
            d.get_temp::<bool>(egui::Id::unique("properties-auto-open"))
                .unwrap_or(false),
            d.get_temp::<f32>(id.with("height")),
        )
    });
    let mut open = manual.unwrap_or(automatic || height.is_none()) || force;
    let (r, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.), Sense::click());
    if response.clicked() {
        response.surrender_focus();
        open = !open;
        manual = Some(open);
        ui.ctx().data_mut(|d| d.insert_temp(id, open));
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, open, title));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("property-group-state", title)), (r, open)));
    let p = ui.painter();
    if response.hovered() {
        p.rect_filled(r, 4., Color32::from_white_alpha(5));
    }
    super::ui::icon(
        p,
        Rect::from_center_size(
            egui::pos2(r.left() + 9., r.center().y),
            egui::vec2(17., 17.),
        ),
        glyph,
        Color32::from_rgb(169, 196, 207),
    );
    let label = p.layout_no_wrap(title.into(), crate::panel_ui::font(13.), Color32::WHITE);
    let label_width = label.size().x;
    p.galley(
        egui::pos2(r.left() + 27., r.center().y - 8.),
        label,
        Color32::WHITE,
    );
    if r.width() - label_width > 100. {
        let galley = p.layout_no_wrap(
            summary.into(),
            FontId::proportional(11.),
            Color32::from_rgb(181, 194, 199),
        );
        if galley.size().x + label_width + 60. < r.width() {
            p.galley(
                egui::pos2(r.right() - 24. - galley.size().x, r.center().y - 7.),
                galley,
                Color32::LIGHT_GRAY,
            );
        }
    }
    super::ui::icon(
        p,
        Rect::from_center_size(
            egui::pos2(r.right() - 8., r.center().y),
            egui::vec2(14., 14.),
        ),
        if open { "Down" } else { "Expand section" },
        Color32::LIGHT_GRAY,
    );
    let clicked = link && crate::manage_presets::header_link(ui, r, 24.);
    if open {
        let top = ui.next_widget_position().y;
        ui.push_id(id, |ui| body(ui));
        ui.add_space(8.);
        let height = ui.next_widget_position().y - top;
        ui.ctx()
            .data_mut(|d| d.insert_temp(id.with("height"), height));
    } else if manual.is_none() {
        ui.ctx().data_mut(|d| {
            let key = egui::Id::unique("properties-extra-height");
            let extra = d.get_temp::<f32>(key).unwrap_or(0.);
            d.insert_temp(key, extra + height.unwrap_or(0.));
        });
    }
    crate::panel_ui::divider(ui);
    (open, clicked)
}
pub(super) fn percent(ui: &mut egui::Ui, label: &str, value: &mut f32, max: f32) {
    ui.label(label);
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 82.).max(40.);
        ui.spacing_mut().slider_width = width;
        ui.visuals_mut().selection.bg_fill = if label == "Volume" {
            volume_color(*value / 100.)
        } else {
            crate::shell::ACCENT
        };
        crate::controls::slider(ui, egui::Slider::new(value, 0. ..=max).show_value(false));
        ui.add_sized(
            [70., 28.],
            egui::DragValue::new(value)
                .range(0. ..=max)
                .speed(1.)
                .suffix(" %"),
        );
    });
}
pub(super) fn preset(
    ui: &mut egui::Ui,
    name: &str,
    index: usize,
    selected: bool,
    width: f32,
) -> bool {
    let (r, response) = ui.allocate_exact_size(egui::vec2(width, 74.), Sense::click());
    let p = ui.painter();
    p.rect_filled(
        r,
        5.,
        if response.hovered() {
            Color32::from_rgb(48, 61, 67)
        } else {
            Color32::from_rgb(40, 47, 50)
        },
    );
    let icon = Rect::from_center_size(
        r.center_top() + egui::vec2(0., 25.),
        egui::vec2(if index == 2 { 39. } else { 32. }, 24.),
    );
    if index == 1 {
        p.circle(
            icon.center(),
            12.,
            Color32::from_rgb(82, 116, 127),
            Stroke::new(1.5, Color32::from_rgb(176, 220, 233)),
        );
    } else {
        p.rect(
            icon,
            if index == 0 { 5. } else { 1. },
            Color32::from_rgb(82, 116, 127),
            Stroke::new(1.5, Color32::from_rgb(176, 220, 233)),
            egui::StrokeKind::Inside,
        );
    }
    p.text(
        r.center_bottom() - egui::vec2(0., 17.),
        egui::Align2::CENTER_CENTER,
        name,
        FontId::proportional(12.),
        Color32::WHITE,
    );
    if selected {
        p.rect_stroke(
            r,
            5.,
            Stroke::new(2., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    response.tip_title(name).clicked()
}
impl Editor {
    pub(super) fn video_annotation_style(&mut self, ui: &mut egui::Ui) {
        if self.mixed_selection() {
            self.mixed_style_groups(ui);
            return;
        }
        if self
            .selected_annotation()
            .is_some_and(|a| a.focus.is_some())
        {
            self.focus_properties(ui, self.selected_annotation());
            return;
        }
        if self.shell.focus.active
            && matches!(
                self.tool,
                Some(crate::Kind::Rectangle | crate::Kind::Ellipse)
            )
            && self.selected_annotation().is_none()
        {
            self.focus_properties(ui, None);
            return;
        }
        let Some(mut a) = self.selected_annotation() else {
            self.image_properties(ui);
            return;
        };
        if self.text_draft.is_some() {
            self.inline_text_properties(ui);
            return;
        }
        if a.kind != crate::Kind::Text {
            if presets_group(ui, |ui| {
                if matches!(a.kind, crate::Kind::Arrow | crate::Kind::Line) {
                    self.arrow_styles(ui);
                } else {
                    self.object_templates(ui, a.kind);
                }
            }) {
                self.manage_presets_from(ui.ctx(), Some(a.kind));
            }
            let before = a.clone();
            let mut selection_edit = None;
            let canvas = [self.session.document.width, self.session.document.height];
            match a.kind {
                crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate => {
                    heading(ui, "Sensitive search", "Privacy & Security");
                    crate::obscure_properties::show(ui, &mut a);
                    ui.weak(crate::privacy_review::VIDEO_LIMITATION);
                    end(ui);
                }
                crate::Kind::Image => {
                    heading(ui, "Style", "Appearance");
                    if a.image
                        .as_ref()
                        .is_some_and(|image| image.vector().is_some())
                    {
                        crate::image_frame::properties(ui, &mut a);
                    }
                    crate::property_layout::slider(
                        ui,
                        "Opacity",
                        egui::Slider::new(&mut a.color[3], 0..=255),
                    );
                    crate::shadow_properties::show(ui, &mut a);
                    end(ui);
                }
                _ => {
                    // Properties audit: the same Appearance group as images.
                    heading(ui, "Style", "Appearance");
                    selection_edit = self.appearance_for_selection(ui, &mut a);
                    end(ui);
                }
            }
            if a.kind == crate::Kind::Step {
                heading(ui, "Steps", "Marker");
                crate::image_properties::marker_fields(ui, &mut a, canvas);
                end(ui);
            }
            if a.is_image_frame() {
                group(ui, "Crop", "Crop image", "", |ui| {
                    crate::image_frame::properties(ui, &mut a)
                });
            }
            // Text and Text fitting live on the Text tab (bh-proposal).
            let mut rotation = false;
            let placement = placement_summary(&a);
            group(ui, "Position", "Position & size", &placement, |ui| {
                geometry_fields(
                    ui,
                    &mut a,
                    [self.session.document.width, self.session.document.height],
                );
                rotation = crate::property_layout::rotation_controls(ui, &mut a)
                    .is_some_and(|r| r[0].changed() || r[1].clicked());
            });
            if let Some((seen, edited)) = selection_edit {
                self.update_text_sidebar_seen(edited, false, Some(seen));
            }
            if a != before || rotation {
                self.update_text_sidebar(a, rotation);
            }
            return;
        }
        // Text objects follow the same order: Presets, Appearance, Text,
        // Text fitting, Position & size (properties audit).
        if presets_group(ui, |ui| {
            self.object_templates(ui, crate::Kind::Text);
        }) {
            self.manage_presets_from(ui.ctx(), Some(crate::Kind::Text));
        }
        let Some(mut a) = self.selected_annotation() else {
            return;
        };
        let before = a.clone();
        heading(ui, "Style", "Appearance");
        let selection_edit = self.appearance_for_selection(ui, &mut a);
        end(ui);
        heading(ui, "Text", "Text");
        if let Some(text) = a.text.as_mut() {
            let content = text.content.clone();
            if ui
                .add_sized(
                    [ui.available_width(), 62.],
                    crate::unicode_input::TextEdit::multiline(&mut text.content)
                        .char_limit(100_000),
                )
                .changed()
            {
                capture_core::text::rich::rebase(text, &content, None);
            }
            font_fields(ui, text);
        }
        if let Some((seen, edited)) = selection_edit {
            self.update_text_sidebar_seen(edited, false, Some(seen));
        } else if a != before {
            self.update_text_sidebar(a.clone(), false);
        }
        self.common_paragraph_controls(ui);
        self.show_justification_grid(ui);
        self.rich_text_buttons(ui);
        end(ui);
        group(ui, "Text", "Text fitting", "", |ui| {
            let Some(mut a) = self.selected_annotation() else {
                return;
            };
            let before = a.clone();
            crate::text_properties::advanced(ui, &mut a);
            if a != before {
                self.update_text_sidebar(a, false);
            }
        });
        let placement = placement_summary(&a);
        group(ui, "Position", "Position & size", &placement, |ui| {
            let Some(mut a) = self.selected_annotation() else {
                return;
            };
            let before = a.clone();
            geometry_fields(
                ui,
                &mut a,
                [self.session.document.width, self.session.document.height],
            );
            let rotation = crate::property_layout::rotation_controls(ui, &mut a)
                .is_some_and(|r| r[0].changed() || r[1].clicked());
            if a != before || rotation {
                self.update_text_sidebar(a, rotation);
            }
        });
    }
}

/// Font, size, bold/italic/underline and alignment, for text in any object.
fn font_fields(ui: &mut egui::Ui, text: &mut capture_core::text::TextStyle) {
    ui.columns(2, |col| {
        col[0].label("Font");
        col[1].label("Size");
        egui::ComboBox::from_id_salt("video-text-font")
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(col[0].available_width())
            .selected_text(text.font.label())
            .show_ui(&mut col[0], |ui| {
                let id = egui::Id::unique("video-font-search");
                let mut q = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
                ui.add(
                    crate::unicode_input::TextEdit::singleline(&mut q)
                        .hint_text("Search installed fonts"),
                );
                ui.ctx().data_mut(|d| d.insert_temp(id, q.clone()));
                use capture_core::text::TextFont;
                for f in [TextFont::UbuntuLight, TextFont::HackRegular]
                    .into_iter()
                    .chain(capture_core::text::system_fonts::choices())
                    .filter(|f| f.label().to_lowercase().contains(&q.to_lowercase()))
                {
                    if ui.selectable_value(&mut text.font, f, f.label()).clicked() {
                        ui.close();
                    }
                }
            });
        let width = col[1].available_width();
        col[1].add_sized(
            [width, 28.],
            egui::DragValue::new(&mut text.size_points)
                .range(6. ..=144.)
                .speed(0.25)
                .suffix(" pt"),
        );
    });
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 40.) / 6.;
        for (label, value) in [
            ("B", &mut text.bold),
            ("I", &mut text.italic),
            ("U", &mut text.underline),
        ] {
            if ui
                .add_sized(
                    [width, 28.],
                    egui::Button::new(match label {
                        "B" => crate::panel_ui::title(label, 13.),
                        "I" => egui::RichText::new(label).font(egui::FontId::new(
                            13.,
                            crate::text_layout::family(&capture_core::text::TextStyle {
                                italic: true,
                                ..Default::default()
                            }),
                        )),
                        _ => egui::RichText::new(label).underline(),
                    })
                    .selected(*value),
                )
                .clicked()
            {
                *value = !*value;
            }
        }
        use capture_core::text::HorizontalAlignment as H;
        for (glyph, value) in [
            ("Align left", H::Left),
            ("Align center", H::Center),
            ("Align right", H::Right),
        ] {
            if super::ui::button(ui, glyph, "", width, text.horizontal == value)
                .tip_title(glyph)
                .clicked()
            {
                text.horizontal = value;
            }
        }
    });
}

pub(crate) fn geometry_fields(ui: &mut egui::Ui, a: &mut crate::Annotation, canvas: [u32; 2]) {
    let (lo, hi) = a.body_bounds();
    let mut values = [lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]];
    let original = values;
    for row in 0..2 {
        ui.columns(2, |columns| {
            for (column, ui) in columns.iter_mut().enumerate() {
                let index = row * 2 + column;
                ui.label(["X", "Y", "Width", "Height"][index]);
                ui.add_sized(
                    [ui.available_width(), 28.],
                    egui::DragValue::new(&mut values[index])
                        .speed(1.)
                        .range(if index < 2 { 0. } else { 1. }..=canvas[index % 2] as f32)
                        .suffix(" px"),
                );
            }
        });
    }
    if values != original {
        let mut object = a.clone();
        let rotation = object.rotation_degrees;
        let mirror = object.mirror_x;
        object.mirror_x = false;
        object.rotation_degrees = 0.;
        if let Ok(resized) = object.resized(
            capture_core::selection::Handle::BottomRight,
            [lo[0] + values[2], lo[1] + values[3]],
            canvas,
        ) {
            object = resized;
        }
        if let Ok(moved) = object.translated([values[0] - lo[0], values[1] - lo[1]], canvas) {
            object = moved;
        }
        object.rotation_degrees = rotation;
        object.mirror_x = mirror;
        *a = object;
    }
}

fn placement_summary(a: &crate::Annotation) -> String {
    let (lo, hi) = a.body_bounds();
    format!(
        "{:.0} × {:.0} px · {:.0}°",
        hi[0] - lo[0],
        hi[1] - lo[1],
        a.rotation_degrees
    )
}

pub(super) fn volume_color(gain: f32) -> Color32 {
    let t = (gain - 1.).abs().clamp(0., 1.);
    let a = crate::shell::ACCENT;
    let b = Color32::from_rgb(226, 89, 87);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
