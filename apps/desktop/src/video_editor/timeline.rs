use super::*;
use crate::tips::Tip as _;
use capture_core::video::links::Layer;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
pub(crate) struct Drag {
    id: u64,
    edge: i8,
    original: Project,
    origin: Pos2,
    target: u64,
    duplicate: bool,
    copy_id: u64,
}
fn time_label(ms: u64) -> String {
    format!("{:02}:{:02}.{:03}", ms / 60000, ms / 1000 % 60, ms % 1000)
}
pub(super) fn block(
    ui: &mut egui::Ui,
    rect: Rect,
    id: egui::Id,
    text: &str,
    color: Color32,
    selected: bool,
) -> egui::Response {
    let r = ui.interact(rect, id, Sense::click_and_drag());
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    p.rect_filled(rect, 4., color);
    p.rect_stroke(
        rect,
        4.,
        Stroke::new(
            if selected { 2. } else { 1. },
            if selected {
                crate::shell::ACCENT
            } else {
                Color32::from_white_alpha(35)
            },
        ),
        egui::StrokeKind::Inside,
    );
    if rect.width() > 38. {
        p.text(
            rect.left_center() + egui::vec2(10., 0.),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(11.),
            Color32::WHITE,
        );
    }
    r
}
pub(super) fn fade_handle(ui: &egui::Ui, rect: Rect, x: f32, enter: bool) {
    let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let inset = 4_f32.min(rect.width().max(0.) * 0.5);
    let point = egui::pos2(
        x.clamp(rect.left() + inset, rect.right() - inset),
        rect.top() + 4.,
    );
    let side = if enter { rect.left() } else { rect.right() };
    let bottom = egui::pos2(side, rect.bottom());
    p.add(egui::Shape::convex_polygon(
        vec![egui::pos2(side, rect.top()), point, bottom],
        Color32::from_black_alpha(95),
        Stroke::NONE,
    ));
    p.line_segment(
        [bottom, point],
        Stroke::new(1.5, Color32::from_rgb(110, 215, 239)),
    );
    p.circle_filled(point, 3., crate::shell::ACCENT);
}
pub(super) fn fade_cursor(ui: &egui::Ui, exit: bool) {
    let Some(pos) = ui.input(|i| i.pointer.hover_pos()) else {
        return;
    };
    ui.ctx().set_cursor_icon(egui::CursorIcon::None);
    let p = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("fade-cursor"),
    ));
    let points = if exit {
        vec![pos, pos + egui::vec2(0., 11.), pos + egui::vec2(12.5, 11.)]
    } else {
        vec![pos, pos + egui::vec2(0., 11.), pos + egui::vec2(-12.5, 11.)]
    };
    p.add(egui::Shape::convex_polygon(
        points,
        Color32::from_rgb(102, 213, 243),
        Stroke::new(1.5, Color32::from_rgb(16, 23, 25)),
    ));
}
impl Editor {
    pub(crate) fn video_playback_key(&mut self) {
        if self.video.voiceover.open && self.video.voiceover.take.is_some() {
            let play = self.video.voiceover.review_clock.is_none();
            let at = if self
                .video
                .voiceover
                .take
                .as_ref()
                .is_some_and(|t| self.video.voiceover.review_at >= t.duration)
            {
                0
            } else {
                self.video.voiceover.review_at
            };
            self.review_voiceover(at, play);
            return;
        }

        let now = Instant::now();
        self.video.space_count = if self
            .video
            .space_last
            .is_some_and(|t| now.duration_since(t) < Duration::from_millis(350))
        {
            self.video.space_count % 3 + 1
        } else {
            1
        };
        self.video.space_last = Some(now);
        match self.video.space_count {
            1 => self.video_play(),
            2 => {
                let time = self.video_clip_start();
                self.video_seek(time);
                self.video_play();
            }
            _ => {
                self.video_seek(0);
                self.video_play();
            }
        }
    }
    pub(crate) fn video_clip_start(&self) -> u64 {
        let Some(v) = &self.session.document.video else {
            return 0;
        };
        if let Some(t) = self.selected.and_then(|id| v.objects.get(&id)) {
            return t.start;
        }
        let starts: Vec<_> = v.segments.iter().map(|s| s.at).collect();
        if let Selection::Recording(i) = self.video.selected
            && let Some(start) = starts.get(i)
        {
            return *start;
        }
        v.objects
            .values()
            .map(|t| t.start)
            .chain(starts)
            .filter(|t| *t <= v.cursor)
            .max()
            .unwrap_or(0)
    }
    pub(crate) fn video_step(&mut self, forward: bool) {
        if let Some(v) = &self.session.document.video {
            let dt = 1000 / v.fps as u64;
            self.video_seek(if forward {
                v.cursor.saturating_add(dt)
            } else {
                v.cursor.saturating_sub(dt)
            });
        }
    }
    pub(crate) fn video_split(&mut self) {
        let Some(mut v) = self.session.document.video.clone() else {
            return;
        };
        let at = v.cursor;
        // Selected media clips, zooms and objects split at the playhead; with nothing
        // selected, the media clip under the playhead splits as before.
        if let Selection::Recording(i) = self.video.selected {
            match v.split_clip(i, at) {
                Ok(()) => self.commit_video(v),
                Err(e) => self.status = e.to_string(),
            }
            return;
        }
        if let Some(id) = self.video.zoom_selected {
            match capture_core::video::split::split_zoom(&mut v, id, at) {
                Ok(_) => self.commit_video(v),
                Err(e) => self.status = e.to_string(),
            }
            return;
        }
        let objects: Vec<u64> = self
            .selection_ordered_ids()
            .into_iter()
            .filter(|id| {
                v.objects
                    .get(id)
                    .is_some_and(|t| t.active(at) && t.start < at)
            })
            .collect();
        if !objects.is_empty() {
            self.commit_text();
            self.commit_property_preview();
            let mut doc = self.session.document.clone();
            for id in objects {
                if let Err(e) = capture_core::video::split::split_object(&mut doc, id, at) {
                    self.status = e.to_string();
                    return;
                }
            }
            self.execute(Command::VideoComposition {
                project: doc.video.unwrap(),
                annotations: doc.annotations,
            });
            return;
        }
        if self.selected.is_some() {
            self.status = "Move the playhead inside the selected object before splitting.".into();
            return;
        }
        match v.split(at) {
            Ok(()) => self.commit_video(v),
            Err(e) => self.status = e.to_string(),
        }
    }
    fn video_rename_dialog(&mut self, ctx: &egui::Context) {
        let Some((id, mut name, first)) = self.video.rename.take() else {
            return;
        };
        let mut result = (false, false);
        let modal = crate::controls::Modal::new(egui::Id::unique("video-rename")).show(ctx, |ui| {
            ui.set_width(320.);
            ui.heading("Rename object");
            ui.add_space(10.);
            ui.label("Name");
            let r = ui.add(
                egui::TextEdit::singleline(&mut name)
                    .desired_width(f32::INFINITY)
                    .char_limit(120),
            );
            if first {
                r.request_focus();
            }
            result = crate::controls::dialog_actions(
                ui,
                "Save",
                "Cancel",
                !name.trim().is_empty(),
                false,
            );
            result.1 |= ui.input(|i| i.key_pressed(egui::Key::Escape));
        });
        result.1 |= modal.should_close();
        if result.0 {
            let mut v = self.session.document.video.clone().unwrap();
            if let Some(t) = v.objects.get_mut(&id) {
                t.label = name.trim().into();
                self.commit_video(v);
            }
        } else if !result.1 {
            self.video.rename = Some((id, name, false));
        }
    }
    pub(crate) fn set_timeline_expanded(&mut self, expanded: bool) {
        self.video.collapsed = !expanded;
        if expanded {
            self.history.recent = false;
        }
    }
    pub(crate) fn show_video_timeline(&mut self, ui: &mut egui::Ui) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.drag = None;
            self.video.record_drag = None;
            self.video.volume_drag = None;
            self.video.timing_preview = None;
        }
        if !self.is_video() || self.shell.library || self.shell.preview_ui {
            return;
        }
        self.video_rename_dialog(ui.ctx());
        self.confirm_delete_track(ui.ctx());
        // Keep the expanded height and use this frame's dock geometry during a toggle.
        let collapsed = self.video.collapsed;
        let transport_overflow = ui.available_width() < 800.;
        let compact_height = if transport_overflow { 58. } else { 44. };
        let panel = egui::Panel::bottom(if collapsed {
            "video-timeline-collapsed"
        } else {
            "video-timeline"
        })
        .resizable(!collapsed)
        .default_size(if collapsed { compact_height } else { 260. })
        .min_size(if collapsed { compact_height } else { 165. })
        .max_size(if collapsed { compact_height } else { 440. })
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(31, 35, 37))
                .inner_margin(8),
        )
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("video-transport-controls")
                .auto_shrink([false, true])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let toggle = super::ui::button(
                            ui,
                            if self.video.collapsed { "Up" } else { "Down" },
                            "Timeline",
                            100.,
                            false,
                        )
                        .tip(if self.video.collapsed {
                            "Expand Timeline and hide Recent Clips"
                        } else {
                            "Collapse Timeline for a larger video preview"
                        });
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::unique("timeline-toggle"), toggle.rect)
                        });
                        if toggle.clicked() {
                            self.set_timeline_expanded(self.video.collapsed);
                        }
                        if super::ui::button(
                            ui,
                            if self.video.playing { "Pause" } else { "Play" },
                            "",
                            28.,
                            self.video.playing,
                        )
                        .tip("Space: play/pause · twice: clip start · three times: project start")
                        .clicked()
                        {
                            self.video_play();
                        }
                        if super::ui::button(ui, "Previous", "", 28., false)
                            .tip("Previous frame")
                            .clicked()
                        {
                            self.video_step(false);
                        }
                        if super::ui::button(ui, "Next", "", 28., false)
                            .tip("Next frame")
                            .clicked()
                        {
                            self.video_step(true);
                        }
                        let v = self.session.document.video.as_ref().unwrap();
                        ui.monospace(format!(
                            "{} / {}",
                            time_label(v.cursor),
                            time_label(v.duration())
                        ));
                        if super::ui::button(ui, "Split", "Split", 72., false).clicked() {
                            self.video_split()
                        }
                        egui::Popup::menu(&super::ui::button(ui, "Trim", "Trim", 72., false)).show(
                            |ui| {
                                let v = self.session.document.video.as_ref().unwrap();
                                let at = v.cursor;
                                let end = v.duration();
                                if ui
                                    .add_enabled(
                                        at > 0,
                                        egui::Button::new("Remove before playhead"),
                                    )
                                    .clicked()
                                {
                                    self.execute(Command::VideoRemove { start: 0, end: at });
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(
                                        at + 1 < end,
                                        egui::Button::new("Remove after playhead"),
                                    )
                                    .clicked()
                                {
                                    self.execute(Command::VideoRemove { start: at + 1, end });
                                    ui.close();
                                }
                            },
                        );
                        let can_jump = !self.jump_targets().is_empty();
                        let jump = ui
                            .add_enabled_ui(can_jump, |ui| {
                                super::ui::button(
                                    ui,
                                    "Jump to Object",
                                    "Jump to Object",
                                    128.,
                                    false,
                                )
                            })
                            .inner
                            .tip("Scroll the timeline to the selected object's clip (F)")
                            .tip_disabled("Select an object on the canvas first");
                        #[cfg(test)]
                        ui.ctx().data_mut(|d| {
                            d.insert_temp(egui::Id::unique("jump-to-object"), jump.rect)
                        });
                        if jump.clicked() {
                            self.jump_to_object();
                        }
                        if super::ui::button(ui, "Add Track", "Import…", 82., false)
                            .tip("Import audio, image or video at the playhead")
                            .clicked()
                        {
                            let v = self.session.document.video.as_ref().unwrap();
                            self.pick_timeline_media(v.cursor, *v.media_tracks.last().unwrap());
                        }
                        self.keyframe_toggle(ui);
                        self.hide_tracks_menu(ui);
                        ui.add_space((ui.available_width() - 164.).max(0.));
                        ui.label("Timeline");
                        if super::ui::button(ui, "Minus", "", 28., false)
                            .tip("Zoom timeline out")
                            .clicked()
                        {
                            self.video.zoom = (self.video.zoom / 1.25).max(0.5);
                        }
                        ui.label(format!("{:.1}×", self.video.zoom));
                        if super::ui::button(ui, "Plus", "", 28., false)
                            .tip("Zoom timeline in")
                            .clicked()
                        {
                            self.video.zoom = (self.video.zoom * 1.25).min(8.);
                        }
                    });
                });
            if collapsed || self.video.collapsed {
                return;
            }
            ui.add_space(6.);
            let mut v = self
                .video
                .timing_preview
                .clone()
                .unwrap_or_else(|| self.session.document.video.clone().unwrap());
            v.cursor = self.video_time();
            let old_zoom = self.video.zoom;
            if ui.rect_contains_pointer(ui.max_rect()) {
                let zoom = (self.gesture_wheel(ui, crate::gesture_input::Gesture::TimelineZoom)
                    * 0.0025)
                    .exp();
                let zoom = zoom
                    * ui.input(|i| {
                        i.events
                            .iter()
                            .filter_map(|e| {
                                if let egui::Event::Zoom(z) = e {
                                    Some(*z)
                                } else {
                                    None
                                }
                            })
                            .product::<f32>()
                    });
                if zoom != 1. {
                    self.video.zoom = (self.video.zoom * zoom).clamp(0.5, 8.);
                }
            }
            let available = ui.available_width();
            let label_width = 210.;
            // Keep time scale independent of content length, including while dragging.
            if self.video.timeline_base == 0 {
                self.video.timeline_base = v.duration().max(10_000);
            }
            let base_width = ((available - label_width - 16.) * self.video.zoom).max(400.);
            let scale = base_width / self.video.timeline_base as f32;
            let visible_end =
                ((self.video.timeline_offset.x + available * 2.) / scale).max(0.) as u64;
            self.video.timeline_extent = self
                .video
                .timeline_extent
                .max(v.duration().saturating_add(30_000))
                .max(v.cursor.saturating_add(30_000))
                .max(visible_end);
            let duration = self.video.timeline_extent.min(14_400_000);
            let width = duration as f32 * scale;
            let old_width = ((available - label_width - 16.) * old_zoom).max(400.)
                / self.video.timeline_base as f32
                * duration as f32;
            let mut scroll = egui::ScrollArea::both().scroll_source(
                egui::scroll_area::ScrollSource::SCROLL_BAR
                    | egui::scroll_area::ScrollSource::MOUSE_WHEEL,
            );
            let middle = egui::PointerButton::Middle;
            if ui.input(|i| {
                i.pointer.button_pressed(middle)
                    && i.pointer
                        .hover_pos()
                        .is_some_and(|p| ui.max_rect().contains(p))
            }) {
                self.video.timeline_panning = true;
            }
            if self.video.timeline_panning {
                let delta = ui.input(|i| i.pointer.delta());
                scroll = scroll
                    .scroll_offset((self.video.timeline_offset - delta).max(egui::Vec2::ZERO));
                ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                if !ui.input(|i| i.pointer.button_down(middle)) {
                    self.video.timeline_panning = false;
                }
            }
            if self.video.zoom != old_zoom {
                let local = ui
                    .input(|i| i.pointer.hover_pos())
                    .map_or(0., |p| (p.x - ui.max_rect().left() - label_width).max(0.));
                let anchor = (self.video.timeline_offset.x + local) / old_width;
                scroll = scroll.horizontal_scroll_offset((anchor * width - local).max(0.));
            }
            // Keep the playhead in view; scrolling by hand wins for a moment.
            let moved = self.video.follow_seen != v.cursor;
            self.video.follow_seen = v.cursor;
            if ui.input(|i| {
                i.smooth_scroll_delta != egui::Vec2::ZERO
                    && i.pointer
                        .hover_pos()
                        .is_some_and(|p| ui.max_rect().contains(p))
            }) {
                self.video.follow_held = Some(Instant::now());
            }
            let held = self.video.timeline_panning
                || self
                    .video
                    .follow_held
                    .is_some_and(|t| t.elapsed().as_secs_f32() < 2.)
                || (!self.video.playing && ui.input(|i| i.pointer.primary_down()));
            if self.video.follow_playhead && moved && !held && self.video.zoom == old_zoom {
                let view = (available - label_width - 16.).max(1.);
                let x = v.cursor as f32 * scale;
                let offset = self.video.timeline_offset.x;
                if x < offset || x > offset + view - 24. {
                    scroll = scroll.horizontal_scroll_offset((x - view * 0.1).max(0.));
                }
            }
            let hidden: Vec<bool> = v
                .layers()
                .into_iter()
                .map(|layer| {
                    self.video.hidden_tracks.contains(&super::track_kind(
                        &self.session.document,
                        &v,
                        layer,
                    ))
                })
                .collect();
            let hidden_count = hidden.iter().filter(|h| **h).count();
            super::keyframes::set_hidden_rows(hidden);
            self.keyframe_rows(&v);
            let key_rows = self.video.keyframes.rows;
            self.expire_jump_flash();
            let view = egui::vec2(
                (available - label_width - 16.).max(1.),
                (ui.available_height() - 36. - 16.).max(1.),
            );
            let scroll = self.jump_scroll(&v, scroll, scale, view, key_rows);
            let scroll_output = scroll
                .auto_shrink([false, false])
                .id_salt("video-tracks")
                .show(ui, |ui| {
                    let rows = v.layers().len() - hidden_count
                        + if ui.input(|i| !i.raw.hovered_files.is_empty()) {
                            self.video.drop_clips.len().max(1)
                        } else {
                            0
                        };
                    let (whole, _) = ui.allocate_exact_size(
                        egui::vec2(
                            label_width + width,
                            36. + rows as f32 * 39. + key_rows.map_or(0., |(_, n)| n as f32 * 25.),
                        ),
                        Sense::hover(),
                    );
                    let left = whole.left() + label_width;
                    let x = |ms: u64| left + ms as f32 * scale;
                    let y = |row: usize| super::keyframes::row_y(whole.top() + 36., row, key_rows);
                    let outer_clip = ui.clip_rect();
                    let pinned_left = outer_clip.left().max(whole.left());
                    let pinned_top = outer_clip.top().max(whole.top());
                    let content_clip = Rect::from_min_max(
                        egui::pos2(pinned_left + label_width, pinned_top + 36.),
                        outer_clip.max,
                    );
                    ui.set_clip_rect(content_clip);
                    let background = ui.interact(
                        Rect::from_min_max(egui::pos2(left, y(0)), whole.max),
                        egui::Id::unique("timeline-marquee"),
                        Sense::click_and_drag(),
                    );
                    if let Some(active) = self.video.active_track {
                        let row = v.layer_index(active);
                        ui.painter().rect_filled(
                            Rect::from_min_size(egui::pos2(left, y(row)), egui::vec2(width, 36.)),
                            0.,
                            Color32::from_rgba_unmultiplied(27, 143, 182, 24),
                        );
                    }
                    let mut clicked = None;
                    let mut menu_action = None;

                    for track in &v.tracks {
                        let row = v.layer_index(Layer::Annotation(*track));
                        if super::keyframes::row_hidden(row) {
                            continue;
                        }
                        for (id, t) in v.objects.iter().filter(|(_, t)| t.track == *track) {
                            let Some(a) = self
                                .session
                                .document
                                .annotations
                                .iter()
                                .find(|a| a.id == *id)
                                .cloned()
                            else {
                                continue;
                            };
                            let rect = Rect::from_min_max(
                                egui::pos2(x(t.start), y(row)),
                                egui::pos2(x(t.end).max(x(t.start) + 4.), y(row) + 33.),
                            );
                            let name = if !t.label.is_empty() {
                                t.label.clone()
                            } else {
                                a.text
                                    .as_ref()
                                    .map(|s| s.content.chars().take(28).collect())
                                    .unwrap_or_else(|| {
                                        if a.focus.is_some() {
                                            "Focus".into()
                                        } else {
                                            format!("{:?}", a.kind)
                                        }
                                    })
                            };
                            let state = self.session.document.object_state(*id);
                            let r = block(
                                ui,
                                rect,
                                egui::Id::unique(("video-object", id)),
                                &name,
                                if state.hidden {
                                    Color32::from_gray(45)
                                } else {
                                    Color32::from_rgb(40, 85, 105)
                                },
                                self.selected == Some(*id),
                            );
                            self.paint_review_mark(ui, &v, *id, rect, scale);
                            self.paint_jump_flash(ui, *id, rect);
                            let keys = self.keyframe_marks(
                                ui,
                                &v,
                                capture_core::video::editing::Item::Annotation(*id),
                                rect,
                                scale,
                                None,
                            );
                            if self.path_marks(ui, &v, *id, rect, scale) || keys {
                                continue;
                            }
                            if r.clicked() {
                                clicked = Some(*id);
                            }
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::unique(("timeline-block", id)), rect)
                            });
                            if r.secondary_clicked() {
                                clicked = Some(*id);
                            }
                            r.context_menu(|ui| {
                                self.timeline_link_menu(ui);
                                if let Some(action) =
                                    super::object_menu::show(ui, &v, *id, state, &name)
                                {
                                    menu_action = Some((*id, action));
                                }
                            });

                            if capture_core::video::editor::Animation::Fade.supports(&a) {
                                for (at, enter) in [
                                    (
                                        t.start
                                            + if t.entrance
                                                == capture_core::video::editor::Animation::None
                                            {
                                                0
                                            } else {
                                                t.entrance_ms
                                            },
                                        true,
                                    ),
                                    (
                                        t.end
                                            - if t.exit
                                                == capture_core::video::editor::Animation::None
                                            {
                                                0
                                            } else {
                                                t.exit_ms
                                            },
                                        false,
                                    ),
                                ] {
                                    fade_handle(ui, rect, x(at), enter);
                                }
                            }
                            if r.hovered() {
                                let edge = ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
                                    (p.x - rect.left()).abs() < 8.
                                        || (p.x - rect.right()).abs() < 8.
                                });
                                ui.ctx().set_cursor_icon(if edge {
                                    egui::CursorIcon::ResizeHorizontal
                                } else {
                                    egui::CursorIcon::Grab
                                });
                            }
                            if r.hovered()
                                && capture_core::video::editor::Animation::Fade.supports(&a)
                                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                            {
                                let start = x(t.start
                                    + if t.entrance == capture_core::video::editor::Animation::None
                                    {
                                        0
                                    } else {
                                        t.entrance_ms
                                    });
                                let end = x(t.end
                                    - if t.exit == capture_core::video::editor::Animation::None {
                                        0
                                    } else {
                                        t.exit_ms
                                    });
                                if pos.y < rect.top() + 12.
                                    && ((pos.x - start).abs() < 12. || (pos.x - end).abs() < 12.)
                                {
                                    fade_cursor(ui, (pos.x - end).abs() < (pos.x - start).abs());
                                }
                            }
                            if r.drag_started_by(egui::PointerButton::Primary)
                                && !state.locked
                                && let Some(pos) = ui.input(|i| i.pointer.press_origin())
                            {
                                self.video.pause();
                                clicked = Some(*id);
                                let duplicate = ui.input(|i| {
                                    self.gesture_matches(
                                        crate::gesture_input::Gesture::DuplicateDrag,
                                        i.modifiers,
                                    )
                                });
                                let edge = if duplicate {
                                    0
                                } else if (pos.x
                                    - x(t.start
                                        + if t.entrance
                                            == capture_core::video::editor::Animation::None
                                        {
                                            0
                                        } else {
                                            t.entrance_ms
                                        }))
                                .abs()
                                    < 12.
                                    && pos.y < rect.top() + 10.
                                    && capture_core::video::editor::Animation::Fade.supports(&a)
                                {
                                    -2
                                } else if (pos.x
                                    - x(t.end
                                        - if t.exit == capture_core::video::editor::Animation::None
                                        {
                                            0
                                        } else {
                                            t.exit_ms
                                        }))
                                .abs()
                                    < 12.
                                    && pos.y < rect.top() + 10.
                                    && capture_core::video::editor::Animation::Fade.supports(&a)
                                {
                                    2
                                } else if (pos.x - rect.left()).abs() < 8. {
                                    -1
                                } else if (pos.x - rect.right()).abs() < 8. {
                                    1
                                } else {
                                    0
                                };
                                let item = capture_core::video::editing::Item::Annotation(*id);
                                if duplicate
                                    && self.video.selection.len() > 1
                                    && self.video.selection.contains(&item)
                                {
                                    self.begin_timeline_duplicate(item, pos);
                                } else if edge == 0 && !duplicate {
                                    self.begin_timeline_move(
                                        capture_core::video::editing::Item::Annotation(*id),
                                        pos,
                                    );
                                } else {
                                    self.video.drag = Some(Drag {
                                        id: *id,
                                        edge,
                                        original: self.session.document.video.clone().unwrap(),
                                        origin: pos,
                                        target: *track,
                                        duplicate,
                                        copy_id: self
                                            .session
                                            .document
                                            .annotations
                                            .iter()
                                            .map(|a| a.id)
                                            .max()
                                            .unwrap_or(0)
                                            .saturating_add(1),
                                    });
                                }
                            }
                        }
                    }
                    if let Some(id) = clicked {
                        self.timeline_focus(
                            capture_core::video::editing::Item::Annotation(id),
                            ui.input(|i| i.modifiers.ctrl || i.modifiers.shift),
                        );
                    }
                    if let Some((id, action)) = menu_action {
                        self.video_object_action(id, action);
                    }
                    self.video_media_rows(ui, &v, whole, left, scale, y(0));
                    self.zoom_timeline(ui, &v, left, y(0), scale);
                    self.timeline_selection_input(ui, &v, background, left, y(0), scale);
                    let px = x(v.cursor);
                    ui.painter().line_segment(
                        [
                            egui::pos2(px, whole.top() + 16.),
                            egui::pos2(px, whole.bottom()),
                        ],
                        Stroke::new(1.5, Color32::from_rgb(244, 84, 94)),
                    );
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(px - 5., whole.top() + 15.),
                            egui::pos2(px + 5., whole.top() + 15.),
                            egui::pos2(px, whole.top() + 21.),
                        ],
                        Color32::from_rgb(244, 84, 94),
                        Stroke::NONE,
                    ));
                    if let Some(drag) = self.video.drag.as_mut()
                        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
                    {
                        let old = &drag.original.objects[&drag.id];
                        let dt = if drag.edge == 0 && ui.input(|i| i.modifiers.shift) {
                            0
                        } else {
                            ((pos.x - drag.origin.x) / scale).round() as i64
                        };
                        let mut t = old.clone();
                        let frame = (1000 / v.fps as u64).max(1);
                        let snap = |time: u64| {
                            if self.video.snap {
                                drag.original.snap_time(
                                    time,
                                    &std::collections::BTreeSet::from([
                                        capture_core::video::editing::Item::Annotation(drag.id),
                                    ]),
                                    (8. / scale).round() as u64,
                                )
                            } else {
                                time
                            }
                        };
                        if drag.edge == -2 {
                            if t.entrance == capture_core::video::editor::Animation::None {
                                t.entrance = capture_core::video::editor::Animation::Fade;
                            }
                            t.entrance_ms = (if old.entrance
                                == capture_core::video::editor::Animation::None
                            {
                                0
                            } else {
                                old.entrance_ms
                            })
                            .saturating_add_signed(dt)
                            .min((t.end - t.start).saturating_sub(
                                if t.exit == capture_core::video::editor::Animation::None {
                                    0
                                } else {
                                    t.exit_ms
                                },
                            ));
                        } else if drag.edge == 2 {
                            if t.exit == capture_core::video::editor::Animation::None {
                                t.exit = capture_core::video::editor::Animation::Fade;
                            }
                            t.exit_ms =
                                (if old.exit == capture_core::video::editor::Animation::None {
                                    0
                                } else {
                                    old.exit_ms
                                })
                                .saturating_add_signed(-dt)
                                .min(
                                    (t.end - t.start).saturating_sub(
                                        if t.entrance
                                            == capture_core::video::editor::Animation::None
                                        {
                                            0
                                        } else {
                                            t.entrance_ms
                                        },
                                    ),
                                );
                        } else if drag.edge < 0 {
                            t.start = snap(old.start.saturating_add_signed(dt))
                                .min(t.end.saturating_sub(frame));
                            t.keyframes.trim_start(t.start as i64 - old.start as i64);
                            if let Some(path) = &mut t.motion_path {
                                std::sync::Arc::make_mut(path).offset_ms = old
                                    .motion_path
                                    .as_ref()
                                    .unwrap()
                                    .offset_ms
                                    .saturating_add_signed(t.start as i64 - old.start as i64);
                            }
                        } else if drag.edge > 0 {
                            t.end = snap(old.end.saturating_add_signed(dt))
                                .clamp((t.start + frame).min(duration), duration);
                        } else {
                            let len = t.end - t.start;
                            t.start = snap(old.start.saturating_add_signed(dt)).min(duration - len);
                            t.end = t.start + len;
                            let row = super::keyframes::row_at(y(0), pos.y, key_rows);
                            if let Some(Layer::Annotation(track)) = v.layers().get(row) {
                                t.track = *track;
                            }
                        }
                        let mut preview = drag.original.clone();
                        let preview_id = if drag.duplicate {
                            drag.copy_id
                        } else {
                            drag.id
                        };
                        if !drag.duplicate && drag.edge.abs() == 1 {
                            for (id, other) in &drag.original.objects {
                                if *id != drag.id && other.track == t.track {
                                    if drag.edge < 0 && other.end <= old.start {
                                        t.start = t.start.max(other.end);
                                    }
                                    if drag.edge > 0 && other.start >= old.end {
                                        t.end = t.end.min(other.start);
                                    }
                                }
                            }
                            t.clamp_animations();
                            preview.objects.insert(preview_id, t);
                        } else {
                            preview.place(preview_id, t);
                        }
                        drag.target = preview.objects[&preview_id].track;
                        if drag.duplicate {
                            let t = &preview.objects[&preview_id];
                            let row = preview.layer_index(Layer::Annotation(t.track));
                            let rect = Rect::from_min_max(
                                egui::pos2(x(t.start), y(row)),
                                egui::pos2(x(t.end), y(row) + 33.),
                            );
                            ui.painter().rect_filled(
                                rect,
                                4.,
                                Color32::from_rgba_unmultiplied(25, 154, 195, 150),
                            );
                            ui.painter().rect_stroke(
                                rect,
                                4.,
                                Stroke::new(2., crate::shell::ACCENT),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                "+ Copy",
                                FontId::proportional(12.),
                                Color32::WHITE,
                            );
                            ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
                        }
                        self.video.timing_preview = Some(preview);
                        if ui.input(|i| i.pointer.any_released()) {
                            let project = self.video.timing_preview.take().unwrap();
                            let duplicate = drag.duplicate;
                            let id = drag.id;
                            let copy_id = drag.copy_id;
                            self.video.drag = None;
                            if duplicate {
                                let before = self.session.document.annotations.len();
                                self.execute(Command::VideoDuplicate {
                                    id,
                                    timing: project.objects.get(&copy_id).cloned(),
                                });
                                if self.session.document.annotations.len() > before {
                                    self.select_only(Some(copy_id));
                                }
                            } else {
                                self.commit_video(project);
                            }
                        }
                    }
                    if let Some((row, _)) = key_rows {
                        self.keyframe_lanes(ui, &v, left, y(row) + 39., width, scale);
                    }
                    ui.set_clip_rect(outer_clip);
                    self.track_headers(
                        ui,
                        &v,
                        Rect::from_min_max(egui::pos2(pinned_left, pinned_top), outer_clip.max),
                        y(0),
                        label_width,
                    );
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            egui::pos2(pinned_left, pinned_top),
                            egui::pos2(outer_clip.right(), pinned_top + 36.),
                        ),
                        0.,
                        Color32::from_rgb(31, 35, 37),
                    );
                    ui.scope_builder(
                        egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            egui::pos2(pinned_left, pinned_top),
                            egui::vec2(label_width - 8., 28.),
                        )),
                        |ui| {
                            let button = super::ui::button(
                                ui,
                                "Add Track",
                                "Add Track",
                                label_width - 12.,
                                false,
                            );
                            #[cfg(test)]
                            ui.ctx().data_mut(|d| {
                                d.insert_temp(egui::Id::unique("timeline-add-track"), button.rect)
                            });
                            egui::Popup::menu(&button).show(|ui| self.add_track_menu(ui));
                        },
                    );
                    ui.set_clip_rect(Rect::from_min_max(
                        egui::pos2(pinned_left + label_width, pinned_top),
                        egui::pos2(outer_clip.right(), pinned_top + 36.),
                    ));
                    let ruler =
                        Rect::from_min_size(egui::pos2(left, pinned_top), egui::vec2(width, 26.));
                    let response = ui.interact(
                        ruler,
                        egui::Id::unique("video-ruler"),
                        Sense::click_and_drag(),
                    );
                    let export_x = x(v.duration());
                    ui.painter().line_segment(
                        [
                            egui::pos2(export_x, pinned_top),
                            egui::pos2(export_x, pinned_top + 34.),
                        ],
                        Stroke::new(2., crate::shell::ACCENT),
                    );
                    ui.painter().text(
                        egui::pos2(export_x - 5., pinned_top + 20.),
                        Align2::RIGHT_TOP,
                        "Project End",
                        FontId::proportional(11.),
                        crate::shell::ACCENT,
                    );
                    #[cfg(test)]
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::unique("timeline-export-end"),
                            (v.duration(), export_x),
                        )
                    });
                    let tick = (duration as f32 / (width / 90.).max(1.) / 1000.)
                        .ceil()
                        .max(1.) as u64
                        * 1000;
                    let first_tick =
                        (((outer_clip.left() - left) / scale).max(0.) as u64 / tick) * tick;
                    let last_tick =
                        (((outer_clip.right() - left) / scale).max(0.) as u64 + tick).min(duration);
                    for at in (first_tick..=last_tick).step_by(tick as usize) {
                        let p = egui::pos2(x(at), pinned_top);
                        ui.painter().text(
                            p,
                            Align2::LEFT_TOP,
                            format!("{}s", at / 1000),
                            FontId::proportional(10.),
                            Color32::from_gray(160),
                        );
                        ui.painter().line_segment(
                            [p + egui::vec2(0., 20.), egui::pos2(p.x, whole.bottom())],
                            Stroke::new(1., Color32::from_gray(49)),
                        );
                    }
                    if (response.clicked() || response.dragged_by(egui::PointerButton::Primary))
                        && let Some(p) = response.interact_pointer_pos()
                    {
                        self.video_seek(((p.x - left) / scale).max(0.) as u64);
                    }
                    let head = egui::pos2(x(v.cursor), pinned_top + 25.);
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![
                            head,
                            head + egui::vec2(-5., -7.),
                            head + egui::vec2(5., -7.),
                        ],
                        crate::shell::ACCENT,
                        Stroke::NONE,
                    ));
                    ui.set_clip_rect(outer_clip);
                    let edge = self
                        .video
                        .drag
                        .as_ref()
                        .map(|d| d.edge)
                        .or_else(|| self.video.record_drag.as_ref().map(|d| d.edge))
                        .or_else(|| self.video.zoom_drag.as_ref().map(|d| d.edge));
                    if let Some(e) = edge {
                        if e.abs() == 2 {
                            fade_cursor(ui, e > 0);
                        } else {
                            ui.ctx().set_cursor_icon(if e == 0 {
                                egui::CursorIcon::Grabbing
                            } else {
                                egui::CursorIcon::ResizeHorizontal
                            });
                        }
                    }
                });
            self.video.timeline_offset = scroll_output.state.clamped_offset();
        });
        if self.video.timeline_panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
        }
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("timeline-panel"), panel.response.rect));
        #[cfg(not(test))]
        let _ = panel;
    }
}
