use crate::{Annotation, Command, Editor, Kind, egui};
use capture_core::selection::Handle;

pub(super) struct EditDrag {
    image_content: bool,
    image_crop: bool,
    original: Annotation,
    from: [f32; 2],
    pub(super) handle: Option<Handle>,
    pub preview: Annotation,
}

fn drawing_shift(ui: &egui::Ui) -> bool {
    ui.input(|i| {
        i.events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::PointerButton {
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                    ..
                } => Some(modifiers.shift),
                _ => None,
            })
            .unwrap_or(i.modifiers.shift)
    })
}

impl Editor {
    pub(super) fn cancel_gesture(&mut self) {
        crate::native_cursor::reset();
        self.icon_library.cancel_drag();
        self.marquee = None;
        self.quick_controls.format_source = None;
        self.quick_controls.format_locked = false;
        self.snapping_guides = [None; 2];
        self.crop = None;
        self.cut_out = None;

        let property = self.property_preview.take().is_some();
        if self.guide_drag.take().is_some()
            | self.rotation_drag.take().is_some()
            | self.edit_drag.take().is_some()
            | self.group_drag.take().is_some()
            | self.step_placement.take().is_some()
            || property
        {
            self.needs_render = true;
        }
        if self.drag_start.is_some() && self.tool == Some(Kind::Eraser) {
            self.needs_render = true;
        }
        self.drag_start = None;
        self.creation = Default::default();
        self.stroke_points.clear();
        self.view.panning = false;
    }

    pub(super) fn tool_matches(tool: Option<Kind>, kind: Kind) -> bool {
        match tool {
            None => true,
            Some(Kind::Rectangle | Kind::Ellipse | Kind::Polygon | Kind::Callout) => {
                matches!(
                    kind,
                    Kind::Rectangle | Kind::Ellipse | Kind::Polygon | Kind::Callout
                )
            }
            Some(Kind::Blur | Kind::Pixelate | Kind::Redact) => {
                matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Redact)
            }
            Some(Kind::Arrow | Kind::Line) => matches!(kind, Kind::Arrow | Kind::Line),
            Some(Kind::Pen | Kind::Highlighter) => matches!(kind, Kind::Pen | Kind::Highlighter),
            Some(active) => kind == active,
        }
    }

    pub(super) fn tool_hit(&self, point: [f32; 2], tolerance: f32) -> Option<u64> {
        let mut candidates: Vec<_> = self.session.document.annotations.iter().rev().collect();
        if let Some(p) = &self.session.document.video {
            // Topmost timeline track wins, then the last object within it.
            candidates.sort_by_key(|a| {
                p.objects.get(&a.id).map_or(usize::MAX, |t| {
                    p.layer_index(capture_core::video::links::Layer::Annotation(t.track))
                })
            });
        }
        candidates
            .into_iter()
            .find(|a| {
                self.session.document.object_editable(a.id)
                    && self.video_object_active(a.id)
                    && (!self.icon_library.active || a.image.as_ref().is_some_and(|b| b.is_icon()))
                    && (self.quick_controls.auto_switch_tool()
                        || self.editing_group.is_some()
                        || Self::tool_matches(self.tool, a.kind))
                    && {
                        let (point, tolerance) = self.video_object_point(a.id, point, tolerance);
                        self.keyframe_annotation(a).hit_test(point, tolerance)
                    }
            })
            .map(|a| a.id)
    }

    pub(super) fn selected_annotation(&self) -> Option<Annotation> {
        if self
            .selected
            .is_some_and(|id| self.session.document.object_state(id).locked)
        {
            return None;
        }
        self.rotation_drag
            .as_ref()
            .map(|drag| drag.preview.clone())
            .or_else(|| self.step_placement.clone())
            .or_else(|| {
                self.text_draft
                    .as_ref()
                    .map(|draft| draft.preview.clone())
                    .or_else(|| self.edit_drag.as_ref().map(|drag| drag.preview.clone()))
                    .or_else(|| self.property_preview.as_ref().map(|p| p.preview.clone()))
                    .or_else(|| {
                        self.session
                            .document
                            .annotations
                            .iter()
                            .find(|a| Some(a.id) == self.selected)
                            .map(|a| self.keyframe_annotation(a))
                    })
            })
    }

    pub(super) fn show_canvas(&mut self, ui: &mut egui::Ui) {
        self.snapping_guides = [None; 2];
        let dimensions = egui::vec2(
            self.session.document.width as f32,
            self.session.document.height as f32,
        );
        let mut viewport = ui.available_rect_before_wrap();
        if self.crop.is_some() {
            // Keep drawing, navigation and crop handles above the dock. The
            // parent UI still owns the full area for the controls below it.
            viewport.max.y = (viewport.max.y - 216.).max(viewport.min.y + 32.);
        }
        // Reserve workspace gutters for guide section actions; never place
        // those buttons over the screenshot or include them in its export.
        let image_viewport = if self.session.document.guide.is_some() {
            viewport.shrink2(egui::vec2(34., 40.))
        } else {
            viewport
        };
        let pointer_on_canvas_layer = ui
            .input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos()))
            .is_none_or(|p| {
                ui.ctx()
                    .layer_id_at(p)
                    .is_none_or(|layer| layer == ui.layer_id())
            });
        let navigation_enabled = pointer_on_canvas_layer
            && !self.video.export_dialog
            && !self.video.gif.open
            && self.video.still_frame.is_none()
            && self.pending.is_none()
            && self.new_image.is_none()
            && self.pdf_import.is_none()
            && self.rotate_image.is_none()
            && self.resize_image.is_none()
            && self.resize_canvas.is_none()
            && self.image_transform.is_none()
            && self.shell.canvas_enabled()
            && !self.shell.help
            && self.capture_result.is_none()
            && self.property_preview.is_none()
            && !egui::Popup::is_any_open(ui.ctx())
            && ui.is_enabled();
        let mut interactive =
            navigation_enabled && self.text_draft.is_none() && !self.shell.raster_preview;
        let mut transform =
            self.view
                .transform(image_viewport, dimensions, ui.ctx().pixels_per_point());
        transform.viewport = viewport;
        interactive &= !self.image_frame_buttons_hovered(ui, transform)
            && !self.guide_controls_hovered(ui, self.guide_transform(transform))
            && !crate::perspective_ui::corner_hovered(
                ui.ctx(),
                ui.input(|i| i.pointer.interact_pos().or(i.pointer.hover_pos())),
            );
        let response = ui.interact(
            viewport,
            ui.scope_id().with("canvas"),
            egui::Sense::click_and_drag(),
        );
        if interactive
            && self.is_video()
            && response.hovered()
            && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
        {
            self.video.pause();
        }
        interactive &= !self.zoom_canvas_input(ui, transform, interactive);
        interactive &= !self.magnified_input(ui, transform, interactive);
        interactive &= !self.video_frame_input(ui, transform, interactive);
        self.frame_context(ui, &response, transform);
        let gesture_idle = self.guide_drag.is_none()
            && self.rotation_drag.is_none()
            && self.group_drag.is_none()
            && self.step_placement.is_none()
            && self.edit_drag.is_none()
            && self.marquee.is_none()
            && self.drag_start.is_none()
            && !self.crop.as_ref().is_some_and(|crop| crop.dragging())
            && !self.cut_out.as_ref().is_some_and(|s| s.dragging());
        let stroke_wheel = self.stroke_wheel(
            ui,
            response.hovered() || self.drag_start.is_some(),
            interactive
                && !ui.ctx().egui_wants_keyboard_input()
                && !self.view.panning
                && self.edit_drag.is_none()
                && self.group_drag.is_none()
                && self.rotation_drag.is_none(),
        );
        if navigation_enabled && gesture_idle {
            if viewport.contains(
                ui.input(|i| i.pointer.hover_pos())
                    .unwrap_or(egui::Pos2::ZERO),
            ) && !stroke_wheel
            {
                let wheel = self.gesture_wheel(ui, crate::gesture_input::Gesture::CanvasZoom);
                let (pinch, scroll, pointer) = ui.input(|i| {
                    (
                        i.events
                            .iter()
                            .filter_map(|e| {
                                if let egui::Event::Zoom(z) = e {
                                    Some(*z)
                                } else {
                                    None
                                }
                            })
                            .product::<f32>(),
                        i.smooth_scroll_delta,
                        i.pointer.hover_pos(),
                    )
                });
                let zoom_delta = pinch * (wheel * 0.0025).exp();
                if zoom_delta != 1.0 {
                    self.view.apply(
                        crate::canvas_view::ZoomAction::Set(crate::canvas_view::ZoomMode::Manual(
                            transform.zoom * zoom_delta,
                        )),
                        pointer,
                    );
                } else if self.text_draft.is_none() && scroll != egui::Vec2::ZERO {
                    self.view.pan_by(scroll);
                    ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                }
            }
            // TextEdit owns the hovered response during inline entry. Middle
            // navigation belongs to the canvas beneath it, independently of
            // primary-button editing and keyboard focus.
            let middle_pressed = ui.input(|i| {
                i.pointer.button_pressed(egui::PointerButton::Middle)
                    && i.pointer
                        .interact_pos()
                        .is_some_and(|p| viewport.contains(p))
                    && i.focused
            });
            if middle_pressed {
                self.view.panning = true;
                self.view.pan_button = egui::PointerButton::Middle;
            } else if interactive
                && response.drag_started_by(egui::PointerButton::Primary)
                && ui.input(|i| i.key_down(egui::Key::Space))
            {
                self.view.panning = true;
                self.view.pan_button = egui::PointerButton::Primary;
            }
            if self.view.panning {
                let delta = if middle_pressed {
                    egui::Vec2::ZERO
                } else {
                    ui.input(|i| i.pointer.delta())
                };
                if self.view.pan_button == egui::PointerButton::Middle {
                    self.view.pan_freely_by(delta);
                } else {
                    self.view.pan_by(delta);
                }
                if !ui.input(|i| i.pointer.button_down(self.view.pan_button) && i.focused) {
                    self.view.panning = false;
                }
            }
            transform =
                self.view
                    .transform(image_viewport, dimensions, ui.ctx().pixels_per_point());
            transform.viewport = viewport;
        } else {
            self.view.panning = false;
        }
        transform = self.guide_transform(transform);
        let scale = transform.scale;
        let display_transform = self.expansion_transform(transform);
        let canvas_rect = display_transform.image;
        let visible = viewport.intersect(canvas_rect);
        // Transparency preview belongs only to the workspace, never to the
        // document renderer or exported pixels.
        let painter = ui.painter().with_clip_rect(visible);
        painter.rect_filled(canvas_rect, 0.0, egui::Color32::from_gray(205));
        let first = (visible.min - canvas_rect.min) / 16.0;
        let last = (visible.max - canvas_rect.min) / 16.0;
        for row in first.y.floor() as u32..last.y.ceil() as u32 {
            for column in first.x.floor() as u32..last.x.ceil() as u32 {
                if (row + column) % 2 == 0 {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            canvas_rect.min + egui::vec2(column as f32 * 16.0, row as f32 * 16.0),
                            egui::vec2(16.0, 16.0),
                        ),
                        0.0,
                        egui::Color32::from_gray(240),
                    );
                }
            }
        }
        if self.session.document.canvas_behavior.allow_overflow {
            self.paint_page_surrounds(ui, display_transform);
        }
        self.paint_canvas_display(ui, display_transform);
        if !self.session.document.canvas_behavior.allow_overflow {
            self.paint_page_surrounds(ui, display_transform);
        }
        self.paint_crop_overflow(ui, display_transform);
        if interactive && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_gesture();
            self.icon_library.active = false;
            self.tool = None;
            self.select_only(None);
            ui.ctx().request_repaint();
            return;
        }
        // Empty workspace is a universal deselection target. Keep the current
        // tool armed, and leave image-only tool gestures inside the image.
        if interactive
            && response.clicked()
            && response.interact_pointer_pos().is_some_and(|p| {
                !visible.contains(p)
                    && !(self.session.document.canvas_behavior.allow_overflow
                        && self
                            .session
                            .document
                            .hit_test(transform.image_point(p), 7.0 / scale)
                            .is_some())
            })
        {
            self.select_only(None);
            return;
        }
        if self.sample_canvas_color(ui, display_transform) {
            return;
        }
        if self.shell.pixel_grid {
            crate::pixel_grid::paint(
                ui,
                transform,
                [self.session.document.width, self.session.document.height],
            );
        }
        if self.shell.annotations_hidden || self.shell.raster_preview {
            if self.view.panning {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if response.hovered() && ui.input(|i| i.key_down(egui::Key::Space)) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            return;
        }
        if self.cut_out.is_some() {
            if let Some(texture) = self.ensure_raster_texture(ui.ctx()) {
                self.cut_out_canvas(ui, &response, transform, interactive, texture);
            }
            self.canvas_pointer(ui, &response, transform, transform, interactive);
            return;
        }

        if self.crop.is_some() {
            self.crop_canvas(ui, &response, transform, interactive);
            self.canvas_pointer(ui, &response, transform, transform, interactive);
            return;
        }
        let base_transform = transform;
        // Resolve a drag's coordinate system at its press origin, before the
        // pointer can leave a small animated object during egui's threshold.
        let transform = self.video_annotation_transform(
            transform,
            ui.input(|i| {
                if i.pointer.primary_down() {
                    i.pointer.press_origin().or(i.pointer.interact_pos())
                } else {
                    i.pointer.interact_pos()
                }
            }),
        );
        let scale = transform.scale;
        if self.redact_placement_input(ui, &response, transform, interactive) {
            return;
        }
        if self.icon_placement_input(ui, &response, transform, interactive) {
            return;
        }
        let selected_handle = |position: egui::Pos2| {
            self.selected_annotation()
                .filter(|a| {
                    self.session.document.object_editable(a.id)
                        && Self::tool_matches(self.tool, a.kind)
                })
                .filter(|a| {
                    a.hit_handle(transform.image_point(position), 7. / transform.scale)
                        .is_some()
                })
                .map(|a| a.id)
        };
        let near_handle = ui
            .input(|i| i.pointer.interact_pos())
            .and_then(selected_handle)
            .is_some();
        let interaction_area = if self.tool.is_none() || near_handle {
            viewport
        } else {
            visible
        };
        let point = |p: egui::Pos2| transform.image_point(p);
        let screen = |p: [f32; 2]| transform.screen_point(p);
        let painter = ui.painter().with_clip_rect(viewport);
        let canvas = [self.session.document.width, self.session.document.height];
        let tolerance = 7.0 / scale;
        // A quick drag can arrive entirely between frames while the previous
        // composite renders. egui then has no held press/drag state to retain.
        let completed_drag = interactive
            .then(|| {
                ui.input(|i| {
                    let mut start = None;
                    let mut completed = None;
                    for event in &i.events {
                        if let egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            ..
                        } = event
                        {
                            if *pressed {
                                start = interaction_area.contains(*pos).then_some(*pos);
                            } else if let Some(from) = start.take()
                                && viewport.contains(*pos)
                                && from.distance(*pos) > 6.0
                            {
                                completed = Some((from, *pos));
                            }
                        }
                    }
                    completed
                })
            })
            .flatten();
        let escaped = interactive && ui.input(|i| i.key_pressed(egui::Key::Escape));
        if escaped {
            self.cancel_gesture();
            ui.ctx().request_repaint();
        }
        let painting = self.quick_controls.format_source.is_some() && interactive && !escaped;
        if painting
            && response.clicked()
            && let Some(p) = response
                .interact_pointer_pos()
                .filter(|p| interaction_area.contains(*p))
            && let Some(id) =
                self.tool_hit(base_transform.image_point(p), 7. / base_transform.scale)
        {
            self.paint_format_to(id);
        }
        let group_transform =
            self.group_transform_input(ui, transform, interactive && !escaped && !painting);
        let rotating = group_transform
            || self.rotation_input(ui, transform, interactive && !escaped && !painting);
        let step_handled = !rotating
            && !painting
            && interactive
            && !escaped
            && self.step_placement_input(ui, &response, transform, base_transform);
        if interactive
            && !escaped
            && !rotating
            && !painting
            && !step_handled
            && !self.view.panning
            && !ui.input(|i| i.key_down(egui::Key::Space))
        {
            let mut entered_group = false;
            if response.clicked()
                && completed_drag.is_none()
                && let Some(p) = response
                    .interact_pointer_pos()
                    .filter(|p| interaction_area.contains(*p))
            {
                let additive = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                let hit = self
                    .selected_annotation()
                    .filter(|a| {
                        self.session.document.object_editable(a.id)
                            && Self::tool_matches(self.tool, a.kind)
                            && a.hit_handle(point(p), 7. / scale).is_some()
                    })
                    .map(|a| a.id)
                    .or_else(|| {
                        self.tool_hit(base_transform.image_point(p), 7. / base_transform.scale)
                    });
                entered_group = response.double_clicked()
                    && !additive
                    && hit.is_some_and(|id| self.enter_group_child(id));
                if !entered_group {
                    self.click_selection(hit, additive);
                }
                if self
                    .selected_annotation()
                    .is_some_and(|a| a.is_image_frame())
                {
                    ui.ctx().data_mut(|d| {
                        d.remove::<u64>(egui::Id::unique("frame-content-mode"));
                    });
                }
                self.auto_activate_selected_tool();
                if self.selected.is_none() && !additive && self.tool == Some(Kind::Eraser) {
                    let p = point(p);
                    let annotation = self.eraser_annotation(vec![p, p]);
                    let id = annotation.id;
                    self.execute(Command::Add { annotation });
                    self.select_only(Some(id));
                }
            }
            if response.double_clicked() {
                if let Some(a) = self.selected_annotation().filter(|a| a.is_image_frame()) {
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(egui::Id::unique("frame-content-mode"), a.id));
                    self.shell.property_tab = crate::shell::PropertyTab::Tool;
                } else if !entered_group
                    && self.selected_annotation().is_some_and(|a| a.text.is_some())
                {
                    self.begin_text_edit();
                } else if entered_group || self.tool.is_none() {
                    self.activate_selected_tool();
                    // Text editing is explicit on a subsequent double click;
                    // entering a group first exposes the child's own handles.
                } else {
                    self.begin_text_edit();
                }
            }
            if response.drag_started_by(egui::PointerButton::Primary) || completed_drag.is_some() {
                // The press origin is stable even after egui's drag threshold.
                if let Some(origin) = completed_drag
                    .map(|(from, _)| from)
                    .or_else(|| {
                        ui.input(|i| {
                            i.pointer.press_origin().or_else(|| {
                                i.events.iter().find_map(|event| match event {
                                    egui::Event::PointerButton {
                                        pos,
                                        button: egui::PointerButton::Primary,
                                        pressed: true,
                                        ..
                                    } => Some(*pos),
                                    _ => None,
                                })
                            })
                        })
                    })
                    .filter(|p| interaction_area.contains(*p))
                    .map(point)
                {
                    let duplicate = ui.input(|i| {
                        i.events
                            .iter()
                            .find_map(|e| match e {
                                egui::Event::PointerButton {
                                    button: egui::PointerButton::Primary,
                                    pressed: true,
                                    modifiers,
                                    ..
                                } => Some(self.gesture_matches(
                                    crate::gesture_input::Gesture::DuplicateDrag,
                                    *modifiers,
                                )),
                                _ => None,
                            })
                            .unwrap_or_else(|| {
                                self.gesture_matches(
                                    crate::gesture_input::Gesture::DuplicateDrag,
                                    i.modifiers,
                                )
                            })
                    });
                    let selected = self
                        .selected_annotation()
                        .filter(|a| self.session.document.object_editable(a.id))
                        .filter(|a| duplicate || Self::tool_matches(self.tool, a.kind));
                    let multiple = self.selected_objects().len() > 1;
                    let handle = selected
                        .as_ref()
                        .filter(|_| !multiple)
                        .and_then(|a| {
                            a.hit_handle(
                                origin,
                                if a.kind == Kind::Callout {
                                    9. / scale
                                } else {
                                    tolerance
                                },
                            )
                        })
                        .filter(|handle| !duplicate || *handle != Handle::Bend);
                    let hit = if duplicate {
                        self.session
                            .document
                            .annotations
                            .iter()
                            .rev()
                            .find(|a| {
                                let (local, tolerance) = self.video_object_point(
                                    a.id,
                                    base_transform.image_point(transform.screen_point(origin)),
                                    7. / base_transform.scale,
                                );
                                self.video_object_active(a.id) && a.hit_test(local, tolerance)
                            })
                            .map(|a| a.id)
                            .or_else(|| handle.and_then(|_| selected.as_ref().map(|a| a.id)))
                    } else {
                        self.tool_hit(
                            base_transform.image_point(transform.screen_point(origin)),
                            7. / base_transform.scale,
                        )
                    };
                    if self.tool.is_none() && hit.is_none() && handle.is_none() {
                        self.marquee = Some(crate::move_tool::Marquee {
                            from: origin,
                            to: origin,
                            additive: ui.input(|i| i.modifiers.shift || i.modifiers.command),
                        });
                    } else if duplicate && handle.is_none() {
                        if let Some(hit) = hit {
                            if !self.is_selected(hit) {
                                self.select_only(Some(hit));
                            }
                            let ids = self
                                .selected_objects()
                                .iter()
                                .map(|a| a.id)
                                .collect::<Vec<_>>();
                            match self.session.document.duplicate_selection(&ids) {
                                Ok(copies) => {
                                    self.group_drag = Some(crate::multi_select::GroupDrag {
                                        transform: None,
                                        preview: copies.clone(),
                                        originals: copies,
                                        from: origin,
                                        duplicate: true,
                                    });
                                    self.needs_render = true;
                                }
                                Err(error) => self.fail(error),
                            }
                        }
                    } else if self.tool.is_none() || handle.is_some() || hit.is_some() {
                        if handle.is_none() && !hit.is_some_and(|id| self.is_selected(id)) {
                            let additive = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                            self.click_selection(hit, additive);
                            self.auto_activate_selected_tool();
                        }
                        let objects = self.selected_objects();
                        if objects.len() > 1 && hit.is_some() {
                            self.group_drag = Some(crate::multi_select::GroupDrag {
                                transform: None,
                                preview: objects.clone(),
                                originals: objects,
                                from: origin,
                                duplicate: false,
                            });
                        } else if let Some(original) = self.selected_annotation() {
                            let cropping = ui.input(|i| i.modifiers.ctrl)
                                || crate::image_frame::crop_mode(ui.ctx(), original.id);
                            let pan_handle = self
                                .image_pan_handle(transform)
                                .is_some_and(|r| r.contains(transform.screen_point(origin)));
                            self.edit_drag = Some(EditDrag {
                                image_crop: handle.is_some_and(|h| {
                                    matches!(
                                        h,
                                        Handle::Left
                                            | Handle::Right
                                            | Handle::Top
                                            | Handle::Bottom
                                            | Handle::TopLeft
                                            | Handle::TopRight
                                            | Handle::BottomLeft
                                            | Handle::BottomRight
                                    )
                                }) && (original.kind == Kind::Image
                                    || original.is_image_frame())
                                    && original.image.as_ref().is_some_and(|b| !b.is_cursor())
                                    && cropping,
                                image_content: handle.is_none()
                                    && (original.kind == Kind::Image || original.is_image_frame())
                                    && original.image.as_ref().is_some_and(|b| !b.is_cursor())
                                    && (cropping || pan_handle),
                                preview: original.clone(),
                                original,
                                from: origin,
                                handle,
                            });
                        }
                    } else if self.tool != Some(Kind::Step) {
                        let origin = [
                            origin[0].clamp(0.0, dimensions.x),
                            origin[1].clamp(0.0, dimensions.y),
                        ];
                        self.drag_start = Some(origin);
                        self.creation = Default::default();
                        self.stroke_points.clear();
                        if (matches!(
                            self.tool,
                            Some(Kind::Pen | Kind::Highlighter | Kind::Eraser)
                        ) || self.drawing_freehand_region())
                            && (self.tool != Some(Kind::Highlighter)
                                || !self.drawing_rectangle_highlighter())
                        {
                            self.stroke_points.push(origin);
                        }
                    }
                }
            }
            if let Some(pointer) = completed_drag
                .map(|(_, to)| to)
                .or_else(|| response.interact_pointer_pos())
                .map(point)
            {
                if let Some(m) = &mut self.marquee {
                    m.to = pointer;
                    ui.ctx().request_repaint();
                }
                if (self.tool != Some(Kind::Highlighter) || !self.drawing_rectangle_highlighter())
                    && self.drag_start.is_some()
                    && (matches!(
                        self.tool,
                        Some(Kind::Pen | Kind::Highlighter | Kind::Eraser)
                    ) || self.drawing_freehand_region())
                {
                    let p = [
                        pointer[0].clamp(0.0, dimensions.x),
                        pointer[1].clamp(0.0, dimensions.y),
                    ];
                    if self
                        .stroke_points
                        .last()
                        .is_none_or(|last| (p[0] - last[0]).hypot(p[1] - last[1]) >= 0.5)
                        && self.stroke_points.len() < 8192
                    {
                        self.stroke_points.push(p);
                        if self.tool == Some(Kind::Eraser) {
                            self.needs_render = true;
                            ui.ctx().request_repaint();
                        }
                    }
                }
                if let (Some(start), Some(kind)) = (self.drag_start, self.tool) {
                    let alt = ui.input(|i| {
                        i.events
                            .iter()
                            .rev()
                            .find_map(|event| match event {
                                egui::Event::PointerButton {
                                    button: egui::PointerButton::Primary,
                                    pressed: false,
                                    modifiers,
                                    ..
                                } => Some(modifiers.alt),
                                _ => None,
                            })
                            .unwrap_or(i.modifiers.alt)
                    });
                    self.creation.bypass =
                        ui.input(|i| !self.quick_controls.snap_when(|w| w.snap_draw, i.modifiers));
                    self.update_creation(start, pointer, kind, drawing_shift(ui), alt);
                }
                let motion_scale = self
                    .selected
                    .map_or(1., |id| self.video_object_motion(id).1);
                let move_tolerance = ui
                    .input(|i| self.quick_controls.snap_when(|w| w.snap_move, i.modifiers))
                    .then_some(6.0 * motion_scale / scale);
                let resize_tolerance = ui
                    .input(|i| {
                        self.quick_controls
                            .snap_when(|w| w.snap_resize, i.modifiers)
                    })
                    .then_some(6.0 / scale);
                let grid = self.quick_controls.snap_grid();
                if let Some(drag) = &mut self.group_drag {
                    let result = if let Some(p) = self
                        .session
                        .document
                        .video
                        .as_ref()
                        .filter(|p| !p.zooms.is_empty())
                    {
                        let delta = [
                            (pointer[0] - drag.from[0]) * transform.scale / base_transform.scale,
                            (pointer[1] - drag.from[1]) * transform.scale / base_transform.scale,
                        ];
                        drag.originals
                            .iter()
                            .map(|a| {
                                let track = p.objects.get(&a.id).map_or_else(
                                    || {
                                        p.annotation_track
                                            .or_else(|| p.tracks.first().copied())
                                            .unwrap_or(0)
                                    },
                                    |t| t.track,
                                );
                                let view = p.layer_view(
                                    capture_core::video::links::Layer::Annotation(track),
                                    canvas,
                                    p.cursor,
                                );
                                a.translated([delta[0] / view.scale, delta[1] / view.scale], canvas)
                            })
                            .collect::<capture_core::Result<Vec<_>>>()
                    } else {
                        crate::canvas_expansion::move_with_candidates(
                            &self.session.document,
                            &drag.originals,
                            [
                                (pointer[0] - drag.from[0]) * motion_scale,
                                (pointer[1] - drag.from[1]) * motion_scale,
                            ],
                            move_tolerance,
                            drag.duplicate,
                            self.quick_controls.snap_candidates(),
                        )
                        .and_then(|result| {
                            capture_core::snapping::settled(result, canvas, grid, move_tolerance)
                        })
                        .map(|result| {
                            self.snapping_guides = result.guides;
                            result.annotations
                        })
                    };
                    match result {
                        Ok(preview) if preview != drag.preview => {
                            drag.preview = preview;
                            self.needs_render = true;
                            ui.ctx().request_repaint();
                        }
                        Err(error) => self.fail(error),
                        _ => {}
                    }
                }
                if let Some(drag) = &mut self.edit_drag {
                    let preview = if drag.image_crop {
                        drag.original
                            .crop_image_handle(drag.handle.unwrap(), pointer)
                    } else if drag.image_content {
                        drag.original.pan_image_by_drag(drag.from, pointer)
                    } else {
                        match drag.handle {
                            Some(handle) if handle.is_radius() => {
                                drag.original.rounded_by_drag(handle, drag.from, pointer)
                            }
                            Some(handle) => {
                                let exclude = [drag.original.id];
                                let (snapped, guides) =
                                    resize_tolerance.map_or((pointer, [None; 2]), |tolerance| {
                                        capture_core::snapping::point(
                                            pointer,
                                            capture_core::snapping::Targets {
                                                all: &self.session.document.annotations,
                                                exclude: &exclude,
                                                canvas,
                                                grid,
                                            },
                                            tolerance,
                                        )
                                    });
                                self.snapping_guides = guides;
                                drag.original
                                    .resized_with_modifiers(
                                        handle,
                                        snapped,
                                        canvas,
                                        ui.input(|i| {
                                            capture_core::resize_modifiers::ResizeModifiers {
                                                constrain: i.modifiers.shift,
                                                from_center: i.modifiers.alt,
                                            }
                                        }),
                                    )
                                    .map(|a| capture_core::steps::label_follows(&drag.original, a))
                            }
                            None => crate::canvas_expansion::move_with_candidates(
                                &self.session.document,
                                std::slice::from_ref(&drag.original),
                                [
                                    (pointer[0] - drag.from[0]) * motion_scale,
                                    (pointer[1] - drag.from[1]) * motion_scale,
                                ],
                                move_tolerance,
                                false,
                                self.quick_controls.snap_candidates(),
                            )
                            .and_then(|result| {
                                capture_core::snapping::settled(
                                    result,
                                    canvas,
                                    grid,
                                    move_tolerance,
                                )
                            })
                            .map(|mut result| {
                                self.snapping_guides = result.guides;
                                result.annotations.remove(0)
                            }),
                        }
                    };
                    match preview.and_then(|a| {
                        if drag.handle.is_some() {
                            a.fit_text_shape(canvas)
                        } else {
                            Ok(a)
                        }
                    }) {
                        Ok(preview) if preview != drag.preview => {
                            drag.preview = preview;
                            self.needs_render = true;
                            ui.ctx().request_repaint();
                        }
                        Err(error) => self.fail(error),
                        _ => {}
                    }
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    || completed_drag.is_some()
                {
                    let before_commit_size =
                        [self.session.document.width, self.session.document.height];
                    let before_commit_image = self.view.current_transform().map(|t| t.image);
                    self.finish_marquee();
                    if let Some(drag) = self.group_drag.take() {
                        if drag.duplicate {
                            let ids = drag.preview.iter().map(|a| a.id).collect::<Vec<_>>();
                            self.commit_expanding_move(drag.preview, true, transform);
                            if ids.iter().all(|id| {
                                self.session
                                    .document
                                    .annotations
                                    .iter()
                                    .any(|a| a.id == *id)
                            }) {
                                self.select_only(None);
                                for id in ids {
                                    if !self.is_selected(id) {
                                        self.click_selection(Some(id), true);
                                    }
                                }
                                self.sync_step_sequence();
                                self.status =
                                    "Duplicated selection. Undo removes all copies from this drag."
                                        .into();
                            }
                        } else if drag.preview != drag.originals {
                            self.commit_expanding_move(drag.preview, false, transform);
                            self.status = "Selection moved. Undo reverses the whole drag.".into();
                        }
                        self.needs_render = true;
                    }
                    if let Some(drag) = self.edit_drag.take() {
                        if drag.preview != drag.original {
                            if drag.handle.is_none() && !drag.image_content {
                                self.commit_expanding_move(vec![drag.preview], false, transform);
                            } else {
                                self.execute(Command::Update {
                                    annotation: drag.preview,
                                });
                            }
                            self.status = "Object updated. Undo reverses the whole drag.".into();
                        }
                        self.needs_render = true;
                    }
                    if let Some(annotation) = self.creation.annotation.clone() {
                        if self.drag_start.take().is_some() {
                            let freehand = annotation.is_freehand();
                            if (freehand && annotation.points.len() >= 2)
                                || (!freehand
                                    && (annotation.start[0] - annotation.end[0])
                                        .hypot(annotation.start[1] - annotation.end[1])
                                        > 2.0)
                            {
                                let id = annotation.id;
                                let kind = annotation.kind;

                                self.execute(Command::Add { annotation });
                                self.select_only(Some(id));

                                if matches!(kind, Kind::Text | Kind::Callout) {
                                    self.begin_text_edit();
                                }
                            }
                        }
                        self.creation = Default::default();
                    }
                    if before_commit_size
                        != [self.session.document.width, self.session.document.height]
                        || before_commit_image != self.view.current_transform().map(|t| t.image)
                    {
                        // This pass already painted the old document. Do not
                        // present it with the new selection/origin on release.
                        ui.ctx()
                            .request_discard("canvas dimensions changed during drag commit");
                    }
                    ui.ctx().request_repaint();
                }
            }
            // Losing focus must not leave an uncommitted preview on the canvas.
            if !ui.input(|i| i.focused) {
                self.cancel_gesture();
            }
        }
        self.paint_grid(ui, transform);
        self.paint_creation(ui, transform);
        if interactive {
            let visible = transform.image.intersect(transform.viewport);
            let guide_painter = painter.with_clip_rect(visible);
            let stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(235, 100, 210));
            if let Some(x) = self.snapping_guides[0] {
                let x = screen([x, 0.0]).x;
                guide_painter.line_segment(
                    [
                        egui::pos2(x, visible.top()),
                        egui::pos2(x, visible.bottom()),
                    ],
                    stroke,
                );
            }
            if let Some(y) = self.snapping_guides[1] {
                let y = screen([0.0, y]).y;
                guide_painter.line_segment(
                    [
                        egui::pos2(visible.left(), y),
                        egui::pos2(visible.right(), y),
                    ],
                    stroke,
                );
            }
        }
        self.paint_hover_annotation(ui, &response, transform, interactive);
        self.selection_outlines(&painter, base_transform);
        let transform = self
            .selected
            .and_then(|id| {
                self.session
                    .document
                    .video
                    .as_ref()
                    .and_then(|p| p.objects.get(&id))
            })
            .map_or(transform, |timing| {
                self.video_layer_transform(
                    base_transform,
                    capture_core::video::links::Layer::Annotation(timing.track),
                )
            });
        let screen = |p: [f32; 2]| transform.screen_point(p);
        self.paint_group_feedback(ui, transform);
        self.paint_marquee(&painter, transform);
        if self.selected_objects().len() < 2
            && let Some(annotation) = self
                .selected_annotation()
                .filter(|a| self.session.document.object_editable(a.id))
        {
            let color = egui::Color32::from_rgb(189, 245, 69);
            crate::line_properties::paint_selection(&painter, &annotation, transform, color);
            let (min, max) = annotation.bounds();
            painter.rect_stroke(
                egui::Rect::from_two_pos(screen(min), screen(max)).expand(2.0),
                0,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Outside,
            );
            for (kind, handle) in annotation.handles() {
                if kind == Handle::Bend {
                    let c = screen(handle);
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            c + egui::vec2(0., -5.),
                            c + egui::vec2(5., 0.),
                            c + egui::vec2(0., 5.),
                            c + egui::vec2(-5., 0.),
                        ],
                        egui::Color32::from_rgb(45, 190, 225),
                        egui::Stroke::new(1., egui::Color32::WHITE),
                    ));
                    continue;
                }
                if matches!(
                    kind,
                    Handle::RadiusTopLeft
                        | Handle::RadiusTopRight
                        | Handle::RadiusBottomLeft
                        | Handle::RadiusBottomRight
                ) {
                    painter.circle_filled(
                        screen(handle),
                        crate::controls::HANDLE_RADIUS,
                        egui::Color32::from_rgb(45, 190, 225),
                    );
                    painter.circle_stroke(
                        screen(handle),
                        crate::controls::HANDLE_RADIUS,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                    );
                    continue;
                }
                let rect = egui::Rect::from_center_size(screen(handle), egui::vec2(9.0, 9.0));
                painter.rect_filled(rect, 1, egui::Color32::from_gray(25));
                painter.rect_stroke(
                    rect,
                    1,
                    egui::Stroke::new(1.5, color),
                    egui::StrokeKind::Inside,
                );
            }
        }
        self.canvas_pointer(ui, &response, transform, base_transform, interactive);
        self.show_image_frame_buttons(ui, transform);
        self.frame_file_drop(ui, transform);
        self.show_guide_controls(ui, transform);
        self.show_inline_text(ui, transform);
        self.paint_rotation_node(ui, transform);
        self.paint_group_handles(ui, transform);
        self.paint_zoom_focus(ui, base_transform);
        self.paint_magnified_handles(ui, transform);
        if self.view.panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::canvas_view::{CanvasView, ZoomAction, ZoomMode};

    pub(crate) fn editor(ctx: &egui::Context) -> Editor {
        Editor {
            project_sessions: Default::default(),
            video: Default::default(),
            group_feedback: None,
            lifecycle: Default::default(),
            editing_group: None,

            quick_controls: crate::quick_controls::QuickControls::default(),
            presets: crate::presets::Presets::default(),
            history: crate::history::HistoryPanel::default(),
            autosave: Default::default(),
            shell: crate::shell::Shell::default(),
            view: CanvasView::default(),
            session: capture_core::Session::new(capture_core::Document::blank(640, 400).unwrap())
                .unwrap(),
            texture: Some(ctx.load_texture(
                "test",
                egui::ColorImage::filled([640, 400], egui::Color32::WHITE),
                egui::TextureOptions::NEAREST,
            )),
            needs_render: false,
            display: Default::default(),
            tool: Some(Kind::Arrow),
            edit_drag: None,
            guide_drag: None,
            rotation_drag: None,
            property_preview: None,
            property_suppressed: false,
            text_draft: None,
            step_placement: None,
            snapping_guides: [None; 2],
            new_image: None,
            pdf_import: None,
            template_picker: None,
            resize_image: None,
            resize_canvas: None,
            export_dialog: None,
            export_preferences: Default::default(),
            icon_library: Default::default(),
            image_transform: None,
            rotate_image: None,
            sharing: Default::default(),
            crop: None,

            cut_out: None,

            color: [255, 0, 0],
            width: 3,
            drag_start: None,
            creation: Default::default(),
            stroke_points: Vec::new(),
            shape_sides: 3,
            tool_shapes: Default::default(),
            preferences: Default::default(),
            step_number: 1,
            step_letters: false,
            step_size: 44,
            step_style_size: 0,
            path: None,
            last_directory: std::path::PathBuf::from("."),
            status: String::new(),
            notice: Default::default(),
            selected: None,
            extra_selected: Default::default(),
            selection_order: vec![],
            group_drag: None,
            marquee: None,
            pending: None,
            allow_close: false,
            last_title: String::new(),
            capture_requested: false,
            clip: Default::default(),
            ocr: Default::default(),
            privacy_review: Default::default(),
            rich_text: Default::default(),
            capture_result: None,
            global_capture: None,
            global_capture_status: String::new(),
            hotkey_owner: None,
            options: Default::default(),
        }
    }

    #[test]
    fn arrow_presets_keep_defaults_out_of_history_and_selected_changes_undo_once() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let preset = capture_core::styles::presets()[2].clone();
        let before = serde_json::to_vec(&editor.session.document).unwrap();
        editor.apply_arrow_style(&preset);
        assert_eq!(editor.color, [240, 85, 55]);
        assert_eq!(editor.width, 3);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            before
        );
        assert!(!editor.session.can_undo());
        let original: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        let arrows = original.annotations.clone();
        let id = arrows.iter().find(|a| a.kind == Kind::Arrow).unwrap().id;
        editor.session = capture_core::Session::new(original).unwrap();
        editor.selected = Some(id);
        editor.tool = None;
        editor.apply_arrow_style(&preset);
        editor.apply_arrow_style(&preset);
        assert_eq!(editor.selected, Some(id));
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, arrows);
        assert!(!editor.session.can_undo());
        editor.execute(Command::Redo);
        assert_eq!(
            editor
                .session
                .document
                .annotations
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .color,
            preset.color
        );
        editor.selected = Some(arrows.iter().find(|a| a.kind != Kind::Arrow).unwrap().id);
        let before = serde_json::to_vec(&editor.session.document).unwrap();
        editor.apply_arrow_style(&capture_core::styles::presets()[0]);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            before
        );
    }

    #[test]
    fn active_history_click_and_canceled_replacement_preserve_edits_and_selection() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let original: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        editor.session = capture_core::Session::new(original).unwrap();
        editor.history.active = Some("current".into());
        editor.execute(Command::FlattenAll);
        let edited = serde_json::to_vec(&editor.session.document).unwrap();
        editor.open_history("current", &ctx);
        assert!(editor.pending.is_none());
        assert_eq!(editor.history.active.as_deref(), Some("current"));
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            edited
        );
        assert!(editor.session.can_undo());
        let next = || crate::FileAction::HistorySnapshot {
            session: capture_core::Session::unsaved(capture_core::Document::blank(4, 3).unwrap())
                .unwrap(),
            id: "next".into(),
        };
        editor.request_action(next(), &ctx);
        assert!(editor.pending.is_some());
        assert_eq!(editor.history.active.as_deref(), Some("current"));
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(981.0, 687.0),
                )),
                focused: true,
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| editor.confirm_unsaved(&ctx),
        );
        output.textures_delta.clear();
        assert!(editor.pending.is_none());
        assert_eq!(editor.history.active.as_deref(), Some("current"));
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            edited
        );
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations.len(), 7);
        assert!(!editor.session.is_modified());
        editor.request_action(next(), &ctx);
        assert!(editor.pending.is_none());
        assert_eq!(editor.history.active.as_deref(), Some("next"));
        assert_eq!(
            (
                editor.session.document.width,
                editor.session.document.height
            ),
            (4, 3)
        );
        assert!(!editor.session.can_undo());
    }

    #[test]
    fn line_bend_node_previews_real_curve_commits_on_release_and_undoes_once() {
        use egui::{Event, Modifiers, PointerButton};
        for kind in [Kind::Line, Kind::Arrow] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            let a = e.new_annotation(kind, [100., 100.], [380., 100.]);
            e.session.document.annotations = vec![a.clone()];
            e.tool = Some(kind);
            e.select_only(Some(a.id));
            let t = frame(&ctx, &mut e, vec![]);
            let from = t.screen_point([240., 100.]);
            let to = t.screen_point([240., 170.]);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            };
            frame(
                &ctx,
                &mut e,
                vec![Event::PointerMoved(from), button(from, true)],
            );
            frame(&ctx, &mut e, vec![Event::PointerMoved(to)]);
            assert_eq!(e.session.document.annotations, std::slice::from_ref(&a));
            let preview = e.selected_annotation().unwrap();
            assert_eq!(
                preview.line_style.unwrap().curve,
                capture_core::line_style::Curve::Circular
            );
            assert!((capture_core::line_style::bend_point(&preview)[1] - 170.).abs() < 0.1);
            frame(&ctx, &mut e, vec![button(to, false)]);
            assert_eq!(e.session.document.annotations, [preview]);
            assert_eq!(
                e.session.document.version,
                capture_core::MAX_PROJECT_VERSION
            );
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, [a]);
            assert!(!e.session.can_undo());
        }
    }

    pub(crate) fn frame(
        ctx: &egui::Context,
        editor: &mut Editor,
        events: Vec<egui::Event>,
    ) -> crate::canvas_view::Transform {
        let mut viewport = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600.0, 400.0),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                viewport = ui.available_rect_before_wrap();
                editor.show_canvas(ui);
            },
        );
        output.textures_delta.clear();
        editor
            .view
            .transform(viewport, egui::vec2(640.0, 400.0), ctx.pixels_per_point())
    }

    #[test]
    fn alt_drag_every_kind_previews_copies_and_commits_one_undo_without_changing_originals() {
        use egui::{Event, Modifiers, PointerButton};
        for kind in [
            Kind::Image,
            Kind::Arrow,
            Kind::Line,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Eraser,
            Kind::Step,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Redact,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Text,
            Kind::Callout,
        ] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            let start = [100.0, 100.0];
            let end = [180.0, 160.0];
            let a = Annotation {
                image: if kind == Kind::Image {
                    capture_core::clipboard_payload::ImagePayload::from_rgba(
                        80,
                        60,
                        vec![255; 80 * 60 * 4],
                    )
                    .unwrap()
                    .into_objects()
                    .unwrap()
                    .objects
                    .remove(0)
                    .image
                } else {
                    None
                },
                step_outline: None,
                line_style: None,
                shadow: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.0,
                mirror_x: false,
                id: 1,
                kind,
                start,
                end,
                color: [40, 90, 160, 255],
                width: 3,
                corner_radius: 0.0,
                fill: None,
                points: match kind {
                    Kind::Polygon => capture_core::polygon::outline(start, end, 5),
                    Kind::Pen | Kind::Highlighter | Kind::Eraser => vec![start, end],
                    _ => vec![],
                },
                text: matches!(kind, Kind::Text | Kind::Step | Kind::Callout).then(|| {
                    capture_core::text::TextStyle {
                        content: "1".into(),
                        ..Default::default()
                    }
                }),
                callout: (kind == Kind::Callout)
                    .then(|| capture_core::callout::CalloutStyle::for_box(start, end, [640, 400])),
            };
            e.session = crate::Session::new(crate::Document {
                version: capture_core::MAX_PROJECT_VERSION,
                annotations: vec![a.clone()],
                ..crate::Document::blank(640, 400).unwrap()
            })
            .unwrap();
            e.tool = Some(Kind::Step);
            e.select_only(Some(1));
            e.quick_controls.set_snapping(false);
            let t = frame(&ctx, &mut e, vec![]);
            let p = t.screen_point([140.0, 130.0]);
            let q = p + egui::vec2(40.0, 30.0) * t.scale;
            let button = |pos, pressed, modifiers| Event::PointerButton {
                pos,
                pressed,
                modifiers,
                button: PointerButton::Primary,
            };
            frame(
                &ctx,
                &mut e,
                vec![
                    Event::ModifiersChanged(Modifiers::ALT),
                    Event::PointerMoved(p),
                    button(p, true, Modifiers::ALT),
                ],
            );
            frame(&ctx, &mut e, vec![Event::PointerMoved(q)]);
            assert!(
                e.group_drag.as_ref().is_some_and(|d| d.duplicate),
                "{kind:?}"
            );
            assert_eq!(e.session.document.annotations, vec![a.clone()]);
            let preview = e.preview_document();
            assert_eq!(preview.annotations.len(), 2);
            assert_eq!(preview.annotations[0], a);
            frame(
                &ctx,
                &mut e,
                vec![
                    button(q, false, Modifiers::NONE),
                    Event::ModifiersChanged(Modifiers::NONE),
                ],
            );
            assert_eq!(e.session.document.annotations, preview.annotations);
            assert_eq!(e.selected, Some(2));
            let mut expected = a.clone();
            expected.id = 2;
            expected = expected.translated([40.0, 30.0], [640, 400]).unwrap();
            assert_eq!(e.session.document.annotations[1], expected);
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, vec![a]);
            assert!(!e.session.can_undo());
            e.execute(Command::Redo);
            assert_eq!(e.session.document.annotations, preview.annotations);
        }
    }
    #[test]
    fn alt_drag_group_fast_release_snaps_and_cancel_preserves_redo() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        for (id, center) in [(1, [100.0, 100.0]), (2, [200.0, 100.0])] {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    center,
                    44,
                    [20, 80, 160, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        e.tool = Some(Kind::Step);
        e.select_all_matching();
        let original = e.session.document.annotations.clone();
        let t = frame(&ctx, &mut e, vec![]);
        let p = t.screen_point([100.0, 100.0]);
        let q = t.screen_point([140.0, 102.0]);
        let button = |pos, pressed, modifiers| Event::PointerButton {
            pos,
            pressed,
            modifiers,
            button: PointerButton::Primary,
        };
        frame(
            &ctx,
            &mut e,
            vec![
                Event::ModifiersChanged(Modifiers::ALT),
                Event::PointerMoved(p),
                button(p, true, Modifiers::ALT),
                Event::PointerMoved(q),
                button(q, false, Modifiers::NONE),
                Event::ModifiersChanged(Modifiers::NONE),
            ],
        );
        assert_eq!(e.session.document.annotations.len(), 4);
        assert_eq!(e.selected_objects().len(), 2);
        assert_eq!(&e.session.document.annotations[..2], original.as_slice());
        let copy = &e.session.document.annotations[2];
        assert_eq!(copy.start, [122.0, 78.0]); // group left snaps to source right; y aligns
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, original);
        assert!(e.session.can_redo());
        e.select_all_matching();
        frame(
            &ctx,
            &mut e,
            vec![
                Event::ModifiersChanged(Modifiers::ALT),
                Event::PointerMoved(p),
                button(p, true, Modifiers::ALT),
            ],
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(q)]);
        assert!(e.group_drag.as_ref().is_some_and(|d| d.duplicate));
        e.cancel_gesture();
        assert_eq!(e.session.document.annotations, original);
        assert!(e.session.can_redo());
    }
    #[test]
    fn top_handle_modifiers_resize_live_without_duplicating_and_undo_once() {
        use egui::{Event, Modifiers, PointerButton};
        for (mods, expected_start, expected_end) in [
            (Modifiers::NONE, [100.0, 80.0], [180.0, 140.0]),
            (Modifiers::SHIFT, [80.0, 80.0], [200.0, 140.0]),
            (Modifiers::ALT, [100.0, 80.0], [180.0, 160.0]),
            (
                Modifiers::SHIFT | Modifiers::ALT,
                [60.0, 80.0],
                [220.0, 160.0],
            ),
        ] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.tool = Some(Kind::Rectangle);
            let a = Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.0,
                mirror_x: false,
                id: 1,
                kind: Kind::Rectangle,
                start: [100.0, 100.0],
                end: [180.0, 140.0],
                color: [30, 90, 150, 255],
                width: 3,
                corner_radius: 0.0,
                fill: None,
                points: vec![],
                text: None,
                callout: None,
            };
            e.session.document.annotations = vec![a.clone()];
            e.select_only(Some(1));
            let t = frame(&ctx, &mut e, vec![]);
            let p = t.screen_point([140.0, 100.0]);
            let q = t.screen_point([160.0, 80.0]);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                pressed,
                modifiers: mods,
                button: PointerButton::Primary,
            };
            frame(
                &ctx,
                &mut e,
                vec![
                    Event::ModifiersChanged(mods),
                    Event::PointerMoved(p),
                    button(p, true),
                ],
            );
            frame(&ctx, &mut e, vec![Event::PointerMoved(q)]);
            assert!(e.group_drag.is_none());
            assert_eq!(e.session.document.annotations, vec![a.clone()]);
            let preview = e.selected_annotation().unwrap();
            assert_eq!((preview.start, preview.end), (expected_start, expected_end));
            frame(&ctx, &mut e, vec![button(q, false)]);
            assert_eq!(e.session.document.annotations, vec![preview]);
            assert_eq!(e.selected, Some(1));
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, vec![a]);
            assert!(!e.session.can_undo());
        }
    }
    #[test]
    fn optional_object_tool_switching_selects_without_creating_or_modifying_objects() {
        use egui::{Event, Modifiers, PointerButton};
        for enabled in [false, true] {
            for initial in [Kind::Line, Kind::Step] {
                if !enabled && initial == Kind::Step {
                    continue;
                } // default Step-on-empty behavior is separately covered
                let ctx = egui::Context::default();
                let mut e = editor(&ctx);
                e.tool = Some(initial);
                let a = Annotation {
                    image: None,
                    step_outline: None,
                    line_style: None,
                    shadow: None,

                    focus: None,

                    highlighter_rect: false,
                    hard_edge: false,
                    group: None,
                    rotation_degrees: 0.0,
                    mirror_x: false,
                    id: 1,
                    kind: Kind::Rectangle,
                    start: [100.0, 100.0],
                    end: [180.0, 140.0],
                    color: [30, 90, 150, 255],
                    width: 3,
                    corner_radius: 0.0,
                    fill: None,
                    points: vec![],
                    text: None,
                    callout: None,
                };
                e.session.document.annotations = vec![a.clone()];
                e.quick_controls.set_auto_switch_tool(enabled);
                let t = frame(&ctx, &mut e, vec![]);
                let p = t.screen_point([140.0, 120.0]);
                let button = |pressed| Event::PointerButton {
                    pos: p,
                    pressed,
                    modifiers: Modifiers::NONE,
                    button: PointerButton::Primary,
                };
                frame(&ctx, &mut e, vec![Event::PointerMoved(p), button(true)]);
                frame(&ctx, &mut e, vec![button(false)]);
                assert_eq!(e.session.document.annotations, vec![a]);
                assert!(!e.session.can_undo());
                assert_eq!(e.selected, enabled.then_some(1));
                assert_eq!(
                    e.tool,
                    Some(if enabled { Kind::Rectangle } else { initial })
                );
                assert!(e.step_placement.is_none());
            }
        }
    }
    #[test]
    fn raster_inspection_blocks_drawing_but_keeps_middle_pan_and_selection() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.tool = Some(Kind::Rectangle);
        e.toggle_raster_preview();
        let before = serde_json::to_vec(&e.session.document).unwrap();
        let t = frame(&ctx, &mut e, vec![]);
        let a = t.screen_point([100.0, 100.0]);
        let b = t.screen_point([180.0, 140.0]);
        let button = |pos, button, pressed| Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        };
        for pointer in [PointerButton::Primary, PointerButton::Middle] {
            frame(
                &ctx,
                &mut e,
                vec![Event::PointerMoved(a), button(a, pointer, true)],
            );
            frame(&ctx, &mut e, vec![Event::PointerMoved(b)]);
            let moved = frame(&ctx, &mut e, vec![button(b, pointer, false)]);
            if pointer == PointerButton::Middle {
                assert!(moved.image.min.distance(t.image.min) > 10.0);
            }
            assert!(e.drag_start.is_none() && e.edit_drag.is_none());
        }
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), before);
        assert!(!e.session.can_undo());
    }

    #[test]
    fn raster_preview_wheel_zoom_in_and_out_keeps_pointer_anchor_without_editing() {
        use egui::{Event, Modifiers};
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.toggle_raster_preview();
        e.view.apply(ZoomAction::Set(ZoomMode::Manual(1.0)), None);
        let original = serde_json::to_vec(&e.session.document).unwrap();
        for direction in [100.0, -100.0] {
            let initial = frame(&ctx, &mut e, vec![]);
            let anchor = initial.viewport.center();
            let image_point = initial.image_point(anchor);
            let zoomed = frame(
                &ctx,
                &mut e,
                vec![
                    Event::PointerMoved(anchor),
                    Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Unknown,
                        delta: egui::vec2(0.0, direction),
                        modifiers: Modifiers::CTRL | Modifiers::COMMAND,
                    },
                ],
            );
            assert!((zoomed.zoom - initial.zoom) * direction > 0.0);
            assert!((zoomed.screen_point(image_point) - anchor).length() < 0.001);
        }
        assert!(e.shell.raster_preview);
        assert!(!e.session.can_undo());
        assert_eq!(serde_json::to_vec(&e.session.document).unwrap(), original);
    }

    #[test]
    fn radius_drag_works_with_shape_tool_and_does_not_create_another_object() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.tool = Some(Kind::Rectangle);
        editor.execute(Command::Add {
            annotation: Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,

                focus: None,

                highlighter_rect: false,
                hard_edge: false,
                group: None,
                rotation_degrees: 0.0,
                mirror_x: false,
                id: 1,
                kind: Kind::Rectangle,
                start: [100.0, 100.0],
                end: [300.0, 240.0],
                width: 2,
                color: [0, 0, 0, 255],
                text: None,
                callout: None,
                points: vec![],
                corner_radius: 0.0,
                fill: None,
            },
        });
        editor.selected = Some(1);
        let transform = frame(&ctx, &mut editor, vec![]);
        let first = transform.screen_point([110.0, 110.0]);
        let last = transform.screen_point([135.0, 135.0]);
        let original_handles = editor.selected_annotation().unwrap().handles();
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(first), button(first, true)],
        );
        frame(&ctx, &mut editor, vec![Event::PointerMoved(last)]);
        assert!(editor.edit_drag.is_some());
        let preview = editor.selected_annotation().unwrap();
        assert!((preview.corner_radius - 50.0).abs() < 0.01);
        assert_eq!(preview.handles(), original_handles);
        assert_eq!(editor.session.document.annotations[0].corner_radius, 0.0);
        frame(&ctx, &mut editor, vec![button(last, false)]);
        assert_eq!(editor.session.document.annotations.len(), 1);
        assert!((editor.session.document.annotations[0].corner_radius - 50.0).abs() < 0.01);
        let finished = editor.selected_annotation().unwrap();
        assert_eq!(finished.handles(), original_handles);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations[0].corner_radius, 0.0);
        editor.execute(Command::Redo);
        assert!((editor.session.document.annotations[0].corner_radius - 50.0).abs() < 0.01);
        let outward = transform.screen_point([105.0, 105.0]);
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(first), button(first, true)],
        );
        frame(&ctx, &mut editor, vec![Event::PointerMoved(outward)]);
        frame(&ctx, &mut editor, vec![button(outward, false)]);
        assert!((editor.session.document.annotations[0].corner_radius - 40.0).abs() < 0.01);
        assert_eq!(
            editor.selected_annotation().unwrap().handles(),
            original_handles
        );
        editor.execute(Command::Undo);
        assert!((editor.session.document.annotations[0].corner_radius - 50.0).abs() < 0.01);
    }
    #[test]
    fn radius_drag_with_visible_sidebar_continues_through_fractional_values_and_reversal() {
        use egui::{Event, Modifiers, PointerButton};
        for (kind, degrees) in [
            (Kind::Rectangle, 0.0),
            (Kind::Rectangle, 25.0),
            (Kind::Callout, 0.0),
            (Kind::Callout, -20.0),
        ] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let mut e = editor(&ctx);
            e.tool = Some(kind);
            let mut shape = e.new_annotation(kind, [100.0, 100.0], [300.0, 240.0]);
            shape.corner_radius = 12.0;
            shape.rotation_degrees = degrees;
            e.session.document.annotations = vec![shape.clone()];
            e.session.document.version = capture_core::MAX_PROJECT_VERSION;
            e.session.document.promote_features();
            e.session.document.validate().unwrap();
            e.select_only(Some(shape.id));
            let frame = |e: &mut Editor, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        focused: true,
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1280.0, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        e.prepare_property_input(&ctx);
                        e.show_shell(ui);
                        e.settle_property_preview(&ctx);
                        egui::CentralPanel::default().show(ui, |ui| e.show_canvas(ui));
                    },
                );
                output.textures_delta.clear();
            };
            frame(&mut e, vec![]);
            frame(&mut e, vec![]);
            let transform = e.view.current_transform().unwrap();
            let (_, grab) = shape
                .handles()
                .into_iter()
                .find(|(h, _)| *h == Handle::RadiusTopLeft)
                .unwrap();
            let origin = transform.screen_point(grab);
            let local_grab = shape.rotation().inverse_point(grab);
            let shifted = |delta| {
                transform.screen_point(
                    shape
                        .rotation()
                        .point([local_grab[0] + delta, local_grab[1]]),
                )
            };
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(
                &mut e,
                vec![Event::PointerMoved(origin), button(origin, true)],
            );
            // Reversals must keep working after many rendered sidebar frames, including
            // subpixel radii that the readout only displays to one decimal place.
            for delta in [8.37, 20.83, 100.0, 18.17, -40.0, 6.23, 15.67] {
                let pointer = shifted(delta);
                for events in [vec![Event::PointerMoved(pointer)], vec![], vec![]] {
                    frame(&mut e, events);
                    assert!(
                        e.edit_drag.is_some(),
                        "canvas gesture must retain ownership"
                    );
                    assert!(
                        e.property_preview.is_none(),
                        "sidebar readout must not start a second transaction"
                    );
                    let preview = e.selected_annotation().unwrap();
                    assert!(
                        (preview.corner_radius - (12.0 + delta).clamp(0.0, 70.0)).abs() < 0.001
                    );
                    assert_eq!(preview.handles(), shape.handles());
                    assert_eq!(e.session.document.annotations, vec![shape.clone()]);
                }
            }
            let last = shifted(15.67);
            frame(&mut e, vec![button(last, false)]);
            assert!(e.edit_drag.is_none() && e.property_preview.is_none());
            assert!(
                (e.session.document.annotations[0].corner_radius - 27.67).abs() < 0.001,
                "{kind:?} {degrees}: {} ({})",
                e.session.document.annotations[0].corner_radius,
                e.status
            );
            let committed = e.session.document.annotations.clone();
            for _ in 0..3 {
                frame(&mut e, vec![]);
                assert_eq!(e.session.document.annotations, committed);
            }
            e.execute(Command::Undo);
            assert_eq!(e.session.document.annotations, vec![shape]);
            assert!(!e.session.can_undo());
        }
    }

    #[test]
    fn eraser_drag_previews_real_alpha_then_commits_once_and_properties_stay_live() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.tool = Some(Kind::Eraser);
        editor.width = 24;
        editor.presets.alpha = 255;
        let original = editor.session.document.render().unwrap();
        let transform = frame(&ctx, &mut editor, vec![]);
        let start = transform.screen_point([100.0, 100.0]);
        let end = transform.screen_point([240.0, 140.0]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        frame(&ctx, &mut editor, vec![Event::PointerMoved(end)]);
        assert!(editor.session.document.annotations.is_empty());
        assert!(!editor.session.can_undo());
        assert!(editor.needs_render);
        let preview = editor.preview_document().render().unwrap();
        assert_eq!(preview.get_pixel(170, 120).0, [0; 4]);
        assert_eq!(editor.session.document.render().unwrap(), original);
        frame(&ctx, &mut editor, vec![button(end, false)]);
        assert_eq!(editor.session.document.annotations.len(), 1);
        assert_eq!(editor.session.document.render().unwrap(), preview);
        let mut brush = editor.selected_annotation().unwrap();
        brush.color[3] = 128;
        brush.width = 8;
        editor.preview_property_edit(brush);
        assert_eq!(
            editor
                .preview_document()
                .render()
                .unwrap()
                .get_pixel(170, 120)[3],
            127
        );
        assert_eq!(editor.session.document.render().unwrap(), preview);
        editor.settle_property_preview(&ctx);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.render().unwrap(), preview);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.render().unwrap(), original);
        assert!(!editor.session.can_undo());
    }

    #[test]
    fn effect_region_drag_and_intensity_preview_preserve_one_step_history() {
        use egui::{Event, Modifiers, PointerButton};
        for kind in [Kind::Blur, Kind::Pixelate] {
            let ctx = egui::Context::default();
            let mut editor = editor(&ctx);
            let source = image::RgbaImage::from_fn(640, 400, |x, y| {
                image::Rgba([
                    if x < 173 { 0 } else { 255 },
                    if y < 117 { 50 } else { 200 },
                    100,
                    255,
                ])
            });
            editor.session =
                crate::Session::new(crate::Document::from_image(source.clone()).unwrap()).unwrap();
            editor.tool = Some(kind);
            editor.width = 8;
            let transform = frame(&ctx, &mut editor, vec![]);
            let start = transform.screen_point([100.0, 60.0]);
            let end = transform.screen_point([240.0, 180.0]);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(
                &ctx,
                &mut editor,
                vec![Event::PointerMoved(start), button(start, true)],
            );
            frame(&ctx, &mut editor, vec![Event::PointerMoved(end)]);
            assert!(editor.session.document.annotations.is_empty());
            frame(&ctx, &mut editor, vec![button(end, false)]);
            assert_eq!(editor.session.document.annotations.len(), 1);
            let mut effect = editor.session.document.annotations[0].clone();
            assert_eq!(effect.kind, kind);
            let initial = editor.session.document.render().unwrap();
            assert!(initial != source, "{kind:?} must change the region");
            effect.width = 20;
            editor.preview_property_edit(effect);
            assert_eq!(editor.session.document.annotations[0].width, 8);
            assert!(
                editor.preview_document().render().unwrap() != initial,
                "{kind:?} intensity preview must change pixels"
            );
            editor.settle_property_preview(&ctx);
            assert_eq!(editor.session.document.annotations[0].width, 20);
            editor.execute(Command::Undo);
            assert_eq!(editor.session.document.render().unwrap(), initial);
            editor.execute(Command::Undo);
            assert_eq!(editor.session.document.render().unwrap(), source);
            assert!(!editor.session.can_undo());
        }
    }

    #[test]
    fn polygon_canvas_drag_creates_an_editable_shape_and_one_undo() {
        use egui::{Event, Modifiers, PointerButton, pos2};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.tool = Some(Kind::Polygon);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut editor, vec![]);
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(pos2(100.0, 100.0)),
                button(pos2(100.0, 100.0), true),
            ],
        );
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(pos2(240.0, 210.0))],
        );
        frame(&ctx, &mut editor, vec![button(pos2(240.0, 210.0), false)]);
        assert_eq!(
            editor.session.document.annotations.len(),
            1,
            "{}",
            editor.status
        );
        let shape = editor.session.document.annotations[0].clone();
        assert_eq!(shape.kind, Kind::Polygon);
        assert_eq!(shape.points.len(), 3);
        assert!(shape.hit_test([170.0, 180.0], 0.0));
        editor.execute(Command::Undo);
        assert!(editor.session.document.annotations.is_empty());
        assert!(!editor.session.can_undo());
        editor.execute(Command::Redo);
        assert_eq!(editor.session.document.annotations, vec![shape]);
    }
    #[test]
    fn rectangle_highlighter_creation_previews_commits_and_keeps_pen_defaults() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut e = editor(&ctx);
        e.tool = Some(Kind::Highlighter);
        let mut groups = capture_core::object_styles::groups(Kind::Highlighter);
        let mut pen = groups[1].styles.remove(1);
        pen.width = 21;
        e.apply_object_template(&pen, false);
        let mut rect = groups[2].styles.remove(3);
        rect.color = [250, 220, 25, 90];
        e.apply_object_template(&rect, false);
        let t = frame(&ctx, &mut e, vec![]);
        let start = t.screen_point([110., 100.]);
        let end = t.screen_point([310., 180.]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        frame(&ctx, &mut e, vec![Event::PointerMoved(end)]);
        let preview = e.creation.annotation.clone().unwrap();
        assert!(preview.highlighter_rect && preview.points.is_empty());
        assert_eq!(preview.color, rect.color);
        assert!(e.session.document.annotations.is_empty());
        assert!(!e.session.can_undo());
        frame(&ctx, &mut e, vec![button(end, false)]);
        assert_eq!(
            e.session.document.annotations,
            std::slice::from_ref(&preview)
        );
        e.execute(Command::Undo);
        assert!(e.session.document.annotations.is_empty());
        assert!(!e.session.can_undo());
        e.execute(Command::Redo);
        e.select_only(None);
        e.update_creation([300., 220.], [180., 180.], Kind::Highlighter, true, false);
        let square = e.creation.annotation.take().unwrap();
        assert_eq!(
            (square.start[0] - square.end[0]).abs(),
            (square.start[1] - square.end[1]).abs()
        );
        e.apply_object_template(&pen, false);
        e.stroke_points = vec![[10., 10.], [30., 50.]];
        let drawn = e.new_annotation(Kind::Highlighter, [10., 10.], [30., 50.]);
        assert!(!drawn.highlighter_rect && drawn.is_freehand());
        assert_eq!(drawn.width, 21);
        e.apply_object_template(&rect, false);
        assert!(e.drawing_rectangle_highlighter());
        let new = e.new_annotation(Kind::Highlighter, [350., 250.], [450., 300.]);
        assert_eq!(new.color, rect.color);
        assert!(new.points.is_empty());
        if let Ok(root) = std::env::var("CLIPDESIGNER_HIGHLIGHTER_UI_DIR") {
            use crate::property_layout::tests::{PaintedOutput, dump_meshes};
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            e.select_only(Some(preview.id));
            let mut paint = || {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(262., 680.),
                        )),
                        focused: true,
                        ..Default::default()
                    },
                    |ui| {
                        e.presets_header(ui, Some(Kind::Highlighter));
                        e.object_templates(ui, Kind::Highlighter);
                        crate::property_layout::section(ui, "Properties");
                        let mut a = preview.clone();
                        crate::property_layout::annotation_properties(ui, &mut a, [640, 400]);
                    },
                ))
            };
            let first = paint();
            let last = paint();
            dump_meshes(&ctx, &root, "pane", first, last);
        }
    }
    #[test]
    fn freehand_curves_commit_once_and_escape_cancels_the_whole_stroke() {
        use egui::{Event, Key, Modifiers, PointerButton, pos2};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.tool = Some(Kind::Pen);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut editor, vec![]);
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(pos2(100.0, 100.0)),
                button(pos2(100.0, 100.0), true),
            ],
        );
        for p in [pos2(130.0, 170.0), pos2(200.0, 100.0), pos2(100.0, 100.0)] {
            frame(&ctx, &mut editor, vec![Event::PointerMoved(p)]);
        }
        frame(&ctx, &mut editor, vec![button(pos2(100.0, 100.0), false)]);
        assert_eq!(editor.session.document.annotations.len(), 1);
        assert!(editor.session.document.annotations[0].points.len() >= 4);
        let before = editor.session.document.annotations.clone();
        editor.execute(Command::Undo);
        assert!(!editor.session.can_undo());
        editor.execute(Command::Redo);
        assert_eq!(editor.session.document.annotations, before);
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(pos2(220.0, 100.0)),
                button(pos2(220.0, 100.0), true),
            ],
        );
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(pos2(260.0, 170.0))],
        );
        frame(
            &ctx,
            &mut editor,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        frame(&ctx, &mut editor, vec![button(pos2(260.0, 170.0), false)]);
        assert_eq!(editor.session.document.annotations, before);
        assert!(editor.stroke_points.is_empty());
    }
    #[test]
    fn step_clicks_advance_and_undo_reuses_the_missing_number() {
        use egui::{Event, Modifiers, PointerButton, pos2};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.tool = Some(Kind::Step);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut editor, vec![]);
        for x in [100.0, 200.0] {
            frame(
                &ctx,
                &mut editor,
                vec![
                    Event::PointerMoved(pos2(x, 100.0)),
                    button(pos2(x, 100.0), true),
                ],
            );
            frame(&ctx, &mut editor, vec![button(pos2(x, 100.0), false)]);
        }
        assert_eq!(editor.session.document.annotations.len(), 2);
        assert_eq!(editor.step_number, 3);
        editor.execute(Command::Undo);
        assert_eq!(editor.step_number, 2);
        editor.execute(Command::Redo);
        assert_eq!(editor.step_number, 3);
        editor.step_letters = true;
        editor.step_number = 1;
        editor.add_step([300.0, 100.0]);
        assert_eq!(
            editor
                .session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "A"
        );
        assert_eq!(editor.step_number, 2);
    }

    #[test]
    fn move_snapping_is_zoom_invariant_toggleable_and_one_transaction() {
        use egui::{Event, Modifiers, PointerButton};
        for zoom in [0.5, 1.0, 2.0] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.view.apply(ZoomAction::Set(ZoomMode::Manual(zoom)), None);
            for (id, center) in [(1, [220.0, 180.0]), (2, [320.0, 260.0])] {
                e.session.document.annotations.push(
                    capture_core::steps::marker(
                        id,
                        center,
                        40,
                        [20, 90, 150, 255],
                        (id as u32, false),
                        [640, 400],
                    )
                    .unwrap(),
                );
            }
            let original = e.session.document.annotations.clone();
            e.tool = Some(Kind::Step);
            let t = frame(&ctx, &mut e, vec![]);
            let p = t.screen_point([220.0, 180.0]);
            let q = t.screen_point([317.0, 180.0]);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            };
            for enabled in [true, false] {
                e.quick_controls.set_snapping(enabled);
                frame(&ctx, &mut e, vec![Event::PointerMoved(p), button(p, true)]);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(600.0, 400.0),
                        )),
                        focused: true,
                        events: vec![Event::PointerMoved(q)],
                        ..Default::default()
                    },
                    |ui| e.show_canvas(ui),
                );
                assert_eq!(output.shapes.iter().any(|s| matches!(&s.shape,
                    egui::Shape::LineSegment{stroke,..} if stroke.color==egui::Color32::from_rgb(235,100,210))),enabled);
                output.textures_delta.clear();
                assert_eq!(e.snapping_guides[0].is_some(), enabled);
                assert_eq!(e.session.document.annotations, original);
                frame(&ctx, &mut e, vec![button(q, false)]);
                let moved = &e.session.document.annotations[0];
                assert!(
                    ((moved.start[0] + moved.end[0]) * 0.5 - if enabled { 320.0 } else { 317.0 })
                        .abs()
                        < 0.01
                );
                assert_eq!(e.selected, Some(1));
                e.execute(Command::Undo);
                assert_eq!(e.session.document.annotations, original);
                assert!(!e.session.can_undo());
            }
        }
    }

    #[test]
    fn completed_annotation_drawing_keeps_active_tool_and_property_selection() {
        use egui::{Event, Modifiers, PointerButton};
        for kind in [
            Kind::Arrow,
            Kind::Line,
            Kind::Text,
            Kind::Callout,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Redact,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Eraser,
        ] {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.tool = Some(kind);
            let t = frame(&ctx, &mut e, vec![]);
            let p = t.screen_point([100.0, 80.0]);
            let q = t.screen_point([260.0, 160.0]);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            };
            frame(&ctx, &mut e, vec![Event::PointerMoved(p), button(p, true)]);
            frame(&ctx, &mut e, vec![Event::PointerMoved(q)]);
            frame(&ctx, &mut e, vec![button(q, false)]);
            assert_eq!(e.session.document.annotations.len(), 1, "{kind:?}");
            assert_eq!(e.tool, Some(kind));
            if kind == Kind::Redact {
                assert!(e.selected.is_none(), "Redact stays ready to repeat");
            } else {
                assert_eq!(e.selected, Some(e.session.document.annotations[0].id));
                assert_eq!(e.selected_annotation().unwrap().kind, kind);
            }
        }
    }

    #[test]
    fn step_press_preview_follows_pointer_commits_once_and_keeps_focus() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.tool = Some(Kind::Step);
        let t = frame(&ctx, &mut e, vec![]);
        let start = t.screen_point([100.0, 100.0]);
        let finish = t.screen_point([340.0, 210.0]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            pressed,
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        assert!(
            e.step_placement.is_some(),
            "visible preview starts on down, not at drag threshold"
        );
        assert_eq!(e.preview_document().annotations.len(), 1);
        assert!(e.session.document.annotations.is_empty());
        assert!(!e.session.can_undo());
        assert_eq!(e.step_number, 1);
        frame(&ctx, &mut e, vec![Event::PointerMoved(finish)]);
        let preview = e.step_placement.as_ref().unwrap();
        assert_eq!(
            [
                (preview.start[0] + preview.end[0]) * 0.5,
                (preview.start[1] + preview.end[1]) * 0.5
            ],
            [340.0, 210.0]
        );
        frame(&ctx, &mut e, vec![button(finish, false)]);
        assert!(e.step_placement.is_none());
        assert_eq!(e.session.document.annotations.len(), 1);
        assert_eq!(e.selected, Some(1));
        assert_eq!(e.tool, Some(Kind::Step));
        assert_eq!(e.step_number, 2);
        let temp = tempfile::tempdir().unwrap();
        let saved = temp.path().join("step.capture.json");
        e.session.save(&saved).unwrap();
        assert_eq!(
            crate::Document::load(&saved).unwrap().render().unwrap(),
            e.session.document.render().unwrap()
        );
        e.execute(Command::Undo);
        assert!(e.session.document.annotations.is_empty());
        assert!(!e.session.can_undo());
        assert_eq!(e.step_number, 1);
        // A whole new placement can also arrive between two display frames.
        frame(
            &ctx,
            &mut e,
            vec![
                Event::PointerMoved(start),
                button(start, true),
                Event::PointerMoved(finish),
                button(finish, false),
            ],
        );
        assert_eq!(e.session.document.annotations.len(), 1);
        assert_eq!(e.selected, Some(e.session.document.annotations[0].id));
        e.execute(Command::Undo);
        frame(
            &ctx,
            &mut e,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        e.cancel_gesture();
        frame(&ctx, &mut e, vec![button(start, false)]);
        assert!(e.session.document.annotations.is_empty());
        assert!(e.session.can_redo());
        assert_eq!(e.step_number, 1);
    }

    #[test]
    fn shift_click_group_drag_and_batch_formatting_preserve_step_labels_and_undo() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.tool = Some(Kind::Step);
        for (id, center) in [(1, [100.0, 100.0]), (2, [220.0, 130.0])] {
            editor.session.document.annotations.push(
                capture_core::steps::marker(
                    id,
                    center,
                    44,
                    [40, 100, 160, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        editor.sync_step_sequence();
        let original = editor.session.document.annotations.clone();
        let t = frame(&ctx, &mut editor, vec![]);
        let button = |pos, pressed, modifiers| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        for (pos, mods) in [
            (t.screen_point([100.0, 100.0]), Modifiers::NONE),
            (t.screen_point([220.0, 130.0]), Modifiers::SHIFT),
        ] {
            frame(
                &ctx,
                &mut editor,
                vec![
                    Event::ModifiersChanged(mods),
                    Event::PointerMoved(pos),
                    button(pos, true, mods),
                ],
            );
            frame(&ctx, &mut editor, vec![button(pos, false, mods)]);
        }
        assert_eq!(editor.selected_objects().len(), 2);
        assert_eq!(editor.step_number, 3);
        assert!(!editor.session.can_undo());
        let p = t.screen_point([100.0, 100.0]);
        let q = t.screen_point([150.0, 140.0]);
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::ModifiersChanged(Modifiers::NONE),
                Event::PointerMoved(p),
                button(p, true, Modifiers::NONE),
            ],
        );
        frame(&ctx, &mut editor, vec![Event::PointerMoved(q)]);
        assert!(editor.group_drag.is_some());
        assert_eq!(editor.session.document.annotations, original);
        for (a, b) in original.iter().zip(&editor.preview_document().annotations) {
            assert!((b.start[0] - a.start[0] - 50.0).abs() < 0.01);
            assert!((b.start[1] - a.start[1] - 40.0).abs() < 0.01);
        }
        frame(&ctx, &mut editor, vec![button(q, false, Modifiers::NONE)]);
        assert_eq!(editor.session.document.annotations.len(), 2);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, original);
        assert!(!editor.session.can_undo());
        // Fast input can press, cross the threshold and release in one frame.
        frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(p),
                button(p, true, Modifiers::NONE),
                Event::PointerMoved(q),
                button(q, false, Modifiers::NONE),
            ],
        );
        assert_ne!(editor.session.document.annotations, original);
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, original);
        let mut edited = editor.selected_annotation().unwrap();
        edited.color = [180, 30, 90, 120];
        editor.preview_property_edit(edited);
        assert_eq!(editor.session.document.annotations, original);
        assert!(
            editor
                .preview_document()
                .annotations
                .iter()
                .all(|a| a.color == [180, 30, 90, 120])
        );
        editor.settle_property_preview(&ctx);
        assert!(
            editor
                .session
                .document
                .annotations
                .iter()
                .all(|a| a.color == [180, 30, 90, 120])
        );
        for (a, b) in original.iter().zip(&editor.session.document.annotations) {
            assert_eq!(a.text, b.text);
            assert_eq!(a.start, b.start);
            assert_eq!(a.end, b.end);
        }
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.annotations, original);
        assert!(!editor.session.can_undo());
        editor.click_selection(Some(1), true);
        assert_eq!(editor.selected_objects().len(), 1);
        editor.click_selection(Some(2), true);
        assert!(editor.selected_objects().is_empty());
    }

    #[test]
    fn middle_drag_pans_without_drawing_or_changing_selection_or_history() {
        use egui::{Event, Modifiers, PointerButton};
        for tool in [
            Some(Kind::Step),
            Some(Kind::Rectangle),
            Some(Kind::Line),
            None,
        ] {
            let ctx = egui::Context::default();
            let mut editor = editor(&ctx);
            editor.tool = tool;
            let original = serde_json::to_vec(&editor.session.document).unwrap();
            let before = frame(&ctx, &mut editor, vec![]);
            let p = egui::pos2(250.0, 180.0);
            let q = p + egui::vec2(45.0, 35.0);
            let button = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Middle,
                pressed,
                modifiers: Modifiers::NONE,
            };
            frame(
                &ctx,
                &mut editor,
                vec![Event::PointerMoved(p), button(p, true)],
            );
            let during = frame(&ctx, &mut editor, vec![Event::PointerMoved(q)]);
            assert!(editor.view.panning);
            assert!((during.image.center() - before.image.center() - (q - p)).length() < 0.01);
            frame(&ctx, &mut editor, vec![button(q, false)]);
            assert!(!editor.view.panning);
            assert!(editor.edit_drag.is_none() && editor.drag_start.is_none());
            assert_eq!(editor.selected, None);
            assert_eq!(editor.tool, tool);
            assert_eq!(
                serde_json::to_vec(&editor.session.document).unwrap(),
                original
            );
            assert!(!editor.session.can_undo());
            editor.view.apply(
                crate::canvas_view::ZoomAction::Set(crate::canvas_view::ZoomMode::Fit),
                None,
            );
            let fit = frame(&ctx, &mut editor, vec![]);
            assert_eq!(fit.image.center(), fit.viewport.center());
        }
    }

    #[test]
    fn active_step_selects_and_drags_existing_marker_without_duplicate_or_sequence_change() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let marker = capture_core::steps::marker(
            1,
            [100.0, 100.0],
            48,
            [20, 90, 150, 255],
            (1, false),
            [640, 400],
        )
        .unwrap();
        editor.session.document.annotations.push(marker.clone());
        editor.tool = Some(Kind::Step);
        editor.sync_step_sequence();
        let original = serde_json::to_vec(&editor.session.document).unwrap();
        let t = frame(&ctx, &mut editor, vec![]);
        let p = t.screen_point([100.0, 100.0]);
        let q = t.screen_point([150.0, 120.0]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(p), button(p, true)],
        );
        frame(&ctx, &mut editor, vec![button(p, false)]);
        assert_eq!(editor.selected, Some(1));
        assert_eq!(editor.session.document.annotations.len(), 1);
        assert_eq!(editor.step_number, 2);
        assert!(!editor.session.can_undo());
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(p), button(p, true)],
        );
        frame(&ctx, &mut editor, vec![Event::PointerMoved(q)]);
        assert!(editor.edit_drag.is_some());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert_ne!(editor.preview_document().annotations[0], marker);
        frame(&ctx, &mut editor, vec![button(q, false)]);
        assert_eq!(editor.session.document.annotations.len(), 1);
        assert_eq!(editor.step_number, 2);
        assert!(
            (editor.session.document.annotations[0].start[0] - marker.start[0] - 50.0).abs() < 0.01
        );
        editor.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        let blank = t.screen_point([400.0, 300.0]);
        frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(blank), button(blank, true)],
        );
        frame(&ctx, &mut editor, vec![button(blank, false)]);
        assert_eq!(editor.session.document.annotations.len(), 2);
        assert_eq!(editor.step_number, 3);
    }

    #[test]
    fn own_tool_hit_filters_occluding_types_and_preserves_shape_and_blur_families() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.quick_controls.set_auto_switch_tool(false);
        let mut a = capture_core::steps::marker(
            1,
            [150.0, 150.0],
            80,
            [10, 20, 30, 255],
            (1, false),
            [640, 400],
        )
        .unwrap();
        a.kind = Kind::Ellipse;
        a.text = None;
        let mut b = a.clone();
        b.id = 2;
        b.kind = Kind::Blur;
        editor.session.document.annotations = vec![a, b];
        editor.tool = Some(Kind::Rectangle);
        assert_eq!(editor.tool_hit([150.0, 150.0], 2.0), Some(1));
        editor.tool = Some(Kind::Pixelate);
        assert_eq!(editor.tool_hit([150.0, 150.0], 2.0), Some(2));
        editor.tool = Some(Kind::Text);
        assert_eq!(editor.tool_hit([150.0, 150.0], 2.0), None);
        editor.tool = None;
        assert_eq!(editor.tool_hit([150.0, 150.0], 2.0), Some(2));
        for kind in [
            Kind::Text,
            Kind::Callout,
            Kind::Step,
            Kind::Line,
            Kind::Arrow,
            Kind::Redact,
            Kind::Pen,
            Kind::Highlighter,
        ] {
            assert!(Editor::tool_matches(Some(kind), kind));
            assert_eq!(
                Editor::tool_matches(Some(kind), Kind::Ellipse),
                kind == Kind::Callout
            );
        }
    }
    #[test]
    fn space_drag_pans_without_annotations_and_control_wheel_anchors_zoom() {
        use egui::{Event, Key, Modifiers, PointerButton, pos2};
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor
            .view
            .apply(ZoomAction::Set(ZoomMode::Manual(2.0)), None);
        let initial = frame(&ctx, &mut editor, vec![]);
        let key = |pressed| Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &ctx,
            &mut editor,
            vec![
                key(true),
                Event::PointerMoved(pos2(250.0, 180.0)),
                button(pos2(250.0, 180.0), true),
            ],
        );
        let panned = frame(
            &ctx,
            &mut editor,
            vec![Event::PointerMoved(pos2(290.0, 200.0))],
        );
        assert_eq!(panned.image.min - initial.image.min, egui::vec2(40.0, 20.0));
        frame(
            &ctx,
            &mut editor,
            vec![button(pos2(290.0, 200.0), false), key(false)],
        );
        assert!(!editor.view.panning);
        assert!(editor.session.document.annotations.is_empty());
        assert!(!editor.session.is_modified());
        let anchor = pos2(350.0, 250.0);
        let before = panned.image_point(anchor);
        let zoomed = frame(
            &ctx,
            &mut editor,
            vec![
                Event::PointerMoved(anchor),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0.0, 100.0),
                    modifiers: Modifiers::CTRL | Modifiers::COMMAND,
                },
            ],
        );
        assert!(zoomed.zoom > panned.zoom);
        assert!((zoomed.screen_point(before) - anchor).length() < 0.001);
        assert!(!editor.session.is_modified());
    }

    #[test]
    fn canvas_wheel_during_a_draft_does_not_change_view_or_document() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor
            .view
            .apply(ZoomAction::Set(ZoomMode::Manual(2.0)), None);
        let before = frame(&ctx, &mut editor, vec![]);
        editor.new_image = Some(crate::new_image::NewImage::default());
        let after = frame(
            &ctx,
            &mut editor,
            vec![
                egui::Event::PointerMoved(egui::pos2(250.0, 180.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    delta: egui::vec2(0.0, 100.0),
                    modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
                },
            ],
        );
        assert_eq!(after.image, before.image);
        assert_eq!(after.zoom, before.zoom);
        assert!(!editor.session.is_modified());
    }

    #[test]
    fn pixel_grid_shortcut_overlay_and_drafts_preserve_document_and_history() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let original = serde_json::to_vec(&editor.session.document).unwrap();
        let pixels = editor.session.document.render().unwrap();
        let key = |modifiers, repeat| egui::Event::Key {
            key: egui::Key::G,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        };
        let run = |editor: &mut Editor, mut events: Vec<egui::Event>| {
            if !events.is_empty() {
                events.push(egui::Event::Key {
                    key: egui::Key::G,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 400.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.pixel_grid_shortcut(&ctx);
                    editor.show_canvas(ui);
                },
            );
            output.textures_delta.clear();
            output.shapes.len()
        };
        let modifiers = egui::Modifiers::CTRL | egui::Modifiers::COMMAND | egui::Modifiers::ALT;
        editor
            .view
            .apply(ZoomAction::Set(ZoomMode::Manual(8.0)), None);
        let without = run(&mut editor, vec![]);
        let with = run(&mut editor, vec![key(modifiers, false)]);
        assert!(editor.shell.pixel_grid);
        assert!(
            with > without + 50,
            "actual canvas paint must include visible grid lines"
        );
        run(
            &mut editor,
            vec![key(modifiers | egui::Modifiers::SHIFT, false)],
        );
        assert!(editor.shell.pixel_grid);
        editor.new_image = Some(crate::new_image::NewImage::default());
        run(&mut editor, vec![key(modifiers, false)]);
        assert!(editor.shell.pixel_grid);
        editor.new_image = None;
        editor.begin_resize();
        run(&mut editor, vec![key(modifiers, false)]);
        assert!(editor.shell.pixel_grid);
        editor.resize_image = None;
        editor.begin_resize_canvas();
        run(&mut editor, vec![key(modifiers, false)]);
        assert!(editor.shell.pixel_grid);
        editor.resize_canvas = None;
        editor.shell.help = true;
        run(&mut editor, vec![key(modifiers, false)]);
        assert!(editor.shell.pixel_grid);
        editor.shell.help = false;
        editor
            .view
            .apply(ZoomAction::Set(ZoomMode::Manual(1.0)), None);
        let below_threshold = run(&mut editor, vec![]);
        let disabled = run(&mut editor, vec![key(modifiers, false)]);
        assert!(!editor.shell.pixel_grid);
        assert_eq!(below_threshold, disabled);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert_eq!(editor.session.document.render().unwrap(), pixels);
        assert!(!editor.session.can_undo());
        assert!(!editor.session.can_redo());
        assert!(!editor.session.is_modified());
    }

    #[test]
    fn resize_popover_follows_anchor_and_outside_click_cancels_without_editing() {
        for size in [egui::vec2(981.0, 687.0), egui::vec2(640.0, 480.0)] {
            let ctx = egui::Context::default();
            let mut editor = editor(&ctx);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let anchor =
                egui::Rect::from_min_size(egui::pos2(200.0, size.y - 36.0), egui::vec2(96.0, 22.0));
            editor.shell.resize_anchor = Some(anchor);
            editor.begin_resize();
            let original = serde_json::to_vec(&editor.session.document).unwrap();
            let run = |editor: &mut Editor, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |_| editor.show_resize(&ctx),
                );
                output.textures_delta.clear();
            };
            for _ in 0..3 {
                run(&mut editor, vec![]);
            }
            let rect = egui::AreaState::load(&ctx, egui::Id::unique("resize_image"))
                .unwrap()
                .rect();
            assert!(screen.contains_rect(rect));
            assert!(
                (rect.center().x - anchor.center().x).abs() < 1.0,
                "popover {rect:?}, anchor {anchor:?}"
            );
            assert!(rect.bottom() <= anchor.top());
            let outside = egui::pos2(size.x - 30.0, 100.0);
            assert!(!rect.contains(outside));
            for pressed in [true, false] {
                run(
                    &mut editor,
                    vec![
                        egui::Event::PointerMoved(outside),
                        egui::Event::PointerButton {
                            pos: outside,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(editor.resize_image.is_none());
            assert_eq!(
                serde_json::to_vec(&editor.session.document).unwrap(),
                original
            );
            assert!(!editor.session.is_modified());
            assert!(!editor.session.can_undo());
        }
    }

    #[test]
    fn flatten_shortcut_requires_exact_modifiers_and_respects_drafts_before_one_step_undo() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let original: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        let pixels = original.render().unwrap();
        editor.session = capture_core::Session::new(original.clone()).unwrap();
        let chord = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        let run = |editor: &mut Editor, modifiers| {
            for pressed in [true, false] {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        events: vec![egui::Event::Key {
                            key: egui::Key::T,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers,
                        }],
                        ..Default::default()
                    },
                    |_| editor.flatten_shortcut(&ctx),
                );
                output.textures_delta.clear();
            }
        };
        run(&mut editor, egui::Modifiers::COMMAND);
        run(&mut editor, chord | egui::Modifiers::ALT);
        assert_eq!(editor.session.document.annotations, original.annotations);
        editor.begin_resize();
        run(&mut editor, chord);
        editor.resize_image = None;
        editor.begin_resize_canvas();
        run(&mut editor, chord);
        editor.resize_canvas = None;
        editor.new_image = Some(crate::new_image::NewImage::default());
        run(&mut editor, chord);
        editor.new_image = None;
        editor.shell.help = true;
        run(&mut editor, chord);
        editor.shell.help = false;
        assert_eq!(editor.session.document.annotations, original.annotations);
        assert!(!editor.session.can_undo());
        run(&mut editor, chord);
        assert!(editor.session.document.annotations.is_empty());
        assert_eq!(editor.session.document.render().unwrap(), pixels);
        editor.execute(Command::Undo);
        assert_eq!(editor.status, "Undo complete. 7 editable objects.");
        assert_eq!(editor.session.document.annotations, original.annotations);
        assert!(!editor.session.is_modified());
    }

    #[test]
    fn resize_confirmation_preserves_document_and_redo_and_escape_cancels_everything() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        let original: capture_core::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        editor.session = capture_core::Session::new(original.clone()).unwrap();
        editor.execute(Command::FlattenAll);
        editor.execute(Command::Undo);
        let before = serde_json::to_vec(&editor.session.document).unwrap();
        editor.begin_resize();
        editor.request_resize([1, 1]);
        let run = |editor: &mut Editor, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(981.0, 687.0),
                    )),
                    focused: true,
                    events,
                    ..Default::default()
                },
                |_| {
                    editor.flatten_shortcut(&ctx);
                    editor.show_resize(&ctx);
                },
            );
            output.textures_delta.clear();
        };
        for _ in 0..3 {
            run(&mut editor, vec![]);
        }
        assert!(
            egui::AreaState::load(&ctx, egui::Id::unique("resize_flatten_confirmation")).is_some()
        );
        let key = |key, modifiers| egui::Event::Key {
            key,
            modifiers,
            physical_key: None,
            pressed: true,
            repeat: false,
        };
        run(
            &mut editor,
            vec![key(
                egui::Key::T,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            )],
        );
        assert!(editor.resize_image.is_some());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            before
        );
        assert!(editor.session.can_redo());
        assert!(!editor.session.can_undo());
        run(
            &mut editor,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(editor.resize_image.is_none());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            before
        );
        assert!(editor.session.can_redo());
        assert!(!editor.session.is_modified());
    }

    #[test]
    fn compact_new_image_outside_click_and_escape_preserve_existing_document_and_history() {
        let ctx = egui::Context::default();
        let mut editor = editor(&ctx);
        editor.session = capture_core::Session::new(
            serde_json::from_str(include_str!(
                "../../../crates/core/tests/fixtures/translation.capture.json"
            ))
            .unwrap(),
        )
        .unwrap();
        editor.execute(Command::FlattenAll);
        editor.execute(Command::Undo);
        let before = serde_json::to_vec(&editor.session.document).unwrap();
        editor.new_image = Some(crate::new_image::NewImage::default());
        let run = |editor: &mut Editor, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    events,
                    ..Default::default()
                },
                |_| editor.show_new_image(&ctx),
            );
            output.textures_delta.clear();
        };
        for _ in 0..3 {
            run(&mut editor, vec![]);
        }
        let bounds = egui::AreaState::load(&ctx, egui::Id::unique("new_image"))
            .unwrap()
            .rect();
        assert!(
            bounds.width() <= 385.0 && bounds.height() <= 400.0,
            "{bounds:?}"
        );
        let outside = egui::pos2(10.0, 10.0);
        assert!(!bounds.contains(outside));
        for pressed in [true, false] {
            run(
                &mut editor,
                vec![
                    egui::Event::PointerMoved(outside),
                    egui::Event::PointerButton {
                        pos: outside,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(editor.new_image.is_some());
        run(
            &mut editor,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(editor.new_image.is_none());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            before
        );
        assert!(editor.session.can_redo());
        assert!(!editor.session.can_undo());
        assert!(!editor.session.is_modified());
    }

    #[test]
    fn initial_line_arrow_and_shape_shift_drawing_preview_commit_and_undo_agree() {
        use egui::{Event, Modifiers, PointerButton};
        for kind in [
            Kind::Line,
            Kind::Arrow,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
        ] {
            for held in [true, false] {
                let ctx = egui::Context::default();
                let mut e = editor(&ctx);
                e.tool = Some(kind);
                let run = |e: &mut Editor, mut events: Vec<Event>, shift| {
                    events.insert(
                        0,
                        Event::ModifiersChanged(if shift {
                            Modifiers::SHIFT
                        } else {
                            Modifiers::NONE
                        }),
                    );
                    let mut viewport = egui::Rect::NOTHING;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(600.0, 400.0),
                            )),
                            events,
                            focused: true,
                            ..Default::default()
                        },
                        |ui| {
                            viewport = ui.available_rect_before_wrap();
                            e.show_canvas(ui);
                        },
                    );
                    output.textures_delta.clear();
                    (
                        e.view.transform(
                            viewport,
                            egui::vec2(640.0, 400.0),
                            ctx.pixels_per_point(),
                        ),
                        output,
                    )
                };
                let (t, _) = run(&mut e, vec![], false);
                let start = [80.0, 80.0];
                let raw = [220.0, 124.0];
                let a = t.screen_point(start);
                let b = t.screen_point(raw);
                let button = |pos, pressed, shift| Event::PointerButton {
                    pos,
                    pressed,
                    button: PointerButton::Primary,
                    modifiers: if shift {
                        Modifiers::SHIFT
                    } else {
                        Modifiers::NONE
                    },
                };
                run(
                    &mut e,
                    vec![Event::PointerMoved(a), button(a, true, false)],
                    false,
                );
                // Shift can be pressed or released after the initial mouse down.
                let (_, preview) = run(&mut e, vec![Event::PointerMoved(b)], held);
                let endpoint =
                    if held && matches!(kind, Kind::Rectangle | Kind::Ellipse | Kind::Polygon) {
                        capture_core::angle_snap::square_endpoint(start, raw, [640, 400])
                    } else {
                        capture_core::angle_snap::endpoint(start, raw, [640, 400], held)
                    };
                assert!(
                    preview
                        .shapes
                        .iter()
                        .any(|s| matches!(&s.shape, egui::Shape::Mesh(_)))
                );
                // Toggle the modifier with a stationary pointer, then restore it.
                run(&mut e, vec![], !held);
                run(&mut e, vec![], held);
                let shown = e.creation.annotation.as_ref().unwrap().end;
                assert!(
                    (shown[0] - endpoint[0]).abs() < 0.01 && (shown[1] - endpoint[1]).abs() < 0.01
                );
                assert!(e.session.document.annotations.is_empty());
                // Modifier attached to release wins even if the global snapshot has reset.
                run(&mut e, vec![button(b, false, held)], false);
                let object = e.session.document.annotations.last().unwrap();
                assert!(
                    (object.end[0] - endpoint[0]).abs() < 0.01
                        && (object.end[1] - endpoint[1]).abs() < 0.01
                );
                assert_eq!(object.kind, kind);
                assert_eq!(e.selected, Some(object.id));
                let reopened: capture_core::Document =
                    serde_json::from_slice(&serde_json::to_vec(&e.session.document).unwrap())
                        .unwrap();
                reopened.validate().unwrap();
                assert_eq!(
                    reopened.render().unwrap(),
                    e.session.document.render().unwrap()
                );
                e.execute(Command::Undo);
                assert!(e.session.document.annotations.is_empty());
                assert!(!e.session.can_undo());
            }
        }
    }
    #[test]
    fn format_painter_click_with_step_tool_does_not_place_another_step() {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        let mut first = capture_core::steps::marker(
            1,
            [100.0, 100.0],
            44,
            [200, 30, 40, 255],
            (1, false),
            [640, 400],
        )
        .unwrap();
        first.text.as_mut().unwrap().bold = true;
        let second = capture_core::steps::marker(
            2,
            [260.0, 150.0],
            44,
            [20, 70, 160, 255],
            (2, false),
            [640, 400],
        )
        .unwrap();
        e.session.document.annotations = vec![first, second];
        let before = e.session.document.annotations.clone();
        e.select_only(Some(1));
        e.tool = Some(Kind::Step);
        e.arm_format_painter();
        let t = frame(&ctx, &mut e, vec![]);
        let p = t.screen_point([260.0, 150.0]);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut e,
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(e.session.document.annotations.len(), 2);
        assert_eq!(e.session.document.annotations[1].color, before[0].color);
        assert_eq!(
            e.session.document.annotations[1]
                .text
                .as_ref()
                .unwrap()
                .content,
            "2"
        );
        assert!(e.quick_controls.format_source.is_none());
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        assert!(!e.session.can_undo());
    }

    #[test]
    fn locked_painter_survives_repeated_canvas_clicks_without_new_objects() {
        let ctx = egui::Context::default();
        let mut e = editor(&ctx);
        e.session.document.annotations = (1..=3)
            .map(|id| {
                capture_core::steps::marker(
                    id,
                    [120.0 * id as f32, 140.0],
                    44,
                    [id as u8 * 50, 40, 100, 255],
                    (id as u32, false),
                    [640, 400],
                )
                .unwrap()
            })
            .collect();
        let before = e.session.document.annotations.clone();
        e.select_only(Some(1));
        e.tool = Some(Kind::Step);
        e.lock_format_painter();
        for id in [2, 3] {
            let t = frame(&ctx, &mut e, vec![]);
            let p = t.screen_point([120.0 * id as f32, 140.0]);
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut e,
                    vec![
                        egui::Event::PointerMoved(p),
                        egui::Event::PointerButton {
                            pos: p,
                            pressed,
                            button: egui::PointerButton::Primary,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
            assert_eq!(e.session.document.annotations.len(), 3);
            assert_eq!(
                e.session.document.annotations[id - 1].color,
                before[0].color
            );
            assert_eq!(
                e.session.document.annotations[id - 1]
                    .text
                    .as_ref()
                    .unwrap()
                    .content,
                id.to_string()
            );
            assert_eq!(e.quick_controls.format_source, Some(before[0].clone()));
            assert!(e.quick_controls.format_locked);
        }
        e.arm_format_painter();
        assert!(e.quick_controls.format_source.is_none());
        e.execute(Command::Undo);
        assert_ne!(e.session.document.annotations, before);
        e.execute(Command::Undo);
        assert_eq!(e.session.document.annotations, before);
        assert!(!e.session.can_undo());
    }

    #[test]
    fn creation_uses_actual_style_and_alt_callout_tail_commits_identical_object() {
        use egui::{Event, Modifiers, PointerButton};
        for (kind, reverse) in [Kind::Rectangle, Kind::Ellipse, Kind::Callout, Kind::Text]
            .into_iter()
            .flat_map(|kind| [false, true].map(|reverse| (kind, reverse)))
        {
            let ctx = egui::Context::default();
            let mut e = editor(&ctx);
            e.tool = Some(kind);
            let mut style = e.new_annotation(kind, [30.0, 30.0], [180.0, 120.0]);
            style.color = [12, 80, 160, 255];
            style.width = 7;
            if matches!(kind, Kind::Rectangle | Kind::Ellipse) {
                style.fill = Some([40, 190, 110, 170]);
            }
            if matches!(kind, Kind::Rectangle | Kind::Callout) {
                style.corner_radius = 20.0;
            }
            let template = capture_core::object_styles::ObjectStyle::from_annotation(
                "Preview fixture".into(),
                &style,
            )
            .unwrap();
            e.apply_object_template(&template, false);
            let t = frame(&ctx, &mut e, vec![]);
            let a = t.screen_point(if reverse {
                [230.0, 160.0]
            } else {
                [60.0, 60.0]
            });
            let b = t.screen_point(if reverse {
                [60.0, 60.0]
            } else {
                [230.0, 160.0]
            });
            let button = |pos, pressed, modifiers| Event::PointerButton {
                pos,
                pressed,
                button: PointerButton::Primary,
                modifiers,
            };
            frame(
                &ctx,
                &mut e,
                vec![Event::PointerMoved(a), button(a, true, Modifiers::NONE)],
            );
            frame(&ctx, &mut e, vec![Event::PointerMoved(b)]);
            let preview = e.creation.annotation.clone().unwrap();
            assert_eq!(preview.color, style.color);
            assert_eq!(preview.fill, style.fill);
            assert_eq!(preview.corner_radius, style.corner_radius);
            assert_eq!(preview.width, 7);
            assert!(e.session.document.annotations.is_empty());
            let release = if kind == Kind::Callout {
                let c = t.screen_point([360.0, 230.0]);
                frame(
                    &ctx,
                    &mut e,
                    vec![
                        Event::ModifiersChanged(Modifiers::ALT),
                        Event::PointerMoved(c),
                    ],
                );
                let tail = e.creation.annotation.as_ref().unwrap();
                assert_eq!(tail.start, preview.start);
                assert_eq!(tail.end, preview.end);
                assert_eq!(tail.callout.as_ref().unwrap().pointer, [360.0, 230.0]);
                c
            } else {
                b
            };
            let shown = e.creation.annotation.clone().unwrap();
            frame(
                &ctx,
                &mut e,
                vec![button(
                    release,
                    false,
                    if kind == Kind::Callout {
                        Modifiers::ALT
                    } else {
                        Modifiers::NONE
                    },
                )],
            );
            assert_eq!(e.session.document.annotations, vec![shown]);
            assert!(e.commit_text());
            e.execute(Command::Undo);
            assert!(e.session.document.annotations.is_empty());
            assert!(!e.session.can_undo());
        }
    }
}
