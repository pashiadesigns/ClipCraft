//! GIF Studio, approved Layout A ("Cards below"): large side-by-side previews,
//! transport with Start here / Stop here, a filmstrip range timeline, a row of
//! GIF cards beside compact output settings, and an export footer.
use super::*;
use crate::tips::Tip as _;
use egui::{Color32, Pos2, Rect, Sense, Stroke};

const PANEL: Color32 = Color32::from_rgb(25, 31, 33);
const CARD: Color32 = Color32::from_rgb(36, 43, 46);
const CARD_SELECTED: Color32 = Color32::from_rgb(35, 65, 74);
const SIZE_TEXT: Color32 = Color32::from_rgb(125, 214, 237);
const ERROR: Color32 = Color32::from_rgb(255, 130, 130);
/// Main-timeline clip geometry: a 36 px row holding a 30 px filmstrip.
const ROW: f32 = 36.;
const STRIP: f32 = 30.;
/// Shortest range a drag can produce.
const MIN_RANGE: u64 = 100;
const MAX_RANGE: u64 = 300_000;

fn bytes(n: u64) -> String {
    if n < 1_048_576 {
        format!("{:.0} KB", n as f64 / 1024.)
    } else {
        format!("{:.2} MB", n as f64 / 1_048_576.)
    }
}
fn seconds(ms: u64) -> String {
    format!("{:.2} s", ms as f64 / 1000.)
}
fn estimate<'a>(s: &'a State, r: &Range) -> Option<&'a capture_core::gif_studio::Estimate> {
    let key = audition_key(r);
    s.estimates
        .iter()
        .find(|(cached, _)| *cached == key)
        .and_then(|(_, v)| v.as_ref().ok())
}
fn thumbnail(strip: &Strip, at: u64) -> Option<egui::TextureId> {
    strip
        .iter()
        .min_by_key(|(t, _)| t.abs_diff(at))
        .map(|(_, id)| *id)
}
fn fit(texture: Option<&egui::TextureHandle>, ui: &egui::Ui, rect: Rect) {
    ui.painter()
        .rect_filled(rect, 4., Color32::from_rgb(13, 16, 17));
    if let Some(t) = texture {
        let size = t.size_vec2();
        let scale = (rect.width() / size.x).min(rect.height() / size.y);
        ui.painter().image(
            t.id(),
            Rect::from_center_size(rect.center(), size * scale),
            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Preparing preview…",
            egui::FontId::proportional(12.),
            ui.visuals().weak_text_color(),
        );
    }
}
fn preview(
    ui: &mut egui::Ui,
    title: &str,
    meta: &str,
    note: &str,
    texture: Option<&egui::TextureHandle>,
    height: f32,
) {
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(8)
        .inner_margin(8)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(meta).size(11.).color(SIZE_TEXT))
                            .truncate(),
                    );
                });
            });
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
            fit(texture, ui, rect);
            ui.add_sized(
                [ui.available_width(), 15.],
                egui::Label::new(egui::RichText::new(note).size(11.).weak()).truncate(),
            );
        });
}
fn new_range(p: &Project, at: u64) -> Range {
    let duration = p.duration().max(1);
    let id = p.gif_ranges.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    let start = at.min(duration - 1);
    Range {
        id,
        name: format!("GIF {id}"),
        start,
        end: (start + 5000).min(duration).max(start + 1),
        settings: Default::default(),
    }
}
fn transport(ui: &mut egui::Ui, p: &mut Project, s: &mut State) {
    let duration = p.duration().max(1);
    ui.horizontal(|ui| {
        let playing = s.playing.is_some();
        if super::super::ui::button(
            ui,
            if playing { "Pause" } else { "Play" },
            if playing { "Pause" } else { "Play" },
            92.,
            playing,
        )
        .clicked()
        {
            s.playing = if playing {
                None
            } else {
                let start = p
                    .gif_ranges
                    .iter()
                    .find(|r| Some(r.id) == s.selected)
                    .map_or(s.cursor, |r| s.cursor.clamp(r.start, r.end - 1));
                Some((Instant::now(), start))
            };
        }
        ui.label(
            egui::RichText::new(format!("{} / {}", seconds(s.cursor), seconds(duration)))
                .monospace()
                .weak(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let selected = p.gif_ranges.iter().position(|r| Some(r.id) == s.selected);
            let stop = ui
                .add_enabled_ui(selected.is_some() && s.job.is_none(), |ui| {
                    super::super::ui::button(ui, "End here", "Stop here", 112., false)
                })
                .inner
                .tip("End the selected GIF at the playhead");
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("gif-stop-here"), stop.rect));
            if stop.clicked()
                && let Some(i) = selected
            {
                let r = &mut p.gif_ranges[i];
                r.end = (s.cursor + 1)
                    .max(r.start + 1)
                    .min(duration)
                    .min(r.start + MAX_RANGE);
                s.playing = None;
            }
            let start = ui
                .add_enabled_ui(s.job.is_none() && p.gif_ranges.len() < 64, |ui| {
                    super::super::ui::button(ui, "Start here", "Start here", 112., false)
                })
                .inner
                .tip("Start the selected GIF at the playhead, or add one here");
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique("gif-start-here"), start.rect));
            if start.clicked() {
                s.playing = None;
                if let Some(i) = selected {
                    let r = &mut p.gif_ranges[i];
                    r.start = s.cursor.min(r.end - 1).max(r.end.saturating_sub(MAX_RANGE));
                } else {
                    let r = new_range(p, s.cursor);
                    s.selected = Some(r.id);
                    s.checked.insert(r.id);
                    p.gif_ranges.push(r);
                }
            }
        });
    });
}
fn timeline(ui: &mut egui::Ui, p: &mut Project, s: &mut State, strip: &Strip) {
    let duration = p.duration().max(1);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW + 18.),
        Sense::click_and_drag(),
    );
    let row = Rect::from_min_size(
        rect.min + egui::vec2(0., 16.),
        egui::vec2(rect.width(), ROW),
    );
    let x = |ms: u64| row.left() + ms as f32 / duration as f32 * row.width();
    let at = |px: f32| (((px - row.left()) / row.width()).clamp(0., 1.) * duration as f32) as u64;
    let painter = ui.painter().with_clip_rect(rect.expand(2.));
    for i in 0..=6 {
        let t = duration * i / 6;
        painter.text(
            egui::pos2(x(t), rect.top()),
            match i {
                0 => egui::Align2::LEFT_TOP,
                6 => egui::Align2::RIGHT_TOP,
                _ => egui::Align2::CENTER_TOP,
            },
            format!("{:.1}s", t as f64 / 1000.),
            egui::FontId::proportional(10.),
            ui.visuals().weak_text_color(),
        );
    }
    painter.rect_filled(row, 4., Color32::from_rgb(21, 26, 27));
    // Filmstrip, the same frames and height as video clips on the main timeline.
    if !strip.is_empty() {
        let w = row.width() / strip.len() as f32;
        let film = painter.with_clip_rect(row.shrink2(egui::vec2(0., 3.)));
        for (i, (_, texture)) in strip.iter().enumerate() {
            film.image(
                *texture,
                Rect::from_min_size(
                    egui::pos2(row.left() + i as f32 * w, row.top() + 3.),
                    egui::vec2(w, STRIP),
                ),
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                Color32::from_white_alpha(140),
            );
        }
    }
    for r in p.gif_ranges.iter().filter(|r| Some(r.id) != s.selected) {
        painter.rect_stroke(
            Rect::from_min_max(
                egui::pos2(x(r.start), row.top() + 5.),
                egui::pos2(x(r.end).max(x(r.start) + 2.), row.bottom() - 5.),
            ),
            3.,
            Stroke::new(1., Color32::from_rgba_unmultiplied(125, 214, 237, 110)),
            egui::StrokeKind::Inside,
        );
    }
    let selected = p.gif_ranges.iter().position(|r| Some(r.id) == s.selected);
    let edge_hit = |px: f32, edge: f32| (px - edge).abs() <= 6.;
    if let Some(i) = selected {
        let r = &p.gif_ranges[i];
        let bar = Rect::from_min_max(
            egui::pos2(x(r.start), row.top() + 1.),
            egui::pos2(x(r.end).max(x(r.start) + 4.), row.bottom() - 1.),
        );
        painter.rect_filled(bar, 5., Color32::from_rgba_unmultiplied(24, 145, 193, 40));
        painter.rect_stroke(
            bar,
            5.,
            Stroke::new(2., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
        let label = format!("{} · {}", r.name, seconds(r.end - r.start));
        let galley =
            painter.layout_no_wrap(label, egui::FontId::proportional(10.5), Color32::WHITE);
        let tag = Rect::from_min_size(
            bar.left_top() + egui::vec2(5., 1.),
            galley.size() + egui::vec2(10., 2.),
        );
        if tag.width() < bar.width() - 10. {
            painter.rect_filled(tag, 3., crate::shell::ACCENT);
            painter.galley(tag.min + egui::vec2(5., 1.), galley, Color32::WHITE);
        }
        for px in [bar.left(), bar.right()] {
            painter.line_segment(
                [
                    egui::pos2(px, bar.top() + 8.),
                    egui::pos2(px, bar.bottom() - 8.),
                ],
                Stroke::new(3., Color32::WHITE),
            );
        }
        if s.range_drag.is_none()
            && let Some(pos) = response.hover_pos()
        {
            ui.ctx().set_cursor_icon(
                if edge_hit(pos.x, bar.left()) || edge_hit(pos.x, bar.right()) {
                    egui::CursorIcon::ResizeHorizontal
                } else if bar.contains(pos) {
                    egui::CursorIcon::Grab
                } else {
                    egui::CursorIcon::Default
                },
            );
        }
    }
    // Press: move or trim the selected range, select another range, or seek.
    // Grab at the press origin: egui reports a drag only after the pointer moved.
    if response.drag_started()
        && let Some(pos) = ui
            .input(|i| i.pointer.press_origin())
            .or_else(|| response.interact_pointer_pos())
    {
        let ms = at(pos.x);
        s.playing = None;
        if let Some(i) = selected {
            let r = &p.gif_ranges[i];
            let edge = if edge_hit(pos.x, x(r.start)) {
                Some(-1)
            } else if edge_hit(pos.x, x(r.end)) {
                Some(1)
            } else if (r.start..r.end).contains(&ms) {
                Some(0)
            } else {
                None
            };
            if edge == Some(0) && s.duplicate_held {
                // The copy is the one that moves; the original stays put.
                let mut copy = r.clone();
                let id = p.gif_ranges.iter().map(|r| r.id).max().unwrap_or(0) + 1;
                copy.id = id;
                copy.name = format!("GIF {id}");
                s.range_drag = Some((id, 0, ms, r.start, r.end));
                s.selected = Some(id);
                p.gif_ranges.push(copy);
            } else if let Some(edge) = edge {
                s.range_drag = Some((r.id, edge, ms, r.start, r.end));
            }
        }
        if s.range_drag.is_none() {
            s.cursor = ms;
        }
    }
    if let Some((id, edge, from, start, end)) = s.range_drag
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        && let Some(r) = p.gif_ranges.iter_mut().find(|r| r.id == id)
    {
        let ms = at(pos.x);
        match edge {
            -1 => {
                r.start = ms
                    .min(end.saturating_sub(MIN_RANGE))
                    .max(end.saturating_sub(MAX_RANGE));
                s.cursor = r.start;
            }
            1 => {
                r.end = ms
                    .max(start + MIN_RANGE)
                    .min(duration)
                    .min(start + MAX_RANGE);
                s.cursor = r.end - 1;
            }
            _ => {
                let length = end - start;
                let shifted = (start as i64 + ms as i64 - from as i64)
                    .clamp(0, duration.saturating_sub(length) as i64)
                    as u64;
                r.start = shifted;
                r.end = shifted + length;
                s.cursor = s.cursor.clamp(r.start, r.end - 1);
            }
        }
        ui.ctx().set_cursor_icon(if edge == 0 {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::ResizeHorizontal
        });
    }
    if ui.input(|i| i.pointer.any_released()) {
        s.range_drag = None;
    }
    if response.dragged()
        && s.range_drag.is_none()
        && let Some(pos) = response.interact_pointer_pos()
    {
        s.cursor = at(pos.x);
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let ms = at(pos.x);
        s.playing = None;
        if let Some(other) = p
            .gif_ranges
            .iter()
            .find(|r| Some(r.id) != s.selected && (r.start..r.end).contains(&ms))
        {
            s.selected = Some(other.id);
        }
        s.cursor = ms;
    }
    let px = x(s.cursor.min(duration));
    painter.line_segment(
        [egui::pos2(px, row.top() - 4.), egui::pos2(px, row.bottom())],
        Stroke::new(2., Color32::WHITE),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(px - 5., row.top() - 6.),
            egui::pos2(px + 5., row.top() - 6.),
            egui::pos2(px, row.top()),
        ],
        Color32::WHITE,
        Stroke::NONE,
    ));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("gif-range-row"), row));
}
fn cards(ui: &mut egui::Ui, p: &mut Project, s: &mut State, strip: &Strip, height: f32) {
    egui::ScrollArea::horizontal()
        .id_salt("gif-cards")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.;
                if p.gif_ranges.is_empty() {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(260., height), Sense::hover());
                    ui.painter().rect_stroke(
                        rect,
                        8.,
                        Stroke::new(1., Color32::from_gray(80)),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No GIFs yet. Add one, or press\nStart here at the playhead.",
                        egui::FontId::proportional(12.),
                        ui.visuals().weak_text_color(),
                    );
                }
                let mut remove = None;
                let mut select = None;
                for r in &p.gif_ranges {
                    let selected = Some(r.id) == s.selected;
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(168., height), Sense::click());
                    // The whole card selects; its toggle and trash icon sit on top.
                    if response.clicked() {
                        select = Some(r.id);
                    }
                    let painter = ui.painter();
                    painter.rect_filled(rect, 7., if selected { CARD_SELECTED } else { CARD });
                    painter.rect_stroke(
                        rect,
                        7.,
                        Stroke::new(
                            1.,
                            if selected {
                                crate::shell::ACCENT
                            } else if response.hovered() {
                                Color32::from_gray(90)
                            } else {
                                Color32::TRANSPARENT
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let inner = rect.shrink(8.);
                    let thumb = Rect::from_min_size(
                        inner.min,
                        egui::vec2(inner.width(), (height - 98.).clamp(24., 52.)),
                    );
                    painter.rect_filled(thumb, 4., Color32::from_rgb(13, 16, 17));
                    if let Some(id) = thumbnail(strip, r.start) {
                        painter.image(
                            id,
                            thumb,
                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::WHITE,
                        );
                    }
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(Rect::from_min_max(
                                egui::pos2(inner.left(), thumb.bottom() + 4.),
                                inner.max,
                            ))
                            .layout(egui::Layout::top_down(egui::Align::Min)),
                    );
                    child.spacing_mut().item_spacing.y = 2.;
                    // Name with the GIF's total length beside it (AZ15).
                    child.horizontal(|ui| {
                        let length = egui::RichText::new(seconds(r.end - r.start))
                            .size(11.)
                            .strong()
                            .color(Color32::from_gray(215));
                        let galley = ui.painter().layout_no_wrap(
                            seconds(r.end - r.start),
                            egui::FontId::proportional(11.),
                            Color32::WHITE,
                        );
                        let name_w = (ui.available_width() - galley.size().x - 26.).max(30.);
                        ui.add_sized(
                            [name_w, 18.],
                            egui::Label::new(egui::RichText::new(&r.name).strong())
                                .truncate()
                                .selectable(false),
                        );
                        ui.label(length).tip("Total length");
                    });
                    child.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}–{}", seconds(r.start), seconds(r.end)))
                                .size(11.)
                                .weak(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(estimate(s, r).map_or_else(
                                    || "…".into(),
                                    |e| format!("≈ {}", bytes(e.bytes)),
                                ))
                                .size(11.)
                                .strong()
                                .color(SIZE_TEXT),
                            );
                        });
                    });
                    child.horizontal(|ui| {
                        let mut on = s.checked.contains(&r.id);
                        if crate::controls::toggle(ui, &mut on, "").changed() {
                            if on {
                                s.checked.insert(r.id);
                            } else {
                                s.checked.remove(&r.id);
                            }
                        }
                        ui.label(egui::RichText::new("Include in batch").size(11.).weak());
                    });
                    let trash = Rect::from_min_size(
                        rect.right_top() + egui::vec2(-28., 6.),
                        egui::vec2(22., 22.),
                    );
                    let hit = ui
                        .interact(
                            trash,
                            egui::Id::unique(("gif-remove", r.id)),
                            Sense::click(),
                        )
                        .tip(format!("Remove {}", r.name));
                    ui.painter().rect_filled(
                        trash,
                        5.,
                        if hit.hovered() {
                            Color32::from_rgba_unmultiplied(239, 106, 98, 40)
                        } else {
                            Color32::from_rgba_unmultiplied(19, 22, 22, 180)
                        },
                    );
                    super::super::ui::icon(
                        ui.painter(),
                        trash.shrink(4.),
                        "Delete effect",
                        if hit.hovered() {
                            ERROR
                        } else {
                            Color32::from_gray(190)
                        },
                    );
                    if hit.clicked() && s.job.is_none() {
                        remove = Some(r.id);
                    }
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::unique(("gif-card", r.id)), rect);
                        d.insert_temp(egui::Id::unique(("gif-trash", r.id)), trash);
                    });
                }
                if let Some(id) = select {
                    s.selected = Some(id);
                    s.playing = None;
                    if let Some(r) = p.gif_ranges.iter().find(|r| r.id == id)
                        && !(r.start..r.end).contains(&s.cursor)
                    {
                        s.cursor = r.start;
                    }
                }
                if let Some(id) = remove {
                    let index = p.gif_ranges.iter().position(|r| r.id == id).unwrap();
                    p.gif_ranges.remove(index);
                    s.checked.remove(&id);
                    if s.selected == Some(id) {
                        s.selected = p
                            .gif_ranges
                            .get(index.min(p.gif_ranges.len().saturating_sub(1)))
                            .map(|r| r.id);
                    }
                    s.playing = None;
                }
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(120., height), Sense::click());
                let enabled = p.gif_ranges.len() < 64 && s.job.is_none();
                ui.painter().rect_stroke(
                    rect,
                    7.,
                    Stroke::new(
                        1.,
                        if response.hovered() && enabled {
                            crate::shell::ACCENT
                        } else {
                            Color32::from_gray(80)
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                super::super::ui::icon(
                    ui.painter(),
                    Rect::from_center_size(
                        rect.center() - egui::vec2(0., 10.),
                        egui::vec2(18., 18.),
                    ),
                    "Plus",
                    Color32::from_gray(if enabled { 200 } else { 110 }),
                );
                ui.painter().text(
                    rect.center() + egui::vec2(0., 12.),
                    egui::Align2::CENTER_CENTER,
                    "Add GIF",
                    egui::FontId::proportional(12.),
                    Color32::from_gray(if enabled { 200 } else { 110 }),
                );
                if response.clicked() && enabled {
                    let r = new_range(p, s.cursor);
                    s.selected = Some(r.id);
                    s.checked.insert(r.id);
                    p.gif_ranges.push(r);
                    s.playing = None;
                }
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::unique("gif-add"), rect));
            });
        });
}
fn settings(ui: &mut egui::Ui, p: &mut Project, s: &mut State, size: [u32; 2]) {
    let duration = p.duration();
    egui::Frame::new()
        .fill(PANEL)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(10, 7))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.;
            let Some(r) = p.gif_ranges.iter_mut().find(|r| Some(r.id) == s.selected) else {
                ui.label(egui::RichText::new("OUTPUT").size(11.).weak());
                ui.weak("Select or add a GIF to change its settings.");
                return;
            };
            let dims = r.settings.dimensions(size);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("OUTPUT").size(11.).weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} × {} px · no audio", dims[0], dims[1]))
                            .size(11.)
                            .weak(),
                    );
                });
            });
            ui.add_sized(
                [ui.available_width(), 24.],
                egui::TextEdit::singleline(&mut r.name)
                    .char_limit(120)
                    .hint_text("GIF name"),
            );
            // Preset names are long ("Crisp instructions"): give them a full row.
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Preset").size(11.).weak());
                let label = (0..4)
                    .find(|i| r.settings == Settings::preset(*i))
                    .map_or("Custom", |i| Settings::PRESETS[i]);
                egui::ComboBox::from_id_salt("gif-preset")
                    .width(ui.available_width())
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        for (i, label) in Settings::PRESETS.iter().enumerate() {
                            if ui
                                .selectable_label(r.settings == Settings::preset(i), *label)
                                .clicked()
                            {
                                r.settings = Settings::preset(i);
                            }
                        }
                    });
            });
            ui.columns(3, |col| {
                col[0].label(egui::RichText::new("Width").size(11.).weak());
                let w = col[0].available_width();
                col[0].add_sized(
                    [w, 22.],
                    egui::DragValue::new(&mut r.settings.width)
                        .range(64..=1920)
                        .suffix(" px"),
                );
                col[1].label(egui::RichText::new("Frame rate").size(11.).weak());
                let w = col[1].available_width();
                col[1].add_sized(
                    [w, 22.],
                    egui::DragValue::new(&mut r.settings.fps)
                        .range(1..=50)
                        .suffix(" fps"),
                );
                col[2].label(egui::RichText::new("Colors").size(11.).weak());
                let w = col[2].available_width();
                col[2].add_sized(
                    [w, 22.],
                    egui::DragValue::new(&mut r.settings.colors).range(2..=256),
                );
            });
            ui.columns(2, |col| {
                col[0].label(egui::RichText::new("Quality").size(11.).weak());
                egui::ComboBox::from_id_salt("gif-quality")
                    .width(col[0].available_width())
                    .selected_text(if r.settings.speed == 1 {
                        "Best"
                    } else if r.settings.speed <= 5 {
                        "Balanced"
                    } else {
                        "Fast"
                    })
                    .show_ui(&mut col[0], |ui| {
                        for (v, n) in [(1, "Best"), (5, "Balanced"), (10, "Fast")] {
                            ui.selectable_value(&mut r.settings.speed, v, n);
                        }
                    });
                col[1].label(egui::RichText::new("Loop").size(11.).weak());
                crate::controls::toggle(&mut col[1], &mut r.settings.repeat, "");
            });
            let r = &*r;
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Estimated size");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(estimate(s, r).map_or_else(
                            || "Updating…".into(),
                            |e| format!("≈ {}", bytes(e.bytes)),
                        ))
                        .strong()
                        .color(SIZE_TEXT),
                    )
                    .tip(estimate(s, r).map_or_else(
                        || "Calculated from your settings and project frames.".to_string(),
                        |e| {
                            if e.exact() {
                                "All frames measured".into()
                            } else {
                                format!("{}–{} · sampled estimate", bytes(e.low), bytes(e.high))
                            }
                        },
                    ));
                });
            });
            let key = audition_key(r);
            if let Some((_, Err(e))) = s.estimates.iter().find(|(k, _)| *k == key) {
                ui.colored_label(ERROR, e);
            }
            if let Err(e) = r.validate_export(duration) {
                ui.colored_label(ERROR, e.to_string());
            }
        });
}
pub(super) fn show(
    ctx: &egui::Context,
    p: &mut Project,
    s: &mut State,
    size: [u32; 2],
    strip: &Strip,
) -> (bool, Option<Vec<u64>>) {
    let width = (ctx.content_rect().width() - 72.).clamp(440., 980.);
    let height = (ctx.content_rect().height() - 80.).clamp(490., 980.);
    let band = if height < 650. { 150. } else { 196. };
    // Previews take what remains after the fixed rows (about 55% at 720 px).
    let preview_h = (height - band - 300.).max(110.);
    let mut close = false;
    let mut export = None;
    let modal = crate::controls::Modal::new(egui::Id::unique("gif-studio")).show(ctx, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing = egui::vec2(10., 8.);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(24., 24.), Sense::hover());
            crate::icons::draw(ui.painter(), rect, "GIF Studio", crate::shell::ACCENT);
            ui.heading("GIF Studio");
        });
        let selected = p.gif_ranges.iter().find(|r| Some(r.id) == s.selected);
        let outside = selected.is_some_and(|r| !(r.start..r.end).contains(&s.cursor));
        let meta = selected.map_or_else(
            || "No GIF selected".to_string(),
            |r| {
                let d = r.settings.dimensions(size);
                format!(
                    "{} × {} px · {} fps · {} colors",
                    d[0], d[1], r.settings.fps, r.settings.colors
                )
            },
        );
        // BB14: one large GIF output preview; the original is not repeated.
        {
            preview(
                ui,
                "GIF output",
                &meta,
                if outside {
                    "Outside GIF range: the preview follows the playhead"
                } else {
                    ""
                },
                s.texture.as_ref(),
                preview_h,
            );
        }
        transport(ui, p, s);
        timeline(ui, p, s, strip);
        ui.horizontal_top(|ui| {
            let settings_w = 320f32.min(ui.available_width() * 0.45);
            let cards_w = ui.available_width() - settings_w - ui.spacing().item_spacing.x;
            ui.allocate_ui_with_layout(
                egui::vec2(cards_w, band),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(cards_w);
                    cards(ui, p, s, strip, band - 14.);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(settings_w, band),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(settings_w);
                    egui::ScrollArea::vertical()
                        .id_salt("gif-settings")
                        .max_height(band)
                        .show(ui, |ui| settings(ui, p, s, size));
                },
            );
        });
        crate::property_layout::divider(ui);
        let checked: Vec<&Range> = p
            .gif_ranges
            .iter()
            .filter(|r| s.checked.contains(&r.id))
            .collect();
        let total: Option<u64> = checked
            .iter()
            .map(|r| estimate(s, r).map(|e| e.bytes))
            .sum();
        ui.horizontal(|ui| {
            if s.message.is_empty() {
                ui.weak(format!(
                    "{} selected · {}",
                    checked.len(),
                    total.map_or_else(
                        || "estimating total…".into(),
                        |v| format!("≈ {} total", bytes(v))
                    )
                ));
            } else {
                ui.add(egui::Label::new(egui::RichText::new(&s.message).weak()).truncate());
            }
            if s.job.is_none() && !s.exported.is_empty() {
                if s.exported.len() == 1
                    && ui.link("Open GIF").clicked()
                    && let Err(e) = crate::export_ui::open_in_shell(&s.exported[0])
                {
                    s.message = format!("Couldn't open the GIF: {e}");
                }
                if let Some(folder) = s.exported[0].parent().map(Path::to_path_buf)
                    && ui.link("Open folder").clicked()
                    && let Err(e) = crate::export_ui::open_in_shell(&folder)
                {
                    s.message = format!("Couldn't open the folder: {e}");
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let all_ok = !checked.is_empty()
                    && checked
                        .iter()
                        .all(|r| r.validate_export(p.duration()).is_ok());
                if ui
                    .add_enabled(
                        s.job.is_none() && all_ok,
                        crate::controls::dialog_button(
                            &format!(
                                "Export {} GIF{}",
                                checked.len(),
                                if checked.len() == 1 { "" } else { "s" }
                            ),
                            true,
                        ),
                    )
                    .clicked()
                {
                    export = Some(checked.iter().map(|r| r.id).collect());
                }
                let selected = p.gif_ranges.iter().find(|r| Some(r.id) == s.selected);
                if ui
                    .add_enabled(
                        s.job.is_none()
                            && selected.is_some_and(|r| r.validate_export(p.duration()).is_ok()),
                        crate::controls::dialog_button("Export this GIF", false),
                    )
                    .clicked()
                    && let Some(r) = selected
                {
                    export = Some(vec![r.id]);
                }
                if ui
                    .add(crate::controls::dialog_button("Close", false))
                    .clicked()
                {
                    close = true;
                }
                if let Some(job) = &s.job {
                    ui.add(egui::ProgressBar::new(s.progress).desired_width(110.));
                    if ui
                        .add(crate::controls::dialog_button("Cancel export", false))
                        .clicked()
                    {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                }
            });
        });
    });
    if export.is_some() {
        s.exported.clear();
    }
    close |= crate::text_edit::cancel_on_escape(&modal);
    (close, export)
}
