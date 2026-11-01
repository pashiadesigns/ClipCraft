use super::*;

fn buttons(d: &Document, t: Transform) -> Vec<(usize, egui::Rect, egui::Rect)> {
    let Some(g) = &d.guide else { return vec![] };
    (0..g.page_count())
        .map(|page| {
            let y = t.screen_point([0., g.page_origin(page) + g.top]).y - 34.;
            if page > 0 && capture_core::guide_sections::PAGE_GAP as f32 * t.scale < 36. {
                return (
                    page,
                    egui::Rect::from_min_size(
                        egui::pos2(t.image.left() - 30., y),
                        egui::vec2(26., 28.),
                    ),
                    egui::Rect::from_min_size(
                        egui::pos2(t.image.right() + 4., y),
                        egui::vec2(26., 28.),
                    ),
                );
            }
            (
                page,
                egui::Rect::from_min_size(egui::pos2(t.image.left(), y), egui::vec2(104., 28.)),
                egui::Rect::from_min_size(
                    egui::pos2(t.image.right() - 104., y),
                    egui::vec2(104., 28.),
                ),
            )
        })
        .collect()
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
            .any(|(_, add, delete)| add.contains(p) || delete.contains(p))
    }
    pub(super) fn show_page_controls(&mut self, ui: &mut egui::Ui, t: Transform) {
        let Some(g) = self.session.document.guide.clone() else {
            return;
        };
        let mut action = None;
        for (page, add, delete) in buttons(&self.session.document, t) {
            if let Some(pages) = &g.pages {
                let top = t.screen_point([0., g.page_origin(page)]).y;
                if page > 0 {
                    let gap = egui::Rect::from_min_max(
                        egui::pos2(
                            t.image.left(),
                            top - capture_core::guide_sections::PAGE_GAP as f32 * t.scale,
                        ),
                        egui::pos2(t.image.right(), top),
                    );
                    ui.painter().with_clip_rect(t.viewport).rect_filled(
                        gap,
                        0,
                        ui.visuals().panel_fill,
                    );
                }
                let rect = egui::Rect::from_min_size(
                    egui::pos2(t.image.left(), top),
                    egui::vec2(pages.size[0] as f32, pages.size[1] as f32) * t.scale,
                );
                ui.painter().with_clip_rect(t.viewport).rect_stroke(
                    rect,
                    0,
                    egui::Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color),
                    egui::StrokeKind::Inside,
                );
            }
            if t.viewport.contains_rect(add) {
                let response = ui
                    .put(
                        add,
                        egui::Button::new(if add.width() < 50. { "+" } else { "Add Page" }),
                    )
                    .on_hover_text("Add Page");
                egui::Popup::menu(&response).show(|ui| {
                    for (label, copy) in [("Blank Page", false), ("Copy of this Page", true)] {
                        if ui.button(label).clicked() {
                            action = Some(Action::AddPage { page, copy });
                            ui.close();
                        }
                    }
                });
            }
            if g.page_count() > 1 && t.viewport.contains_rect(delete) {
                let response = ui
                    .put(
                        delete,
                        egui::Button::new(if delete.width() < 50. {
                            "×"
                        } else {
                            "Delete Page"
                        }),
                    )
                    .on_hover_text("Delete Page");
                egui::Popup::menu(&response)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        ui.label(format!("Delete page {} and its contents?", page + 1));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                        });
                    });
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
            width: self.session.document.width,
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
