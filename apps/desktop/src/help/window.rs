//! The Help window: article list, step list and article, plus the Shortcuts
//! and About pages (approved help-navigation-proposal).
use super::{Block, articles, search};
use crate::egui;
use crate::tips::Tip as _;
use egui::{Color32, RichText};

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum Page {
    #[default]
    Article,
    Shortcuts,
    About,
}
#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    query: String,
    selected: Option<usize>,
    page: Page,
    textures: std::collections::HashMap<String, Option<egui::TextureHandle>>,
    /// The article list is folded to its strip.
    collapsed: bool,
    /// The step the reader has reached, highlighted in the step list.
    step: usize,
    /// A step chosen in the step list, scrolled to on the next frame.
    jump: Option<usize>,
    /// The article drawn last, so a new one starts at its top.
    shown: Option<usize>,
    filter: String,
}
const ACCENT: Color32 = Color32::from_rgb(24, 145, 193);
const MUTED: Color32 = Color32::from_rgb(163, 177, 181);
const PANE: Color32 = Color32::from_rgb(38, 45, 48);
const RAIL: Color32 = Color32::from_rgb(27, 33, 36);
const LINE: Color32 = Color32::from_rgb(74, 86, 90);
const FIELD: Color32 = Color32::from_rgb(29, 35, 38);
const SELECTED: Color32 = Color32::from_rgb(31, 81, 104);
const HOVER: Color32 = Color32::from_rgb(54, 64, 69);
const STEP_BODY: Color32 = Color32::from_rgb(211, 218, 220);
const LIST_WIDTH: f32 = 260.;
const STRIP_WIDTH: f32 = 44.;
const STEP_LIST_WIDTH: f32 = 180.;
/// The step list hides rather than squeeze the article below this.
const MIN_ARTICLE_WIDTH: f32 = 420.;

impl State {
    #[cfg(test)]
    pub(super) fn set_page(&mut self, page: Page) {
        self.page = page;
    }
    #[cfg(test)]
    pub(super) fn is_collapsed(&self) -> bool {
        self.collapsed
    }
    /// Opens an article from the list or F1. One with steps gets the room.
    pub(super) fn open_article(&mut self, index: usize) {
        self.selected = Some(index);
        self.collapsed = articles().get(index).is_some_and(|a| a.has_steps());
    }
    fn texture(&mut self, ctx: &egui::Context, key: &str) -> Option<egui::TextureHandle> {
        self.textures
            .entry(key.to_owned())
            .or_insert_with(|| {
                super::pictures::decode(key)
                    .or_else(|| super::art::draw(key))
                    .map(|img| {
                        ctx.load_texture(
                            format!("help-art-{key}"),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [img.width() as usize, img.height() as usize],
                                img.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        )
                    })
            })
            .clone()
    }
}
fn highlighted(text: &str, words: &[String], size: f32, strong: bool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let lower = text.to_lowercase();
    let mut i = 0;
    let bytes = text.len();
    while i < bytes {
        let hit = words
            .iter()
            .filter(|w| !w.is_empty())
            .find_map(|w| lower[i..].starts_with(w.as_str()).then_some(w.len()));
        let (len, mark) = match hit {
            Some(n) if text.is_char_boundary(i + n) => (n, true),
            _ => (text[i..].chars().next().map_or(1, char::len_utf8), false),
        };
        let mut format = egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: if strong { Color32::WHITE } else { MUTED },
            ..Default::default()
        };
        if mark {
            format.background = Color32::from_rgba_unmultiplied(242, 201, 76, 70);
        }
        job.append(&text[i..i + len], 0., format);
        i += len;
    }
    job
}
fn where_strip(ui: &mut egui::Ui, location: &str) {
    if location.is_empty() {
        return;
    }
    egui::Frame::new()
        .fill(RAIL)
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                ui.label(RichText::new("Where to find it").size(12.).color(MUTED));
                ui.label(RichText::new(location).size(12.).color(Color32::WHITE));
            });
        });
}
fn picture(ui: &mut egui::Ui, state: &mut State, key: &str, width: f32) {
    if let Some(t) = state.texture(ui.ctx(), key) {
        // Pictures are stored at twice their natural size; a small dialog
        // stays small instead of filling the column.
        let natural = t.size_vec2().x / 2.;
        let shown = if super::pictures::find(key).is_some() {
            width.min(natural)
        } else {
            width
        };
        let size = t.size_vec2() * (shown / t.size_vec2().x);
        ui.add(
            egui::Image::new(&t)
                .fit_to_exact_size(size)
                .corner_radius(6),
        )
        .tip("A sample project in ClipDesigner");
    }
}

impl crate::Editor {
    pub(crate) fn show_help(&mut self, ctx: &egui::Context) {
        if let Some((article, query)) = super::take_request(ctx) {
            let state = &mut self.shell.help_window;
            state.open = true;
            state.page = Page::Article;
            state.query = query;
            state.collapsed = false;
            if let Some(index) = article {
                state.open_article(index);
            }
        }
        if !self.shell.help_window.open {
            self.shell.help = false;
            return;
        }
        self.shell.help = true;
        let mut close = false;
        let mut change_shortcuts = false;
        let keys = self.quick_controls.keybinds();
        let screen = ctx.content_rect();
        let size = egui::vec2(1180., 760.).min(screen.size() - egui::vec2(40., 40.));
        let window = egui::Window::new("ClipDesigner Help")
            .id(egui::Id::unique("clipdesigner-help"))
            .title_bar(false)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(48, 56, 59))
                    .stroke(egui::Stroke::new(1.5, crate::shell::ACCENT))
                    .corner_radius(10)
                    .inner_margin(egui::Margin::symmetric(16, 14)),
            )
            .default_size(size)
            .default_pos(screen.center() - size / 2.)
            .min_size(egui::vec2(520., 360.))
            .max_size(screen.size() - egui::vec2(40., 40.))
            .collapsible(false)
            .show(ctx, |ui| {
                // Filling the window keeps its size the same on every tab.
                ui.set_min_size(ui.available_size());
                let state = &mut self.shell.help_window;
                ui.horizontal(|ui| {
                    ui.label(crate::panel_ui::title("ClipDesigner Help", 20.));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(26., 26.), egui::Sense::click());
                        let response = response.tip("Close");
                        if response.hovered() {
                            ui.painter()
                                .rect_filled(rect, 5., Color32::from_white_alpha(20));
                        }
                        crate::video_editor::ui::icon(
                            ui.painter(),
                            rect.shrink(6.),
                            "Close",
                            ui.visuals().text_color(),
                        );
                        close = response.clicked();
                    });
                });
                ui.add_space(6.);
                crate::panel_ui::rail(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (page, label, glyph, tip) in [
                            (
                                Page::Article,
                                "Articles",
                                "Help book",
                                "Search and read the help articles.",
                            ),
                            (
                                Page::Shortcuts,
                                "Shortcuts",
                                "Keybinds",
                                "Every keyboard shortcut, including ones you changed.",
                            ),
                            (
                                Page::About,
                                "About",
                                "Help info",
                                "Version, license and third-party notices.",
                            ),
                        ] {
                            if crate::panel_ui::tab(
                                ui,
                                label,
                                glyph,
                                state.page == page,
                                118.,
                                true,
                            )
                            .tip(tip)
                            .clicked()
                            {
                                state.page = page;
                            }
                        }
                    });
                });
                ui.add_space(10.);
                match state.page {
                    Page::Article => article_page(ui, state),
                    Page::Shortcuts => change_shortcuts = shortcuts_page(ui, state, &keys),
                    Page::About => about_page(ui),
                }
            });
        #[cfg(test)]
        if let Some(window) = &window {
            ctx.data_mut(|d| d.insert_temp(egui::Id::unique("help-rect"), window.response.rect));
        }
        let _ = window;
        if change_shortcuts {
            self.open_keyboard_shortcuts();
        }
        if close || change_shortcuts {
            self.shell.help_window.open = false;
            self.shell.help = false;
        }
    }
}
/// A child area filling one column of the Articles pane.
fn column(ui: &mut egui::Ui, rect: egui::Rect) -> egui::Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    )
}
fn article_page(ui: &mut egui::Ui, state: &mut State) {
    let mut hits = search(&state.query);
    let searching = !state.query.trim().is_empty();
    if !searching {
        // Browsing: one heading per section, in the order sections first appear (BI7).
        let mut order: Vec<&str> = vec![];
        for a in articles() {
            if !order.contains(&a.section.as_str()) {
                order.push(&a.section);
            }
        }
        hits.sort_by_key(|h| {
            let s = &articles()[h.index].section;
            (
                order.iter().position(|o| o == s).unwrap_or(usize::MAX),
                h.index,
            )
        });
    }
    if searching
        && state
            .selected
            .is_none_or(|s| !hits.iter().any(|h| h.index == s))
    {
        state.selected = hits.first().map(|h| h.index);
    }
    let selected = state.selected.unwrap_or(0);
    let article = (!hits.is_empty())
        .then(|| articles().get(selected))
        .flatten();
    let has_steps = article.is_some_and(|a| a.has_steps());
    // A search in progress keeps its results in view.
    let collapsed = state.collapsed && has_steps && !searching;
    let pane = ui.available_rect_before_wrap();
    ui.allocate_rect(pane, egui::Sense::hover());
    ui.painter().rect_filled(pane, 8., PANE);
    let list_width = if collapsed { STRIP_WIDTH } else { LIST_WIDTH };
    let step_list = has_steps && pane.width() - list_width - STEP_LIST_WIDTH >= MIN_ARTICLE_WIDTH;
    let list = egui::Rect::from_min_size(pane.min, egui::vec2(list_width, pane.height()));
    let steps = egui::Rect::from_min_size(
        list.right_top(),
        egui::vec2(if step_list { STEP_LIST_WIDTH } else { 0. }, pane.height()),
    );
    let body = egui::Rect::from_min_max(steps.right_top(), pane.max);
    let divider = egui::Stroke::new(1., LINE);
    ui.painter().vline(list.right(), pane.y_range(), divider);
    if step_list {
        ui.painter().vline(steps.right(), pane.y_range(), divider);
    }
    if collapsed {
        article_strip(ui, list, state);
    } else {
        article_list(ui, list, state, &hits, has_steps);
    }
    if step_list && let Some(a) = article {
        step_list_column(ui, steps, state, a);
    }
    if let Some(a) = article {
        article_column(ui, body, state, a, selected);
    }
}
/// The folded list: the whole strip opens it again.
fn article_strip(ui: &mut egui::Ui, rect: egui::Rect, state: &mut State) {
    let response = ui
        .interact(rect, egui::Id::unique("help-strip"), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .tip("Show all articles");
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Show all articles"));
    let hot = response.hovered() || response.has_focus();
    if hot {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius {
                nw: 8,
                sw: 8,
                ne: 0,
                se: 0,
            },
            ACCENT,
        );
    }
    crate::video_editor::ui::icon(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::vec2(16., 16.)),
        "Help strip",
        if hot { Color32::WHITE } else { MUTED },
    );
    if response.clicked() {
        state.collapsed = false;
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("help-strip-rect"), rect));
}
fn article_list(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    state: &mut State,
    hits: &[super::Hit],
    has_steps: bool,
) {
    let mut ui = column(ui, rect.shrink(10.));
    let ui = &mut ui;
    let selected = state.selected.unwrap_or(0);
    ui.horizontal(|ui| {
        let width = ui.available_width() - if has_steps { 36. } else { 0. };
        let field = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .id(egui::Id::unique("help-search"))
                .hint_text("Search help, e.g. hide email")
                .desired_width(width),
        );
        if field.changed() {
            state.selected = None;
        }
        if has_steps {
            let (r, response) = ui.allocate_exact_size(egui::vec2(30., 30.), egui::Sense::click());
            let response = response.tip("Hide the article list");
            ui.painter().rect_filled(
                r,
                5.,
                if response.hovered() {
                    Color32::from_rgb(74, 84, 89)
                } else {
                    Color32::from_rgb(57, 67, 71)
                },
            );
            crate::video_editor::ui::icon(
                ui.painter(),
                r.shrink(7.),
                "Help collapse",
                Color32::WHITE,
            );
            if response.clicked() {
                state.collapsed = true;
            }
        }
    });
    ui.add_space(8.);
    let keys = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowUp),
        )
    });
    if let Some(pos) = hits.iter().position(|h| h.index == selected) {
        if keys.0 && pos + 1 < hits.len() {
            state.selected = Some(hits[pos + 1].index);
        }
        if keys.1 && pos > 0 {
            state.selected = Some(hits[pos - 1].index);
        }
    }
    let searching = !state.query.trim().is_empty();
    egui::ScrollArea::vertical()
        .id_salt("help-results")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if hits.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "No articles match \"{}\". Try a tool name or what you want to do, like \"hide email\".",
                        state.query.trim()
                    ))
                    .color(MUTED),
                );
            }
            let mut section = "";
            for hit in hits {
                let a = &articles()[hit.index];
                if !searching && a.section != section {
                    section = &a.section;
                    ui.add_space(8.);
                    ui.horizontal(|ui| {
                        ui.add_space(6.);
                        crate::controls::dialog_section_label(ui, section, None);
                    });
                    ui.add_space(2.);
                }
                let wrap = ui.available_width() - 16.;
                let mut lines = vec![highlighted(&a.title, &hit.matched, 12.5, true)];
                if searching && !a.location.is_empty() {
                    lines.push(highlighted(&a.location, &hit.matched, 11., false));
                }
                let galleys: Vec<_> = lines
                    .into_iter()
                    .map(|mut job| {
                        job.wrap.max_width = wrap;
                        ui.painter().layout_job(job)
                    })
                    .collect();
                let on = hit.index == state.selected.unwrap_or(0);
                if list_row(ui, &galleys, on).clicked() {
                    state.open_article(hit.index);
                }
            }
            if !hits.is_empty() && searching {
                ui.label(
                    RichText::new(format!(
                        "{} result{} · ↑↓ to move",
                        hits.len(),
                        if hits.len() == 1 { "" } else { "s" }
                    ))
                    .small()
                    .color(MUTED),
                );
            }
        });
}
fn list_row(
    ui: &mut egui::Ui,
    galleys: &[std::sync::Arc<egui::Galley>],
    on: bool,
) -> egui::Response {
    let height = galleys.iter().map(|g| g.size().y).sum::<f32>() + 10.;
    let (r, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), height),
        egui::Sense::click(),
    );
    if on {
        ui.painter().rect_filled(r, 5., SELECTED);
    } else if response.hovered() {
        ui.painter().rect_filled(r, 5., HOVER);
    }
    let mut y = r.top() + 5.;
    for galley in galleys {
        ui.painter()
            .galley(egui::pos2(r.left() + 8., y), galley.clone(), Color32::WHITE);
        y += galley.size().y;
    }
    response
}
fn step_list_column(ui: &mut egui::Ui, rect: egui::Rect, state: &mut State, a: &super::Article) {
    let mut ui = column(ui, rect.shrink2(egui::vec2(8., 14.)));
    let ui = &mut ui;
    ui.horizontal(|ui| {
        ui.add_space(8.);
        crate::controls::dialog_section_label(ui, "Steps", None);
    });
    ui.add_space(6.);
    egui::ScrollArea::vertical()
        .id_salt("help-step-list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            let titles = a.blocks.iter().filter_map(|b| match b {
                Block::Step(_, _, _, title) => Some(title),
                _ => None,
            });
            for (index, title) in titles.enumerate() {
                let galley = ui.painter().layout(
                    title.clone(),
                    egui::FontId::proportional(12.5),
                    Color32::WHITE,
                    ui.available_width() - 42.,
                );
                let height = (galley.size().y + 12.).max(30.);
                let (r, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), height),
                    egui::Sense::click(),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::Role::Button, true, index == state.step, title)
                });
                let on = index == state.step;
                if on {
                    ui.painter().rect_filled(r, 6., SELECTED);
                } else if response.hovered() {
                    ui.painter().rect_filled(r, 6., HOVER);
                }
                let center = egui::pos2(r.left() + 17., r.top() + 15.);
                ui.painter().circle_filled(
                    center,
                    9.,
                    if on {
                        ACCENT
                    } else {
                        Color32::from_rgb(57, 67, 71)
                    },
                );
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    (index + 1).to_string(),
                    egui::FontId::proportional(10.5),
                    Color32::WHITE,
                );
                ui.painter().galley(
                    egui::pos2(r.left() + 34., r.center().y - galley.size().y / 2.),
                    galley,
                    Color32::WHITE,
                );
                if response.clicked() {
                    state.step = index;
                    state.jump = Some(index);
                }
            }
        });
}
fn article_column(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    state: &mut State,
    a: &super::Article,
    selected: usize,
) {
    let fresh = state.shown != Some(selected);
    if fresh {
        state.shown = Some(selected);
        state.step = 0;
        state.jump = None;
    }
    let mut ui = column(ui, rect);
    let ui = &mut ui;
    let mut area = egui::ScrollArea::vertical()
        .id_salt("help-article")
        .auto_shrink([false, false]);
    if fresh {
        area = area.vertical_scroll_offset(0.);
    }
    let width = (rect.width() - 52.).clamp(200., 720.);
    let jump = state.jump.take();
    let mut tops = vec![];
    let mut offset = 0f32;
    let output = area.show(ui, |ui| {
        let origin = ui.next_widget_position().y;
        offset = (ui.clip_rect().top() - origin).max(0.);
        egui::Frame::new()
            .inner_margin(egui::Margin {
                left: 26,
                right: 26,
                top: 18,
                bottom: 40,
            })
            .show(ui, |ui| {
                ui.set_width(width);
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.spacing_mut().item_spacing.y = 0.;
                ui.label(
                    RichText::new(format!("{} › {}", a.section, a.title))
                        .size(11.5)
                        .color(MUTED),
                );
                ui.add_space(2.);
                ui.label(RichText::new(&a.title).size(21.).strong());
                ui.add_space(10.);
                if let Some(hero) = &a.hero {
                    picture(ui, state, hero, width.min(460.));
                    ui.add_space(12.);
                }
                where_strip(ui, &a.location);
                ui.add_space(12.);
                let text_width = width.min(560.);
                let mut step = 0;
                let mut in_steps = false;
                for block in &a.blocks {
                    if let Block::Step(t, icon, pic, title) = block {
                        if !in_steps {
                            ui.add_space(10.);
                            in_steps = true;
                        }
                        step += 1;
                        let top = ui.next_widget_position().y;
                        tops.push(top - origin);
                        let (line, _) =
                            ui.allocate_exact_size(egui::vec2(width, 1.), egui::Sense::hover());
                        ui.painter()
                            .hline(line.x_range(), line.top(), egui::Stroke::new(1., LINE));
                        if jump == Some(step - 1) {
                            ui.scroll_to_rect(line, Some(egui::Align::TOP));
                        }
                        ui.add_space(28.);
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.;
                            let (r, _) =
                                ui.allocate_exact_size(egui::vec2(30., 28.), egui::Sense::hover());
                            let center = egui::pos2(r.left() + 14., r.center().y);
                            ui.painter().circle_filled(center, 14., ACCENT);
                            ui.painter().text(
                                center,
                                egui::Align2::CENTER_CENTER,
                                step.to_string(),
                                egui::FontId::proportional(13.),
                                Color32::WHITE,
                            );
                            ui.vertical(|ui| {
                                ui.set_max_width(width - 42.);
                                ui.add_space(2.);
                                ui.horizontal_top(|ui| {
                                    ui.label(RichText::new(title).size(15.).strong());
                                    if let Some(icon) = icon {
                                        let (r, _) = ui.allocate_exact_size(
                                            egui::vec2(28., 22.),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(
                                            r,
                                            5.,
                                            Color32::from_rgb(30, 36, 39),
                                        );
                                        step_icon(ui.painter(), r.shrink(2.), icon);
                                    }
                                });
                                ui.add_space(2.);
                                ui.scope(|ui| {
                                    ui.set_max_width(text_width.min(width - 42.));
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(t).size(13.5).color(STEP_BODY),
                                        )
                                        .wrap(),
                                    )
                                    .tip("A step in this task");
                                });
                                if let Some(pic) = pic {
                                    ui.add_space(14.);
                                    picture(ui, state, pic, width - 42.);
                                }
                            });
                        });
                        ui.add_space(28.);
                        continue;
                    }
                    if in_steps {
                        let (line, _) =
                            ui.allocate_exact_size(egui::vec2(width, 1.), egui::Sense::hover());
                        ui.painter()
                            .hline(line.x_range(), line.top(), egui::Stroke::new(1., LINE));
                        ui.add_space(16.);
                        in_steps = false;
                    }
                    match block {
                        Block::Step(..) => {}
                        Block::Text(t) => {
                            ui.scope(|ui| {
                                ui.set_max_width(text_width);
                                ui.label(RichText::new(t).size(13.5));
                            });
                        }
                        Block::Art(key, caption) => {
                            picture(ui, state, key, width);
                            ui.add_space(4.);
                            ui.label(RichText::new(caption).small().color(MUTED));
                        }
                        Block::Pair(l, r, caption) => {
                            ui.horizontal(|ui| {
                                picture(ui, state, l, width / 2. - 6.);
                                picture(ui, state, r, width / 2. - 6.);
                            });
                            ui.add_space(4.);
                            ui.label(RichText::new(caption).small().color(MUTED));
                        }
                        Block::Loop(t) => {
                            ui.label(
                                RichText::new(format!("▶ {t}"))
                                    .small()
                                    .color(Color32::from_rgb(125, 214, 237)),
                            );
                        }
                        Block::Tip(t) => {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(30, 40, 43))
                                .stroke(egui::Stroke::new(1., ACCENT))
                                .corner_radius(6)
                                .inner_margin(8)
                                .show(ui, |ui| {
                                    ui.set_max_width(text_width);
                                    ui.label(RichText::new(format!("Tip: {t}")).size(13.));
                                });
                        }
                    }
                    ui.add_space(12.);
                }
            });
    });
    let view = output.inner_rect.height();
    if let Some(last) = tops.len().checked_sub(1) {
        let mut at = tops.iter().rposition(|t| *t <= offset + 60.).unwrap_or(0);
        if offset > 0. && offset + view >= output.content_size.y - 4. {
            at = last;
        }
        state.step = at;
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("help-step-tops"), tops));
}
/// A group's name and icon, then each row's label and keys.
pub(super) type ShortcutGroup = (&'static str, &'static str, Vec<(String, String)>);
/// The shortcut groups: name, then each row's label and action.
pub(super) fn shortcut_groups(keys: &capture_core::keybinds::Keybinds) -> [ShortcutGroup; 6] {
    use capture_core::editor_keybinds::{Action, Function, TOOL_DEFAULTS};
    let mut groups = [
        ("File", "Open", vec![]),
        ("Edit", "Help edit", vec![]),
        ("Tools", "Selection", vec![]),
        ("Arrange", "Layers", vec![]),
        ("View", "Zoom", vec![]),
        ("Video", "Play", vec![]),
    ];
    for (name, chord) in [("Rename project", "F2"), ("Help", "F1")] {
        groups[0].2.push((name.to_owned(), chord.to_owned()));
    }
    for (tool, _) in TOOL_DEFAULTS {
        if let Some(chord) = keys.editor.get(Action::Tool(tool)) {
            let name = crate::shell::TOOLS
                .iter()
                .find(|t| t.id == tool)
                .map_or("Tool", |t| t.name);
            groups[2].2.push((name.to_owned(), chord.label()));
        }
    }
    for f in Function::ALL {
        let group = match f {
            Function::New | Function::Open | Function::Save | Function::SaveAs => 0,
            Function::Undo
            | Function::Redo
            | Function::Cut
            | Function::Copy
            | Function::CopyAll
            | Function::Paste
            | Function::PasteStyle
            | Function::NewFromClipboard
            | Function::Delete
            | Function::SelectAll => 1,
            Function::Group
            | Function::Ungroup
            | Function::BringForward
            | Function::SendBackward
            | Function::FlattenAll => 3,
            Function::ZoomIn
            | Function::ZoomOut
            | Function::ActualSize
            | Function::FitCanvas
            | Function::PixelGrid
            | Function::ToggleUi
            | Function::ToggleRecent
            | Function::ToggleAnnotations => 4,
            Function::VideoPlayback | Function::JumpToObject => 5,
        };
        if let Some(chord) = keys.editor.get(Action::Function(f)) {
            let label = if f == Function::VideoPlayback {
                "Play / Pause"
            } else {
                f.label()
            };
            groups[group].2.push((label.to_owned(), chord.label()));
        }
    }
    groups
}
pub(super) fn key_caps(chord: &str) -> Vec<String> {
    let mut caps: Vec<String> = chord
        .split('+')
        .filter(|k| !k.is_empty())
        .map(str::to_owned)
        .collect();
    if chord.ends_with('+') {
        caps.push("+".to_owned());
    }
    caps
}
/// Returns true when the reader asks to change their shortcuts.
fn shortcuts_page(
    ui: &mut egui::Ui,
    state: &mut State,
    keys: &capture_core::keybinds::Keybinds,
) -> bool {
    let mut change = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .id(egui::Id::unique("help-shortcut-filter"))
                .hint_text("Filter shortcuts")
                .desired_width(280.),
        );
        ui.label(
            RichText::new("Shows your own shortcuts, including any you changed.")
                .size(12.)
                .color(MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            change = ui
                .button("Change shortcuts…")
                .tip("Open File › Options › Keybinds.")
                .clicked();
        });
    });
    ui.add_space(10.);
    let query = state.filter.trim().to_lowercase();
    let groups: Vec<_> = shortcut_groups(keys)
        .into_iter()
        .filter_map(|(name, glyph, rows)| {
            let rows: Vec<_> = rows
                .into_iter()
                .filter(|(label, chord)| {
                    query.is_empty()
                        || label.to_lowercase().contains(&query)
                        || chord.to_lowercase().contains(&query)
                })
                .collect();
            (!rows.is_empty()).then_some((name, glyph, rows))
        })
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("help-shortcuts")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if groups.is_empty() {
                ui.label(
                    RichText::new(format!("No shortcuts match \"{}\".", state.filter.trim()))
                        .color(MUTED),
                );
                return;
            }
            let count = ((ui.available_width() / 300.) as usize).clamp(1, 3);
            // Each group goes in the shortest column, so they use the full width.
            let mut heights = vec![0usize; count];
            let mut placed = vec![vec![]; count];
            for group in &groups {
                let shortest = (0..count).min_by_key(|c| heights[*c]).unwrap_or(0);
                heights[shortest] += group.2.len() + 2;
                placed[shortest].push(group);
            }
            ui.columns(count, |columns| {
                for (column, groups) in columns.iter_mut().zip(placed) {
                    for (name, glyph, rows) in groups {
                        shortcut_card(column, name, glyph, rows);
                        column.add_space(12.);
                    }
                }
            });
        });
    change
}
fn shortcut_card(ui: &mut egui::Ui, name: &str, glyph: &str, rows: &[(String, String)]) {
    egui::Frame::new()
        .fill(PANE)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(15., 15.), egui::Sense::hover());
                crate::video_editor::ui::icon(
                    ui.painter(),
                    r,
                    glyph,
                    Color32::from_rgb(125, 214, 237),
                );
                ui.label(RichText::new(name).size(13.).strong());
            });
            ui.add_space(4.);
            for (label, chord) in rows {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(12.5));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 3.;
                        for cap in key_caps(chord).iter().rev() {
                            egui::Frame::new()
                                .fill(FIELD)
                                .stroke(egui::Stroke::new(1., LINE))
                                .corner_radius(4)
                                .inner_margin(egui::Margin::symmetric(6, 2))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(cap).size(11.).strong());
                                });
                        }
                    });
                });
            }
        });
}
fn about_page(ui: &mut egui::Ui) {
    let pane = ui.available_rect_before_wrap();
    ui.allocate_rect(pane, egui::Sense::hover());
    ui.painter().rect_filled(pane, 8., PANE);
    let width = (pane.width() - 48.).clamp(200., 760.);
    let height = 200f32;
    let inner = egui::Rect::from_center_size(
        pane.center(),
        egui::vec2(width, height.min(pane.height() - 24.)),
    );
    let mut ui = column(ui, inner);
    ui.columns(2, |columns| {
        let ui = &mut columns[0];
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(56., 56.), egui::Sense::hover());
            crate::brand::paint(ui, r);
            ui.vertical(|ui| {
                ui.label(RichText::new("ClipDesigner").size(24.).strong());
                ui.label(
                    RichText::new(format!(
                        "Version {}{}",
                        env!("CARGO_PKG_VERSION"),
                        option_env!("CLIPDESIGNER_BUILD")
                            .map(|b| format!(" · build {b}"))
                            .unwrap_or_default()
                    ))
                    .color(MUTED),
                );
            });
        });
        ui.add_space(14.);
        ui.label("Capture, mark up and share instructional images and videos. ClipDesigner is open source under the Apache License 2.0.");
        ui.add_space(10.);
        ui.label("Help works offline. Nothing is sent anywhere.");
        let ui = &mut columns[1];
        crate::controls::dialog_section_label(ui, "Details", None);
        ui.add_space(8.);
        for (name, value) in [
            ("License", "Apache 2.0"),
            ("Platform", "Windows x64"),
            ("Updates", "Manual"),
        ] {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(value);
                });
            });
            ui.add_space(6.);
            let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.), egui::Sense::hover());
            ui.painter().hline(line.x_range(), line.top(), egui::Stroke::new(1., LINE));
            ui.add_space(6.);
        }
        ui.add_space(8.);
        let notices = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("THIRD-PARTY-NOTICES.txt")))
            .filter(|p| p.exists());
        match notices {
            Some(path) => {
                if ui
                    .button("Third-party notices")
                    .tip("Open the licenses of the libraries and fonts ClipDesigner uses.")
                    .clicked()
                {
                    let _ = crate::export_ui::open_in_shell(&path);
                }
            }
            None => {
                ui.label(
                    RichText::new("Third-party notices ship beside ClipDesigner.exe in release packages.")
                        .color(MUTED),
                );
            }
        }
    });
}

/// The Help window's own marks, drawn on the mockup's 16-unit grid.
pub(crate) fn draw_glyph(p: &egui::Painter, rect: egui::Rect, name: &str, color: Color32) -> bool {
    let scale = rect.width().min(rect.height()) / 16.;
    let origin = rect.center() - egui::vec2(8., 8.) * scale;
    let at = |x: f32, y: f32| origin + egui::vec2(x, y) * scale;
    let stroke = egui::Stroke::new(1.4, color);
    let line = |points: &[(f32, f32)]| {
        p.add(egui::Shape::line(
            points.iter().map(|(x, y)| at(*x, *y)).collect(),
            stroke,
        ));
    };
    match name {
        "Help book" => {
            line(&[
                (2., 3.),
                (5., 2.5),
                (8., 3.5),
                (11., 2.5),
                (14., 3.),
                (14., 13.),
                (11., 12.5),
                (8., 13.5),
                (5., 12.5),
                (2., 13.),
                (2., 3.),
            ]);
            line(&[(8., 3.5), (8., 13.5)]);
        }
        "Help info" => {
            p.circle_stroke(at(8., 8.), 6.2 * scale, stroke);
            line(&[(8., 7.3), (8., 11.)]);
            p.circle_filled(at(8., 5.), 0.9 * scale, color);
        }
        "Help edit" => line(&[
            (3., 13.),
            (4., 10.),
            (11., 3.),
            (13., 5.),
            (6., 12.),
            (3., 13.),
        ]),
        "Help collapse" => line(&[(10., 3.), (5., 8.), (10., 13.)]),
        "Help strip" => {
            for x in [4.5, 8., 11.5] {
                line(&[(x, 3.), (x, 13.)]);
            }
        }
        _ => return false,
    }
    true
}

/// The picture beside a step: the ribbon's own icon for its two ribbon-only
/// names, the shared tool icon for the rest.
fn step_icon(painter: &egui::Painter, r: egui::Rect, icon: &str) {
    if matches!(icon, "Capture" | "Save As") {
        crate::ribbon::draw_icon(painter, r.center(), icon, Color32::WHITE);
    } else {
        crate::icons::draw(painter, r.shrink(5.), icon, Color32::WHITE);
    }
}

#[cfg(test)]
mod step_icon_tests {
    use super::*;
    #[test]
    fn every_step_icon_in_the_articles_draws_something() {
        let ctx = egui::Context::default();
        let mut checked = 0;
        for article in crate::help::articles() {
            for block in &article.blocks {
                let crate::help::Block::Step(_, Some(icon), _, _) = block else {
                    continue;
                };
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    step_icon(
                        ui.painter(),
                        egui::Rect::from_min_size(egui::pos2(10., 10.), egui::vec2(44., 36.)),
                        icon,
                    );
                });
                output.textures_delta.clear();
                assert!(!output.shapes.is_empty(), "{icon} drew nothing");
                checked += 1;
            }
        }
        assert!(checked > 0);
    }
}
