use super::*;
use crate::tips::Tip as _;
use capture_core::history::MediaKind;

/// The sidebar is always open with labels (approved Library layout A).
const SIDEBAR_WIDTH: f32 = 230.;
const FAINT: egui::Color32 = egui::Color32::from_rgb(127, 139, 143);
const CHIP_FILL: egui::Color32 = egui::Color32::from_rgb(31, 75, 92);
const CHIP_INK: egui::Color32 = egui::Color32::from_rgb(125, 214, 237);
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A section heading in the sidebar: a small icon and an uppercase name.
fn section_heading(ui: &mut egui::Ui, title: &str, icon: &str) -> egui::Rect {
    ui.add_space(10.);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.), egui::Sense::hover());
    super::library_icons::paint_sized(
        ui.painter(),
        egui::pos2(rect.left() + 14., rect.center().y),
        icon,
        FAINT,
        14.,
    );
    let mut job = egui::text::LayoutJob::default();
    job.append(
        &title.to_uppercase(),
        0.,
        egui::TextFormat {
            font_id: egui::FontId::proportional(10.5),
            color: FAINT,
            extra_letter_spacing: 0.8,
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        egui::pos2(rect.left() + 28., rect.center().y - galley.size().y / 2.),
        galley,
        FAINT,
    );
    rect
}

/// A sidebar row: an optional icon, the name and its count on the right.
/// `drop` marks a folder that the captures being dragged would land in.
fn nav_row(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    count: Option<usize>,
    icon: Option<&str>,
    drop: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.), egui::Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 5., crate::shell::ACCENT);
    } else if drop {
        painter.rect_filled(rect, 5., crate::shell::ACCENT.gamma_multiply(0.45));
    } else if response.hovered() {
        painter.rect_filled(rect, 5., ui.visuals().widgets.hovered.weak_bg_fill);
    }
    if drop {
        painter.rect_stroke(
            rect,
            5.,
            egui::Stroke::new(2., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let ink = if selected || drop {
        egui::Color32::WHITE
    } else {
        ui.visuals().text_color()
    };
    let mut left = rect.left() + 8.;
    if let Some(icon) = icon {
        super::library_icons::paint_sized(
            painter,
            egui::pos2(left + 8., rect.center().y),
            icon,
            ink,
            15.,
        );
        left += 24.;
    }
    let mut right = rect.right() - 8.;
    if let Some(count) = count {
        let tint = if selected || drop {
            egui::Color32::from_rgb(214, 242, 251)
        } else {
            FAINT
        };
        let galley =
            painter.layout_no_wrap(count.to_string(), egui::FontId::proportional(12.), tint);
        right -= galley.size().x;
        painter.galley(
            egui::pos2(right, rect.center().y - galley.size().y / 2.),
            galley,
            tint,
        );
        right -= 8.;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(
        label.to_owned(),
        egui::FontId::proportional(13.),
        ink,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((right - left).max(12.));
    let galley = painter.layout_job(job);
    painter.galley(
        egui::pos2(left, rect.center().y - galley.size().y / 2.),
        galley,
        ink,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, label)
    });
    response
}

/// A filter in use, shown over the grid; clicking it removes the filter.
fn chip(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let galley =
        ui.painter()
            .layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.), CHIP_INK);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(galley.size().x + 34., 22.), egui::Sense::click());
    let painter = ui.painter();
    let hovered = response.hovered();
    painter.rect_filled(
        rect,
        11.,
        if hovered {
            egui::Color32::from_rgb(40, 94, 114)
        } else {
            CHIP_FILL
        },
    );
    painter.galley(
        egui::pos2(rect.left() + 10., rect.center().y - galley.size().y / 2.),
        galley,
        CHIP_INK,
    );
    let c = egui::pos2(rect.right() - 13., rect.center().y);
    let stroke = egui::Stroke::new(
        1.4,
        if hovered {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(170, 182, 186)
        },
    );
    painter.line_segment(
        [c + egui::vec2(-3.5, -3.5), c + egui::vec2(3.5, 3.5)],
        stroke,
    );
    painter.line_segment(
        [c + egui::vec2(-3.5, 3.5), c + egui::vec2(3.5, -3.5)],
        stroke,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Remove {text}"))
    });
    response.tip("Remove this filter")
}

fn short_date(entry: &Entry, this_year: u32) -> String {
    let [year, month, day] = entry.created_date();
    let month = MONTHS[(month.clamp(1, 12) - 1) as usize];
    if year == this_year {
        format!("{month} {day}")
    } else {
        format!("{month} {day}, {year}")
    }
}
fn size_text(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.)
    } else {
        format!("{} KB", (bytes / 1000).max(1))
    }
}
fn kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "Images",
        MediaKind::Steps => "Step guides",
        MediaKind::Video => "Videos",
        MediaKind::Template => "From templates",
    }
}
/// A filter a chip can remove.
#[derive(Clone, Copy)]
enum Filter {
    Search,
    Pinned,
    Kind,
    Folder,
    App,
    Site,
    Year,
    Tag,
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
    anchor: Option<String>,
    marquee: Option<(egui::Pos2, std::collections::BTreeSet<String>, bool)>,
    year: Option<u32>,
    kind: Option<MediaKind>,
    pub(super) tag: Option<String>,
    folder: Option<String>,
    app: Option<String>,
    site: Option<String>,
    /// Only pinned captures.
    pinned: bool,
    /// Captures being dragged onto a folder.
    dragging: Option<Vec<String>>,
    naming: Option<Naming>,
    order: Order,
    zoom: f32,
    apps_open: bool,
    sites_open: bool,
    positions: std::collections::HashMap<String, (crate::motion::Tween, crate::motion::Tween)>,
    /// The long PDF whose parts are shown under its stacked card.
    pub(super) open_set: Option<String>,
    /// Export or scan every part once the first part has opened, and when it
    /// was asked for (it lapses if the part never opens).
    pub(crate) after_open: Option<(String, PartFollow, f64)>,
}
/// What to do once the first part of a long PDF opens from the Library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartFollow {
    Export,
    Scan,
}
/// Long PDFs (linked parts) show as one stacked card (approved
/// rtifacts/by-proposal/library-and-import.html option A). Returns the
/// list with one card per set (its first part) and every set's parts in order.
pub(super) fn stack_parts(
    visible: Vec<Entry>,
) -> (Vec<Entry>, std::collections::BTreeMap<String, Vec<Entry>>) {
    let mut sets: std::collections::BTreeMap<String, Vec<Entry>> = Default::default();
    for e in &visible {
        if let Some(p) = &e.part {
            sets.entry(p.set.clone()).or_default().push(e.clone());
        }
    }
    for parts in sets.values_mut() {
        parts.sort_by_key(|e| e.part.as_ref().map_or(0, |p| p.index));
    }
    let mut shown = std::collections::BTreeSet::new();
    let list = visible
        .into_iter()
        .filter(|e| match &e.part {
            Some(p) => shown.insert(p.set.clone()),
            None => true,
        })
        .map(|e| match &e.part {
            // The stack is drawn with its first part.
            Some(p) => sets[&p.set][0].clone(),
            None => e,
        })
        .collect();
    (list, sets)
}
/// "312 pages" when the parts know their pages.
fn stack_pages(parts: &[Entry]) -> Option<u32> {
    let first = parts.first()?.part.as_ref()?.pages[0];
    let last = parts.last()?.part.as_ref()?.pages[1];
    (first > 0 && last >= first).then(|| last - first + 1)
}
impl Default for View {
    fn default() -> Self {
        Self {
            selected: Default::default(),
            anchor: None,
            marquee: None,
            year: None,
            kind: None,
            tag: None,
            folder: None,
            app: None,
            site: None,
            pinned: false,
            dragging: None,
            naming: None,
            order: Order::Newest,
            zoom: 100.0,
            apps_open: false,
            sites_open: false,
            positions: Default::default(),
            open_set: None,
            after_open: None,
        }
    }
}
impl View {
    pub(super) fn start_import(&mut self) {
        self.clear_filters();
        self.selected.clear();
        self.anchor = None;
        self.marquee = None;
    }
    /// Every sidebar filter off: All captures.
    fn clear_filters(&mut self) {
        self.year = None;
        self.kind = None;
        self.tag = None;
        self.folder = None;
        self.app = None;
        self.site = None;
        self.pinned = false;
    }
    fn any_filter(&self) -> bool {
        self.year.is_some()
            || self.kind.is_some()
            || self.tag.is_some()
            || self.folder.is_some()
            || self.app.is_some()
            || self.site.is_some()
            || self.pinned
    }
    fn remove(&mut self, filter: Filter) {
        match filter {
            Filter::Search => {}
            Filter::Pinned => self.pinned = false,
            Filter::Kind => self.kind = None,
            Filter::Folder => self.folder = None,
            Filter::App => self.app = None,
            Filter::Site => self.site = None,
            Filter::Year => self.year = None,
            Filter::Tag => self.tag = None,
        }
    }
    fn select(&mut self, id: &str, visible: &[Entry], modifiers: egui::Modifiers) {
        if modifiers.shift {
            let index = |id: &str| visible.iter().position(|e| e.id == id);
            if let Some((a, b)) = self.anchor.as_deref().and_then(index).zip(index(id)) {
                if !modifiers.alt && !modifiers.ctrl {
                    self.selected.clear();
                }
                self.selected
                    .extend(visible[a.min(b)..=a.max(b)].iter().map(|e| e.id.clone()));
                return;
            }
        }
        if modifiers.alt || modifiers.ctrl {
            if !self.selected.remove(id) {
                self.selected.insert(id.into());
            }
        } else {
            self.selected = [id.into()].into();
        }
        self.anchor = Some(id.into());
    }
    pub(super) fn filtered(&self, entries: &[Entry], query: &str) -> Vec<Entry> {
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
                    && (self.folder.is_none() || self.folder == e.organize.folder)
                    && (self.app.is_none() || self.app == e.organize.app)
                    && (self.site.is_none() || self.site == e.organize.site)
                    && (!self.pinned || e.organize.pinned)
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
        .tip("Change Library thumbnail size; images stay unchanged.");
    }
    pub(super) fn show_library(&mut self, ui: &mut egui::Ui) {
        let entries = self.history.entries();
        let now = ui.input(|i| i.time);
        let duration = if crate::motion::enabled() { 0.260 } else { 0. };
        egui::Panel::top("library_top_bar")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(43, 50, 53))
                    .inner_margin(egui::Margin::symmetric(10, 8)),
            )
            .show(ui, |ui| self.library_top_bar(ui));
        let width = SIDEBAR_WIDTH.min((ui.available_width() - 160.).max(120.));
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("library-sidebar-width"), width));
        egui::Panel::left("library_sidebar")
            .default_size(width)
            .min_size(width)
            .max_size(width)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(52, 57, 60))
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        sw: 8,
                        ne: 0,
                        se: 0,
                    })
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("library_categories")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.library_sidebar(ui, &entries);
                        ui.add_space(16.0);
                    });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::same(12)))
            .show(ui, |ui| {
                self.library_import_progress(ui);
                let visible = self.history.library.filtered(&entries, &self.history.query);
                self.history
                    .library
                    .selected
                    .retain(|id| entries.iter().any(|e| &e.id == id));
                self.library_title(ui, visible.len());
                if !self.history.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.history.error);
                }
                if visible.is_empty() {
                    ui.add_space(24.0);
                    ui.label(if entries.is_empty() {
                        "Your local snapshots will appear here."
                    } else {
                        "No matching captures. Remove a filter to see more."
                    });
                    return;
                }
                let this_year = {
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis() as u64);
                    Entry {
                        created_ms: now_ms,
                        ..visible[0].clone()
                    }
                    .created_date()[0]
                };
                self.show_folder_naming(ui.ctx());
                let mut open = None;
                self.history
                    .library
                    .selected
                    .retain(|id| visible.iter().any(|e| &e.id == id));
                let (visible, sets) = stack_parts(visible);
                // A stack is selected as a whole: its first part stands for
                // every part, so Delete, Pin and folders act on the document.
                for parts in sets.values() {
                    let lib = &mut self.history.library;
                    let whole = lib.selected.contains(&parts[0].id);
                    for p in &parts[1..] {
                        if whole {
                            lib.selected.insert(p.id.clone());
                        } else {
                            lib.selected.remove(&p.id);
                        }
                    }
                }
                if self
                    .history
                    .library
                    .open_set
                    .as_ref()
                    .is_some_and(|set| !sets.contains_key(set))
                {
                    self.history.library.open_set = None;
                }
                let mut follow = None;
                if ui.rect_contains_pointer(ui.available_rect_before_wrap())
                    && !ui.ctx().egui_wants_keyboard_input()
                {
                    let delta = self.gesture_wheel(ui, crate::gesture_input::Gesture::LibraryZoom);
                    if delta != 0. {
                        self.history.library.zoom =
                            (self.history.library.zoom + delta.signum() * 10.).clamp(50., 200.);
                    }
                }
                egui::ScrollArea::vertical()
                    .id_salt("library_grid_scroll")
                    .scroll_source(egui::scroll_area::ScrollSource {
                        drag: egui::scroll_area::DragScroll::Never,
                        ..Default::default()
                    })
                    .show(ui, |ui| {
                        let origin = ui.cursor().min;
                        let area = ui.clip_rect();
                        let mut cards = Vec::new();
                        let (columns, size) =
                            grid_geometry(ui.available_width(), self.history.library.zoom);
                        // The name, then the date (Details adds size and dimensions).
                        let card_height = size.y + 46.;
                        let row_height = card_height + 16.;
                        let rows = visible.len().div_ceil(columns);
                        // The open stack's parts show in a band under its row.
                        let part_size = size * 0.8;
                        let band_height = part_size.y + 104.;
                        let band_row = self.history.library.open_set.as_ref().and_then(|set| {
                            visible
                                .iter()
                                .position(|e| e.part.as_ref().is_some_and(|p| &p.set == set))
                                .map(|i| i / columns)
                        });
                        let full_width = ui.available_width();
                        ui.allocate_space(egui::vec2(
                            full_width,
                            rows as f32 * row_height
                                + if band_row.is_some() { band_height } else { 0. },
                        ));
                        self.history
                            .library
                            .positions
                            .retain(|id, _| entries.iter().any(|e| &e.id == id));
                        for (index, entry) in visible.iter().enumerate() {
                            let x = (index % columns) as f32 * (size.x + 12.);
                            let y = (index / columns) as f32 * row_height
                                + if band_row.is_some_and(|r| index / columns > r) {
                                    band_height
                                } else {
                                    0.
                                };
                            let stack = entry
                                .part
                                .as_ref()
                                .and_then(|p| sets.get(&p.set))
                                .filter(|parts| parts.len() > 1);
                            let motion = self
                                .history
                                .library
                                .positions
                                .entry(entry.id.clone())
                                .or_insert_with(|| {
                                    (crate::motion::Tween::new(x), crate::motion::Tween::new(y))
                                });
                            let pos = egui::vec2(
                                motion.0.advance(x, now, duration),
                                motion.1.advance(y, now, duration),
                            );
                            if (pos.x - x).abs() + (pos.y - y).abs() > 0.01 {
                                ui.ctx().request_repaint();
                            }
                            let rect = egui::Rect::from_min_size(
                                origin + pos,
                                egui::vec2(size.x, card_height),
                            );
                            let card = ui.scope_builder(
                                egui::UiBuilder::new()
                                    .max_rect(rect)
                                    .layout(egui::Layout::top_down(egui::Align::Min)),
                                |ui| {
                                    ui.set_width(size.x);
                                    ui.set_max_width(size.x);
                                    if let Some(parts) = stack {
                                        stacked_card(self, ui, entry, parts, size);
                                    } else {
                                        self.history_card(ui, entry, size);
                                    }
                                    let label = match (stack, &entry.part) {
                                        (Some(_), Some(p)) => p.title.clone(),
                                        _ => entry.label.clone(),
                                    };
                                    ui.add(egui::Label::new(&label).truncate()).tip(&label);
                                    let mut line = short_date(entry, this_year);
                                    if let Some(parts) = stack {
                                        line = format!(
                                            "{} parts{}",
                                            parts.len(),
                                            stack_pages(parts)
                                                .map_or(String::new(), |n| format!(" · {n} pages"))
                                        );
                                    }
                                    if self.quick_controls.thumbnail_details() {
                                        line += &format!(
                                            " · {} × {} · {}",
                                            entry.width,
                                            entry.height,
                                            size_text(entry.bytes)
                                        );
                                    }
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(line).size(11.5).color(FAINT),
                                        )
                                        .truncate(),
                                    )
                                    .tip(entry.created_utc());
                                },
                            );
                            let response = ui.interact(
                                card.response.rect,
                                ui.scope_id().with(("library-card", &entry.id)),
                                egui::Sense::click_and_drag(),
                            );
                            if response.secondary_clicked()
                                && !self.history.library.selected.contains(&entry.id)
                            {
                                self.history.library.select(
                                    &entry.id,
                                    &visible,
                                    egui::Modifiers::NONE,
                                );
                            }
                            if response.double_clicked() && self.history.available(&entry.id) {
                                open = Some(entry.id.clone());
                            } else if response.clicked() {
                                // Clicking a stack opens its parts below it.
                                if let Some(p) = stack.and(entry.part.as_ref()) {
                                    let lib = &mut self.history.library;
                                    lib.open_set = if lib.open_set.as_ref() == Some(&p.set) {
                                        None
                                    } else {
                                        Some(p.set.clone())
                                    };
                                }
                                self.history.library.select(
                                    &entry.id,
                                    &visible,
                                    ui.input(|i| i.modifiers),
                                );
                            }
                            if response.drag_started() {
                                let lib = &mut self.history.library;
                                lib.dragging = Some(if lib.selected.contains(&entry.id) {
                                    lib.selected.iter().cloned().collect()
                                } else {
                                    vec![entry.id.clone()]
                                });
                            }
                            if entry.organize.pinned {
                                super::recent::paint_star(ui, card.response.rect, true);
                            }
                            let available = self.history.available(&entry.id);
                            self.history_context(&response, entry, available);
                            cards.push((entry.id.clone(), card.response.rect));
                        }
                        if let (Some(row), Some(parts)) = (
                            band_row,
                            self.history
                                .library
                                .open_set
                                .as_ref()
                                .and_then(|set| sets.get(set)),
                        ) {
                            let band = egui::Rect::from_min_size(
                                origin + egui::vec2(0., (row + 1) as f32 * row_height - 8.),
                                egui::vec2(full_width, band_height - 8.),
                            );
                            let (opened, follow_up, close) =
                                parts_band(self, ui, band, parts, part_size);
                            if let Some(id) = opened {
                                open = Some(id);
                            }
                            if let Some(f) = follow_up {
                                follow = Some((parts[0].clone(), f));
                            }
                            if close {
                                self.history.library.open_set = None;
                            }
                            cards.push((String::new(), band));
                        }
                        let (pressed, released, down, pos, modifiers) = ui.input(|i| {
                            (
                                i.pointer.primary_pressed(),
                                i.pointer.primary_released(),
                                i.pointer.primary_down(),
                                i.pointer.interact_pos(),
                                i.modifiers,
                            )
                        });
                        if pressed
                            && ui.rect_contains_pointer(area)
                            && let Some(pos) = pos
                            && (modifiers.alt
                                || modifiers.ctrl
                                || modifiers.shift
                                || !cards.iter().any(|(_, r)| r.contains(pos)))
                        {
                            self.history.library.marquee = Some((
                                pos - origin.to_vec2(),
                                self.history.library.selected.clone(),
                                modifiers.alt || modifiers.ctrl || modifiers.shift,
                            ));
                        }
                        if let Some((start, initial, add)) = self.history.library.marquee.clone()
                            && let Some(pos) = pos
                        {
                            let start = start + origin.to_vec2();
                            if start.distance(pos) > 6. {
                                let rect = egui::Rect::from_two_pos(start, pos);
                                let mut selected = if add { initial } else { Default::default() };
                                selected.extend(
                                    cards
                                        .iter()
                                        .filter(|(_, r)| r.intersects(rect))
                                        .map(|(id, _)| id.clone()),
                                );
                                self.history.library.selected = selected;
                                ui.painter().rect_filled(
                                    rect,
                                    0.,
                                    egui::Color32::from_rgba_unmultiplied(25, 156, 209, 28),
                                );
                                ui.painter().rect_stroke(
                                    rect,
                                    0.,
                                    egui::Stroke::new(1., crate::shell::ACCENT),
                                    egui::StrokeKind::Inside,
                                );
                            } else if released
                                && !add
                                && !cards.iter().any(|(_, r)| r.contains(pos))
                            {
                                self.history.library.selected.clear();
                                self.history.library.anchor = None;
                            }
                        }
                        // Captures being dragged travel with a label until they
                        // land on a folder in the sidebar.
                        if let Some(ids) = &self.history.library.dragging
                            && let Some(pos) = pos
                        {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                            let n = ids.len();
                            let text = format!(
                                "Move {n} {} to a folder",
                                if n == 1 { "capture" } else { "captures" }
                            );
                            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                                egui::Order::Tooltip,
                                egui::Id::unique("library-drag"),
                            ));
                            let galley = painter.layout_no_wrap(
                                text,
                                egui::FontId::proportional(12.),
                                egui::Color32::WHITE,
                            );
                            let rect = egui::Rect::from_min_size(
                                pos + egui::vec2(18., 14.),
                                galley.size() + egui::vec2(18., 10.),
                            );
                            painter.rect_filled(rect, 6., crate::shell::ACCENT);
                            painter.galley(
                                rect.min + egui::vec2(9., 5.),
                                galley,
                                egui::Color32::WHITE,
                            );
                        }
                        if released || !down {
                            self.history.library.marquee = None;
                            self.history.library.dragging = None;
                        }
                        #[cfg(test)]
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(egui::Id::unique("library-cards"), cards));
                    });
                if let Some((first, f)) = follow
                    && let Some(p) = &first.part
                {
                    self.history.library.after_open = Some((p.set.clone(), f, now));
                    open = Some(first.id.clone());
                }
                if let Some(id) = open {
                    self.open_history(&id, &ui.ctx().clone());
                }
            });
    }
    /// After a stack's Export all parts or Find Sensitive Info in all: once
    /// its first part is open, start that for every part. Runs every frame.
    pub(crate) fn part_follow_up(&mut self, ctx: &egui::Context) {
        let Some((set, follow, asked)) = self.history.library.after_open.clone() else {
            return;
        };
        if ctx.input(|i| i.time) - asked > 20. {
            self.history.library.after_open = None;
            return;
        }
        if self.shell.library
            || self
                .session
                .document
                .part
                .as_ref()
                .is_none_or(|p| p.set != set)
        {
            return;
        }
        self.history.library.after_open = None;
        match follow {
            PartFollow::Export => self.begin_export(false),
            PartFollow::Scan => {
                self.open_privacy_review();
                self.privacy_review_all_parts();
            }
        }
    }
}

/// A stack: two page edges behind the first part's card and a badge with
/// the number of parts.
fn stacked_card(
    e: &mut Editor,
    ui: &mut egui::Ui,
    entry: &Entry,
    parts: &[Entry],
    size: egui::Vec2,
) {
    let edge = 6.;
    let top = ui.cursor().min;
    let p = ui.painter();
    for (k, shade) in [(2., 70), (1., 95)] {
        p.rect_filled(
            egui::Rect::from_min_size(
                top + egui::vec2(edge * k, 0.),
                size - egui::vec2(edge * 2., edge * 2.),
            ),
            4,
            egui::Color32::from_gray(shade),
        );
    }
    ui.add_space(edge * 2.);
    let card = e.history_card(ui, entry, size - egui::vec2(edge * 2., edge * 2.));
    let text = format!("{} parts", parts.len());
    let galley =
        ui.painter()
            .layout_no_wrap(text, egui::FontId::proportional(10.5), egui::Color32::WHITE);
    let badge = egui::Rect::from_min_size(
        card.rect.left_top() + egui::vec2(6., 6.),
        egui::vec2(galley.size().x + 12., 15.),
    );
    ui.painter().rect_filled(badge, 4, crate::shell::ACCENT);
    ui.painter().galley(
        badge.center() - galley.size() / 2.,
        galley,
        egui::Color32::WHITE,
    );
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("library-stack", entry.id.as_str())),
            parts.len(),
        )
    });
}

/// The band under an open stack: its title, Export all parts, Find
/// Sensitive Info in all, Close, and each part with its pages. Returns a
/// part to open, a follow-up and whether Close was clicked.
fn parts_band(
    e: &mut Editor,
    ui: &mut egui::Ui,
    band: egui::Rect,
    parts: &[Entry],
    part_size: egui::Vec2,
) -> (Option<String>, Option<PartFollow>, bool) {
    let (mut opened, mut follow, mut close) = (None, None, false);
    ui.painter()
        .rect_filled(band, 8., egui::Color32::from_rgb(43, 51, 54));
    ui.painter().rect_stroke(
        band,
        8.,
        egui::Stroke::new(1., crate::shell::ACCENT),
        egui::StrokeKind::Inside,
    );
    let title = parts[0]
        .part
        .as_ref()
        .map_or(String::new(), |p| p.title.clone());
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(band.shrink2(egui::vec2(12., 10.)))
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            ui.horizontal(|ui| {
                let pages = stack_pages(parts)
                    .map_or(String::new(), |n| format!(" · {n} pages"));
                ui.label(
                    egui::RichText::new(format!("{title}{pages} in {} parts", parts.len()))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    use crate::tips::Tip as _;
                    if ui
                        .add(crate::controls::dialog_button("Close", false))
                        .clicked()
                    {
                        close = true;
                    }
                    if ui
                        .add(crate::controls::dialog_button("Find Sensitive Info in all", false))
                        .tip("Find Sensitive Info in all · Scan every part of this PDF, one part after another.")
                        .clicked()
                    {
                        follow = Some(PartFollow::Scan);
                    }
                    if ui
                        .add(crate::controls::dialog_button("Export all parts\u{2026}", true))
                        .tip("Export all parts · Every part of this PDF, in order, in one file.")
                        .clicked()
                    {
                        follow = Some(PartFollow::Export);
                    }
                });
            });
            ui.add_space(6.);
            egui::ScrollArea::horizontal()
                .id_salt(("library-parts", title.as_str()))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for part in parts {
                            ui.vertical(|ui| {
                                ui.set_width(part_size.x);
                                let r = e.history_card_action(
                                    ui,
                                    part,
                                    part_size,
                                    Some("Open this part."),
                                );
                                let label = part.part.as_ref().map_or(String::new(), |p| {
                                    if p.pages[0] > 0 {
                                        format!("Pages {}\u{2013}{}", p.pages[0], p.pages[1])
                                    } else {
                                        format!("Part {} of {}", p.index, p.total)
                                    }
                                });
                                ui.label(egui::RichText::new(label).size(11.5));
                                if r.clicked() && e.history.available(&part.id) {
                                    opened = Some(part.id.clone());
                                }
                            });
                        }
                    });
                });
        },
    );
    (opened, follow, close)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn year_query_and_sort_combine_without_mutating_history() {
        let make = |id: &str, label: &str, time, bytes| Entry {
            kind: Default::default(),
            tags: vec![],
            organize: Default::default(),
            part: None,
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

#[cfg(test)]
#[path = "library_tests.rs"]
mod gesture_tests;

/// The New folder or Rename folder dialog.
struct Naming {
    /// None is a new folder.
    old: Option<String>,
    name: String,
    error: String,
}
impl Editor {
    fn run_library(
        &mut self,
        edit: impl FnOnce(&mut capture_core::history::History) -> capture_core::Result<()>,
    ) {
        let Some(store) = self.history.store.as_mut() else {
            return;
        };
        match edit(store) {
            Ok(()) => {
                self.history.error.clear();
                self.history.refresh();
            }
            Err(e) => self.history.error = crate::guided::friendly(&e.to_string()),
        }
    }
    /// Search, Sort, size, Details, Import and Refresh in one bar.
    fn library_top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(24., 26.), egui::Sense::hover());
            super::library_icons::paint(
                ui.painter(),
                r.center(),
                "Search",
                ui.visuals().text_color(),
            );
            ui.add(
                crate::unicode_input::TextEdit::singleline(&mut self.history.query)
                    .id(egui::Id::unique("library-search"))
                    .hint_text("Search Library")
                    .char_limit(120)
                    .desired_width(260.),
            )
            .tip("Search names, tags, capture types and dimensions. Every word must match.");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::library_icons::button(
                    ui,
                    "Refresh",
                    "Refresh Library",
                    true,
                    false,
                    egui::vec2(28., 26.),
                )
                .clicked()
                {
                    self.history.refresh();
                }
                self.library_import_button(ui);
                let mut details = self.quick_controls.thumbnail_details();
                if crate::controls::toggle(ui, &mut details, "Show thumbnail details").changed() {
                    self.quick_controls.set_thumbnail_details(details);
                }
                self.library_zoom(ui);
                egui::ComboBox::from_id_salt("library_order")
                    .selected_text(format!("Sort: {}", self.history.library.order.label()))
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
        });
    }
    /// What the grid shows, its counts, and a chip for every filter in use.
    fn library_title(&mut self, ui: &mut egui::Ui, shown: usize) {
        let lib = &self.history.library;
        let name = if let Some(folder) = &lib.folder {
            folder.clone()
        } else if lib.pinned {
            "Pinned".into()
        } else if let Some(kind) = lib.kind {
            kind_name(kind).into()
        } else if let Some(tag) = &lib.tag {
            tag.clone()
        } else if let Some(app) = &lib.app {
            app.clone()
        } else if let Some(site) = &lib.site {
            site.clone()
        } else if let Some(year) = lib.year {
            year.to_string()
        } else {
            "All captures".into()
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(name).size(16.).strong());
            ui.label(
                egui::RichText::new(format!(
                    "{shown} {} · {} selected",
                    if shown == 1 { "capture" } else { "captures" },
                    lib.selected.len()
                ))
                .color(FAINT),
            );
        });
        let mut chips = vec![];
        if let Some(folder) = &lib.folder {
            chips.push((format!("Folder: {folder}"), Filter::Folder));
        }
        if lib.pinned {
            chips.push(("Pinned".into(), Filter::Pinned));
        }
        if let Some(kind) = lib.kind {
            chips.push((kind_name(kind).into(), Filter::Kind));
        }
        if let Some(tag) = &lib.tag {
            chips.push((format!("Tag: {tag}"), Filter::Tag));
        }
        if let Some(app) = &lib.app {
            chips.push((format!("App: {app}"), Filter::App));
        }
        if let Some(site) = &lib.site {
            chips.push((format!("Website: {site}"), Filter::Site));
        }
        if let Some(year) = lib.year {
            chips.push((format!("Date: {year}"), Filter::Year));
        }
        if !self.history.query.trim().is_empty() {
            chips.push((
                format!("Search: {}", self.history.query.trim()),
                Filter::Search,
            ));
        }
        if chips.is_empty() {
            return;
        }
        let mut removed = None;
        ui.horizontal_wrapped(|ui| {
            for (text, filter) in &chips {
                if chip(ui, text).clicked() {
                    removed = Some(*filter);
                }
            }
        });
        ui.add_space(4.);
        match removed {
            Some(Filter::Search) => self.history.query.clear(),
            Some(filter) => self.history.library.remove(filter),
            None => {}
        }
    }
    /// Library, Type, Folders, Source, Date and Tags, each with its counts.
    fn library_sidebar(&mut self, ui: &mut egui::Ui, entries: &[Entry]) {
        let lib = &mut self.history.library;
        section_heading(ui, "Library", "Library");
        if nav_row(
            ui,
            !lib.any_filter(),
            "All captures",
            Some(entries.len()),
            None,
            false,
        )
        .tip("Show every capture")
        .clicked()
        {
            lib.clear_filters();
        }
        let pinned = entries.iter().filter(|e| e.organize.pinned).count();
        if nav_row(ui, lib.pinned, "Pinned", Some(pinned), None, false)
            .tip("Show the captures pinned in Recent")
            .clicked()
        {
            lib.pinned = !lib.pinned;
        }
        section_heading(ui, "Type", "Type");
        for kind in [
            MediaKind::Image,
            MediaKind::Steps,
            MediaKind::Video,
            MediaKind::Template,
        ] {
            let count = entries.iter().filter(|e| e.kind == kind).count();
            let on = lib.kind == Some(kind);
            if nav_row(ui, on, kind_name(kind), Some(count), None, false).clicked() {
                lib.kind = (!on).then_some(kind);
            }
        }
        self.library_folders(ui, entries);
        self.library_sources(ui, entries);
        let lib = &mut self.history.library;
        section_heading(ui, "Date", "Dates");
        let mut years = std::collections::BTreeMap::<u32, usize>::new();
        for entry in entries {
            *years.entry(entry.created_date()[0]).or_default() += 1;
        }
        for (&year, &count) in years.iter().rev() {
            let on = lib.year == Some(year);
            if nav_row(ui, on, &year.to_string(), Some(count), None, false).clicked() {
                lib.year = (!on).then_some(year);
            }
        }
        section_heading(ui, "Tags", "Tags");
        let mut tags = std::collections::BTreeSet::new();
        for e in entries {
            tags.extend(e.tags.iter().cloned());
        }
        if tags.is_empty() {
            ui.label(
                egui::RichText::new("Right-click a capture to add tags.")
                    .size(11.5)
                    .color(FAINT),
            );
        }
        for tag in tags {
            let count = entries
                .iter()
                .filter(|e| e.tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)))
                .count();
            let on = self.history.library.tag.as_ref() == Some(&tag);
            let response = nav_row(ui, on, &tag, Some(count), None, false);
            if response.clicked() {
                self.history.library.tag = (!on).then(|| tag.clone());
            }
            response.context_menu(|ui| {
                if crate::controls::menu_action(ui, "Rename tag everywhere…").clicked() {
                    self.begin_global_tag(&tag, false);
                    ui.close();
                }
                if crate::controls::menu_action(ui, "Delete tag everywhere…").clicked() {
                    self.begin_global_tag(&tag, true);
                    ui.close();
                }
            });
        }
    }
    /// Folders in the Library sidebar. Drop captures on one to move them.
    fn library_folders(&mut self, ui: &mut egui::Ui, entries: &[Entry]) {
        let heading = section_heading(ui, "Folders", "Folder");
        let add = egui::Rect::from_center_size(
            heading.right_center() - egui::vec2(12., 0.),
            egui::vec2(24., 20.),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(add));
        if super::library_icons::button(
            &mut child,
            "Add",
            "New folder",
            true,
            false,
            egui::vec2(24., 20.),
        )
        .clicked()
        {
            self.history.library.naming = Some(Naming {
                old: None,
                name: String::new(),
                error: String::new(),
            });
        }
        let folders: Vec<String> = self
            .history
            .store
            .as_ref()
            .map(|s| s.folders().to_vec())
            .unwrap_or_default();
        let (released, pointer) =
            ui.input(|i| (i.pointer.primary_released(), i.pointer.interact_pos()));
        let dragging = self.history.library.dragging.is_some();
        let mut moved = None;
        let mut delete = None;
        if folders.is_empty() {
            ui.label(
                egui::RichText::new("Click + to make a folder.")
                    .size(11.5)
                    .color(FAINT),
            );
        }
        for name in &folders {
            let count = entries
                .iter()
                .filter(|e| e.organize.folder.as_deref() == Some(name.as_str()))
                .count();
            let chosen = self.history.library.folder.as_ref() == Some(name);
            let next =
                egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 26.));
            let over =
                dragging && pointer.is_some_and(|p| next.contains(p) && ui.clip_rect().contains(p));
            let response = nav_row(ui, chosen, name, Some(count), Some("Folder"), over);
            if response.clicked() {
                self.history.library.folder = (!chosen).then(|| name.clone());
            }
            if over && released {
                moved = Some(name.clone());
            }
            response.context_menu(|ui| {
                if crate::controls::menu_action(ui, "Rename folder…").clicked() {
                    self.history.library.naming = Some(Naming {
                        old: Some(name.clone()),
                        name: name.clone(),
                        error: String::new(),
                    });
                    ui.close();
                }
                if crate::controls::menu_action(ui, "Delete folder").clicked() {
                    delete = Some(name.clone());
                    ui.close();
                }
            });
        }
        if let Some(name) = moved
            && let Some(ids) = self.history.library.dragging.take()
        {
            self.run_library(|store| store.move_to_folder(&ids, Some(&name)));
        }
        if let Some(name) = delete {
            if self.history.library.folder.as_ref() == Some(&name) {
                self.history.library.folder = None;
            }
            self.run_library(|store| store.delete_folder(&name));
        }
    }
    /// By app and By website, filled in from what was recorded at capture.
    fn library_sources(&mut self, ui: &mut egui::Ui, entries: &[Entry]) {
        section_heading(ui, "Source", "Websites");
        for (label, is_app) in [("By app", true), ("By website", false)] {
            let lib = &mut self.history.library;
            let mut counts = std::collections::BTreeMap::<String, usize>::new();
            for e in entries {
                let name = if is_app {
                    &e.organize.app
                } else {
                    &e.organize.site
                };
                if let Some(name) = name {
                    *counts.entry(name.clone()).or_default() += 1;
                }
            }
            let open = if is_app {
                &mut lib.apps_open
            } else {
                &mut lib.sites_open
            };
            if nav_row(
                ui,
                false,
                &format!("{label} ›"),
                Some(counts.len()),
                None,
                false,
            )
            .tip(if is_app {
                "Show or hide the apps your captures came from"
            } else {
                "Show or hide the websites your captures came from"
            })
            .clicked()
            {
                *open = !*open;
            }
            if !*open {
                continue;
            }
            ui.indent(("library-source", label), |ui| {
                if counts.is_empty() {
                    ui.label(
                        egui::RichText::new("New captures appear here.")
                            .size(11.5)
                            .color(FAINT),
                    );
                }
                for (name, count) in counts {
                    let value = if is_app { &mut lib.app } else { &mut lib.site };
                    let chosen = value.as_ref() == Some(&name);
                    if nav_row(ui, chosen, &name, Some(count), None, false).clicked() {
                        *value = (!chosen).then_some(name);
                    }
                }
            });
        }
    }
    fn show_folder_naming(&mut self, ctx: &egui::Context) {
        let Some(mut naming) = self.history.library.naming.take() else {
            return;
        };
        let (mut save, mut cancel) = (false, false);
        let modal =
            crate::controls::Modal::new(egui::Id::unique("library-folder-name")).show(ctx, |ui| {
                ui.set_width(320.);
                crate::controls::dialog_title(
                    ui,
                    if naming.old.is_some() {
                        "Rename folder"
                    } else {
                        "New folder"
                    },
                );
                ui.add(
                    crate::unicode_input::TextEdit::singleline(&mut naming.name)
                        .desired_width(f32::INFINITY),
                );
                if !naming.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &naming.error);
                }
                (save, cancel) = crate::controls::dialog_actions(
                    ui,
                    "Save",
                    "Cancel",
                    !naming.name.trim().is_empty(),
                    false,
                );
            });
        cancel |= crate::text_edit::cancel_on_escape(&modal);
        if cancel {
            return;
        }
        if save && let Some(store) = self.history.store.as_mut() {
            let result = match &naming.old {
                Some(old) => store.rename_folder(old, &naming.name),
                None => store.create_folder(&naming.name).map(|_| ()),
            };
            match result {
                Ok(()) => {
                    self.history.refresh();
                    return;
                }
                Err(e) => naming.error = crate::guided::friendly(&e.to_string()),
            }
        }
        self.history.library.naming = Some(naming);
    }
    /// Move to folder, shared by single and multiple selections.
    pub(super) fn folder_menu(&mut self, ui: &mut egui::Ui, ids: &[String]) {
        let folders: Vec<String> = self
            .history
            .store
            .as_ref()
            .map(|s| s.folders().to_vec())
            .unwrap_or_default();
        if folders.is_empty() {
            return;
        }
        let mut target = None;
        ui.menu_button("Move to folder", |ui| {
            if crate::controls::menu_action(ui, "No folder").clicked() {
                target = Some(None);
                ui.close();
            }
            for name in &folders {
                if crate::controls::menu_action(ui, name).clicked() {
                    target = Some(Some(name.clone()));
                    ui.close();
                }
            }
        });
        if let Some(folder) = target {
            self.run_library(|store| store.move_to_folder(ids, folder.as_deref()));
        }
    }
}
