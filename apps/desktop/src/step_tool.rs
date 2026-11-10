use crate::{Command, Editor, Kind, egui};

/// Shared press-origin resolution for click-and-place tools (Step and Icon).
pub(super) fn placement_origin(ui: &egui::Ui) -> Option<egui::Pos2> {
    ui.input(|i| {
        i.events
            .iter()
            .find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } => Some(*pos),
                _ => None,
            })
            .or_else(|| i.pointer.press_origin())
            .or_else(|| i.pointer.interact_pos())
    })
}

#[derive(Clone)]
struct CursorPreview {
    annotation: crate::Annotation,
    pixels: [u32; 2],
    texture: egui::TextureHandle,
}

impl Editor {
    /// Project-scoped placement state must not leak out of a guide into the
    /// next capture, template, new canvas or Library project.
    pub(super) fn reset_document_tools(&mut self) {
        self.quick_controls.snap_recent.clear();
        self.icon_library.reset_document();
        self.step_placement = None;
        self.step_letters = capture_core::steps::highest_number(&self.session.document, false)
            .is_none()
            && capture_core::steps::highest_number(&self.session.document, true).is_some();
        self.step_number =
            capture_core::steps::next_number(&self.session.document, self.step_letters);
    }

    pub(super) fn paint_step_cursor(
        &self,
        ui: &egui::Ui,
        transform: crate::canvas_view::Transform,
        pointer: egui::Pos2,
    ) -> bool {
        let shift = ui.input(|i| i.modifiers.shift);
        let point = self.constrain_placement(Kind::Step, transform.image_point(pointer), shift);
        let Ok(mut placed) = self.step_marker(point) else {
            return false;
        };
        let center = [
            (placed.start[0] + placed.end[0]) * 0.5,
            (placed.start[1] + placed.end[1]) * 0.5,
        ];
        if let Ok(mut result) = crate::canvas_expansion::move_with_candidates(
            &self.session.document,
            std::slice::from_ref(&placed),
            [point[0] - center[0], point[1] - center[1]],
            self.quick_controls
                .snapping()
                .then_some(6. / transform.scale),
            true,
            self.quick_controls.snap_candidates(),
        ) {
            placed = result.annotations.remove(0);
        }
        let center = [
            (placed.start[0] + placed.end[0]) * 0.5,
            (placed.start[1] + placed.end[1]) * 0.5,
        ];
        let center = self.constrain_placement(Kind::Step, center, shift);
        // Canonical local geometry keeps pointer movement out of the texture key.
        let Ok(a) = self.step_marker([0., 0.]) else {
            return false;
        };
        let (mut lo, mut hi) = a.bounds();
        let marker_center = [(a.start[0] + a.end[0]) * 0.5, (a.start[1] + a.end[1]) * 0.5];
        if let Some((shadow_lo, shadow_hi)) = capture_core::shadow::bounds(&a) {
            for axis in 0..2 {
                lo[axis] = lo[axis].min(shadow_lo[axis]);
                hi[axis] = hi[axis].max(shadow_hi[axis]);
            }
        }
        lo = lo.map(|v| v - 2.);
        hi = hi.map(|v| v + 2.);
        let extent = egui::vec2(hi[0] - lo[0], hi[1] - lo[1]);
        let density =
            (transform.scale * ui.ctx().pixels_per_point()).min(1024. / extent.max_elem());
        let pixels = [
            (extent.x * density).ceil().max(1.) as u32,
            (extent.y * density).ceil().max(1.) as u32,
        ];
        let key = egui::Id::unique("step-placement-preview");
        let mut cached = ui.ctx().data(|d| d.get_temp::<CursorPreview>(key));
        if cached
            .as_ref()
            .is_none_or(|c| c.annotation != a || c.pixels != pixels)
        {
            let Ok(image) = capture_core::display::render_annotation_preview(
                &a,
                capture_core::display::Viewport {
                    size: pixels,
                    origin: lo.map(f64::from),
                    scale: f64::from(density),
                },
            ) else {
                return false;
            };
            let texture = ui.ctx().load_texture(
                "step-placement-preview",
                egui::ColorImage::from_rgba_unmultiplied(
                    [pixels[0] as usize, pixels[1] as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::LINEAR,
            );
            cached = Some(CursorPreview {
                annotation: a,
                pixels,
                texture,
            });
            ui.ctx()
                .data_mut(|d| d.insert_temp(key, cached.clone().unwrap()));
        }
        let min = transform.screen_point([
            center[0] + lo[0] - marker_center[0],
            center[1] + lo[1] - marker_center[1],
        ]);
        let rect = egui::Rect::from_min_size(min, extent * transform.scale);
        let c = cached.unwrap();
        // Exactly one mark while placing: the marker as it will be placed. The
        // system cursor stays hidden so nothing else alternates with it.
        ui.painter()
            .with_clip_rect(transform.viewport.intersect(transform.image))
            .image(
                c.texture.id(),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                egui::Color32::WHITE,
            );
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("step-hover-preview"),
                (c.annotation.clone(), rect, c.texture.id()),
            )
        });
        true
    }

    /// A newly pressed Step is visible immediately, but creation and numbering
    /// form one document transaction when the pointer is released.
    pub(super) fn step_placement_input(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        transform: crate::canvas_view::Transform,
        base_transform: crate::canvas_view::Transform,
    ) -> bool {
        if self.tool != Some(Kind::Step) {
            return false;
        }
        if self.step_placement.is_none() && ui.input(|i|i.events.iter().any(|event|matches!(event,
            egui::Event::PointerButton {button:egui::PointerButton::Primary,pressed:true,modifiers,..} if self.gesture_matches(crate::gesture_input::Gesture::DuplicateDrag,*modifiers)))) {
            return false;
        }
        let (focused, pressed, released, pointer, additive, space) = ui.input(|i| {
            (
                i.focused,
                i.pointer.button_pressed(egui::PointerButton::Primary),
                i.pointer.button_released(egui::PointerButton::Primary),
                i.pointer.interact_pos(),
                i.modifiers.command || i.modifiers.alt,
                i.key_down(egui::Key::Space),
            )
        });
        if !focused {
            let handled = self.step_placement.is_some();
            self.cancel_gesture();
            return handled;
        }
        if pressed
            && !additive
            && !space
            && !self.view.panning
            && self.edit_drag.is_none()
            && self.group_drag.is_none()
            && let Some(pointer) = placement_origin(ui)
            && transform
                .viewport
                .intersect(transform.image)
                .contains(pointer)
        {
            let point = transform.image_point(pointer);
            let tolerance = 7.0 / transform.scale;
            let on_handle = self
                .selected_annotation()
                .is_some_and(|a| a.kind == Kind::Step && a.hit_handle(point, tolerance).is_some());
            if !on_handle
                && self
                    .tool_hit(
                        base_transform.image_point(pointer),
                        7. / base_transform.scale,
                    )
                    .is_none()
            {
                match self.step_marker(point) {
                    Ok(step) => {
                        self.select_only(None);
                        self.step_placement = Some(step);
                        self.needs_render = true;
                    }
                    Err(error) => self.status = error.to_string(),
                }
            }
        }
        let pointer = pointer.map(|p| {
            transform.screen_point(self.constrain_placement(
                Kind::Step,
                transform.image_point(p),
                ui.input(|i| i.modifiers.shift),
            ))
        });
        let anchor = self.icon_library.anchor;
        let Some(step) = &mut self.step_placement else {
            return false;
        };
        if let Some(pointer) = pointer.or_else(|| response.interact_pointer_pos()) {
            let point = transform.image_point(pointer);
            let center = [
                (step.start[0] + step.end[0]) * 0.5,
                (step.start[1] + step.end[1]) * 0.5,
            ];
            if let Ok(mut result) = crate::canvas_expansion::move_with_candidates(
                &self.session.document,
                std::slice::from_ref(step),
                [point[0] - center[0], point[1] - center[1]],
                self.quick_controls
                    .snapping()
                    .then_some(6.0 / transform.scale),
                true,
                self.quick_controls.snap_candidates(),
            ) {
                self.snapping_guides = result.guides;
                let mut moved = result.annotations.remove(0);
                if ui.input(|i| i.modifiers.shift)
                    && let Some((Kind::Step, anchor)) = anchor
                {
                    let c = [
                        (moved.start[0] + moved.end[0]) * 0.5,
                        (moved.start[1] + moved.end[1]) * 0.5,
                    ];
                    let axis =
                        usize::from((point[0] - anchor[0]).abs() > (point[1] - anchor[1]).abs());
                    moved.start[axis] += anchor[axis] - c[axis];
                    moved.end[axis] += anchor[axis] - c[axis];
                }
                if moved != *step {
                    *step = moved;
                    self.needs_render = true;
                }
            }
        }
        if released {
            let step = self.step_placement.take().unwrap();
            self.commit_step(step, Some(transform));
        }
        ui.ctx().request_repaint();
        true
    }

    pub(super) fn step_marker(&self, point: [f32; 2]) -> capture_core::Result<crate::Annotation> {
        capture_core::steps::marker(
            self.session.next_id(),
            point,
            self.step_size,
            [self.color[0], self.color[1], self.color[2], 255],
            (self.next_step_number(), self.step_letters),
            [self.session.document.width, self.session.document.height],
        )
        .map(|a| {
            let mut a = self.styled_new_annotation(a);
            // A remembered style carries its label at the size it was made for.
            // The label keeps that proportion as the marker is resized.
            let side = (a.end[0] - a.start[0]).abs();
            if let Some(now) = a.text.as_ref().map(|t| t.size_points) {
                let ratio = if self.step_style_size > 0 {
                    side / self.step_style_size as f32
                } else {
                    capture_core::steps::label_size(side) / now
                };
                capture_core::steps::scale_label(&mut a, ratio);
            }
            a
        })
    }

    fn commit_step(
        &mut self,
        annotation: crate::Annotation,
        transform: Option<crate::canvas_view::Transform>,
    ) {
        let id = annotation.id;
        if let Some(transform) = transform {
            self.commit_expanding_move(vec![annotation], true, transform);
        } else {
            self.execute(Command::Add { annotation });
        }
        self.needs_render = true;
        if self.session.document.annotations.iter().any(|a| a.id == id) {
            self.select_only(Some(id));
            self.sync_step_sequence();
        }
    }

    #[cfg(test)]
    pub(super) fn add_step(&mut self, point: [f32; 2]) {
        match self.step_marker(point) {
            Ok(annotation) => {
                self.commit_step(annotation, None);
            }
            Err(error) => self.status = error.to_string(),
        }
    }
    pub(super) fn step_properties(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let numeric = ui
                .selectable_value(&mut self.step_letters, false, "1, 2, 3")
                .clicked();
            let letters = ui
                .selectable_value(&mut self.step_letters, true, "A, B, C")
                .clicked();
            if numeric || letters {
                self.step_number =
                    capture_core::steps::next_number(&self.session.document, self.step_letters);
            }
        });
        let has_sequence =
            capture_core::steps::highest_number(&self.session.document, self.step_letters)
                .is_some();
        if has_sequence {
            self.sync_step_sequence();
        }
        ui.horizontal(|ui| {
            if has_sequence {
                ui.label(if self.step_number <= 999 {
                    format!(
                        "Next step: {}",
                        capture_core::steps::label(self.step_number, self.step_letters).unwrap()
                    )
                } else {
                    "Sequence limit reached (999)".into()
                });
            } else {
                let label = ui.label("Start at");
                ui.add(egui::DragValue::new(&mut self.step_number).range(1..=999))
                    .labelled_by(label.id);
            }
        });
        if self.selected_annotation().is_none() {
            crate::property_layout::slider(
                ui,
                "Marker size",
                egui::Slider::new(&mut self.step_size, 1..=16384).logarithmic(true),
            );
        }
    }
    pub(super) fn sync_step_sequence(&mut self) {
        if self.tool == Some(Kind::Step) {
            self.step_number =
                capture_core::steps::next_number(&self.session.document, self.step_letters);
        }
    }

    pub(super) fn next_step_number(&self) -> u32 {
        capture_core::steps::highest_number(&self.session.document, self.step_letters)
            .map_or(self.step_number, |number| number.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_switch_resets_sequence_for_new_captures_and_restores_existing_labels() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let root = tempfile::tempdir().unwrap();
        e.history = crate::history::HistoryPanel::isolated(root.path());
        e.tool = Some(Kind::Step);
        e.step_number = 18;
        e.add_step([100., 100.]);
        let guide = e.session.document.clone();
        for active in [Some(Kind::Step), None] {
            e.tool = active;
            e.perform_action(
                crate::FileAction::CreateCanvas(crate::new_image::CanvasSpec {
                    width: 640,
                    height: 400,
                    color: [255; 4],
                }),
                &ctx,
            );
            assert_eq!(e.next_step_number(), 1);
            e.add_step([100., 100.]);
            assert_eq!(
                e.session.document.annotations[0]
                    .text
                    .as_ref()
                    .unwrap()
                    .content,
                "1"
            );
            e.perform_action(
                crate::FileAction::HistorySnapshot {
                    session: crate::Session::new(guide.clone()).unwrap(),
                    id: "isolated-guide".into(),
                },
                &ctx,
            );
            assert_eq!(e.next_step_number(), 19);
            e.accept_capture(
                crate::Document::blank(640, 400).unwrap(),
                capture_core::capture_options::Settings {
                    preview: true,
                    clipboard: false,
                    ..Default::default()
                },
                |_| panic!("No OS clipboard operation expected"),
            );
            assert_eq!(e.next_step_number(), 1);
            // Opening a lettered project selects its own sequence; a subsequent
            // blank clipboard project starts with numeric 1 again.
            let mut letters = guide.clone();
            letters.annotations[0].text.as_mut().unwrap().content = "D".into();
            e.perform_action(
                crate::FileAction::Template {
                    session: crate::Session::new(letters).unwrap(),
                    title: "Lettered".into(),
                },
                &ctx,
            );
            assert!(e.step_letters);
            assert_eq!(
                e.step_marker([100., 100.]).unwrap().text.unwrap().content,
                "E"
            );
            e.perform_action(
                crate::FileAction::FromClipboard(
                    crate::Session::new(crate::Document::blank(640, 400).unwrap()).unwrap(),
                ),
                &ctx,
            );
            assert!(!e.step_letters);
            assert_eq!(e.next_step_number(), 1);
        }
    }

    #[test]
    fn selecting_and_editing_step_carries_size_format_and_sequence_into_next_marker() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(Kind::Step);
        e.add_step([100., 100.]);
        let mut step = e.session.document.annotations[0].clone();
        step.color = [55, 110, 190, 255];
        step.end = [step.start[0] + 80., step.start[1] + 80.];
        step.text.as_mut().unwrap().content = "D".into();
        e.execute(Command::Update {
            annotation: step.clone(),
        });
        e.select_only(Some(step.id));
        e.select_only(None);
        let next = e.step_marker([300., 100.]).unwrap();
        assert_eq!(next.color, step.color);
        assert_eq!(next.text.as_ref().unwrap().content, "E");
        assert_eq!(next.end[0] - next.start[0], 80.);
        assert_eq!(
            e.constrain_placement(Kind::Step, [400., 110.], true)[1],
            (step.start[1] + step.end[1]) * 0.5
        );
    }
    #[test]
    fn placement_reads_highest_label_after_gaps_manual_edits_reopen_and_undo() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(Kind::Step);
        e.step_number = 8;
        e.add_step([60.0, 60.0]);
        assert_eq!(
            e.session.document.annotations[0]
                .text
                .as_ref()
                .unwrap()
                .content,
            "8"
        );
        e.add_step([120.0, 60.0]);
        assert_eq!(
            e.session.document.annotations[1]
                .text
                .as_ref()
                .unwrap()
                .content,
            "9"
        );
        e.execute(Command::Undo);
        e.execute(Command::Undo);
        for (id, n) in [1, 2, 4, 8].into_iter().enumerate() {
            e.session.document.annotations.push(
                capture_core::steps::marker(
                    id as u64 + 1,
                    [60.0 + id as f32 * 70.0, 60.0],
                    40,
                    [255, 0, 0, 255],
                    (n, false),
                    [640, 400],
                )
                .unwrap(),
            );
        }
        // Deliberately stale UI counter cannot override labels on the canvas.
        e.step_number = 1;
        e.add_step([400.0, 60.0]);
        assert_eq!(
            e.session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "9"
        );
        e.execute(Command::Undo);
        assert_eq!(e.step_number, 9);
        let mut edited = e.session.document.annotations[0].clone();
        edited.text.as_mut().unwrap().content = "17".into();
        e.execute(Command::Update { annotation: edited });
        e.add_step([400.0, 120.0]);
        assert_eq!(
            e.session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "18"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sequence.json");
        e.session.save(&path).unwrap();
        e.session =
            capture_core::Session::new(capture_core::Document::load(&path).unwrap()).unwrap();
        e.step_number = 1;
        e.add_step([400.0, 180.0]);
        assert_eq!(
            e.session
                .document
                .annotations
                .last()
                .unwrap()
                .text
                .as_ref()
                .unwrap()
                .content,
            "19"
        );
        e.execute(Command::Undo);
        assert_eq!(e.step_number, 19);
        e.execute(Command::Redo);
        assert_eq!(e.step_number, 20);
    }

    #[test]
    fn letter_sequence_is_independent_and_limit_never_duplicates_last_marker() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.tool = Some(Kind::Step);
        e.step_number = 999;
        e.add_step([60.0, 60.0]);
        e.add_step([120.0, 60.0]);
        assert_eq!(e.session.document.annotations.len(), 1);
        assert!(e.status.contains("1–999"));
        e.step_letters = true;
        e.step_number = 26;
        e.add_step([120.0, 60.0]);
        e.add_step([180.0, 60.0]);
        assert_eq!(
            e.session.document.annotations[2]
                .text
                .as_ref()
                .unwrap()
                .content,
            "AA"
        );
    }
}
