pub(super) mod actions;
pub(crate) mod autosave;
mod import;
mod library;
mod library_icons;
mod manage;
mod recent;
mod recovery;
use crate::tips::Tip as _;
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
    pub(crate) store: Option<History>,
    error: String,
    textures: HashMap<String, Option<egui::TextureHandle>>,
    thumbnail_versions: HashMap<String, String>,
    thumbnail_pending_versions: HashMap<String, String>,
    thumbnail_jobs: HashMap<String, std::sync::mpsc::Receiver<Option<image::RgbaImage>>>,
    thumbnail_use: HashMap<String, u64>,
    thumbnail_clock: u64,
    thumbnail_scale: u16,
    thumbnail_workers: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    availability: HashMap<String, bool>,
    query: String,
    library: library::View,
    dialog: Option<actions::Dialog>,
    manage: Option<manage::Dialog>,
    import: import::Import,
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
        let root = crate::storage_locations::configured(
            capture_core::quick_preferences::Folder::ManagedProjects,
        )
        .or_else(|| std::env::var_os("CLIPCRAFT_HISTORY_DIR").map(PathBuf::from))
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
        let ids: std::collections::HashSet<_> = self.entries().into_iter().map(|e| e.id).collect();
        self.textures.retain(|id, _| ids.contains(id));
        self.thumbnail_jobs.retain(|id, _| ids.contains(id));
        self.thumbnail_use.retain(|id, _| ids.contains(id));
        self.thumbnail_versions.retain(|id, _| ids.contains(id));
        self.thumbnail_pending_versions
            .retain(|id, _| ids.contains(id));
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
    /// Every Library thumbnail has been decoded and uploaded (public screenshots).
    #[cfg(test)]
    pub(crate) fn thumbnails_settled(&self) -> bool {
        self.thumbnail_jobs.is_empty()
            && self
                .entries()
                .iter()
                .all(|e| self.textures.get(&e.id).is_some_and(Option::is_some))
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
        // Disk masters cover 200% tile zoom at 400% display scaling. Upload once
        // for this display's maximum tile size, independent of Library zoom.
        let scale = (ctx.pixels_per_point().clamp(1., 4.) * 100.).round() as u16;
        if self.thumbnail_scale != scale {
            self.thumbnail_scale = scale;
            self.textures.clear();
            self.thumbnail_jobs.clear();
            self.thumbnail_use.clear();
        }
        self.finish_thumbnails(ctx);
        self.thumbnail_clock = self.thumbnail_clock.wrapping_add(1);
        self.thumbnail_use
            .insert(entry.id.clone(), self.thumbnail_clock);
        if let Some(texture) = self.textures.get(&entry.id)
            && self
                .thumbnail_versions
                .get(&entry.id)
                .is_some_and(|v| v == entry.storage_id())
        {
            return texture.clone();
        }
        if !self.thumbnail_jobs.contains_key(&entry.id) {
            // Bounded asynchronous decode: never block a wheel/drag frame on a
            // document render or PNG decode, and never launch a thread per tile.
            if self.thumbnail_jobs.len() >= 2 {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                return self.textures.get(&entry.id).cloned().flatten();
            }
            use std::sync::atomic::Ordering;
            if self
                .thumbnail_workers
                .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    (n < 2).then_some(n + 1)
                })
                .is_err()
            {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
                return self.textures.get(&entry.id).cloned().flatten();
            }
            let workers = self.thumbnail_workers.clone();
            let root = self.store.as_ref()?.root().to_owned();
            let id = entry.id.clone();
            let wake = ctx.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            self.thumbnail_jobs.insert(id.clone(), rx);
            self.thumbnail_pending_versions
                .insert(id.clone(), entry.storage_id().into());
            std::thread::spawn(move || {
                struct Active(std::sync::Arc<std::sync::atomic::AtomicUsize>);
                impl Drop for Active {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::AcqRel);
                    }
                }
                let _active = Active(workers);
                let result = History::open(&root)
                    .ok()
                    .and_then(|store| store.master_thumbnail(&id).ok())
                    .map(|master| {
                        image::imageops::thumbnail(
                            &master,
                            (320 * u32::from(scale) / 100).min(master.width()),
                            (192 * u32::from(scale) / 100).min(master.height()),
                        )
                    });
                let _ = tx.send(result);
                wake.request_repaint();
            });
        }
        self.textures.get(&entry.id).cloned().flatten()
    }
    fn finish_thumbnails(&mut self, ctx: &egui::Context) {
        let ready: Vec<_> = self
            .thumbnail_jobs
            .iter()
            .filter_map(|(id, rx)| match rx.try_recv() {
                Ok(image) => Some((id.clone(), image)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some((id.clone(), None)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
            })
            .collect();
        for (id, image) in ready {
            self.thumbnail_jobs.remove(&id);
            if let Some(version) = self.thumbnail_pending_versions.remove(&id) {
                self.thumbnail_versions.insert(id.clone(), version);
            }
            let Some(image) = image else {
                self.textures.entry(id).or_insert(None);
                continue;
            };
            while self
                .textures
                .values()
                .filter_map(|t| t.as_ref())
                .map(|t| t.size()[0] * t.size()[1] * 4)
                .sum::<usize>()
                + image.as_raw().len()
                > 192 * 1024 * 1024
            {
                let victim = self
                    .textures
                    .keys()
                    .min_by_key(|id| self.thumbnail_use.get(*id).copied().unwrap_or(0))
                    .cloned();
                if let Some(id) = victim {
                    self.textures.remove(&id);
                    self.thumbnail_use.remove(&id);
                } else {
                    break;
                }
            }
            let texture = ctx.load_texture(
                format!("history-{}", id),
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            self.textures.insert(id.clone(), Some(texture.clone()));
        }
    }
    pub(super) fn available(&mut self, id: &str) -> bool {
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
        // Reopening an unchanged local project reuses its existing Library
        // snapshot. Explicit Duplicate continues through its separate action.
        if origin == Origin::Project
            && self.session.document.video.is_none()
            && let Ok(bytes) = serde_json::to_vec(&self.session.document)
            && let Some(store) = &self.history.store
            && let Some(entry) = self
                .history
                .entries()
                .into_iter()
                .filter(|e| e.label == label && e.origin == origin)
                .find(|entry| {
                    store
                        .load(&entry.id)
                        .ok()
                        .and_then(|doc| serde_json::to_vec(&doc).ok())
                        .is_some_and(|saved| saved == bytes)
                })
        {
            self.history.active = Some(entry.id.clone());
            self.autosave
                .reset(&self.session.document, Some(entry), label, origin);
            return;
        }
        self.history.active = None;
        self.autosave
            .reset(&self.session.document, None, label, origin);
    }
    pub(crate) fn open_history(&mut self, id: &str, ctx: &egui::Context) {
        self.reopen_in_recent(id);
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
    pub(super) fn history_card(
        &mut self,
        ui: &mut egui::Ui,
        entry: &Entry,
        card_size: egui::Vec2,
    ) -> egui::Response {
        self.history_card_action(ui, entry, card_size, None)
    }
    pub(crate) fn history_card_action(
        &mut self,
        ui: &mut egui::Ui,
        entry: &Entry,
        card_size: egui::Vec2,
        action: Option<&str>,
    ) -> egui::Response {
        let ctx = ui.ctx().clone();
        let active = action.is_none() && self.history.active.as_deref() == Some(&entry.id);
        let available = self.history.available(&entry.id);
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
            if let Some(action) = action {
                action
            } else if self.shell.library {
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
                format!(
                    "{} {label}",
                    action.unwrap_or(if active { "Current" } else { "Open" })
                ),
            )
        });
        if ui.is_rect_visible(rect) {
            let texture = self.history.thumbnail(entry, &ctx);
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
            {
                let badge = if !available {
                    "Missing"
                } else if entry.kind == capture_core::history::MediaKind::Video {
                    "Video"
                } else if entry.kind == capture_core::history::MediaKind::Steps {
                    "Step guide"
                } else if entry.kind == capture_core::history::MediaKind::Template {
                    "From template"
                } else if matches!(entry.origin, Origin::Project | Origin::Saved) {
                    "Clip"
                } else {
                    "Image"
                };
                let text = painter.layout_no_wrap(
                    badge.to_owned(),
                    egui::FontId::proportional(10.5),
                    egui::Color32::WHITE,
                );
                let badge_rect = egui::Rect::from_min_size(
                    rect.left_bottom() + egui::vec2(6.0, -20.0),
                    egui::vec2(text.size().x + 12.0, 15.0),
                );
                painter.rect_filled(badge_rect, 4, egui::Color32::from_black_alpha(170));
                painter.galley(
                    badge_rect.center() - text.size() / 2.0,
                    text,
                    egui::Color32::WHITE,
                );
            }
            if action.is_none()
                && if self.shell.library {
                    self.history.library.selected.contains(&entry.id)
                } else {
                    active
                }
            {
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
        response.tip(&detail)
    }
    pub(crate) fn show_history(&mut self, ui: &mut egui::Ui, compact: bool) {
        if !compact {
            self.show_library(ui);
            return;
        }
        let entries = recent::recent_order(self.history.entries());
        if !self.history.error.is_empty() {
            ui.colored_label(egui::Color32::LIGHT_RED, &self.history.error);
        }
        if entries.is_empty() {
            ui.label("Your recent images will appear here.");
            return;
        }
        let mut open = None;
        egui::ScrollArea::horizontal()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .id_salt("recent_scroll")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for entry in &entries {
                        let response = self.history_card(ui, entry, egui::vec2(160.0, 96.0));
                        let available = self.history.available(&entry.id);
                        self.history_context(&response, entry, available);
                        self.recent_marks(ui, &response, entry);
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
    pub(crate) fn set_recent_visible(&mut self, visible: bool) {
        self.history.recent = visible;
        if visible && self.is_video() {
            self.video.collapsed = true;
        }
    }
    pub(crate) fn show_recent_history(&mut self, ui: &mut egui::Ui) {
        if self.history.recent && self.is_video() {
            // A restored video session must also honor the current dock choice.
            self.video.collapsed = true;
        }
        if self.history.recent && !self.shell.library_open() {
            egui::Panel::bottom("recent_history")
                .default_size(112.0)
                .resizable(false)
                .show(ui, |ui| {
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(egui::Id::unique("recent-panel"), ui.max_rect())
                    });
                    self.show_history(ui, true);
                });
        }
    }
}

#[cfg(test)]
mod roundup_tests {
    use super::*;
    #[test]
    fn reopening_same_project_reuses_entry_and_refresh_keeps_texture_alive() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.history = HistoryPanel::isolated(dir.path());
        let id = e
            .history
            .store
            .as_mut()
            .unwrap()
            .record(&e.session.document, "review-guide.iclip", Origin::Project)
            .unwrap()
            .id;
        let image = ctx.load_texture(
            "cached-preview",
            egui::ColorImage::filled([16, 16], egui::Color32::BLUE),
            Default::default(),
        );
        e.history.textures.insert(id.clone(), Some(image.clone()));
        e.record_history("review-guide.iclip", Origin::Project);
        assert_eq!(e.history.active, Some(id.clone()));
        assert_eq!(e.history.store.as_ref().unwrap().entries().len(), 1);
        e.history.refresh();
        assert_eq!(e.history.textures[&id].as_ref().unwrap().id(), image.id());
        e.session.document.width += 1;
        e.record_history("review-guide.iclip", Origin::Project);
        assert!(e.history.active.is_none());
    }
}
