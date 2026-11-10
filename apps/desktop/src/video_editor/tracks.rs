use super::*;
use crate::tips::Tip as _;
use capture_core::video::links::Layer;
use egui::{Color32, Rect, Sense, Stroke};

impl Editor {
    pub(super) fn track_headers(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        viewport: Rect,
        top: f32,
        width: f32,
    ) {
        let mut reorder = None;
        let mut change = None;
        let clip = Rect::from_min_max(
            viewport.min + egui::vec2(0., 36.),
            egui::pos2(viewport.left() + width, viewport.bottom()),
        );
        let painter = ui.painter().with_clip_rect(clip);
        painter.rect_filled(clip, 0., Color32::from_rgb(31, 35, 37));
        for (row, layer) in p.layers().into_iter().enumerate() {
            let rect = Rect::from_min_size(
                egui::pos2(
                    viewport.left(),
                    super::keyframes::row_y(top, row, self.video.keyframes.rows),
                ),
                egui::vec2(width - 4., 34.),
            );
            if !rect.intersects(clip) || super::keyframes::row_hidden(row) {
                continue;
            }
            let label = match layer {
                Layer::Zoom(t) => format!(
                    "Zooms {}{}",
                    p.layers()
                        .iter()
                        .filter(|l| matches!(l, Layer::Zoom(_)))
                        .position(|l| *l == Layer::Zoom(t))
                        .unwrap_or(0)
                        + 1,
                    if p.zooms.iter().any(|z| z.track() == t && z.pinned) {
                        " · pinned"
                    } else {
                        ""
                    }
                ),
                Layer::Annotation(id) => format!(
                    "Annotations {}",
                    p.tracks.iter().position(|t| *t == id).unwrap_or(0) + 1
                ),
                Layer::Media(id) => p.track_label(id),
            };
            let label = p.custom_track_name(layer).unwrap_or(&label).to_owned();
            let audio = matches!(layer,Layer::Media(id) if matches!(p.track_kind(id),capture_core::video::editor::TrackKind::Audio|capture_core::video::editor::TrackKind::Voiceover));
            let has_audio = matches!(layer,Layer::Media(id) if p.segments.iter().any(|s|s.track==id&&!s.audio.is_empty()) || audio);
            let controls = if has_audio && !audio {
                72.
            } else if has_audio {
                48.
            } else {
                24.
            };
            let glyph = match layer {
                Layer::Annotation(_) => "Layers",
                Layer::Zoom(_) => "Zoom",
                Layer::Media(id) => match p.track_kind(id) {
                    capture_core::video::editor::TrackKind::Voiceover => "Microphone",
                    capture_core::video::editor::TrackKind::Audio => "Audio",
                    _ => "Recording",
                },
            };
            ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                ui.set_clip_rect(clip);
                if self.video.active_track == Some(layer) {
                    ui.painter()
                        .rect_filled(rect, 4., Color32::from_rgb(40, 63, 73));
                }
                let grip = Rect::from_min_size(rect.min, egui::vec2(22., 34.));
                let r = ui
                    .interact(grip, egui::Id::unique(("track-grip", layer)), Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::Grab)
                    .tip("Drag to reorder track");
                for x in [8., 13.] {
                    for y in [11., 17., 23.] {
                        ui.painter().circle_filled(
                            rect.min + egui::vec2(x, y),
                            1.,
                            Color32::LIGHT_GRAY,
                        );
                    }
                }
                if r.drag_started_by(egui::PointerButton::Primary) {
                    self.video.track_drag = Some(layer);
                }
                let header = ui.interact(
                    Rect::from_min_max(
                        rect.min + egui::vec2(24., 0.),
                        rect.max - egui::vec2(controls, 0.),
                    ),
                    egui::Id::unique(("track-header", layer)),
                    Sense::click(),
                );
                super::ui::icon(
                    ui.painter(),
                    Rect::from_center_size(
                        rect.left_center() + egui::vec2(33., 0.),
                        egui::vec2(15., 15.),
                    ),
                    glyph,
                    Color32::LIGHT_GRAY,
                );
                ui.painter()
                    .with_clip_rect(header.rect.intersect(clip))
                    .text(
                        rect.left_center() + egui::vec2(46., 0.),
                        egui::Align2::LEFT_CENTER,
                        &label,
                        egui::FontId::proportional(12.),
                        Color32::LIGHT_GRAY,
                    );
                if header.clicked() {
                    self.video.active_track = Some(layer);
                    if let Layer::Annotation(id) = layer
                        && let Some(v) = &mut self.session.document.video
                    {
                        v.annotation_track = Some(id);
                    }
                }
                if header.double_clicked()
                    && has_audio
                    && let Layer::Media(id) = layer
                    && let Some(index) = p
                        .segments
                        .iter()
                        .position(|c| c.track == id && !c.audio.is_empty())
                {
                    self.video.selected = if audio {
                        Selection::Audio(index)
                    } else {
                        Selection::Recording(index)
                    };
                    self.video.fx.track_scope = true;
                    self.open_effects_workstation();
                }
                header.context_menu(|ui| {
                    if crate::controls::menu_action(ui, "Rename track…").clicked() {
                        self.video.track_rename = Some((layer, label.clone(), true));
                        ui.close();
                    }
                    for (title, delta) in [
                        ("Move up", -1),
                        ("Move down", 1),
                        ("Send to top", -10000),
                        ("Send to bottom", 10000),
                    ] {
                        if crate::controls::menu_action(ui, title).clicked() {
                            let mut next = p.clone();
                            next.move_layer(layer, delta);
                            change = Some(next);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if let Layer::Media(id) = layer {
                        if crate::controls::menu_action(ui, "Import media here…").clicked() {
                            self.pick_timeline_media(p.cursor, id);
                            ui.close();
                        }
                        if p.track_kind(id) == capture_core::video::editor::TrackKind::Voiceover
                            && crate::controls::menu_action(ui, "Record narration here…").clicked()
                        {
                            self.open_voiceover();
                            self.video.voiceover.target = Some(id);
                            ui.close();
                        }
                    }
                    if crate::controls::menu_action(ui, "Delete track…").clicked() {
                        self.video.delete_track = Some(layer);
                        ui.close();
                    }
                });
                let mut right = rect.right() - 12.;
                if !audio {
                    let hidden = p.layer_hidden(layer);
                    let r = Rect::from_center_size(
                        egui::pos2(right, rect.center().y),
                        egui::vec2(22., 26.),
                    );
                    super::ui::icon(
                        ui.painter(),
                        r.shrink(3.),
                        if hidden {
                            "Show annotations"
                        } else {
                            "Hide annotations"
                        },
                        if hidden {
                            Color32::GRAY
                        } else {
                            Color32::LIGHT_GRAY
                        },
                    );
                    if ui
                        .interact(
                            r,
                            egui::Id::unique(("track-visibility", layer)),
                            Sense::click(),
                        )
                        .tip(if hidden { "Show track" } else { "Hide track" })
                        .clicked()
                    {
                        let mut next = p.clone();
                        next.track_settings_mut(layer).hidden = !hidden;
                        change = Some(next);
                    }
                    right -= 24.;
                }
                if let Layer::Media(id) = layer
                    && has_audio
                {
                    let muted = p.track_muted(id);
                    let r = Rect::from_center_size(
                        egui::pos2(right, rect.center().y),
                        egui::vec2(22., 26.),
                    );
                    super::ui::icon(
                        ui.painter(),
                        r.shrink(3.),
                        if muted { "Muted" } else { "Audio" },
                        if muted {
                            crate::shell::ACCENT
                        } else {
                            Color32::LIGHT_GRAY
                        },
                    );
                    if ui
                        .interact(r, egui::Id::unique(("track-mute", id)), Sense::click())
                        .tip(if muted { "Unmute track" } else { "Mute track" })
                        .clicked()
                    {
                        let mut next = p.clone();
                        next.track_settings_mut(layer).muted = !muted;
                        change = Some(next);
                    }
                    right -= 24.;
                }
                if let Layer::Media(id) = layer
                    && p.segments
                        .iter()
                        .any(|s| s.track == id && !s.audio.is_empty())
                {
                    self.volume_knob(
                        ui,
                        p,
                        id,
                        Rect::from_center_size(
                            egui::pos2(right, rect.center().y),
                            egui::vec2(22., 26.),
                        ),
                    );
                }
            });
        }
        if let Some(next) = change {
            self.execute_audio_edit(Command::Video { project: next });
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.track_drag = None;
        }
        if let Some(layer) = self.video.track_drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let row = (super::keyframes::row_at(top, pos.y, self.video.keyframes.rows))
                .min(p.layers().len().saturating_sub(1));
            let ghost = Rect::from_min_size(
                egui::pos2(viewport.left() + 4., pos.y - 15.),
                egui::vec2(width - 8., 30.),
            );
            painter.rect_filled(ghost, 4., Color32::from_rgb(45, 110, 135));
            painter.text(
                ghost.center(),
                egui::Align2::CENTER_CENTER,
                "Move track",
                egui::FontId::proportional(12.),
                Color32::WHITE,
            );
            painter.line_segment(
                [
                    egui::pos2(
                        viewport.left(),
                        super::keyframes::row_y(top, row, self.video.keyframes.rows),
                    ),
                    egui::pos2(
                        viewport.left() + width,
                        super::keyframes::row_y(top, row, self.video.keyframes.rows),
                    ),
                ],
                Stroke::new(2., crate::shell::ACCENT),
            );
            if ui.input(|i| i.pointer.any_released()) {
                if viewport.contains(pos) {
                    reorder = Some((layer, row));
                }
                self.video.track_drag = None;
            }
        }
        if let Some((layer, row)) = reorder {
            let mut next = p.clone();
            next.reorder_layer(layer, row);
            self.commit_video(next);
        }
    }
    pub(super) fn confirm_delete_track(&mut self, ctx: &egui::Context) {
        if let Some((layer, mut name, focus)) = self.video.track_rename.clone() {
            let mut action = (false, false);
            let modal = crate::controls::Modal::new(egui::Id::unique("rename-video-track")).show(
                ctx,
                |ui| {
                    ui.set_width(300.);
                    ui.heading("Rename track");
                    let r = ui.text_edit_singleline(&mut name);
                    if focus {
                        r.request_focus();
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(name.chars().count()),
                                )));
                            state.store(ui.ctx(), r.id);
                        }
                    }
                    action = crate::controls::dialog_actions(
                        ui,
                        "Save",
                        "Cancel",
                        !name.trim().is_empty(),
                        false,
                    );
                },
            );
            action.1 |= modal.should_close();
            if action.0 {
                if let Some(mut p) = self.session.document.video.clone() {
                    p.track_settings_mut(layer).name = name.trim().chars().take(80).collect();
                    self.execute_audio_edit(Command::Video { project: p });
                }
                self.video.track_rename = None;
            } else if action.1 {
                self.video.track_rename = None;
            } else {
                self.video.track_rename = Some((layer, name, false));
            }
        }
        let Some(layer) = self.video.delete_track else {
            return;
        };
        let mut action = (false, false);
        let modal = crate::controls::Modal::new(egui::Id::unique("delete-video-track")).show(ctx,|ui|{ui.set_width(320.);ui.heading("Delete track?");ui.label("The track and its clips will be removed. Original media files are kept. Undo restores the track.");action=crate::controls::dialog_actions(ui,"Delete track","Cancel",true,true);});
        action.1 |= modal.should_close();
        if action.0 {
            self.execute(Command::VideoDeleteTrack { layer });
            self.video.delete_track = None;
            self.video.active_track = None;
        } else if action.1 || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.delete_track = None;
        }
    }
}
