use super::*;

fn page_rect(d: &Document, page: usize, t: Transform) -> egui::Rect {
    let g = d.guide.as_ref().unwrap();
    let size = g.pages.as_ref().map_or([d.width, d.height], |p| p.size);
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
            let y = rect.top() - 32.;
            let centers = [
                rect.left() + 14.,
                rect.center().x - if g.page_count() > 1 { 16. } else { 0. },
                rect.center().x + 16.,
                rect.right() - 14.,
            ];
            (
                page,
                centers.map(|x| {
                    egui::Rect::from_center_size(egui::pos2(x, y + 14.), egui::vec2(28., 28.))
                }),
            )
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
            p.rect_stroke(
                egui::Rect::from_center_size(c - egui::vec2(2., 0.), egui::vec2(12., 16.)),
                1,
                stroke,
                egui::StrokeKind::Inside,
            );
            let plus = c + egui::vec2(5., 3.);
            p.line_segment(
                [plus - egui::vec2(4., 0.), plus + egui::vec2(4., 0.)],
                stroke,
            );
            p.line_segment(
                [plus - egui::vec2(0., 4.), plus + egui::vec2(0., 4.)],
                stroke,
            );
        }
        _ => {
            p.rect_stroke(
                egui::Rect::from_center_size(c + egui::vec2(0., 2.), egui::vec2(10., 12.)),
                1,
                stroke,
                egui::StrokeKind::Inside,
            );
            p.line_segment([c + egui::vec2(-7., -5.), c + egui::vec2(7., -5.)], stroke);
            p.line_segment([c + egui::vec2(-2., -8.), c + egui::vec2(2., -8.)], stroke);
            for x in [-2., 2.] {
                p.line_segment([c + egui::vec2(x, -1.), c + egui::vec2(x, 5.)], stroke);
            }
        }
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("page-button", _page, index)), rect));
    response
}

#[derive(Clone)]
struct PageDraft {
    width: u32,
    height: u32,
    automatic: bool,
}

impl Editor {
    pub(super) fn page_controls_hovered(&self, p: egui::Pos2, t: Transform) -> bool {
        buttons(&self.session.document, t)
            .iter()
            .any(|(_, rects)| rects.iter().any(|r| r.contains(p)))
    }
    pub(super) fn show_page_controls(&mut self, ui: &mut egui::Ui, t: Transform) {
        let Some(g) = self.session.document.guide.clone() else {
            return;
        };
        let mut action = None;
        for (page, rects) in buttons(&self.session.document, t) {
            let rect = page_rect(&self.session.document, page, t);
            let painter = ui.painter().with_clip_rect(t.viewport);
            if page > 0
                && let Some(pages) = &g.pages
            {
                let previous = page_rect(&self.session.document, page - 1, t);
                let gap = if pages.horizontal {
                    egui::Rect::from_min_max(
                        egui::pos2(previous.right(), rect.top()),
                        egui::pos2(rect.left(), rect.bottom()),
                    )
                } else {
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left(), previous.bottom()),
                        egui::pos2(rect.right(), rect.top()),
                    )
                };
                painter.rect_filled(gap, 0, ui.visuals().panel_fill);
            }
            painter.rect_stroke(
                rect,
                0,
                egui::Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color),
                egui::StrokeKind::Inside,
            );
            for (index, button_rect) in rects.into_iter().enumerate() {
                if !t.viewport.contains_rect(button_rect) || (index == 2 && g.page_count() == 1) {
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
                        egui::Popup::menu(&response)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                ui.label(format!("Delete page {} and its contents?", page + 1));
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Cancel").clicked() {
                                            ui.close();
                                        }
                                        if ui
                                            .add(
                                                egui::Button::new("Delete")
                                                    .fill(egui::Color32::from_rgb(178, 48, 57)),
                                            )
                                            .clicked()
                                        {
                                            action = Some(Action::DeletePage { page });
                                            ui.close();
                                        }
                                    },
                                );
                            });
                    }
                    _ => {}
                }
            }
        }
        if let Some(action) = action {
            self.apply_section(action, t);
        }
    }

    pub(crate) fn guide_page_settings(&mut self, ui: &mut egui::Ui) {
        let Some(g) = self.session.document.guide.clone() else {
            return;
        };
        let response = ui.button("Page size…");
        let key = egui::Id::unique("guide-page-size-draft");
        let initial = || PageDraft {
            width: g.page_width(self.session.document.width),
            height: g
                .pages
                .as_ref()
                .map_or(self.session.document.height, |p| p.size[1]),
            automatic: g.pages.as_ref().is_none_or(|p| p.auto_overflow),
        };
        if response.clicked() {
            ui.ctx().data_mut(|d| d.insert_temp(key, initial()));
        }
        let mut action = None;
        egui::Popup::menu(&response).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(285.);
            let mut draft = ui.ctx().data(|d|d.get_temp::<PageDraft>(key)).unwrap_or_else(initial);
            ui.horizontal(|ui| {
                for (label,size) in [("Letter",[816,1056]),("A4",[794,1123]),("Landscape",[1056,816])] {
                    if ui.button(label).clicked() { [draft.width,draft.height] = size; }
                }
            });
            crate::property_layout::section_help(ui,"Dimensions","Pixels. Width changes scale objects proportionally. Overflow moves whole sections to new pages; a section taller than a page must be shortened or given a taller page.");
            ui.horizontal(|ui| { ui.label("Width"); ui.add(egui::DragValue::new(&mut draft.width).range(128..=32768)); });
            ui.horizontal(|ui| { ui.label("Height"); ui.add(egui::DragValue::new(&mut draft.height).range(128..=32768)); });
            crate::controls::toggle(ui,&mut draft.automatic,"Automatically add overflow pages");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                if ui.button("Cancel").clicked() { ui.close(); }
                if ui.add(egui::Button::new("Apply").fill(ui.visuals().selection.bg_fill)).clicked() {
                    action = Some(Action::PageSize { width: draft.width, height: draft.height, auto_overflow: draft.automatic }); ui.close();
                }
            });
            ui.ctx().data_mut(|d|d.insert_temp(key,draft));
        });
        if ui.button("Distribute sections").on_hover_text("Fill pages in reading order and create additional pages for overflow. Undo restores the layout.").clicked() { action = Some(Action::Paginate); }
        if let Some(action) = action
            && self.commit_text()
        {
            self.execute(Command::Guide { action });
        }
    }
}
