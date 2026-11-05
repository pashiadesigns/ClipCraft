use super::*;
use capture_core::video::{
    links::Layer,
    zoom::{Motion, Zoom},
};
use egui::{Color32, Rect, Stroke};
pub(super) struct Drag {
    pub original: Project,
    pub id: u64,
    pub origin: egui::Pos2,
    pub edge: i8,
}
impl Editor {
    pub(crate) fn video_object_motion(&self, id: u64) -> ([f32; 2], f32) {
        let Some(p) = &self.session.document.video else {
            return ([0., 0.], 1.);
        };
        let Some(t) = p.objects.get(&id) else {
            return ([0., 0.], 1.);
        };
        let Some(a) = self
            .session
            .document
            .annotations
            .iter()
            .find(|a| a.id == id)
        else {
            return ([0., 0.], 1.);
        };
        let Some(animated) = capture_core::video::editor::animated(a, t, p.cursor) else {
            return ([0., 0.], 1.);
        };
        let active = if p.cursor.saturating_sub(t.start) < t.entrance_ms {
            t.entrance
        } else if t.end.saturating_sub(p.cursor) < t.exit_ms {
            t.exit
        } else {
            capture_core::video::editor::Animation::None
        };
        if !matches!(
            active,
            capture_core::video::editor::Animation::Float
                | capture_core::video::editor::Animation::Scale
        ) {
            return ([0., 0.], 1.);
        }
        let (lo, hi) = a.body_bounds();
        let (next, end) = animated.body_bounds();
        let axis = usize::from((hi[1] - lo[1]).abs() > (hi[0] - lo[0]).abs());
        let scale = if (hi[axis] - lo[axis]).abs() > 0.01 {
            ((end[axis] - next[axis]) / (hi[axis] - lo[axis])).max(0.001)
        } else {
            1.
        };
        ([next[0] - lo[0] * scale, next[1] - lo[1] * scale], scale)
    }
    pub(crate) fn video_layer_transform(
        &self,
        t: crate::canvas_view::Transform,
        layer: Layer,
    ) -> crate::canvas_view::Transform {
        if self.video.zoom_edit {
            return t;
        }
        let Some(p) = self
            .video
            .timing_preview
            .as_ref()
            .or(self.session.document.video.as_ref())
        else {
            return t;
        };
        let v = p.layer_view(
            layer,
            [self.session.document.width, self.session.document.height],
            p.cursor,
        );
        crate::canvas_view::Transform {
            image: Rect::from_min_size(t.screen_point(v.offset), t.image.size() * v.scale),
            scale: t.scale * v.scale,
            viewport: t.viewport.intersect(t.image),
            ..t
        }
    }
    pub(crate) fn video_object_point(
        &self,
        id: u64,
        point: [f32; 2],
        tolerance: f32,
    ) -> ([f32; 2], f32) {
        let Some(p) = self.session.document.video.as_ref() else {
            return (point, tolerance);
        };
        let Some(t) = p.objects.get(&id) else {
            return (point, tolerance);
        };
        let v = p.layer_view(
            Layer::Annotation(t.track),
            [self.session.document.width, self.session.document.height],
            p.cursor,
        );
        let point = v.inverse(point);
        let (offset, scale) = self.video_object_motion(id);
        (
            [
                (point[0] - offset[0]) / scale,
                (point[1] - offset[1]) / scale,
            ],
            tolerance / v.scale / scale,
        )
    }
    pub(crate) fn video_annotation_transform(
        &self,
        t: crate::canvas_view::Transform,
        pos: Option<egui::Pos2>,
    ) -> crate::canvas_view::Transform {
        let Some(p) = self.session.document.video.as_ref() else {
            return t;
        };
        let selected = self.selected;
        let hovering = pos.and_then(|pos| self.tool_hit(t.image_point(pos), 7. / t.scale));
        let dragging =
            self.edit_drag.is_some() || self.group_drag.is_some() || self.rotation_drag.is_some();
        let object = if dragging { selected } else { hovering };
        let track = object
            .or(selected)
            .and_then(|id| p.objects.get(&id))
            .map(|v| v.track)
            .or(p.annotation_track)
            .or_else(|| p.tracks.first().copied());
        let t = track.map_or(t, |id| self.video_layer_transform(t, Layer::Annotation(id)));
        if let Some(id) = object {
            let (offset, scale) = self.video_object_motion(id);
            crate::canvas_view::Transform {
                image: Rect::from_min_size(t.screen_point(offset), t.image.size() * scale),
                scale: t.scale * scale,
                ..t
            }
        } else {
            t
        }
    }
    pub(super) fn start_zoom_insert(&mut self) {
        self.video.pause();
        self.cancel_gesture();
        self.select_only(None);
        self.tool = None;
        self.icon_library.active = false;
        self.video.zoom_draw = true;
        self.video.zoom_from = None;
        self.status = "Draw a focus area on the canvas. Its proportions match the project.".into();
    }
    pub(crate) fn zoom_canvas_input(
        &mut self,
        ui: &egui::Ui,
        t: crate::canvas_view::Transform,
        enabled: bool,
    ) -> bool {
        if self.zoom_focus_input(ui, t, enabled) {
            return true;
        }
        if !self.video.zoom_draw {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.zoom_draw = false;
            self.video.zoom_from = None;
            return true;
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
            let point = t.image_point(pos);
            let point = [
                (point[0] / self.session.document.width as f32).clamp(0., 1.),
                (point[1] / self.session.document.height as f32).clamp(0., 1.),
            ];
            if enabled
                && t.image.intersect(t.viewport).contains(pos)
                && ui.input(|i| i.pointer.primary_pressed())
            {
                self.video.zoom_from = Some(point);
            }
            if let Some(from) = self.video.zoom_from {
                let [x, y, side] = focus_rect(from, point);
                let r = Rect::from_min_max(
                    t.screen_point([
                        x * self.session.document.width as f32,
                        y * self.session.document.height as f32,
                    ]),
                    t.screen_point([
                        (x + side) * self.session.document.width as f32,
                        (y + side) * self.session.document.height as f32,
                    ]),
                );
                ui.ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::unique("zoom-focus-preview"),
                    ))
                    .with_clip_rect(t.viewport)
                    .rect(
                        r,
                        2.,
                        Color32::from_rgba_unmultiplied(25, 158, 200, 30),
                        Stroke::new(2., crate::shell::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                if ui.input(|i| i.pointer.primary_released()) {
                    let mut p = self.session.document.video.clone().unwrap();
                    let existing = self
                        .video
                        .zoom_selected
                        .and_then(|id| p.zooms.iter().position(|z| z.id == id));
                    let id = existing.map_or_else(
                        || p.zooms.iter().map(|z| z.id).max().unwrap_or(0) + 1,
                        |i| p.zooms[i].id,
                    );
                    let mut z = existing.map_or_else(
                        || {
                            Zoom::new(
                                id,
                                p.cursor.min(14_395_000),
                                p.cursor.saturating_add(5000).min(14_400_000),
                            )
                        },
                        |i| p.zooms[i].clone(),
                    );
                    z.center = [x + side / 2., y + side / 2.];
                    z.scale = 1. / side;
                    if let Some(i) = existing {
                        p.zooms[i] = z;
                    } else {
                        p.zooms.push(z);
                        p.layer_order.insert(0, Layer::Zoom(id));
                    }
                    self.commit_video(p);
                    if self
                        .session
                        .document
                        .video
                        .as_ref()
                        .is_some_and(|p| p.zooms.iter().any(|z| z.id == id))
                    {
                        self.status =
                            "Zoom ready. Adjust focus, timing and motion in Properties.".into();
                    }
                    self.video.zoom_draw = false;
                    self.video.zoom_from = None;
                    self.video.zoom_selected = Some(id);
                    self.video.selected = Selection::Objects;
                    self.video.tab = PropertyTab::Style;
                    self.shell.properties = true;
                    self.set_timeline_expanded(true);
                }
            }
        }
        true
    }
    pub(super) fn zoom_properties(&mut self, ui: &mut egui::Ui, id: u64) {
        let Some(mut p) = self.session.document.video.clone() else {
            return;
        };
        let before = p.clone();
        let Some(z) = p.zooms.iter_mut().find(|z| z.id == id) else {
            self.video.zoom_selected = None;
            return;
        };
        if self.video.tab == PropertyTab::Style {
            property_design::heading(ui, "Zoom in", "Focus & magnification");
            if super::ui::button(
                ui,
                "Position",
                if self.video.zoom_edit {
                    "Done editing focus"
                } else {
                    "Edit focus on canvas"
                },
                ui.available_width(),
                self.video.zoom_edit,
            )
            .clicked()
            {
                self.video.zoom_edit = !self.video.zoom_edit;
                self.video.zoom_focus_drag = None;
                self.video.timing_preview = None;
                self.video.pause();
                self.needs_render = true;
            }
            if self.video.zoom_edit {
                ui.small("Drag the focus area or its corners. The unzoomed view keeps the current playhead time.");
            }
            crate::property_layout::slider(
                ui,
                "Zoom",
                egui::Slider::new(&mut z.scale, 1. ..=10.).suffix("×"),
            );
            for (i, label) in ["Horizontal center", "Vertical center"]
                .into_iter()
                .enumerate()
            {
                crate::property_layout::slider(
                    ui,
                    label,
                    egui::Slider::new(&mut z.center[i], 0. ..=1.)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.)),
                );
            }
            if ui.button("Draw focus area again").clicked() {
                self.video.zoom_draw = true;
                self.video.zoom_from = None;
            }
            property_design::end(ui);
        }
        if self.video.tab == PropertyTab::Timing {
            property_design::heading(ui, "Timing", "Timing");
            ui.columns(2, |c| {
                super::properties::seconds_field(&mut c[0], "Start", &mut z.start, 14_399_999);
                super::properties::seconds_field(&mut c[1], "End", &mut z.end, 14_400_000);
            });
            z.end = z.end.max(z.start + 1);
            property_design::end(ui);
        }
        if self.video.tab == PropertyTab::Motion {
            property_design::heading(ui, "Motion", "Motion");
            for exit in [false, true] {
                ui.push_id(exit, |ui| {
                    ui.label(if exit { "Zoom out" } else { "Zoom in" });
                    let (mode, duration) = if exit {
                        (&mut z.exit, &mut z.exit_ms)
                    } else {
                        (&mut z.entrance, &mut z.entrance_ms)
                    };
                    egui::ComboBox::from_id_salt("zoom-motion")
                        .width(ui.available_width())
                        .selected_text(mode.label())
                        .show_ui(ui, |ui| {
                            for value in Motion::ALL {
                                ui.selectable_value(mode, value, value.label());
                            }
                        });
                    if *mode != Motion::Snap {
                        super::properties::seconds_field(ui, "Duration", duration, z.end - z.start);
                    } else {
                        *duration = 0;
                    }
                });
            }
            z.entrance_ms = z.entrance_ms.min(z.end - z.start);
            z.exit_ms = z.exit_ms.min(z.end - z.start - z.entrance_ms);
            crate::controls::toggle(ui, &mut z.return_to_full, "Animate return to full canvas");
            ui.small(if z.pinned {
                "Pinned above new tracks. Drag this track lower to unpin."
            } else {
                "Only layers below this track are affected. Drag it to the top to pin again."
            });
            property_design::end(ui);
        }
        z.entrance_ms = z.entrance_ms.min(z.end - z.start);
        z.exit_ms = z.exit_ms.min(z.end - z.start - z.entrance_ms);
        if ui.button("Delete Zoom").clicked() {
            p.remove_layer(Layer::Zoom(id));
            self.video.zoom_selected = None;
        }
        if p != before {
            self.commit_video(p);
        }
    }
    pub(super) fn zoom_timeline(
        &mut self,
        ui: &mut egui::Ui,
        p: &Project,
        left: f32,
        top: f32,
        scale: f32,
    ) {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.zoom_drag = None;
            self.video.timing_preview = None;
        }
        for z in &p.zooms {
            let row = p.layer_index(Layer::Zoom(z.id));
            let r = Rect::from_min_max(
                egui::pos2(left + z.start as f32 * scale, top + row as f32 * 39.),
                egui::pos2(left + z.end as f32 * scale, top + row as f32 * 39. + 33.),
            );
            let response = super::timeline::block(
                ui,
                r,
                egui::Id::unique(("zoom-block", z.id)),
                &format!("Zoom · {:.1}×", z.scale),
                Color32::from_rgb(55, 111, 131),
                self.video.zoom_selected == Some(z.id),
            );
            let entrance = left
                + (z.start
                    + if z.entrance == Motion::Snap {
                        0
                    } else {
                        z.entrance_ms
                    }) as f32
                    * scale;
            let exit =
                left + (z.end - if z.exit == Motion::Snap { 0 } else { z.exit_ms }) as f32 * scale;
            super::timeline::fade_handle(ui, r, entrance, true);
            super::timeline::fade_handle(ui, r, exit, false);
            let handle = |pos: egui::Pos2| -> i8 {
                if pos.y < r.top() + 12.
                    && ((pos.x - entrance).abs() < 12. || (pos.x - exit).abs() < 12.)
                {
                    if (pos.x - entrance).abs() <= (pos.x - exit).abs() {
                        -2
                    } else {
                        2
                    }
                } else if (pos.x - r.left()).abs() < 8. {
                    -1
                } else if (pos.x - r.right()).abs() < 8. {
                    1
                } else {
                    0
                }
            };
            if response.hovered() {
                let edge = ui.input(|i| i.pointer.hover_pos()).is_some_and(|pos| {
                    (pos.x - r.left()).abs() < 8. || (pos.x - r.right()).abs() < 8.
                });
                ui.ctx().set_cursor_icon(if edge {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::Grab
                });
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                    && handle(pos).abs() == 2
                {
                    super::timeline::fade_cursor(ui, handle(pos) > 0);
                }
            }
            if response.clicked() || response.secondary_clicked() {
                self.timeline_focus(
                    capture_core::video::editing::Item::Zoom(z.id),
                    ui.input(|i| i.modifiers.ctrl || i.modifiers.shift),
                );
                self.video.tab = PropertyTab::Style;
            }
            response.context_menu(|ui| {
                if crate::controls::menu_action(ui, "Delete Zoom").clicked() {
                    self.execute(Command::VideoDeleteTrack {
                        layer: Layer::Zoom(z.id),
                    });
                    self.video.zoom_selected = None;
                    ui.close();
                }
            });
            if response.drag_started_by(egui::PointerButton::Primary)
                && let Some(pos) = ui.input(|i| i.pointer.press_origin())
            {
                if handle(pos) == 0 {
                    self.begin_timeline_move(capture_core::video::editing::Item::Zoom(z.id), pos);
                } else {
                    self.tool = None;
                    self.icon_library.active = false;
                    self.video.pause();
                    self.timeline_focus(capture_core::video::editing::Item::Zoom(z.id), false);
                    self.video.zoom_drag = Some(Drag {
                        original: p.clone(),
                        id: z.id,
                        origin: pos,
                        edge: handle(pos),
                    });
                }
            }
        }
        if let Some(d) = &self.video.zoom_drag
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        {
            let mut next = d.original.clone();
            let delta = ((pos.x - d.origin.x) / scale).round() as i64;
            let excluded =
                std::collections::BTreeSet::from([capture_core::video::editing::Item::Zoom(d.id)]);
            let old = d.original.zooms.iter().find(|z| z.id == d.id).unwrap();
            let edge_time = match d.edge {
                -2 => {
                    old.start
                        + if old.entrance == Motion::Snap {
                            0
                        } else {
                            old.entrance_ms
                        }
                }
                2 => {
                    old.end
                        - if old.exit == Motion::Snap {
                            0
                        } else {
                            old.exit_ms
                        }
                }
                -1 => old.start,
                _ => old.end,
            };
            let proposed = edge_time.saturating_add_signed(delta);
            let snapped = if self.video.snap {
                d.original
                    .snap_time(proposed, &excluded, (8. / scale).round() as u64)
            } else {
                proposed
            };
            let delta = snapped as i64 - edge_time as i64;
            let z = next.zooms.iter_mut().find(|z| z.id == d.id).unwrap();
            match d.edge {
                -2 => {
                    z.entrance_ms = snapped
                        .saturating_sub(z.start)
                        .min(z.end - z.start - z.exit_ms);
                    if z.entrance == Motion::Snap && z.entrance_ms > 0 {
                        z.entrance = Motion::Smooth;
                    }
                }
                2 => {
                    z.exit_ms = z
                        .end
                        .saturating_sub(snapped)
                        .min(z.end - z.start - z.entrance_ms);
                    if z.exit == Motion::Snap && z.exit_ms > 0 {
                        z.exit = Motion::Smooth;
                    }
                }
                -1 => z.start = z.start.saturating_add_signed(delta).min(z.end - 1),
                1 => {
                    z.end = z
                        .end
                        .saturating_add_signed(delta)
                        .clamp(z.start + 1, 14_400_000)
                }
                _ => {
                    let length = z.end - z.start;
                    z.start = z
                        .start
                        .saturating_add_signed(delta)
                        .min(14_400_000 - length);
                    z.end = z.start + length;
                }
            }
            z.entrance_ms = z.entrance_ms.min(z.end - z.start);
            z.exit_ms = z.exit_ms.min(z.end - z.start - z.entrance_ms);
            if d.edge.abs() == 2 {
                super::timeline::fade_cursor(ui, d.edge > 0);
            } else {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
            self.video.timing_preview = Some(next);
            if ui.input(|i| i.pointer.primary_released()) {
                self.video.zoom_drag = None;
                let next = self.video.timing_preview.take().unwrap();
                self.commit_video(next);
            }
            ui.ctx().request_repaint();
        }
    }
}
fn focus_rect(a: [f32; 2], b: [f32; 2]) -> [f32; 3] {
    let side = (a[0] - b[0]).abs().max((a[1] - b[1]).abs()).clamp(0.1, 1.);
    [
        a[0].min(b[0]).min(1. - side),
        a[1].min(b[1]).min(1. - side),
        side,
    ]
}
