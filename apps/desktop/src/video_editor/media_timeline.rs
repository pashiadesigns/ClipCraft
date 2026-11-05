use super::*;
use capture_core::video::editor::MediaKind;
use capture_core::video::links::Layer;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
pub(crate) struct MediaDrag {
    original: Project,
    index: usize,
    origin: Pos2,
    scale: f32,
    pub(super) edge: i8,
    duplicate: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum VolumeTarget {
    Track(u64),
    Clip(usize, usize),
}
pub(crate) struct VolumeDrag {
    original: Project,
    target: VolumeTarget,
    start: f32,
}

impl Editor {
    fn preview_volume(&mut self, target: VolumeTarget, value: f32) {
        let mut p = self
            .video
            .volume_drag
            .as_ref()
            .map(|d| d.original.clone())
            .unwrap_or_else(|| self.session.document.video.clone().unwrap());
        match target {
            VolumeTarget::Track(id) => {
                p.media_levels.insert(id, value.clamp(0., 2.));
            }
            VolumeTarget::Clip(i, a) => {
                let Some(audio) = p.segments.get_mut(i).and_then(|c| c.audio.get_mut(a)) else {
                    return;
                };
                audio.volume = value.clamp(0., 2.);
            }
        }
        if let Some(tx) = &self.video.audio_updates {
            let _ = tx.send(p.clone());
        }
        self.video.timing_preview = Some(p);
    }
    fn start_volume(&mut self, target: VolumeTarget, start: f32) {
        if self.video.volume_drag.is_none() {
            self.video.volume_drag = Some(VolumeDrag {
                original: self.session.document.video.clone().unwrap(),
                target,
                start,
            });
        }
    }
    pub(super) fn volume_knob(&mut self, ui: &mut egui::Ui, v: &Project, track: u64, rect: Rect) {
        let value = v.media_levels.get(&track).copied().unwrap_or(1.);
        let response = ui.interact(
            rect,
            egui::Id::unique(("track-volume", track)),
            Sense::click_and_drag(),
        );
        let center = rect.center();
        let radius = 7.;
        let points = |fraction: f32| {
            (0..=32)
                .map(|i| {
                    let a = (135. + 270. * fraction * i as f32 / 32.).to_radians();
                    center + egui::vec2(a.cos(), a.sin()) * radius
                })
                .collect::<Vec<_>>()
        };
        ui.painter().add(egui::Shape::line(
            points(1.),
            Stroke::new(2.5, Color32::from_gray(75)),
        ));
        if value > 0. {
            ui.painter().add(egui::Shape::line(
                points(value / 2.),
                Stroke::new(2.5, super::property_design::volume_color(value)),
            ));
        }
        let a = (135. + 135. * value).to_radians();
        ui.painter().line_segment(
            [center, center + egui::vec2(a.cos(), a.sin()) * 4.],
            Stroke::new(1.5, Color32::WHITE),
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(("track-volume", track)), rect));
        if response.drag_started_by(egui::PointerButton::Primary) {
            self.start_volume(VolumeTarget::Track(track), value);
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(d) = &self.video.volume_drag
            && d.target == VolumeTarget::Track(track)
        {
            let value = (d.start
                + ui.input(|i| {
                    i.pointer
                        .interact_pos()
                        .zip(i.pointer.press_origin())
                        .map_or(0., |(now, start)| now.x - start.x)
                }) / 100.)
                .clamp(0., 2.);
            self.preview_volume(d.target, value);
        }
        if response.double_clicked() {
            let mut p = v.clone();
            p.media_levels.remove(&track);
            self.commit_video(p);
        }
        response
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
            .on_hover_text(format!(
                "Track volume: {:.0}%\nDrag left/right · 100% normal · double-click resets",
                value * 100.
            ));
    }
    pub(super) fn video_media_rows(
        &mut self,
        ui: &mut egui::Ui,
        v: &Project,
        whole: Rect,
        left: f32,
        scale: f32,
        top: f32,
    ) {
        let x = |time: u64| left + time as f32 * scale;
        let y = |row: usize| top + row as f32 * 39.;
        let mut changed = None;
        for track in &v.media_tracks {
            let row = v.layer_index(Layer::Media(*track));
            for (index, c) in v
                .segments
                .iter()
                .enumerate()
                .filter(|(_, c)| c.track == *track)
            {
                let rect = Rect::from_min_max(
                    egui::pos2(x(c.at), y(row)),
                    egui::pos2(x(c.until()).max(x(c.at) + 4.), y(row) + 33.),
                );
                let color = match c.kind() {
                    MediaKind::Audio => Color32::from_rgb(50, 99, 76),
                    MediaKind::Image => Color32::from_rgb(91, 72, 104),
                    MediaKind::Video => Color32::from_rgb(63, 77, 98),
                };
                let r = super::timeline::block(
                    ui,
                    rect,
                    egui::Id::unique(("media-clip", c.id)),
                    &c.label,
                    if c.hidden {
                        Color32::from_gray(45)
                    } else {
                        color
                    },
                    matches!(self.video.selected, Selection::Recording(i)|Selection::Audio(i) if v.linked_indices(i).contains(&index)),
                );
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique(("timeline-media-block", c.id)), rect)
                });
                let overview = c
                    .source
                    .as_ref()
                    .and_then(|s| self.video.media_overviews.get(&s.path));
                let thumbnails = overview.map(|o| &o.0).unwrap_or(&self.video.thumbnails);
                let wave = overview
                    .map(|o| o.1.as_slice())
                    .unwrap_or_else(|| self.video.waves.first().map_or(&[], |w| w.as_slice()));
                let paint = ui
                    .painter()
                    .with_clip_rect(rect.shrink(2.).intersect(ui.clip_rect()));
                if c.kind() != MediaKind::Audio && (c.source.is_none() || overview.is_some()) {
                    for px in (((ui.clip_rect().left() - rect.left()).max(0.) as usize / 58) * 58
                        ..(ui.clip_rect().right() - rect.left())
                            .min(rect.width())
                            .max(0.) as usize)
                        .step_by(58)
                    {
                        let time = c.source_time(c.at + (px as f32 / scale) as u64);
                        if let Some((_, thumb)) = thumbnails
                            .iter()
                            .rev()
                            .find(|(t, _)| *t <= time)
                            .or_else(|| thumbnails.first())
                        {
                            let r = Rect::from_min_size(
                                rect.left_top() + egui::vec2(px as f32, 1.),
                                egui::vec2(58., 32.),
                            );
                            paint.image(
                                thumb.id(),
                                r,
                                Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                Color32::from_white_alpha(170),
                            );
                        }
                    }
                }
                if !wave.is_empty() && !c.audio.is_empty() {
                    let duration = c.source.as_ref().map_or(v.source_duration, |s| s.duration);
                    for px in (((ui.clip_rect().left() - rect.left()).max(0.) as usize / 3) * 3
                        ..(ui.clip_rect().right() - rect.left())
                            .min(rect.width())
                            .max(0.) as usize)
                        .step_by(3)
                    {
                        let time = c.source_time(c.at + (px as f32 / scale) as u64);
                        let index = (time as usize * wave.len() / duration.max(1) as usize)
                            .min(wave.len() - 1);
                        let h = wave[index] * 10.;
                        let point = rect.left_bottom() + egui::vec2(px as f32, -8.);
                        paint.line_segment(
                            [point - egui::vec2(0., h), point + egui::vec2(0., h)],
                            Stroke::new(1., Color32::from_rgba_unmultiplied(126, 220, 169, 110)),
                        );
                    }
                }
                paint.text(
                    rect.left_center() + egui::vec2(10., 0.),
                    Align2::LEFT_CENTER,
                    &c.label,
                    FontId::proportional(11.),
                    Color32::WHITE,
                );
                if c.link.is_some() {
                    super::ui::icon(
                        ui.painter(),
                        Rect::from_min_size(
                            rect.right_top() - egui::vec2(24., -9.),
                            egui::vec2(16., 16.),
                        ),
                        "Link",
                        crate::shell::ACCENT,
                    );
                }
                #[cfg(test)]
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::unique(("media-block", c.id)), rect));
                let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                super::timeline::fade_handle(ui, rect, x(c.at + c.fade_in), true);
                super::timeline::fade_handle(ui, rect, x(c.until() - c.fade_out), false);
                for other in v.segments.iter().filter(|o| {
                    o.track == *track && (o.at, o.id) < (c.at, c.id) && o.until() > c.at
                }) {
                    let end = other.until().min(c.until());
                    let r = Rect::from_min_max(
                        egui::pos2(x(c.at), rect.top()),
                        egui::pos2(x(end), rect.bottom()),
                    );
                    painter.rect_filled(r, 0., Color32::from_rgba_unmultiplied(37, 166, 210, 65));
                    for pair in [
                        [r.left_bottom(), r.right_top()],
                        [r.left_top(), r.right_bottom()],
                    ] {
                        painter
                            .line_segment(pair, Stroke::new(1.5, Color32::from_rgb(140, 220, 240)));
                    }
                }
                if r.clicked() || r.secondary_clicked() {
                    self.timeline_focus(
                        capture_core::video::editing::Item::Media(c.id),
                        ui.input(|i| i.modifiers.ctrl || i.modifiers.shift),
                    );
                }
                if r.double_clicked() && !c.audio.is_empty() {
                    self.timeline_focus(capture_core::video::editing::Item::Media(c.id), false);
                    self.video.fx.track_scope = false;
                    self.open_effects_workstation();
                }
                r.context_menu(|ui| {
                    self.timeline_link_menu(ui);
                    for (audio_index, a) in c.audio.iter().enumerate() {
                        ui.label(&a.label);
                        let mut gain = a.volume;
                        let response = ui
                            .scope(|ui| {
                                ui.visuals_mut().selection.bg_fill =
                                    super::property_design::volume_color(gain);
                                crate::controls::slider(
                                    ui,
                                    egui::Slider::new(&mut gain, 0. ..=2.)
                                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.))
                                        .text("Volume"),
                                )
                            })
                            .inner;
                        let target = VolumeTarget::Clip(index, audio_index);
                        if response.drag_started_by(egui::PointerButton::Primary)
                            || (response.changed() && ui.input(|i| i.pointer.primary_down()))
                        {
                            self.start_volume(target, a.volume);
                        }
                        if response.changed() {
                            if self.video.volume_drag.is_some() {
                                self.preview_volume(target, gain);
                            } else {
                                let mut p = v.clone();
                                p.segments[index].audio[audio_index].volume = gain;
                                changed = Some(p);
                            }
                        }
                    }
                    ui.separator();
                    if c.kind() != MediaKind::Audio
                        && !c.audio.is_empty()
                        && crate::controls::menu_action(ui, "Separate audio from video").clicked()
                    {
                        let mut p = v.clone();
                        match p.separate_audio(index) {
                            Ok(()) => changed = Some(p),
                            Err(e) => self.status = e.to_string(),
                        }
                        ui.close();
                    }
                    if c.link.is_some() {
                        if crate::controls::menu_action(ui, "Select linked clip").clicked() {
                            if let Some(i) =
                                v.linked_indices(index).into_iter().find(|i| *i != index)
                            {
                                self.video.selected = Selection::Recording(i);
                            }
                            ui.close();
                        }
                        if crate::controls::menu_action(ui, "Unlink clips").clicked() {
                            let mut p = v.clone();
                            p.unlink(index);
                            changed = Some(p);
                            ui.close();
                        }
                    }
                    if c.kind() != MediaKind::Audio
                        && crate::controls::menu_action(
                            ui,
                            if c.hidden {
                                "Show picture for this segment"
                            } else {
                                "Hide picture for this segment"
                            },
                        )
                        .clicked()
                    {
                        let mut p = v.clone();
                        p.segments[index].hidden = !c.hidden;
                        changed = Some(p);
                        ui.close();
                    }
                    ui.separator();
                    if c.kind() == MediaKind::Video
                        && ui
                            .add_enabled_ui(c.at < v.cursor && v.cursor < c.until(), |ui| {
                                crate::controls::menu_action(ui, "Generate still frame…")
                            })
                            .inner
                            .on_hover_text(
                                "Split here and insert a frame with an adjustable hold duration",
                            )
                            .clicked()
                    {
                        self.open_still_frame(c.id);
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Split at playhead").clicked() {
                        let mut p = v.clone();
                        match p.split_clip(index, v.cursor) {
                            Ok(()) => changed = Some(p),
                            Err(e) => self.status = e.to_string(),
                        };
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Duplicate").clicked() {
                        let mut p = v.clone();
                        let mut copy = c.clone();
                        copy.at = c.until();
                        match p.edit_linked(index, copy, true) {
                            Ok(()) => changed = Some(p),
                            Err(e) => self.status = e.to_string(),
                        };
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Move to new track").clicked() {
                        let mut p = v.clone();
                        p.segments[index].track = p.add_media_track();
                        changed = Some(p);
                        ui.close();
                    }
                    if crate::controls::menu_action(ui, "Clear fades").clicked() {
                        let mut p = v.clone();
                        p.segments[index].fade_in = 0;
                        p.segments[index].fade_out = 0;
                        changed = Some(p);
                        ui.close();
                    }
                    if !c.audio.is_empty()
                        && crate::controls::menu_action(
                            ui,
                            if c.audio.iter().all(|a| a.muted) {
                                "Unmute clip"
                            } else {
                                "Mute clip"
                            },
                        )
                        .clicked()
                    {
                        let mut p = v.clone();
                        let muted = !c.audio.iter().all(|a| a.muted);
                        for a in &mut p.segments[index].audio {
                            a.muted = muted;
                        }
                        changed = Some(p);
                        ui.close();
                    }
                    ui.separator();
                    if crate::controls::menu_action(ui, "Delete · leave gap").clicked() {
                        let mut p = v.clone();
                        p.delete_linked(index);
                        changed = Some(p);
                        self.video.selected = Selection::Objects;
                        ui.close();
                    }
                    if ui
                        .add_enabled_ui(c.duration() < v.duration(), |ui| {
                            crate::controls::menu_action(
                                ui,
                                "Ripple delete · close time on all tracks",
                            )
                        })
                        .inner
                        .clicked()
                    {
                        self.execute(Command::VideoRemove {
                            start: c.at,
                            end: c.until(),
                        });
                        self.video.selected = Selection::Objects;
                        ui.close();
                    }
                });
                let edge = |pos: Pos2| {
                    if pos.y < rect.top() + 12. && (pos.x - x(c.at + c.fade_in)).abs() < 10. {
                        -2
                    } else if pos.y < rect.top() + 12.
                        && (pos.x - x(c.until() - c.fade_out)).abs() < 10.
                    {
                        2
                    } else if (pos.x - rect.left()).abs() < 8. {
                        -1
                    } else if (pos.x - rect.right()).abs() < 8. {
                        1
                    } else {
                        0
                    }
                };
                if r.hovered() {
                    let e: i8 = ui.input(|i| i.pointer.hover_pos()).map_or(0, edge);
                    if e.abs() == 2 {
                        super::timeline::fade_cursor(ui, e > 0);
                    } else {
                        ui.ctx().set_cursor_icon(if e == 0 {
                            egui::CursorIcon::Grab
                        } else {
                            egui::CursorIcon::ResizeHorizontal
                        });
                    }
                }
                if r.drag_started_by(egui::PointerButton::Primary)
                    && let Some(origin) = ui.input(|i| i.pointer.press_origin())
                {
                    self.video.pause();
                    self.select_only(None);
                    self.video.selected = Selection::Recording(index);
                    let duplicate = ui.input(|i| {
                        self.gesture_matches(
                            crate::gesture_input::Gesture::DuplicateDrag,
                            i.modifiers,
                        )
                    });
                    if edge(origin) == 0 && !duplicate {
                        self.begin_timeline_move(
                            capture_core::video::editing::Item::Media(c.id),
                            origin,
                        );
                    } else {
                        self.tool = None;
                        self.icon_library.active = false;
                        self.video.record_drag = Some(MediaDrag {
                            original: self.session.document.video.clone().unwrap(),
                            index,
                            origin,
                            scale,
                            edge: if duplicate { 0 } else { edge(origin) },
                            duplicate,
                        });
                    }
                }
            }
        }
        if let Some(project) = changed {
            self.commit_video(project);
        }
        if ui.input(|i| i.pointer.any_released())
            && let Some(d) = self.video.volume_drag.take()
            && let Some(p) = self.video.timing_preview.take()
            && !p.same_edit(&d.original)
        {
            self.commit_video(p);
        }
        let media_bounds = Rect::from_min_max(
            egui::pos2(left, top),
            egui::pos2(whole.right(), whole.bottom()),
        );
        let mut pointer = ui.input(|i| i.pointer.hover_pos().or(i.pointer.interact_pos()));
        let file_drag =
            ui.input(|i| !i.raw.hovered_files.is_empty() || !i.raw.dropped_files.is_empty());
        #[cfg(windows)]
        if file_drag {
            pointer = crate::image_frame::native_drop_pointer(
                self.hotkey_owner,
                ui.ctx().pixels_per_point(),
            )
            .or(pointer);
        }
        if file_drag {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
        let hover = pointer.filter(|p| media_bounds.intersect(ui.clip_rect()).contains(*p));
        if let Some(pos) = hover {
            let files = ui.input(|i| i.raw.hovered_files.len());
            if files > 0 {
                let paths = ui.input(|i| {
                    i.raw
                        .hovered_files
                        .iter()
                        .filter_map(|f| f.path.clone())
                        .collect::<Vec<_>>()
                });
                if paths != self.video.drop_paths {
                    self.video.drop_paths = paths.clone();
                    self.video.drop_duration = None;
                    self.video.drop_clips.clear();
                    let (tx, rx) = mpsc::channel();
                    self.video.drop_probe = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(super::imports::import_files(&paths));
                    });
                }
                if let Some(result) = self
                    .video
                    .drop_probe
                    .as_ref()
                    .and_then(|r| r.try_recv().ok())
                {
                    self.video.drop_probe = None;
                    match result {
                        Ok(clips) => {
                            self.video.drop_duration =
                                Some(clips.iter().map(|s| s.duration()).sum());
                            self.video.drop_clips = clips;
                        }
                        Err(error) => self.status = error,
                    }
                }
                if self.video.drop_probe.is_some() {
                    ui.ctx().request_repaint_after(Duration::from_millis(40));
                }
                let row = ((pos.y - top) / 39.).floor().max(0.) as usize;
                let target = match v.layers().get(row) {
                    Some(Layer::Media(id)) => *id,
                    _ => *v.media_tracks.last().unwrap(),
                };
                let at = ((pos.x - left) / scale).max(0.) as u64;
                let plan = super::imports::plan_import(v, &self.video.drop_clips, at, target);
                let previews: Vec<_> = if self.video.drop_clips.is_empty() {
                    vec![(
                        v.layer_index(Layer::Media(target)),
                        at,
                        5000,
                        "Reading media…".to_owned(),
                    )]
                } else {
                    plan.segments[v.segments.len()..]
                        .iter()
                        .map(|c| {
                            (
                                plan.layer_index(Layer::Media(c.track)),
                                c.at,
                                c.duration(),
                                format!(
                                    "{} · {:.1}s",
                                    if v.media_tracks.contains(&c.track) {
                                        "Drop"
                                    } else {
                                        "New track"
                                    },
                                    c.duration() as f64 / 1000.
                                ),
                            )
                        })
                        .collect()
                };
                for (row, at, length, label) in previews {
                    let ghost = Rect::from_min_size(
                        egui::pos2(left + at as f32 * scale, top + row as f32 * 39.),
                        egui::vec2((length as f32 * scale).max(12.), 33.),
                    );
                    ui.painter().rect(
                        ghost,
                        4.,
                        Color32::from_rgba_unmultiplied(25, 154, 195, 90),
                        Stroke::new(2., crate::shell::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter()
                        .with_clip_rect(ghost.intersect(ui.clip_rect()))
                        .text(
                            ghost.center(),
                            Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(12.),
                            Color32::WHITE,
                        );
                }
            }
            let paths = ui
                .input_mut(|i| std::mem::take(&mut i.raw.dropped_files))
                .into_iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>();
            if !paths.is_empty() {
                let row = ((pos.y - top) / 39.).floor().max(0.) as usize;
                let track = match v.layers().get(row) {
                    Some(Layer::Media(id)) => *id,
                    _ => *v.media_tracks.last().unwrap(),
                };
                self.import_timeline_media(paths, ((pos.x - left) / scale).max(0.) as u64, track);
            }
        }
        if let Some(d) = &self.video.record_drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let mut project = d.original.clone();
            let original = &d.original.segments[d.index];
            let dt = if d.edge == 0 && ui.input(|i| i.modifiers.shift) {
                0
            } else {
                ((pos.x - d.origin.x) / d.scale).round() as i64
            };
            let mut c = original.clone();
            let frame = (1000 / project.fps as u64).min(c.duration()).max(1);
            let snap = |t: u64| {
                if self.video.snap {
                    project.snap_time(
                        t,
                        &project.linked_items(&std::collections::BTreeSet::from([
                            capture_core::video::editing::Item::Media(original.id),
                        ])),
                        (8. / d.scale).round() as u64,
                    )
                } else {
                    t
                }
            };
            match d.edge {
                -2 => {
                    c.fade_in = original
                        .fade_in
                        .saturating_add_signed(dt)
                        .min(c.duration() - c.fade_out)
                }
                2 => {
                    c.fade_out = original
                        .fade_out
                        .saturating_add_signed(-dt)
                        .min(c.duration() - c.fade_in)
                }
                -1 => {
                    let target = snap(c.at.saturating_add_signed(dt));
                    let delta = (target as i64 - c.at as i64).clamp(
                        -((c.start as f64 / c.speed) as i64),
                        (c.duration() - frame) as i64,
                    );
                    c.at = c.at.saturating_add_signed(delta);
                    c.start = c
                        .start
                        .saturating_add_signed((delta as f64 * c.speed).round() as i64);
                }
                1 => {
                    let limit = c
                        .source
                        .as_ref()
                        .map_or(project.source_duration, |s| s.duration);
                    let target = snap(c.until().saturating_add_signed(dt)).max(c.at + frame);
                    c.end =
                        (c.start + ((target - c.at) as f64 * c.speed).round() as u64).min(limit);
                }
                _ => {
                    c.at = snap(c.at.saturating_add_signed(dt));
                    let row = ((pos.y - top) / 39.).floor().max(0.) as usize;
                    if let Some(Layer::Media(id)) = project.layers().get(row) {
                        c.track = *id;
                    }
                }
            }
            c.clamp_fades();
            let len = c.duration();
            for a in &mut c.audio {
                a.fade_in = a.fade_in.min(len);
                a.fade_out = a.fade_out.min(len);
            }
            if let Err(e) = project.edit_linked(d.index, c.clone(), d.duplicate) {
                self.status = e.to_string();
                return;
            }
            let row = project.layer_index(Layer::Media(c.track));
            let ghost = Rect::from_min_max(
                egui::pos2(x(c.at), y(row)),
                egui::pos2(x(c.until()), y(row) + 33.),
            );
            ui.painter().rect_stroke(
                ghost,
                4.,
                Stroke::new(2., crate::shell::ACCENT),
                egui::StrokeKind::Inside,
            );
            self.video.timing_preview = Some(project);
            if ui.input(|i| i.pointer.any_released()) {
                let p = self.video.timing_preview.take().unwrap();
                self.video.record_drag = None;
                self.commit_video(p);
            }
        }
    }
}
