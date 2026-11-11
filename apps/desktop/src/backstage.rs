//! Original full-window File workspace, using the user's supplied layout reference.
use crate::tips::Tip as _;
use crate::{Editor, FileAction, egui};
use egui::{Color32, RichText};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Page {
    Home,
    New,
    Open,
    Info,
    Export,
}

impl Editor {
    pub(super) fn open_file_workspace(&mut self) {
        if self.commit_text() {
            self.cancel_gesture();
            self.shell.preview_ui = false;
            self.shell.backstage = Some(Page::Home);
            self.shell.recent_limit = 10;
        }
    }
    pub(super) fn show_backstage(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.new_image.is_none()
            && self.pdf_import.is_none()
            && self.pending.is_none()
            && self.template_picker.is_none()
            && !egui::Popup::is_any_open(&ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.shell.backstage = None;
            ctx.request_repaint();
        }
        let page = self.shell.backstage.unwrap_or(Page::Home);
        egui::Panel::left("file_sidebar")
            .exact_size(158.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(crate::shell::ACCENT)
                    .inner_margin(6),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                if nav(ui, "Back", false).clicked() {
                    self.shell.backstage = None;
                }
                ui.add_space(10.0);
                for (target, label) in [
                    (Page::Home, "Home"),
                    (Page::New, "New"),
                    (Page::Open, "Open"),
                ] {
                    if nav(ui, label, page == target).clicked() {
                        self.shell.backstage = Some(target);
                    }
                }
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                if nav(ui, "Info", page == Page::Info).clicked() {
                    self.shell.backstage = Some(Page::Info);
                }
                if nav(ui, "Save", false).clicked() {
                    self.save(false);
                }
                if nav(ui, "Save As", false).clicked() {
                    self.save(true);
                }
                if nav(ui, "Export", page == Page::Export).clicked() {
                    self.shell.backstage = Some(Page::Export);
                }
                if nav(ui, "Library", false).clicked() && self.commit_text() {
                    self.shell.backstage = None;
                    self.shell.library = true;
                    self.history.refresh();
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if nav(ui, "Help", false).clicked() {
                        crate::help::request(&ctx, None, "");
                        self.shell.backstage = None;
                    }
                    if nav(ui, "Preferences", false).clicked() {
                        self.open_preferences(crate::preferences::Section::General);
                    }
                    if nav(ui, "Exit", false).clicked() {
                        self.request_action(FileAction::Close, &ctx);
                    }
                    ui.separator();
                });
            });
        if matches!(page, Page::Home | Page::New) {
            egui::Panel::right("file_templates")
                .exact_size(TEMPLATES_W)
                .resizable(false)
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(24, 32, 35))
                        .stroke(egui::Stroke::new(1., LINE))
                        .inner_margin(egui::Margin {
                            left: 16,
                            right: 16,
                            top: 18,
                            bottom: 16,
                        }),
                )
                .show(ui, |ui| self.file_templates_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(50, 50, 50))
                    .inner_margin(egui::Margin {
                        left: 40,
                        right: if matches!(page, Page::Home | Page::New) {
                            22
                        } else {
                            6
                        },
                        top: 24,
                        bottom: 16,
                    }),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("file_page_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width().min(1180.0));
                        ui.horizontal(|ui| {
                            let (r, _) = ui
                                .allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::hover());
                            crate::brand::paint(ui, r);
                            ui.label(RichText::new("ClipDesigner").size(26.0).strong());
                        });
                        ui.add_space(22.0);
                        match page {
                            Page::Home | Page::New => self.file_new_page(ui, &ctx),
                            Page::Open => {
                                ui.heading("Open");
                                ui.add_space(16.0);
                                if ui
                                    .add_sized(
                                        [220.0, 36.0],
                                        egui::Button::new("Browse images and projects…"),
                                    )
                                    .clicked()
                                {
                                    self.request_action(FileAction::Open, &ctx);
                                }
                                ui.label("Open an image or an editable ClipDesigner project.");
                                ui.add_space(30.0);
                                self.backstage_recent(ui);
                            }
                            Page::Info => self.file_info(ui),
                            Page::Export => {
                                ui.heading("Export");
                                ui.add_space(18.0);
                                if ui
                                    .add_sized(
                                        [220.0, 38.0],
                                        egui::Button::new("Export PNG / JPEG / PDF…"),
                                    )
                                    .clicked()
                                {
                                    self.export();
                                }
                                ui.label("Save a flattened image to a location you choose.");
                                ui.add_space(16.0);
                                if ui
                                    .add_sized(
                                        [220.0, 38.0],
                                        egui::Button::new("Save editable project…"),
                                    )
                                    .clicked()
                                {
                                    self.save(true);
                                }
                                ui.label(
                                    "Keep the original image, annotations and formatting editable.",
                                );
                                ui.add_space(16.0);
                                if ui
                                    .add_sized(
                                        [220.0, 38.0],
                                        egui::Button::new("Copy complete image"),
                                    )
                                    .clicked()
                                {
                                    self.copy_all();
                                }
                                ui.add_space(20.0);
                                if ui
                                    .add_sized([220.0, 38.0], egui::Button::new("Print preview…"))
                                    .clicked()
                                {
                                    self.begin_export(true);
                                }
                            }
                        }
                    });
            });
    }
    /// File > New (approved AZ mockup 4): quick starts as icon cards in two
    /// groups, then one row of Recent. Templates live in the right-hand panel.
    fn file_new_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        heading(ui, "Image", "New Image", None);
        ui.add_space(10.);
        match cards(
            ui,
            &[
                ("Capture", "Capture Clip", "Current capture settings", true),
                ("Blank page", "Blank image", "Color or transparent", false),
                ("New clipboard", "From clipboard", "Paste an image", false),
                ("PDF", "From PDF", "Pages into a guide", false),
            ],
        ) {
            Some(0) => self.request_action(FileAction::Capture, ctx),
            Some(1) => {
                self.new_image = Some(crate::new_image::NewImage::remembered(
                    self.quick_controls.blank_defaults(),
                ));
            }
            Some(2) => self.new_from_clipboard(ctx),
            Some(_) => self.new_from_pdf(),
            None => {}
        }
        ui.add_space(18.);
        heading(ui, "Camera", "New Video", None);
        ui.add_space(10.);
        match cards(
            ui,
            &[
                ("Camera", "Capture Video", "Record your screen", true),
                ("Blank page", "Blank video", "Size and duration", false),
                ("Video file", "From file", "Start from a video", false),
                ("Image", "From images", "Show pictures in order", false),
            ],
        ) {
            Some(0) => {
                self.open_video_capture();
                self.clip.recording_target = 0;
            }
            Some(1) => self.new_video(capture_core::video::templates::Template::Blank),
            Some(2) => self.new_video_from_file(),
            Some(_) => self.new_video_from_images(ctx),
            None => {}
        }
        ui.add_space(18.);
        self.recent_row(ui);
    }
    /// One row of the most recent Library items; Open shows the full list.
    fn recent_row(&mut self, ui: &mut egui::Ui) {
        let mut entries = self.history.entries();
        entries.sort_by_key(|e| std::cmp::Reverse(e.updated_ms.unwrap_or(e.created_ms)));
        let width = ui.available_width();
        let columns = (((width + GAP) / (RECENT_MIN + GAP)).floor() as usize).clamp(1, 6);
        let shown = columns.min(entries.len());
        let count = format!("· {shown} of {}", entries.len());
        heading(
            ui,
            "",
            "Recent",
            (!entries.is_empty()).then_some(count.as_str()),
        );
        ui.add_space(10.);
        if entries.is_empty() {
            ui.label(
                RichText::new(
                    "Your library images appear here. Use Open to browse for an image or project.",
                )
                .color(MUTED),
            );
            return;
        }
        let card = (width - GAP * (columns as f32 - 1.)) / columns as f32;
        let mut open = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for entry in &entries[..shown] {
                ui.vertical(|ui| {
                    ui.set_width(card);
                    let response = self.history_card(ui, entry, egui::vec2(card, 70.));
                    let available = self.history.available(&entry.id);
                    self.history_context(&response, entry, available);
                    if response.clicked() && available {
                        open = Some(entry.id.clone());
                    }
                    ui.add(egui::Label::new(RichText::new(&entry.label).color(MUTED)).truncate())
                        .tip(&entry.label);
                });
            }
        });
        if let Some(id) = open {
            self.open_history(&id, &ui.ctx().clone());
        }
    }
    fn backstage_recent(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recent");
        if ui
            .add(
                crate::unicode_input::TextEdit::singleline(&mut self.shell.recent_query)
                    .hint_text("Search for an image")
                    .desired_width(240.),
            )
            .changed()
        {
            self.shell.recent_limit = 10;
        }
        ui.add_space(10.0);
        let mut entries = self.history.entries();
        entries.retain(|e| e.matches_query(&self.shell.recent_query));
        entries.sort_by_key(|e| std::cmp::Reverse(e.updated_ms.unwrap_or(e.created_ms)));
        if entries.is_empty() {
            ui.label(
                "Your library images appear here. Use Open to browse for an image or project.",
            );
        }
        let visible = self.shell.recent_limit.min(entries.len());
        let mut open = None;
        egui::ScrollArea::horizontal()
            .id_salt("file_recent_thumbnails")
            .show(ui, |ui| {
                for row in entries[..visible].chunks(10) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;
                        for entry in row {
                            ui.vertical(|ui| {
                                ui.set_width(80.);
                                let response = self.history_card(ui, entry, egui::vec2(80., 48.));
                                let available = self.history.available(&entry.id);
                                self.history_context(&response, entry, available);
                                if response.clicked() && available {
                                    open = Some(entry.id.clone());
                                }
                                ui.add(egui::Label::new(&entry.label).truncate())
                                    .tip(&entry.label);
                            });
                        }
                    });
                    ui.add_space(10.);
                }
            });
        if visible < entries.len() && ui.button("Show more").clicked() {
            self.shell.recent_limit += 10;
        }
        if let Some(id) = open {
            self.open_history(&id, &ui.ctx().clone());
        }
    }
    pub(crate) fn open_keyboard_shortcuts(&mut self) {
        self.open_preferences(crate::preferences::Section::Shortcuts);
    }
}

fn nav(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let (r, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 37.0), egui::Sense::click());
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            r,
            3.0,
            if selected {
                Color32::from_rgb(13, 81, 115)
            } else {
                Color32::from_rgb(38, 166, 211)
            },
        );
        if selected {
            ui.painter().rect_stroke(
                r,
                3.0,
                egui::Stroke::new(1.0, Color32::from_rgb(172, 222, 243)),
                egui::StrokeKind::Inside,
            );
        }
    }
    let c = r.left_center() + egui::vec2(17.0, 0.0);
    let color = Color32::WHITE;
    if label == "Back" {
        ui.painter()
            .circle_stroke(c, 9.0, egui::Stroke::new(1.2, color));
        ui.painter().arrow(
            c + egui::vec2(5.0, 0.0),
            egui::vec2(-9.0, 0.0),
            egui::Stroke::new(1.2, color),
        );
    } else {
        crate::ribbon::draw_icon(
            ui.painter(),
            c,
            match label {
                "Save" => "Save As",
                "Home" => "Editor",
                "New" | "Info" | "Open" => "Copy",
                _ => label,
            },
            color,
        );
    }
    if label != "Back" {
        ui.painter().text(
            r.left_center() + egui::vec2(38.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            color,
        );
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, label));
    response.tip(if label == "Back" {
        "Return to the workspace · Esc"
    } else {
        label
    })
}
const TEMPLATES_W: f32 = 360.;
const GAP: f32 = 10.;
const CARD_MIN: f32 = 170.;
const RECENT_MIN: f32 = 140.;
const CARD: Color32 = Color32::from_rgb(36, 43, 46);
const LINE: Color32 = Color32::from_rgb(52, 61, 64);
const MUTED: Color32 = Color32::from_gray(150);
const ICON_HI: Color32 = Color32::from_rgb(125, 214, 237);

/// A group heading with an optional icon and a muted count.
pub(super) fn heading(ui: &mut egui::Ui, icon: &str, title: &str, count: Option<&str>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.;
        if !icon.is_empty() {
            let (r, _) = ui.allocate_exact_size(egui::vec2(18., 18.), egui::Sense::hover());
            crate::video_editor::ui::icon(ui.painter(), r, icon, Color32::from_rgb(169, 196, 207));
        }
        ui.label(
            RichText::new(title)
                .size(15.)
                .strong()
                .color(Color32::WHITE),
        );
        if let Some(count) = count {
            ui.label(RichText::new(count).size(12.).color(MUTED));
        }
    });
}

/// Text cut to one line with an ellipsis.
fn one_line(ui: &egui::Ui, text: &str, size: f32, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_owned(),
        egui::TextFormat::simple(egui::FontId::proportional(size), color),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: width.max(1.),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    ui.painter().layout_job(job)
}

/// A row-wrapping grid of quick-start icon cards. Returns the clicked index.
fn cards(ui: &mut egui::Ui, items: &[(&str, &str, &str, bool)]) -> Option<usize> {
    let width = ui.available_width();
    let columns = (((width + GAP) / (CARD_MIN + GAP)).floor() as usize).max(1);
    let card = (width - GAP * (columns as f32 - 1.)) / columns as f32;
    let mut clicked = None;
    for (row, chunk) in items.chunks(columns).enumerate() {
        if row > 0 {
            ui.add_space(GAP);
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            for (i, (icon, name, description, primary)) in chunk.iter().enumerate() {
                if new_card(ui, card, icon, name, description, *primary).clicked() {
                    clicked = Some(row * columns + i);
                }
            }
        });
    }
    clicked
}

fn new_card(
    ui: &mut egui::Ui,
    width: f32,
    icon: &str,
    name: &str,
    description: &str,
    primary: bool,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(width, 62.), egui::Sense::click());
    let hot = response.hovered() || response.has_focus();
    let p = ui.painter();
    p.rect_filled(r, 9., CARD);
    p.rect_stroke(
        r,
        9.,
        egui::Stroke::new(1., if hot { crate::shell::ACCENT } else { LINE }),
        egui::StrokeKind::Inside,
    );
    let tile = egui::Rect::from_min_size(
        egui::pos2(r.left() + 10., r.center().y - 17.),
        egui::vec2(34., 34.),
    );
    p.rect_filled(
        tile,
        8.,
        if primary {
            crate::shell::ACCENT
        } else {
            Color32::from_rgb(26, 35, 38)
        },
    );
    crate::video_editor::ui::icon(
        p,
        egui::Rect::from_center_size(tile.center(), egui::vec2(19., 19.)),
        icon,
        if primary { Color32::WHITE } else { ICON_HI },
    );
    let left = tile.right() + 10.;
    let text_w = r.right() - 10. - left;
    let title = one_line(ui, name, 13.5, Color32::WHITE, text_w);
    let detail = one_line(ui, description, 11.5, MUTED, text_w);
    p.galley(
        egui::pos2(left, r.center().y - title.size().y),
        title,
        Color32::WHITE,
    );
    p.galley(egui::pos2(left, r.center().y + 2.), detail, MUTED);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    response.tip(description)
}

/// Equal-width two-way switch. Returns the newly chosen side, if any.
pub(super) fn segmented(ui: &mut egui::Ui, labels: [&str; 2], second: bool) -> Option<bool> {
    segmented_choice(ui, &labels, usize::from(second)).map(|i| i == 1)
}

/// Equal-width switch of several choices. Returns the newly chosen one, if any.
pub(super) fn segmented_choice(
    ui: &mut egui::Ui,
    labels: &[&str],
    selected: usize,
) -> Option<usize> {
    let (r, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.), egui::Sense::hover());
    ui.painter()
        .rect_filled(r, 7., Color32::from_rgb(30, 38, 41));
    ui.painter()
        .rect_stroke(r, 7., egui::Stroke::new(1., LINE), egui::StrokeKind::Inside);
    let mut chosen = None;
    let part = r.width() / labels.len() as f32;
    for (i, label) in labels.iter().copied().enumerate() {
        let half = egui::Rect::from_min_size(
            r.min + egui::vec2(i as f32 * part, 0.),
            egui::vec2(part, r.height()),
        )
        .shrink(3.);
        let on = i == selected;
        let response = ui.interact(
            half,
            ui.scope_id().with(("segment", label)),
            egui::Sense::click(),
        );
        if on {
            ui.painter()
                .rect_filled(half, 5., Color32::from_rgb(35, 65, 74));
        } else if response.hovered() {
            ui.painter()
                .rect_filled(half, 5., Color32::from_rgb(44, 52, 56));
        }
        ui.painter().text(
            half.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(13.),
            if on {
                Color32::WHITE
            } else {
                Color32::from_gray(175)
            },
        );
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
        if response.clicked() && !on {
            chosen = Some(i);
        }
    }
    chosen
}

/// A template tile: tinted icon header, then name and description. Saved
/// templates get a dashed inner outline.
pub(super) fn template_tile(
    ui: &mut egui::Ui,
    width: f32,
    icon: &str,
    name: &str,
    description: &str,
    tint: Color32,
    mine: bool,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(width, 112.), egui::Sense::click());
    let hot = response.hovered() || response.has_focus();
    let p = ui.painter();
    p.rect_filled(r, 8., CARD);
    let head = egui::Rect::from_min_size(r.min, egui::vec2(width, 74.));
    let mut mesh = egui::Mesh::default();
    let dark = Color32::from_rgb(28, 34, 38);
    let corners = [
        (head.left_top(), tint),
        (head.right_top(), dark),
        (head.right_bottom(), dark),
        (head.left_bottom(), tint.lerp_to_gamma(dark, 0.5)),
    ];
    for (pos, color) in corners {
        mesh.colored_vertex(pos, color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.with_clip_rect(egui::Rect::from_min_max(
        r.min,
        egui::pos2(r.right(), head.bottom()),
    ))
    .add(egui::Shape::Mesh(mesh.into()));
    if mine {
        let inner = head.shrink(6.);
        for (a, b) in [
            (inner.left_top(), inner.right_top()),
            (inner.right_top(), inner.right_bottom()),
            (inner.right_bottom(), inner.left_bottom()),
            (inner.left_bottom(), inner.left_top()),
        ] {
            p.extend(egui::Shape::dashed_line(
                &[a, b],
                egui::Stroke::new(1., Color32::from_rgba_unmultiplied(125, 214, 237, 90)),
                4.,
                3.,
            ));
        }
    }
    crate::video_editor::ui::icon(
        p,
        egui::Rect::from_center_size(head.center(), egui::vec2(24., 24.)),
        icon,
        Color32::from_white_alpha(200),
    );
    let text_w = width - 16.;
    let title = one_line(ui, name, 12., Color32::WHITE, text_w);
    let detail = one_line(ui, description, 11., MUTED, text_w);
    p.galley(
        egui::pos2(r.left() + 8., head.bottom() + 6.),
        title,
        Color32::WHITE,
    );
    p.galley(
        egui::pos2(r.left() + 8., head.bottom() + 22.),
        detail,
        MUTED,
    );
    p.rect_stroke(
        r,
        8.,
        egui::Stroke::new(1., if hot { crate::shell::ACCENT } else { LINE }),
        egui::StrokeKind::Inside,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
    if name == "From PDF" {
        return response.tip(
            "From PDF\nPages into a guide: choose pages, then annotate, redact and export like any image.",
        );
    }
    response.tip(format!("{name}\n{description}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn file_new_has_icon_cards_one_recent_row_and_a_templates_panel() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        let dir = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(dir.path());
        e.open_file_workspace();
        e.status = "This status must not appear in File".into();
        let mut first = frame(&ctx, &mut e, 1280., vec![]);
        let out = frame(&ctx, &mut e, 1280., vec![]);
        // Left column: icon cards in two groups, then Recent.
        let new_image = text_rect(&out, "New Image");
        let capture = text_rect(&out, "Capture Clip");
        let blank = text_rect(&out, "Blank image");
        assert!(capture.left() < blank.left());
        assert!((capture.center().y - blank.center().y).abs() < 1.);
        for label in ["From clipboard", "From file", "Blank video"] {
            text_rect(&out, label);
        }
        let new_video = text_rect(&out, "New Video");
        let recent = text_rect(&out, "Recent");
        assert!(new_image.top() < new_video.top() && new_video.top() < recent.top());
        for gone in [
            "From template",
            "Video template",
            "Custom size",
            "Image Templates",
            "Show more templates",
        ] {
            assert!(
                crate::color_widgets::tests::text_rect(&out.shapes, gone).is_none(),
                "{gone}"
            );
        }
        // Right column: Templates with Image/Video switch, search and tiles.
        let templates = text_rect(&out, "Templates");
        assert!(templates.left() > capture.right() + 300.);
        assert!(templates.right() < 1280.);
        let first_template = text_rect(&out, capture_core::templates::ALL[0].label);
        assert!(first_template.left() >= templates.left() - 1.);
        text_rect(&out, "Search templates");
        assert!(crate::color_widgets::tests::text_rect(&out.shapes, "Tutorial").is_none());
        assert!(crate::color_widgets::tests::text_rect(&out.shapes, &e.status).is_none());
        first.0.textures_delta.append(out.0.textures_delta.clone());
        if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_FILE_DIR") {
            dump_meshes(&ctx, &dir, "file-new", first, out);
        }
        let out = frame(&ctx, &mut e, 1280., vec![]);
        click(&ctx, &mut e, text_rect(&out, "Video"));
        let out = frame(&ctx, &mut e, 1280., vec![]);
        for label in ["Tutorial", "Product walkthrough", "Announcement"] {
            assert!(text_rect(&out, label).left() >= templates.left() - 1.);
        }
        assert!(
            crate::color_widgets::tests::text_rect(
                &out.shapes,
                capture_core::templates::ALL[0].label
            )
            .is_none()
        );
        click(&ctx, &mut e, text_rect(&out, "Image"));
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("file-templates-query"),
                "shortcut".to_string(),
            )
        });
        let out = frame(&ctx, &mut e, 1280., vec![]);
        text_rect(&out, "Shortcut sheet");
        text_rect(&out, "· 1");
        assert!(
            crate::color_widgets::tests::text_rect(
                &out.shapes,
                capture_core::templates::ALL[0].label
            )
            .is_none()
        );
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("file-templates-query"), String::new()));
        let out = frame(&ctx, &mut e, 1280., vec![]);
        let template = text_rect(&out, capture_core::templates::ALL[2].label);
        click(&ctx, &mut e, template);
        assert!(
            e.template_picker.is_none(),
            "A specific card must skip the chooser"
        );
        assert_eq!(
            e.session
                .document
                .guide
                .as_ref()
                .unwrap()
                .template
                .as_deref(),
            Some(capture_core::templates::ALL[2].id)
        );
    }
    #[test]
    fn file_recent_shows_ten_small_thumbnails_and_expands_in_batches() {
        let root = tempfile::tempdir().unwrap();
        let mut history = capture_core::history::History::open(root.path()).unwrap();
        for n in 0..23 {
            history
                .create(
                    &capture_core::Document::canvas(80, 50, [30, 90 + n, 160, 255]).unwrap(),
                    &format!("Example {n:02}"),
                    capture_core::history::Origin::Image,
                )
                .unwrap();
        }
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.shell.backstage = Some(Page::Open);
        let overlays = |out: &PaintedOutput| {
            out.shapes
                .iter()
                .filter_map(|s| {
                    if let egui::Shape::Text(t) = &s.shape {
                        (t.galley.text() == "Image").then_some(t.pos)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let first = frame(&ctx, &mut e, 1280., vec![]);
        let output = frame(&ctx, &mut e, 1280., vec![]);
        let row = overlays(&output);
        assert_eq!(row.len(), 10);
        assert!(row.iter().all(|p| p.y == row[0].y));
        assert!(row.windows(2).all(|p| (p[1].x - p[0].x - 88.).abs() < 0.1));
        let show = text_rect(&output, "Show more");
        if let Ok(dir) = std::env::var("CLIPDESIGNER_HEADLESS_FILE_DIR") {
            dump_meshes(&ctx, &dir, "recent-ten", first, output);
        }
        click(&ctx, &mut e, show);
        let output = frame(&ctx, &mut e, 1280., vec![]);
        assert_eq!(overlays(&output).len(), 20);
        click(&ctx, &mut e, text_rect(&output, "Show more"));
        let output = frame(&ctx, &mut e, 1280., vec![]);
        assert_eq!(overlays(&output).len(), 23);
        assert!(crate::color_widgets::tests::text_rect(&output.shapes, "Show more").is_none());
        e.open_file_workspace();
        assert_eq!(e.shell.recent_limit, 10);
    }
    fn frame(
        ctx: &egui::Context,
        e: &mut Editor,
        width: f32,
        events: Vec<egui::Event>,
    ) -> PaintedOutput {
        PaintedOutput(
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(
                            width,
                            if ctx
                                .data(|d| d.get_temp::<bool>(egui::Id::unique("tall-file-test")))
                                .unwrap_or(false)
                            {
                                2100.
                            } else {
                                720.
                            },
                        ),
                    )),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    e.show_shell(ui);
                    e.show_new_image(ui.ctx());
                },
            ),
        )
    }
    fn text_rect(output: &PaintedOutput, label: &str) -> egui::Rect {
        output
            .0
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    (t.galley.text() == label)
                        .then_some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("Missing {label}"))
    }
    fn click(ctx: &egui::Context, e: &mut Editor, r: egui::Rect) {
        for pressed in [true, false] {
            frame(
                ctx,
                e,
                1280.0,
                vec![
                    egui::Event::PointerMoved(r.center()),
                    egui::Event::PointerButton {
                        pos: r.center(),
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn capture_clip_keeps_the_view_behind_its_dialog() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        for page in [None, Some(Page::Home)] {
            e.shell.backstage = page;
            e.open_clip();
            assert!(e.clip.clip_dialog);
            assert_eq!(e.shell.backstage, page);
            e.clip.clip_dialog = false;
            assert_eq!(e.shell.backstage, page, "cancel stays there too");
        }
    }
    #[test]
    fn file_navigation_and_cancel_preserve_document_and_editor_selection() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(crate::Kind::Step);
        e.add_step([100.0, 100.0]);
        let before = e.session.document.clone();
        let selection = e.selected;
        frame(&ctx, &mut e, 1280.0, vec![]);
        let output = frame(&ctx, &mut e, 1280.0, vec![]);
        click(&ctx, &mut e, text_rect(&output, "File"));
        assert_eq!(e.shell.backstage, Some(Page::Home));
        assert!(!e.shell.canvas_enabled());
        assert!(!e.editor_shortcuts_idle());
        for (label, page) in [
            ("Open", Page::Open),
            ("Info", Page::Info),
            ("Export", Page::Export),
            ("New", Page::New),
        ] {
            let output = frame(&ctx, &mut e, 1280.0, vec![]);
            click(&ctx, &mut e, text_rect(&output, label));
            assert_eq!(e.shell.backstage, Some(page));
        }
        let output = frame(&ctx, &mut e, 1280.0, vec![]);
        click(&ctx, &mut e, text_rect(&output, "Blank image"));
        assert!(e.new_image.is_some());
        let output = frame(&ctx, &mut e, 1280.0, vec![]);
        click(&ctx, &mut e, text_rect(&output, "Cancel"));
        assert!(e.new_image.is_none());
        frame(
            &ctx,
            &mut e,
            1280.0,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(e.shell.backstage.is_none());
        assert!(e.shell.canvas_enabled());
        assert_eq!(e.selected, selection);
        assert_eq!(
            serde_json::to_value(&e.session.document).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        e.execute(crate::Command::Undo);
        assert!(e.session.document.annotations.is_empty());
    }
    #[test]
    fn full_window_file_pages_render_at_compact_and_wide_sizes() {
        for width in [900.0, 1280.0] {
            for page in [Page::Home, Page::New, Page::Open, Page::Info, Page::Export] {
                let ctx = egui::Context::default();
                crate::shell::configure(&ctx);
                let mut e = crate::canvas::tests::editor(&ctx);
                e.shell.backstage = Some(page);
                let before = e.session.document.clone();
                let first = frame(&ctx, &mut e, width, vec![]);
                let output = frame(&ctx, &mut e, width, vec![]);
                assert!(text_rect(&output, "ClipDesigner").max.x < width);
                assert_eq!(
                    serde_json::to_value(&before).unwrap(),
                    serde_json::to_value(&e.session.document).unwrap()
                );
                if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_FILE_DIR") {
                    dump_meshes(&ctx, &root, &format!("{page:?}-{width}"), first, output);
                }
            }
        }
    }
}
