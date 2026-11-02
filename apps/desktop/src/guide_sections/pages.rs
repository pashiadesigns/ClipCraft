use super::*;

pub(super) fn page_rect(d: &Document, page: usize, t: Transform) -> egui::Rect {
    let g = d.guide.as_ref().unwrap();
    let size = g.page_size(page, [d.width, d.height]);
    egui::Rect::from_min_size(
        t.screen_point(g.page_offset(page)),
        egui::vec2(size[0] as f32, size[1] as f32) * t.scale,
    )
}
fn buttons(d: &Document, t: Transform) -> Vec<(usize, [egui::Rect; 4])> {
    let Some(g) = &d.guide else { return vec![] };
    (0..g.page_count())
        .map(|page| {
            let rect = page_rect(d, page, t);
            let rail = rail_width(d, t);
            let x = [rect.left() - rail * 0.5, rect.right() + rail * 0.5];
            let clamp = |y: f32| {
                y.clamp(
                    rect.top() + 14.,
                    (rect.bottom() - 14.).max(rect.top() + 14.),
                )
            };
            (
                page,
                [
                    (x[0], clamp(rect.top() + 18.)),
                    (x[1], clamp(rect.top() + 52.)),
                    (x[0], clamp(rect.top() + 52.)),
                    (x[1], clamp(rect.top() + 18.)),
                ]
                .map(|(x, y)| egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(28., 28.))),
            )
        })
        .collect()
}
pub(super) fn button_rects(d: &Document, t: Transform) -> Vec<egui::Rect> {
    if rail_width(d, t) < 32. {
        return vec![];
    }
    let count = d.guide.as_ref().map_or(0, |g| g.page_count());
    buttons(d, t)
        .into_iter()
        .flat_map(|(_, rects)| {
            rects
                .into_iter()
                .enumerate()
                .filter_map(move |(i, r)| (i != 2 || count > 1).then_some(r))
        })
        .collect()
}
fn icon_button(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    _page: usize,
    index: usize,
    enabled: bool,
) -> egui::Response {
    let names = [
        "Move page before previous",
        "Add Page",
        "Delete Page",
        "Move page after next",
    ];
    let mut button = egui::Button::new("");
    if index == 2 {
        button = button.fill(egui::Color32::from_rgb(178, 48, 57));
    }
    let response = ui
        .add_enabled_ui(enabled, |ui| ui.put(rect, button))
        .inner
        .on_hover_text(names[index]);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, names[index]));
    let c = rect.center();
    let p = ui.painter();
    let stroke = egui::Stroke::new(
        1.6,
        if enabled {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_gray(95)
        },
    );
    match index {
        0 | 3 => {
            let dir = if index == 0 { -1. } else { 1. };
            p.line_segment(
                [c - egui::vec2(7. * dir, 0.), c + egui::vec2(7. * dir, 0.)],
                stroke,
            );
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(1. * dir, -6.),
                    c + egui::vec2(7. * dir, 0.),
                    c + egui::vec2(1. * dir, 6.),
                ],
                stroke,
            ));
        }
        1 => {
            p.add(egui::Shape::line(
                vec![
                    c + egui::vec2(-2., -8.),
                    c + egui::vec2(-9., -8.),
                    c + egui::vec2(-9., 8.),
                    c + egui::vec2(1., 8.),
                    c + egui::vec2(1., -5.),
                    c + egui::vec2(-2., -8.),
                    c + egui::vec2(-2., -5.),
                    c + egui::vec2(1., -5.),
                ],
                stroke,
            ));
            let plus = c + egui::vec2(7., 1.);
            p.line_segment(
                [plus - egui::vec2(4., 0.), plus + egui::vec2(4., 0.)],
                stroke,
            );
            p.line_segment(
                [plus - egui::vec2(0., 4.), plus + egui::vec2(0., 4.)],
                stroke,
            );
        }
        _ => crate::history::actions::icon(ui, c, "Delete", stroke.color),
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("page-button", _page, index)), rect));
    response
}

impl Editor {
    pub(super) fn show_page_controls(
        &mut self,
        ui: &mut egui::Ui,
        t: Transform,
        visible: &[egui::Rect],
    ) {
        let Some(g) = self.session.document.guide.clone() else {
            return;
        };
        let mut action = None;
        for (page, rects) in buttons(&self.session.document, t) {
            for (index, button_rect) in rects.into_iter().enumerate() {
                if !t.viewport.contains_rect(button_rect)
                    || !visible.contains(&button_rect)
                    || (index == 2 && g.page_count() == 1)
                {
                    continue;
                }
                let enabled = match index {
                    0 => page > 0,
                    3 => page + 1 < g.page_count(),
                    _ => true,
                };
                let response = icon_button(ui, button_rect, page, index, enabled);
                match index {
                    0 | 3 if response.clicked() => {
                        action = Some(Action::MovePage {
                            page,
                            index: if index == 0 { page - 1 } else { page + 1 },
                        })
                    }
                    1 => {
                        egui::Popup::menu(&response).show(|ui| {
                            for (label, copy) in
                                [("Blank Page", false), ("Copy of this Page", true)]
                            {
                                if ui.button(label).clicked() {
                                    action = Some(Action::AddPage { page, copy });
                                    ui.close();
                                }
                            }
                        });
                    }
                    2 => {
                        let _popup = egui::Popup::menu(&response)
                            .width(230.)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                ui.set_width(230.);
                                ui.strong(format!("Delete page {}?", page + 1));
                                ui.label("This removes the page and its contents.");
                                let (delete, cancel) = crate::controls::dialog_actions(
                                    ui, "Delete", "Cancel", true, true,
                                );
                                if delete {
                                    action = Some(Action::DeletePage { page });
                                    ui.close();
                                }
                                if cancel {
                                    ui.close();
                                }
                            });
                        #[cfg(test)]
                        if let Some(popup) = _popup {
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(
                                    egui::Id::unique("delete-page-popup"),
                                    popup.response.rect,
                                )
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(action) = action {
            self.apply_section(action, t);
        }
    }
}

pub(super) fn rail_width(d: &Document, t: Transform) -> f32 {
    (d.guide
        .as_ref()
        .and_then(|g| g.pages.as_ref())
        .map_or(capture_core::guide_sections::PAGE_GAP, |p| p.gap) as f32
        * t.scale
        * 0.35)
        .min(40.)
}
impl Editor {
    /// Structural page separation is independent of annotation visibility and gestures.
    pub(crate) fn paint_page_surrounds(&self, ui: &egui::Ui, t: Transform) {
        let d = self
            .guide_drag
            .as_ref()
            .map_or(&self.session.document, |v| &v.preview);
        let Some(g) = &d.guide else {
            return;
        };
        let p = ui.painter().with_clip_rect(t.viewport);
        let dark = egui::Color32::from_rgb(18, 21, 22);
        let width = rail_width(d, t);
        for i in 0..g.page_count() {
            let r = page_rect(d, i, t);
            if i + 1 < g.page_count() {
                let next = page_rect(d, i + 1, t);
                p.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(r.right(), t.image.top()),
                        egui::pos2(next.left(), t.image.bottom()),
                    ),
                    0,
                    dark,
                );
            }
            if r.bottom() < t.image.bottom() {
                p.rect_filled(
                    egui::Rect::from_min_max(
                        r.left_bottom(),
                        egui::pos2(r.right(), t.image.bottom()),
                    ),
                    0,
                    dark,
                );
            }
        }
        for i in 0..g.page_count() {
            let r = page_rect(d, i, t);
            for x in [r.left() - width, r.right()] {
                p.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, r.top()),
                        egui::vec2(width, r.height()),
                    ),
                    0,
                    crate::shell::BAR,
                );
            }
            p.rect_stroke(
                r,
                0,
                egui::Stroke::new(1., egui::Color32::from_gray(100)),
                egui::StrokeKind::Inside,
            );
        }
    }
}
