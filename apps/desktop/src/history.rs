mod actions;
pub(crate) mod autosave;
mod library;
mod manage;
use crate::{Editor, FileAction, egui};
use capture_core::{
    Session,
    history::{Entry, History, Origin},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub struct HistoryPanel {
    store: Option<History>,
    error: String,
    textures: HashMap<String, Option<egui::TextureHandle>>,
    availability: HashMap<String, bool>,
    query: String,
    library: library::View,
    dialog: Option<actions::Dialog>,
    manage: Option<manage::Dialog>,
    pub(crate) active: Option<String>,
    pub recent: bool,
}
impl HistoryPanel {
    pub(crate) fn root(&self) -> Option<PathBuf> {
        self.store.as_ref().map(|s| s.root().to_owned())
    }
    #[cfg(test)]
    pub(crate) fn isolated(root: &std::path::Path) -> Self {
        Self {
            store: Some(History::open(root).unwrap()),
            ..Default::default()
        }
    }
    pub fn new() -> Self {
        let root = std::env::var_os("CLIPCRAFT_HISTORY_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .map(|p| PathBuf::from(p).join("ClipCraft").join("history-v1"))
            });
        match root.map(|root| History::open(&root).map_err(|e| e.to_string())) {
            Some(Ok(store)) => Self {
                store: Some(store),
                ..Default::default()
            },
            Some(Err(error)) => Self {
                error,
                ..Default::default()
            },
            None => Self {
                error: "Local history storage is unavailable on this system.".into(),
                ..Default::default()
            },
        }
    }
    pub fn refresh(&mut self) {
        if self.store.is_none() {
            let recent = self.recent;
            let query = std::mem::take(&mut self.query);
            let library = std::mem::take(&mut self.library);
            *self = Self::new();
            self.recent = recent;
            self.query = query;
            self.library = library;
            return;
        }
        if let Some(store) = &mut self.store {
            match store.refresh() {
                Ok(()) => self.error.clear(),
                Err(e) => self.error = e.to_string(),
            }
        }
        self.textures.clear();
        self.availability.clear();
    }
    pub(crate) fn entry(&self, id: &str) -> Option<Entry> {
        self.store
            .as_ref()?
            .entries()
            .iter()
            .find(|e| e.id == id)
            .cloned()
    }
    pub(crate) fn entries(&self) -> Vec<Entry> {
        self.store
            .as_ref()
            .map(|s| s.entries().to_vec())
            .unwrap_or_default()
    }
    pub(super) fn retain_capture(
        &mut self,
        document: &capture_core::Document,
    ) -> Result<(), String> {
        let store = self
            .store
            .as_mut()
            .ok_or("Local Library storage is unavailable")?;
        store
            .create(
                document,
                if document.guide.is_some() {
                    "Step capture"
                } else {
                    "Image capture"
                },
                Origin::Capture,
            )
            .map_err(|e| e.to_string())?;
        self.refresh();
        Ok(())
    }
    fn thumbnail(&mut self, entry: &Entry, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if let Some(texture) = self.textures.get(&entry.id) {
            return texture.clone();
        }
        let image = match self.store.as_ref()?.thumbnail(&entry.id) {
            Ok(image) => image,
            Err(_) => {
                self.textures.insert(entry.id.clone(), None);
                return None;
            }
        };
        let texture = ctx.load_texture(
            format!("history-{}", entry.id),
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        self.textures
            .insert(entry.id.clone(), Some(texture.clone()));
        Some(texture)
    }
    fn available(&mut self, id: &str) -> bool {
        *self
            .availability
            .entry(id.to_owned())
            .or_insert_with(|| self.store.as_ref().is_some_and(|s| s.available(id)))
    }
}

fn age(created_ms: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let seconds = now.saturating_sub(created_ms / 1000);
    if seconds < 60 {
        "just now".into()
    } else if seconds < 3600 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h ago", seconds / 3600)
    } else {
        format!("{}d ago", seconds / 86400)
    }
}
impl Editor {
    pub(crate) fn record_history(&mut self, label: &str, origin: Origin) {
        self.history.active = None;
        self.autosave
            .reset(&self.session.document, None, label, origin);
    }
    pub(crate) fn open_history(&mut self, id: &str, ctx: &egui::Context) {
        if self.history.active.as_deref() == Some(id) {
            self.shell.return_to_editor();
            return;
        }
        let result = self
            .history
            .store
            .as_ref()
            .ok_or_else(|| "Local history is unavailable.".to_owned())
            .and_then(|store| {
                store
                    .load(id)
                    .and_then(Session::unsaved)
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(session) => self.request_action(
                FileAction::HistorySnapshot {
                    session,
                    id: id.to_owned(),
                },
                ctx,
            ),
            Err(error) => {
                self.history.refresh();
                self.status = format!(
                    "Could not open history image: {error}. Your current image is unchanged."
                )
            }
        }
    }
    fn history_card(
        &mut self,
        ui: &mut egui::Ui,
        entry: &Entry,
        card_size: egui::Vec2,
    ) -> egui::Response {
        let ctx = ui.ctx().clone();
        let active = self.history.active.as_deref() == Some(&entry.id);
        let available = self.history.available(&entry.id);
        let texture = self.history.thumbnail(entry, &ctx);
        let label = if entry.label.is_empty() {
            entry.origin.label()
        } else {
            &entry.label
        };
        let detail = format!(
            "{label}\n{} · {} × {} px · {}\nCreated: {}\nStored snapshot and thumbnail: {:.1} KB\n{}",
            entry.origin.label(),
            entry.width,
            entry.height,
            age(entry.created_ms),
            entry.created_utc(),
            entry.bytes as f64 / 1000.0,
            if self.shell.library {
                "Click to select; double-click to open. Shift selects a range; Alt adds or removes items."
            } else if active {
                "Current image. Selecting it keeps your edits."
            } else {
                "Open this editable Library image. Changes save automatically."
            }
        );
        let (rect, response) = ui.allocate_exact_size(card_size, egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::Button,
                available,
                format!("{} {label}", if active { "Current" } else { "Open" }),
            )
        });
        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect_filled(rect, 4, egui::Color32::from_gray(47));
            if let Some(texture) = &texture {
                let size = texture.size_vec2();
                let scale = ((card_size.x - 16.0).max(1.0) / size.x)
                    .min((card_size.y - 8.0).max(1.0) / size.y);
                let image_rect = egui::Rect::from_center_size(rect.center(), size * scale);
                let image_painter = painter.with_clip_rect(image_rect.intersect(ui.clip_rect()));
                image_painter.rect_filled(image_rect, 0, egui::Color32::from_gray(240));
                for y in 0..(image_rect.height() / 6.0).ceil() as i32 {
                    for x in 0..(image_rect.width() / 6.0).ceil() as i32 {
                        if (x + y) % 2 == 0 {
                            image_painter.rect_filled(
                                egui::Rect::from_min_size(
                                    image_rect.min + egui::vec2(x as f32 * 6.0, y as f32 * 6.0),
                                    egui::vec2(6.0, 6.0),
                                ),
                                0,
                                egui::Color32::from_gray(205),
                            );
                        }
                    }
                }
                image_painter.image(
                    texture.id(),
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            let badge = if !available {
                "MISSING"
            } else if entry.kind == capture_core::history::MediaKind::Steps {
                "STEPS"
            } else if matches!(entry.origin, Origin::Project | Origin::Saved) {
                "CLIP"
            } else {
                "IMAGE"
            };
            let badge_rect = egui::Rect::from_min_size(
                rect.left_bottom() + egui::vec2(5.0, -18.0),
                egui::vec2(52.0, 14.0),
            );
            painter.rect_filled(badge_rect, 2, egui::Color32::from_black_alpha(210));
            painter.text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                badge,
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );
            if if self.shell.library {
                self.history.library.selected.contains(&entry.id)
            } else {
                active
            } {
                painter.rect_stroke(
                    rect.shrink(1.0),
                    4,
                    egui::Stroke::new(3.0, egui::Color32::from_rgb(25, 156, 209)),
                    egui::StrokeKind::Inside,
                );
                if active && !self.library_saved() {
                    painter.circle_filled(
                        rect.left_top() + egui::vec2(8.0, 8.0),
                        5.0,
                        egui::Color32::from_rgb(244, 191, 65),
                    );
                }
            }
        }
        response.on_hover_text(&detail)
    }
    pub(crate) fn show_history(&mut self, ui: &mut egui::Ui, compact: bool) {
        if !compact {
            self.show_library(ui);
            return;
        }
        let entries = self.history.entries();
        if !self.history.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.history.error);
        }
        if entries.is_empty() {
            ui.label("Your recent images will appear here.");
            return;
        }
        let mut open = None;
        egui::ScrollArea::horizontal()
            .id_salt("recent_scroll")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for entry in &entries {
                        let response = self.history_card(ui, entry, egui::vec2(160.0, 96.0));
                        let available = self.history.available(&entry.id);
                        self.history_context(&response, entry, available);
                        if response.clicked() && self.history.available(&entry.id) {
                            open = Some(entry.id.clone());
                        }
                    }
                });
            });
        if let Some(id) = open {
            self.open_history(&id, &ui.ctx().clone());
        }
    }
    pub(crate) fn show_recent_history(&mut self, ui: &mut egui::Ui) {
        if self.history.recent && !self.shell.library_open() {
            egui::Panel::bottom("recent_history")
                .default_size(112.0)
                .resizable(false)
                .show(ui, |ui| self.show_history(ui, true));
        }
    }
}
