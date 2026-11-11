use super::*;
use crate::tips::Tip as _;
use capture_core::{
    Kind,
    selection::Handle,
    video::{frame::Frame, links::Layer},
};
use egui::{Color32, Stroke};
pub(super) struct Drag {
    original: Project,
    index: usize,
    from: [f32; 2],
    handle: Option<Handle>,
    crop: bool,
    /// Alt+drag: `index` is a copy that exists only in `original` until the drop.
    duplicate: bool,
    source: usize,
}
/// A copy of clip `index` on a track of its own, so it doesn't crossfade with
/// the original. Returns the project and the copy's index.
pub(super) fn duplicate_clip(p: &Project, index: usize) -> Option<(Project, usize)> {
    let mut copy = p.clone();
    let first_new = copy.next_media_id();
    let clip = copy.segments.get(index)?.clone();
    copy.edit_linked(index, clip, true).ok()?;
    let at = copy.segments.iter().position(|c| {
        c.id >= first_new && c.kind() != capture_core::video::editor::MediaKind::Audio
    })?;
    let track = copy.add_media_track();
    copy.segments[at].track = track;
    Some((copy, at))
}
/// Corner inset (0), Circle inset (1) or Fullscreen (2), placed absolutely.
pub(super) fn apply_clip_preset(
    c: &mut capture_core::video::editor::Segment,
    preset: usize,
    ratio: f32,
) {
    let f = c.frame.get_or_insert_with(Frame::default);
    match preset {
        0 => {
            c.rect = [0.69, 0.62, 0.28, 0.32];
            f.shape = Kind::Rectangle;
            f.radius = 20.;
        }
        1 => {
            c.rect = [0.97 - 0.32 / ratio, 0.65, 0.32 / ratio, 0.32];
            f.shape = Kind::Ellipse;
            f.radius = 0.;
        }
        _ => {
            c.rect = [0., 0., 1., 1.];
            f.shape = Kind::Rectangle;
            f.radius = 0.;
        }
    }
}
impl Editor {
    pub(super) fn video_frame_properties(
        &mut self,
        ui: &mut egui::Ui,
        c: &mut capture_core::video::editor::Segment,
    ) {
        let keyed_base = c.clone();
        let age = self.video_time().saturating_sub(c.at);
        *c = keyed_base.keyed(self.video_time());
        use super::property_design as design;
        design::heading(ui, "Presets", "Presets");
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 16.) / 3.;
            for (name, preset) in [("Corner inset", 0), ("Circle inset", 1), ("Fullscreen", 2)] {
                let selected = match preset {
                    1 => c.frame.as_ref().is_some_and(|f| f.shape == Kind::Ellipse),
                    2 => c.rect == [0., 0., 1., 1.],
                    _ => {
                        c.rect[2] < 0.5
                            && c.frame.as_ref().is_some_and(|f| f.shape == Kind::Rectangle)
                    }
                };
                if design::preset(ui, name, preset, selected, width) {
                    let ratio =
                        self.session.document.width as f32 / self.session.document.height as f32;
                    apply_clip_preset(c, preset, ratio);
                    // Every selected clip takes the whole preset, not the change.
                    self.video.clip_preset = Some(preset);
                }
            }
        });
        design::end(ui);
        let mut color = c.frame.as_ref().map_or_else(Default::default, |f| f.color);
        let old_color = color;
        crate::finish_ui::color_controls(ui, &mut color, crate::finish_ui::Place::Pane);
        if color != old_color {
            let fill = self.session.document.video.as_ref().is_some_and(|v| v.fill);
            c.frame
                .get_or_insert_with(|| Frame {
                    fill,
                    ..Default::default()
                })
                .color = color;
        }
        let before = c.frame.clone().unwrap_or(Frame {
            radius: 0.,
            ..Default::default()
        });
        if let Some(capture_core::reusable_presets::Payload::VideoFrame { frame, rect }) =
            self.presets.reusable.controls(
                ui,
                capture_core::reusable_presets::Payload::VideoFrame {
                    frame: before.clone(),
                    rect: c.rect,
                },
            )
        {
            c.frame = Some(frame);
            c.rect = rect;
        }
        let mut f = c.frame.clone().unwrap_or(before.clone());
        design::heading(ui, "Fit preview", "Frame");
        ui.columns(2, |col| {
            col[0].label("Shape");
            egui::ComboBox::from_id_salt("video-frame-shape")
                .width(col[0].available_width())
                .selected_text(match f.shape {
                    Kind::Ellipse => "Circle / oval",
                    Kind::Polygon => "Polygon",
                    _ => "Rectangle",
                })
                .show_ui(&mut col[0], |ui| {
                    for (kind, label) in [
                        (Kind::Rectangle, "Rectangle"),
                        (Kind::Ellipse, "Circle / oval"),
                        (Kind::Polygon, "Polygon"),
                    ] {
                        ui.selectable_value(&mut f.shape, kind, label);
                    }
                });
            if f.shape == Kind::Polygon {
                col[1].label("Sides");
                col[1].add_sized(
                    [col[1].available_width(), 28.],
                    egui::DragValue::new(&mut f.sides).range(3..=12),
                );
            } else {
                col[1].label("Corner radius");
                col[1].add_enabled_ui(f.shape == Kind::Rectangle, |ui| {
                    ui.add_sized(
                        [ui.available_width(), 28.],
                        egui::DragValue::new(&mut f.radius)
                            .range(0. ..=256.)
                            .suffix(" px"),
                    );
                });
            }
        });
        let mut opacity = f.opacity as f32 / 255. * 100.;
        design::percent(ui, "Opacity", &mut opacity, 100.);
        f.opacity = (opacity * 2.55).round() as u8;
        design::end(ui);
        let summary = format!(
            "{} · {:.0}%",
            if f.fill { "Fill" } else { "Fit" },
            100. / f.view.rect[2].max(f.view.rect[3])
        );
        let crop_open = design::group_forced(
            ui,
            "Crop",
            "Crop footage",
            &summary,
            self.video.frame_crop,
            |ui| {
                crate::controls::toggle(ui, &mut self.video.frame_crop, "Edit crop on canvas");
                ui.small("Drag inside the frame to reposition footage. The frame stays in place.");
                let mut zoom = 1. / f.view.rect[2].max(f.view.rect[3]);
                if crate::property_layout::slider(
                    ui,
                    "Zoom",
                    egui::Slider::new(&mut zoom, 1. ..=10.),
                )
                .changed()
                    && let Ok(v) = f.view.zoomed(zoom * f.view.rect[2].max(f.view.rect[3]))
                {
                    f.view = v;
                }
                crate::controls::segments(ui, |ui| {
                    ui.selectable_value(&mut f.fill, true, "Fill");
                    ui.selectable_value(&mut f.fill, false, "Fit");
                });
                let [x, y, w, h] = f.view.rect;
                let mut edges = [x, y, 1. - x - w, 1. - y - h];
                for (i, label) in ["Left", "Top", "Right", "Bottom"].iter().enumerate() {
                    crate::property_layout::slider(
                        ui,
                        label,
                        egui::Slider::new(&mut edges[i], 0. ..=0.95)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.)),
                    );
                }
                let candidate = capture_core::image_frame::ImageView {
                    rect: [
                        edges[0],
                        edges[1],
                        1. - edges[0] - edges[2],
                        1. - edges[1] - edges[3],
                    ],
                };
                if candidate.validate().is_ok() {
                    f.view = candidate;
                }
                if ui.button("Reset crop").clicked() {
                    f.view = Default::default();
                }
            },
        );
        if !crop_open {
            self.video.frame_crop = false;
        }
        let summary = format!(
            "{} px · shadow {}",
            f.width,
            if f.shadow.is_some() { "on" } else { "off" }
        );
        design::group(ui, "Outline", "Outline & shadow", &summary, |ui| {
            ui.horizontal(|ui| {
                ui.label("Outline");
                crate::color_widgets::rgba_live(ui, &mut f.outline);
                ui.add(
                    egui::DragValue::new(&mut f.width)
                        .range(0..=128)
                        .suffix(" px"),
                );
            });
            let mut style = f
                .line_style
                .unwrap_or_else(|| capture_core::line_style::Style::for_kind(f.shape));
            egui::ComboBox::from_id_salt("video-frame-stroke")
                .selected_text(style.pattern.label())
                .show_ui(ui, |ui| {
                    for p in capture_core::line_style::Pattern::ALL {
                        ui.selectable_value(&mut style.pattern, p, p.label());
                    }
                });
            if style
                != f.line_style
                    .unwrap_or_else(|| capture_core::line_style::Style::for_kind(f.shape))
            {
                f.line_style = Some(style);
            }
            let mut shadow = f.shadow.is_some();
            if crate::controls::toggle(ui, &mut shadow, "Shadow").changed() {
                f.shadow = shadow.then(Default::default);
            }
            if let Some(shadow) = &mut f.shadow {
                ui.horizontal(|ui| {
                    ui.label("Shadow color");
                    crate::color_widgets::rgba_live(ui, &mut shadow.color);
                });
                crate::property_layout::slider(
                    ui,
                    "Direction",
                    egui::Slider::new(&mut shadow.angle, 0. ..=360.),
                );
                crate::property_layout::slider(
                    ui,
                    "Distance",
                    egui::Slider::new(&mut shadow.distance, 0. ..=64.),
                );
                crate::property_layout::slider(
                    ui,
                    "Softness",
                    egui::Slider::new(&mut shadow.blur, 0. ..=64.),
                );
            }
        });
        let placement = format!("{:.0}% × {:.0}%", c.rect[2] * 100., c.rect[3] * 100.);
        design::group(ui, "Position", "Position & size", &placement, |ui| {
            ui.columns(2, |col| {
                for (i, label) in ["Left", "Top", "Width", "Height"].iter().enumerate() {
                    let column = &mut col[i % 2];
                    column.label(*label);
                    column.add_sized(
                        [column.available_width(), 28.],
                        egui::DragValue::new(&mut c.rect[i])
                            .range(0. ..=1.)
                            .speed(0.005),
                    );
                }
            });
            c.rect[2] = c.rect[2].clamp(0.01, 1.);
            c.rect[3] = c.rect[3].clamp(0.01, 1.);
            c.rect[0] = c.rect[0].clamp(0., 1. - c.rect[2]);
            c.rect[1] = c.rect[1].clamp(0., 1. - c.rect[3]);
            if ui.button("Reset position & size").clicked() {
                c.rect = [0., 0., 1., 1.];
                f.rotation = 0.;
            }
            crate::property_layout::slider(
                ui,
                "Rotation",
                egui::Slider::new(&mut f.rotation, -180. ..=180.).suffix("°"),
            );
            crate::controls::toggle(ui, &mut f.mirror, "Flip horizontal");
            crate::controls::toggle(ui, &mut f.flip_y, "Flip vertical");
        });
        if f != before {
            c.frame = Some(f);
        }
        let mut keys = keyed_base.keyframes.clone();
        *c = keys.edit_media(&keyed_base, c, age, self.video.keyframes.auto_key);
    }
    pub(crate) fn video_frame_input(
        &mut self,
        ui: &egui::Ui,
        t: crate::canvas_view::Transform,
        enabled: bool,
    ) -> bool {
        if !self.is_video()
            || self.tool.is_some()
            || self.icon_library.active
            || self.video.playing
            || self.edit_drag.is_some()
            || self.group_drag.is_some()
            || self.rotation_drag.is_some()
        {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.video.frame_drag = None;
            self.video.timing_preview = None;
            self.video.frame_crop = false;
            return false;
        }
        let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
            return false;
        };
        if self.video_frame_zoom_buttons(ui, t, enabled) {
            return true;
        }
        let base_t = t;
        let t = self
            .video
            .frame_drag
            .as_ref()
            .map(|d| d.original.segments[d.index].track)
            .or_else(|| {
                if let Selection::Recording(i) = self.video.selected {
                    self.session
                        .document
                        .video
                        .as_ref()
                        .and_then(|p| p.segments.get(i))
                        .map(|c| c.track)
                } else {
                    None
                }
            })
            .map_or(t, |track| {
                self.video_layer_transform(t, Layer::Media(track))
            });
        let point = t.image_point(pos);
        let size = [self.session.document.width, self.session.document.height];
        if let Some(d) = &self.video.frame_drag {
            // The copy has no decoded frame yet; show it with the original's.
            let copy_layer = d.duplicate.then(|| {
                (
                    d.original.segments[d.index].id,
                    d.original.segments.get(d.source).map(|c| c.id),
                )
            });
            let mut p = d.original.clone();
            let original = p.segments[d.index].clone();
            p.segments[d.index] = original.keyed(p.cursor);
            let c = &mut p.segments[d.index];
            let mut a = geometry(c, size);
            if d.crop {
                let f = c.frame.get_or_insert_with(Frame::default);
                let (sine, cosine) = (-f.rotation.to_radians()).sin_cos();
                let dx = point[0] - d.from[0];
                let dy = point[1] - d.from[1];
                let delta = [
                    -(dx * cosine - dy * sine) / (c.rect[2] * size[0] as f32) * f.view.rect[2],
                    -(dx * sine + dy * cosine) / (c.rect[3] * size[1] as f32) * f.view.rect[3],
                ];
                if let Ok(v) = f.view.panned(delta) {
                    f.view = v;
                }
            } else {
                let moved = if let Some(handle) = d.handle {
                    a.resized_with_modifiers(
                        handle,
                        point,
                        size,
                        ui.input(|i| capture_core::resize_modifiers::ResizeModifiers {
                            constrain: i.modifiers.shift,
                            from_center: i.modifiers.alt,
                        }),
                    )
                    .map(|moved| capture_core::steps::label_follows(&a, moved))
                } else {
                    a.translated([point[0] - d.from[0], point[1] - d.from[1]], size)
                };
                if let Ok(value) = moved {
                    a = value;
                    let (lo, hi) = a.body_bounds();
                    let rect = [
                        lo[0] / size[0] as f32,
                        lo[1] / size[1] as f32,
                        (hi[0] - lo[0]) / size[0] as f32,
                        (hi[1] - lo[1]) / size[1] as f32,
                    ];
                    if rect[0] >= 0.
                        && rect[1] >= 0.
                        && rect[0] + rect[2] <= 1.001
                        && rect[1] + rect[3] <= 1.001
                    {
                        c.rect = rect;
                        c.frame.get_or_insert_with(Frame::default).radius = a.corner_radius;
                    }
                }
            }
            let mut keys = original.keyframes.clone();
            p.segments[d.index] = keys.edit_media(
                &original,
                c,
                p.cursor.saturating_sub(original.at),
                self.video.keyframes.auto_key,
            );
            self.video.timing_preview = Some(p);
            ui.ctx().set_cursor_icon(if d.crop {
                egui::CursorIcon::Grabbing
            } else {
                d.handle
                    .map_or(egui::CursorIcon::Move, crate::canvas_cursor::handle_cursor)
            });
            if d.duplicate {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Copy);
            }
            if ui.input(|i| i.pointer.any_released()) {
                // A press with Alt that never moved leaves no copy behind.
                let unmoved =
                    d.duplicate && (point[0] - d.from[0]).hypot(point[1] - d.from[1]) < 2.;
                let (duplicate, copied, source) = (d.duplicate, d.index, d.source);
                self.video.frame_drag = None;
                let next = self.video.timing_preview.take().filter(|_| !unmoved);
                self.video.selected = Selection::Recording(source);
                if let Some(p) = next {
                    self.commit_video(p);
                    if duplicate {
                        self.video.selected = Selection::Recording(copied);
                    }
                }
            }
            if let Some((copy, Some(source))) = copy_layer
                && !self.video.frame_layers.iter().any(|(id, _)| *id == copy)
                && let Some(bitmap) = self
                    .video
                    .frame_layers
                    .iter()
                    .find(|(id, _)| *id == source)
                    .map(|(_, b)| b.clone())
            {
                self.video.frame_layers.push((copy, bitmap));
                self.needs_render = true;
            }
            ui.ctx().request_repaint();
            return true;
        }
        if !enabled || !t.viewport.intersect(t.image).contains(pos) {
            return false;
        }
        // While the button is held, decide by where the press began: a drag that
        // started on an object or its handle stays with the canvas even after the
        // pointer crosses footage during egui's drag threshold.
        let pos = ui
            .input(|i| i.pointer.primary_down().then(|| i.pointer.press_origin()))
            .flatten()
            .unwrap_or(pos);
        let point = t.image_point(pos);
        // A selected object's resize and rotation handles sit above the footage
        // and may extend past the object; they belong to the object, not the frame.
        if self.selected.is_some() {
            let object_t = self.video_annotation_transform(base_t, Some(pos));
            let on_handle = self.selected_annotation().is_some_and(|a| {
                self.session.document.object_editable(a.id)
                    && a.hit_handle(object_t.image_point(pos), 7. / object_t.scale)
                        .is_some()
            });
            if on_handle || self.rotation_hit(pos, object_t) {
                return false;
            }
        }
        let p = self.session.document.video.as_ref().unwrap();
        let mut hit = None;
        if let Selection::Recording(i) = self.video.selected
            && let Some(c) = p.segments.get(i)
            && c.kind() != capture_core::video::editor::MediaKind::Audio
            && c.active(p.cursor)
            && !c.hidden
        {
            let a = geometry(&c.keyed(p.cursor), size);
            if let Some(handle) = a.hit_handle(point, 7. / t.scale).filter(|h| !h.is_radius()) {
                hit = Some((i, Some(handle)));
            }
        }
        if hit.is_none() {
            for layer in p.layers() {
                match layer {
                    Layer::Zoom(_) => (),
                    Layer::Annotation(track) => {
                        if self.session.document.annotations.iter().any(|a| {
                            self.video_object_active(a.id) && p.objects[&a.id].track == track && {
                                let (point, tol) = self.video_object_point(
                                    a.id,
                                    base_t.image_point(pos),
                                    3. / base_t.scale,
                                );
                                self.keyframe_annotation(a).hit_test(point, tol)
                            }
                        }) {
                            break;
                        }
                    }
                    Layer::Media(track) => {
                        if let Some((i, _)) = p.segments.iter().enumerate().rev().find(|(_, c)| {
                            c.track == track
                                && c.kind() != capture_core::video::editor::MediaKind::Audio
                                && c.active(p.cursor)
                                && !c.hidden
                                && geometry(&c.keyed(p.cursor), size).hit_test(
                                    self.video_layer_transform(base_t, Layer::Media(track))
                                        .image_point(pos),
                                    0.,
                                )
                        }) {
                            hit = Some((i, None));
                            break;
                        }
                    }
                }
            }
        }
        let Some((index, handle)) = hit else {
            return false;
        };
        let t = self.video_layer_transform(base_t, Layer::Media(p.segments[index].track));
        let point = t.image_point(pos);
        let original = p.clone();
        let a = geometry(&p.segments[index].keyed(p.cursor), size);
        let (lo, hi) = a.bounds();
        let area =
            egui::Rect::from_two_pos(t.screen_point(lo), t.screen_point(hi)).intersect(t.viewport);
        ui.interact(
            area,
            egui::Id::unique(("video-frame-menu", p.segments[index].id)),
            egui::Sense::click(),
        )
        .context_menu(|ui| {
            if crate::controls::menu_action(ui, "Frame properties").clicked() {
                self.video.selected = Selection::Recording(index);
                self.video.tab = PropertyTab::Style;
                self.video.frame_crop = false;
                self.shell.properties = true;
                ui.close();
            }
            if crate::controls::menu_action(ui, "Crop footage").clicked() {
                self.video.selected = Selection::Recording(index);
                self.video.tab = PropertyTab::Style;
                self.video.frame_crop = true;
                self.shell.properties = true;
                ui.close();
            }
            if crate::controls::menu_action(ui, "Reset crop").clicked() {
                let mut next = original.clone();
                next.segments[index]
                    .frame
                    .get_or_insert_with(Frame::default)
                    .view = Default::default();
                self.commit_video(next);
                ui.close();
            }
            if crate::controls::menu_action(ui, "Hide picture for this segment").clicked() {
                let mut next = original.clone();
                next.segments[index].hidden = true;
                self.commit_video(next);
                ui.close();
            }
            if original.segments[index].link.is_some()
                && crate::controls::menu_action(ui, "Unlink from narration").clicked()
            {
                let mut next = original.clone();
                next.unlink(index);
                self.commit_video(next);
                ui.close();
            }
        });

        let double_clicked = ui.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
        let center = t.screen_point(
            a.rotation()
                .point([(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5]),
        );
        let crop_grip =
            self.video.selected == Selection::Recording(index) && center.distance(pos) <= 12.;
        if double_clicked || (crop_grip && ui.input(|i| i.pointer.primary_pressed())) {
            self.video.frame_crop = true;
        }
        if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
            if !double_clicked && !crop_grip && self.video.selected != Selection::Recording(index) {
                self.video.frame_crop = false;
            }
            self.video.pause();
            self.select_only(None);
            self.video.selected = Selection::Recording(index);
            self.video.tab = PropertyTab::Style;
            let crop = self.video.frame_crop;
            let wanted = !crop
                && handle.is_none()
                && ui.input(|i| {
                    self.gesture_matches(crate::gesture_input::Gesture::DuplicateDrag, i.modifiers)
                });
            let copy = wanted.then(|| duplicate_clip(&original, index)).flatten();
            let source = index;
            let (original, index, duplicate) = match copy {
                Some((project, copied)) => (project, copied, true),
                None => (original, index, false),
            };
            self.video.frame_drag = Some(Drag {
                original,
                index,
                from: point,
                handle,
                crop,
                duplicate,
                source,
            });
        }
        ui.ctx().set_cursor_icon(if self.video.frame_crop {
            egui::CursorIcon::Grab
        } else {
            handle.map_or(egui::CursorIcon::Move, crate::canvas_cursor::handle_cursor)
        });
        true
    }
    fn video_frame_zoom_buttons(
        &mut self,
        ui: &egui::Ui,
        t: crate::canvas_view::Transform,
        enabled: bool,
    ) -> bool {
        let Selection::Recording(index) = self.video.selected else {
            return false;
        };
        let Some(p) = self.session.document.video.as_ref() else {
            return false;
        };
        let Some(c) = p.segments.get(index).filter(|c| {
            c.active(p.cursor)
                && !c.hidden
                && c.kind() != capture_core::video::editor::MediaKind::Audio
        }) else {
            return false;
        };
        let t = self.video_layer_transform(t, Layer::Media(c.track));
        let a = geometry(
            &c.keyed(p.cursor),
            [self.session.document.width, self.session.document.height],
        );
        let mut hit = false;
        let mut factor = None;
        let owner_id = egui::Id::unique(("video-frame-zoom-press", c.id));
        let mut pressed = ui.ctx().data_mut(|data| data.get_temp::<usize>(owner_id));
        let (pointer, down, released) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.pointer.primary_pressed(),
                input.pointer.primary_released(),
            )
        });
        for (i, r) in zoom_buttons(&a, t).into_iter().enumerate() {
            let r = r.intersect(t.viewport).intersect(t.image);
            #[cfg(test)]
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(("video-frame-zoom-rect", i)), r));
            let response = ui
                .interact(
                    r,
                    egui::Id::unique(("video-frame-zoom", i)),
                    egui::Sense::click(),
                )
                .tip(if i == 0 {
                    "Zoom footage out"
                } else {
                    "Zoom footage in"
                });
            // Geometry owns the entire press/release, even before egui has
            // resolved the previous frame's overlapping canvas hit targets.
            hit |= ui.input(|input| input.pointer.interact_pos().is_some_and(|p| r.contains(p)))
                || response.is_pointer_button_down_on()
                || response.clicked();
            if enabled && down && pointer.is_some_and(|p| r.contains(p)) {
                pressed = Some(i);
                ui.ctx().data_mut(|data| data.insert_temp(owner_id, i));
            }
            hit |= pressed.is_some();
            if enabled && released && pressed == Some(i) && pointer.is_some_and(|p| r.contains(p)) {
                factor = Some(if i == 0 { 1. / 1.2 } else { 1.2 });
            }
        }
        if released || !enabled {
            ui.ctx().data_mut(|data| data.remove::<usize>(owner_id));
        }
        if let Some(factor) = factor {
            let mut next = p.clone();
            let base = next.segments[index].clone();
            next.segments[index] = base.keyed(p.cursor);
            let f = next.segments[index]
                .frame
                .get_or_insert_with(Frame::default);
            if let Ok(view) = f.view.zoomed(factor) {
                f.view = view;
                next.segments[index] = base.keyframes.clone().edit_media(
                    &base,
                    &next.segments[index],
                    p.cursor.saturating_sub(base.at),
                    self.video.keyframes.auto_key,
                );
                self.commit_video(next);
            }
        }
        hit
    }
    pub(super) fn paint_video_frame_handles(
        &self,
        ui: &egui::Ui,
        t: crate::canvas_view::Transform,
    ) {
        let p = self
            .video
            .timing_preview
            .as_ref()
            .or(self.session.document.video.as_ref());
        let (Some(p), Selection::Recording(i)) = (p, self.video.selected) else {
            return;
        };
        let Some(c) = p.segments.get(i) else { return };
        if c.kind() == capture_core::video::editor::MediaKind::Audio
            || !c.active(p.cursor)
            || c.hidden
        {
            return;
        }
        let t = self.video_layer_transform(t, Layer::Media(c.track));
        let a = geometry(
            &c.keyed(p.cursor),
            [self.session.document.width, self.session.document.height],
        );
        let painter = ui.painter().with_clip_rect(t.viewport.intersect(t.image));
        let points: Vec<_> = a
            .frame_boundary()
            .into_iter()
            .map(|p| t.screen_point(a.rotation().point(p)))
            .collect();
        if points.len() > 1 {
            let mut points = points;
            points.push(points[0]);
            painter.add(egui::Shape::line(
                points,
                Stroke::new(1.5, crate::shell::ACCENT),
            ));
        }
        if !self.video.frame_crop {
            for (handle, point) in a.handles() {
                if handle.is_radius() {
                    continue;
                }
                painter.circle_filled(t.screen_point(point), 4., Color32::WHITE);
            }
        }
        for (i, r) in zoom_buttons(&a, t).into_iter().enumerate() {
            painter.rect_filled(r, 4., Color32::from_rgb(36, 49, 55));
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                if i == 0 { "−" } else { "+" },
                egui::FontId::proportional(18.),
                Color32::WHITE,
            );
        }
        let grip = egui::Rect::from_center_size(
            t.screen_point(
                a.rotation()
                    .point([(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5]),
            ),
            egui::vec2(24., 24.),
        );
        let response = ui
            .interact(
                grip,
                egui::Id::unique("video-pan-handle"),
                egui::Sense::hover(),
            )
            .on_hover_cursor(egui::CursorIcon::Grab)
            .tip("Drag to reposition footage inside its frame");
        crate::image_frame::paint_pan_handle(&painter, grip, response.hovered());
    }
}
fn geometry(c: &capture_core::video::editor::Segment, size: [u32; 2]) -> capture_core::Annotation {
    let mut style = c.frame.clone().unwrap_or(Frame {
        radius: 0.,
        ..Default::default()
    });
    style.fill = true;
    style.view = Default::default();
    style
        .annotation(
            Arc::new(image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))),
            c.rect,
            size,
            c.id,
        )
        .expect("validated frame geometry")
}

fn zoom_buttons(a: &capture_core::Annotation, t: crate::canvas_view::Transform) -> [egui::Rect; 2] {
    let (_, hi) = a.body_bounds();
    let at = t.screen_point(a.rotation().point(hi)) - egui::vec2(4., 4.);
    [
        egui::Rect::from_min_max(at - egui::vec2(54., 24.), at - egui::vec2(28., 0.)),
        egui::Rect::from_min_max(at - egui::vec2(26., 24.), at),
    ]
}
