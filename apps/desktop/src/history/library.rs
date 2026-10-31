use super::*;
use capture_core::history::MediaKind;

fn filter(ui: &mut egui::Ui, selected: bool, label: &str, icon: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.), egui::Sense::click());
    let visuals = ui.style().interact_selectable(&response, selected);
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 4., visuals.bg_fill);
    }
    crate::ribbon::draw_icon(
        ui.painter(),
        rect.left_center() + egui::vec2(14., 0.),
        icon,
        visuals.text_color(),
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(32., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.),
        visuals.text_color(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    response
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Order {
    #[default]
    Newest,
    Oldest,
    Name,
    Size,
}
impl Order {
    fn label(self) -> &'static str {
        match self {
            Self::Newest => "Newest first",
            Self::Oldest => "Oldest first",
            Self::Name => "Name",
            Self::Size => "Largest first",
        }
    }
}
pub(super) struct View {
    pub(super) selected: std::collections::BTreeSet<String>,
    pub(super) selecting: bool,
    year: Option<u32>,
    kind: Option<MediaKind>,
    tag: Option<String>,
    order: Order,
    zoom: f32,
}
impl Default for View {
    fn default() -> Self {
        Self {
            selected: Default::default(),
            selecting: false,
            year: None,
            kind: None,
            tag: None,
            order: Order::Newest,
            zoom: 100.0,
        }
    }
}
impl View {
    fn filtered(&self, entries: &[Entry], query: &str) -> Vec<Entry> {
        let mut result: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.matches_query(query)
                    && self.year.is_none_or(|year| e.created_date()[0] == year)
                    && self.kind.is_none_or(|kind| e.kind == kind)
                    && self
                        .tag
                        .as_ref()
                        .is_none_or(|tag| e.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            })
            .cloned()
            .collect();
        result.sort_by(|a, b| {
            let order = match self.order {
                Order::Newest => b.created_ms.cmp(&a.created_ms),
                Order::Oldest => a.created_ms.cmp(&b.created_ms),
                Order::Name => a.label.to_lowercase().cmp(&b.label.to_lowercase()),
                Order::Size => b.bytes.cmp(&a.bytes),
            };
            order
                .then_with(|| b.created_ms.cmp(&a.created_ms))
                .then_with(|| a.id.cmp(&b.id))
        });
        result
    }
}
fn grid_geometry(width: f32, percent: f32) -> (usize, egui::Vec2) {
    let width = width.clamp(1.0, 4096.0);
    let percent = if percent.is_finite() {
        percent.clamp(50.0, 200.0)
    } else {
        100.0
    };
    let size = (160.0 * percent / 100.0).min(width);
    let columns = ((width + 12.0) / (size + 12.0)).floor().max(1.0) as usize;
    (columns, egui::vec2(size, size * 0.6))
}
impl Editor {
    pub(crate) fn library_zoom(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(format!("{:.0}%", self.history.library.zoom), |ui| {
            ui.label("Library thumbnail size");
            crate::controls::slider(
                ui,
                egui::Slider::new(&mut self.history.library.zoom, 50.0..=200.0).suffix("%"),
            );
            if ui.button("Actual thumbnail size · 100%").clicked() {
                self.history.library.zoom = 100.0;
                ui.close();
            }
        })
        .response
        .on_hover_text("Change Library thumbnail size; images stay unchanged.");
    }
    pub(super) fn show_library(&mut self, ui: &mut egui::Ui) {
        let entries = self.history.entries();
        let mut years = std::collections::BTreeMap::<u32, usize>::new();
        for entry in &entries {
            *years.entry(entry.created_date()[0]).or_default() += 1;
        }
        egui::Panel::left("library_sidebar").default_size(280.0).min_size(280.0).max_size(280.0).resizable(false)
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(52,57,60)).corner_radius(egui::CornerRadius{nw:8,sw:8,ne:0,se:0}).inner_margin(egui::Margin::same(12)))
            .show(ui,|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.history.query).hint_text("Search Library").char_limit(120).desired_width(f32::INFINITY))
                    .on_hover_text("Search names, tags, capture types and dimensions. Every word must match.");
                ui.horizontal(|ui| {
                    if ui.add_enabled(!self.history.query.is_empty() || self.history.library.year.is_some() || self.history.library.kind.is_some() || self.history.library.tag.is_some(),egui::Button::new("Clear filters")).clicked() {
                        self.history.query.clear(); self.history.library.year=None; self.history.library.kind=None; self.history.library.tag=None;
                    }
                    crate::shell::badge(ui,self.shell.badges,"LIB-005");
                });
                ui.add_space(10.0);
                egui::ScrollArea::vertical().id_salt("library_categories").show(ui,|ui| {
                    for (kind, label, icon) in [(None, "All Files", "Library"), (Some(MediaKind::Image), "Images", "Image"), (Some(MediaKind::Steps), "Step Captures", "Steps")] {
                        let count = entries.iter().filter(|e| kind.is_none_or(|k| e.kind == k)).count();
                        if filter(ui,self.history.library.kind == kind,&format!("{label}  ({count})"),icon).clicked() { self.history.library.kind = kind; }
                    }
                    ui.add_enabled_ui(false,|ui|filter(ui,false,"Videos  (0)","Video"));
                    ui.separator();
                    if ui.selectable_label(self.history.library.year.is_none(),"All dates").clicked() {self.history.library.year=None;}
                    for (&year,count) in years.iter().rev() {
                        if ui.selectable_label(self.history.library.year==Some(year),format!("{year}  ({count})")).clicked() {self.history.library.year=Some(year);}
                    }
                    ui.small("Snapshot dates · UTC");
                    ui.separator();
                    ui.collapsing("Tags", |ui| {
                        let mut tags = std::collections::BTreeSet::new();
                        for e in &entries { tags.extend(e.tags.iter().cloned()); }
                        if tags.is_empty() { ui.small("Right-click a capture to add tags."); }
                        if ui.selectable_label(self.history.library.tag.is_none(), "All tags").clicked() { self.history.library.tag = None; }
                        for tag in tags { if ui.selectable_label(self.history.library.tag.as_ref() == Some(&tag), &tag).clicked() { self.history.library.tag = Some(tag); } }
                    });
                    for label in ["Applications","Websites"] {
                        ui.collapsing(label,|ui| {ui.label("Not available yet.");crate::shell::badge(ui,self.shell.badges,"LIB-002");});
                    }
                    ui.add_space(16.0);
                    ui.small("Editable Library · saves never remove older items. Manage capacity with Storage.");
                    crate::shell::badge(ui,self.shell.badges,"LIB-006");
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::same(12)))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading("Library");
                    if ui.button("Refresh").clicked() {
                        self.history.refresh();
                    }
                    if ui.button("Return to Editor").clicked() {
                        self.shell.return_to_editor();
                    }
                    egui::ComboBox::from_id_salt("library_order")
                        .selected_text(self.history.library.order.label())
                        .show_ui(ui, |ui| {
                            for order in [Order::Newest, Order::Oldest, Order::Name, Order::Size] {
                                ui.selectable_value(
                                    &mut self.history.library.order,
                                    order,
                                    order.label(),
                                );
                            }
                        });
                });
                let visible = self.history.library.filtered(&entries, &self.history.query);
                self.library_bulk_bar(ui, &visible);
                ui.label(format!("{} of {} captures", visible.len(), entries.len()));
                if !self.history.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.history.error);
                }
                if visible.is_empty() {
                    ui.add_space(24.0);
                    ui.label(if entries.is_empty() {
                        "Your local snapshots will appear here."
                    } else {
                        "No matching images. Clear filters to see all snapshots."
                    });
                    return;
                }
                let mut open = None;
                egui::ScrollArea::vertical()
                    .id_salt("library_grid_scroll")
                    .show(ui, |ui| {
                        let (columns, size) =
                            grid_geometry(ui.available_width(), self.history.library.zoom);
                        egui::Grid::new("library_grid")
                            .num_columns(columns)
                            .spacing(egui::vec2(12.0, 16.0))
                            .show(ui, |ui| {
                                for row in visible.chunks(columns) {
                                    for entry in row {
                                        ui.vertical(|ui| {
                                            ui.set_width(size.x);
                                            ui.set_max_width(size.x);
                                            if self.history.library.selecting {
                                                let selected = self
                                                    .history
                                                    .library
                                                    .selected
                                                    .contains(&entry.id);
                                                if ui
                                                    .selectable_label(
                                                        selected,
                                                        if selected {
                                                            "Selected"
                                                        } else {
                                                            "Select"
                                                        },
                                                    )
                                                    .clicked()
                                                {
                                                    if selected {
                                                        self.history
                                                            .library
                                                            .selected
                                                            .remove(&entry.id);
                                                    } else {
                                                        self.history
                                                            .library
                                                            .selected
                                                            .insert(entry.id.clone());
                                                    }
                                                }
                                            }
                                            if self.history_card(ui, entry, size) {
                                                open = Some(entry.id.clone());
                                            }
                                            let response = ui
                                                .add(
                                                    egui::Label::new(&entry.label)
                                                        .truncate()
                                                        .sense(egui::Sense::click()),
                                                )
                                                .on_hover_text(&entry.label);
                                            self.history_context(&response, entry, true);
                                            if response.clicked() {
                                                open = Some(entry.id.clone());
                                            }
                                            ui.add(
                                                egui::Label::new(format!(
                                                    "{} × {} · {}",
                                                    entry.width,
                                                    entry.height,
                                                    entry.kind.label()
                                                ))
                                                .truncate(),
                                            );
                                            ui.add(
                                                egui::Label::new(entry.created_utc()).truncate(),
                                            )
                                            .on_hover_text(entry.created_utc());
                                        });
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                if let Some(id) = open {
                    if self.history.library.selecting {
                        if !self.history.library.selected.remove(&id) {
                            self.history.library.selected.insert(id);
                        }
                    } else {
                        self.open_history(&id, &ui.ctx().clone());
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn year_query_and_sort_combine_without_mutating_history() {
        let make = |id: &str, label: &str, time, bytes| Entry {
            kind: Default::default(),
            tags: vec![],
            id: id.into(),
            label: label.into(),
            origin: Origin::Project,
            width: 640,
            height: 400,
            created_ms: time,
            bytes,
            revision: None,
            updated_ms: None,
        };
        let entries = vec![
            make("a", "Gamma", 0, 10),
            make("b", "alpha", 951_782_400_000, 30),
            make("c", "Beta", 1_793_354_469_427, 20),
        ];
        let before = entries.clone();
        let mut view = View::default();
        let ids = |items: Vec<Entry>| items.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(view.filtered(&entries, "")), ["c", "b", "a"]);
        view.order = Order::Oldest;
        assert_eq!(ids(view.filtered(&entries, "")), ["a", "b", "c"]);
        view.order = Order::Name;
        assert_eq!(ids(view.filtered(&entries, "PROJECT 640")), ["b", "c", "a"]);
        view.order = Order::Size;
        assert_eq!(ids(view.filtered(&entries, "")), ["b", "c", "a"]);
        view.year = Some(2000);
        assert_eq!(ids(view.filtered(&entries, "ALPHA")), ["b"]);
        assert!(view.filtered(&entries, "Beta").is_empty());
        view.year = Some(2020);
        assert!(view.filtered(&entries, "").is_empty());
        assert_eq!(entries, before);
        let mut classified = entries.clone();
        classified[0].kind = MediaKind::Steps;
        classified[0].tags = vec!["Guide".into()];
        view.year = None;
        view.kind = Some(MediaKind::Steps);
        view.tag = Some("guide".into());
        assert_eq!(ids(view.filtered(&classified, "")), ["a"]);
        view.kind = Some(MediaKind::Image);
        assert!(view.filtered(&classified, "").is_empty());
        view.tag = None;
        assert_eq!(view.filtered(&classified, "").len(), 2);
    }
    #[test]
    fn thumbnail_grid_stays_inside_available_width_at_zoom_extremes() {
        for width in [100.0, 320.0, 640.0, 1600.0] {
            for percent in [-20.0, 50.0, 100.0, 200.0, 500.0, f32::NAN] {
                let (columns, size) = grid_geometry(width, percent);
                assert!(columns > 0 && size.x.is_finite() && size.y.is_finite());
                assert!(size.x * columns as f32 + 12.0 * (columns - 1) as f32 <= width + 0.01);
                assert!((size.y / size.x - 0.6).abs() < 0.0001);
            }
        }
    }
}
