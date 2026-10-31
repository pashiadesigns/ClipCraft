use crate::{Command, Editor, canvas_view::Transform, egui};
use capture_core::cut_out::CutAxis;

pub(super) struct CutOut {
    axis: CutAxis,
    origin: Option<u32>,
    band: Option<[u32; 2]>,
    review: bool,
}
impl CutOut {
    pub(super) fn cursor(&self) -> egui::CursorIcon {
        if self.review {
            egui::CursorIcon::Default
        } else if self.axis == CutAxis::Horizontal {
            egui::CursorIcon::ResizeVertical
        } else {
            egui::CursorIcon::ResizeHorizontal
        }
    }
    pub(super) fn busy(&self) -> bool {
        self.dragging() || self.review
    }
    pub(super) fn dragging(&self) -> bool {
        self.origin.is_some()
    }
    fn reset(&mut self) {
        self.origin = None;
        self.band = None;
        self.review = false;
    }
}

impl Editor {
    pub(super) fn begin_cut_out(&mut self) {
        self.cancel_gesture();
        self.fill_tool.active = false;
        self.select_only(None);
        self.tool = None;
        self.cut_out = Some(CutOut {
            axis: CutAxis::Horizontal,
            origin: None,
            band: None,
            review: false,
        });
        self.status="Cut Out: drag across a horizontal strip. The joined image previews live; release applies or reviews flattening.".into();
    }
    fn apply_cut_out(&mut self) {
        let Some(cut) = self.cut_out.as_ref() else {
            return;
        };
        let Some([start, end]) = cut.band else { return };
        let result = self.session.execute(Command::CutOut {
            axis: cut.axis,
            start,
            end,
        });
        match result {
            Ok(()) => {
                self.select_only(None);
                self.needs_render = true;
                self.view = crate::canvas_view::CanvasView::default();
                self.status = format!(
                    "Cut Out complete: {} × {} px. Undo restores the image and objects.",
                    self.session.document.width, self.session.document.height
                );
            }
            Err(error) => self.status = error.to_string(),
        }
        if let Some(cut) = self.cut_out.as_mut() {
            cut.reset();
        }
    }
    pub(super) fn cut_out_properties(&mut self, ui: &mut egui::Ui) {
        crate::property_layout::section_help(
            ui,
            "Cut Out Properties",
            "Drag in either direction. The remaining sides join live. Release keeps one edit; images with objects require flattening review. Undo restores editability; original pixels remain in the project. Torn, wave and zigzag edges and reference preset matching are planned.",
        );
        crate::shell::badge(ui, self.shell.badges, "EDIT-044");
        let Some(cut) = self.cut_out.as_mut() else {
            return;
        };
        let previous = cut.axis;
        ui.horizontal(|ui| {
            ui.selectable_value(&mut cut.axis, CutAxis::Horizontal, "Horizontal");
            ui.selectable_value(&mut cut.axis, CutAxis::Vertical, "Vertical");
        });
        if previous != cut.axis {
            cut.reset();
        }
        ui.label("Straight edge");
        let size = [self.session.document.width, self.session.document.height];
        if let Some([start, end]) = cut.band {
            if let Ok(new) = cut.axis.output_size(size, start, end) {
                ui.label(format!(
                    "Remove {} px · {} × {} px",
                    end - start,
                    new[0],
                    new[1]
                ));
            } else {
                ui.label("Keep at least one row or column.");
            }
        }
        let mut apply = false;
        if cut.review {
            ui.separator();
            ui.label(format!(
                "This merges {} editable objects into the image.",
                self.session.document.annotations.len()
            ));
            apply = ui.button("Flatten and cut out").clicked();
            if ui.button("Cancel cut out").clicked() {
                cut.reset();
                self.status = "Cut Out cancelled. Image unchanged.".into();
            }
        }
        ui.separator();
        crate::shell::badge(ui, self.shell.badges, "EDIT-016");
        if apply {
            self.apply_cut_out();
        }
    }

    pub(super) fn cut_out_canvas(
        &mut self,
        ui: &mut egui::Ui,
        response: &egui::Response,
        transform: Transform,
        interactive: bool,
        texture: egui::TextureId,
    ) {
        let size = [self.session.document.width, self.session.document.height];
        let Some(cut) = self.cut_out.as_mut() else {
            return;
        };
        let axis = cut.axis.dimension();
        let visible = transform.image.intersect(transform.viewport);
        let coordinate = |p: egui::Pos2| {
            transform.image_point(p)[axis]
                .round()
                .clamp(0.0, size[axis] as f32) as u32
        };
        let mut apply = false;
        if interactive {
            if ui.input(|i| !i.focused || i.key_pressed(egui::Key::Escape)) {
                cut.reset();
            } else if !cut.review
                && !self.view.panning
                && !ui.input(|i| i.key_down(egui::Key::Space))
            {
                if response.drag_started_by(egui::PointerButton::Primary)
                    && let Some(pos) = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|p| visible.contains(*p))
                {
                    cut.origin = Some(coordinate(pos));
                }
                if let Some(origin) = cut.origin
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let end = coordinate(pos);
                    cut.band = Some([origin.min(end), origin.max(end)]);
                    ui.ctx().request_repaint();
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && cut.origin.take().is_some()
                {
                    if let Some([start, end]) = cut.band
                        && cut.axis.output_size(size, start, end).is_ok()
                    {
                        cut.review = !self.session.document.annotations.is_empty();
                        apply = !cut.review;
                    } else {
                        cut.reset();
                    }
                }
            }
        }
        if let Some([start, end]) = cut.band
            && let Ok(new) = cut.axis.output_size(size, start, end)
        {
            // Preview uses two slices of the already rendered texture, without
            // raster encodes or history writes on pointer movement. Coordinates
            // remain anchored to the original image throughout the gesture.
            paint_joined_preview(ui, transform, texture, size, new, cut.axis, [start, end]);
        } else if response.hovered()
            && let Some(pos) = response.hover_pos()
        {
            let n = coordinate(pos) as f32;
            let mut a = [0.0, 0.0];
            let mut b = [size[0] as f32, size[1] as f32];
            a[axis] = n;
            b[axis] = n;
            ui.painter().with_clip_rect(visible).line_segment(
                [transform.screen_point(a), transform.screen_point(b)],
                egui::Stroke::new(1.0, egui::Color32::LIGHT_BLUE),
            );
        }
        if apply {
            self.apply_cut_out();
            ui.ctx().request_repaint();
        }
    }
}

fn paint_joined_preview(
    ui: &mut egui::Ui,
    transform: Transform,
    texture: egui::TextureId,
    size: [u32; 2],
    new: [u32; 2],
    axis: CutAxis,
    [start, end]: [u32; 2],
) {
    let painter = ui
        .painter()
        .with_clip_rect(transform.viewport.intersect(transform.image));
    painter.rect_filled(transform.image, 0.0, crate::shell::CANVAS);
    let frame = egui::Rect::from_min_size(
        transform.image.min,
        egui::vec2(new[0] as f32, new[1] as f32) * transform.scale,
    );
    painter.rect_filled(frame, 0.0, egui::Color32::from_gray(205));
    let p = painter.with_clip_rect(frame.intersect(transform.viewport));
    let first = (frame.intersect(transform.viewport).min - frame.min) / 16.0;
    let last = (frame.intersect(transform.viewport).max - frame.min) / 16.0;
    for row in first.y.floor() as u32..last.y.ceil() as u32 {
        for col in first.x.floor() as u32..last.x.ceil() as u32 {
            if (row + col) % 2 == 0 {
                p.rect_filled(
                    egui::Rect::from_min_size(
                        frame.min + egui::vec2(col as f32, row as f32) * 16.0,
                        egui::vec2(16.0, 16.0),
                    ),
                    0.0,
                    egui::Color32::from_gray(240),
                );
            }
        }
    }
    let i = axis.dimension();
    for (source_start, source_end, dest_start) in [(0, start, 0), (end, size[i], start)] {
        if source_start == source_end {
            continue;
        }
        let mut lo = [0.0, 0.0];
        let mut hi = [size[0] as f32, size[1] as f32];
        lo[i] = dest_start as f32;
        hi[i] = (dest_start + source_end - source_start) as f32;
        let dest = egui::Rect::from_min_max(transform.screen_point(lo), transform.screen_point(hi));
        let mut uvlo = [0.0, 0.0];
        let mut uvhi = [1.0, 1.0];
        uvlo[i] = source_start as f32 / size[i] as f32;
        uvhi[i] = source_end as f32 / size[i] as f32;
        p.image(
            texture,
            dest,
            egui::Rect::from_min_max(egui::pos2(uvlo[0], uvlo[1]), egui::pos2(uvhi[0], uvhi[1])),
            egui::Color32::WHITE,
        );
    }
    let mut a = [0.0, 0.0];
    let mut b = [new[0] as f32, new[1] as f32];
    a[i] = start as f32;
    b[i] = start as f32;
    painter.line_segment(
        [transform.screen_point(a), transform.screen_point(b)],
        egui::Stroke::new(1.0, egui::Color32::LIGHT_BLUE),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_cut_preview_waits_for_review_and_one_undo_restores_objects() {
        use egui::{Event, Modifiers, PointerButton};
        let ctx = egui::Context::default();
        let mut editor = crate::canvas::tests::editor(&ctx);
        let doc: crate::Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/translation.capture.json"
        ))
        .unwrap();
        let original = serde_json::to_vec(&doc).unwrap();
        editor.session = crate::Session::new(doc).unwrap();
        editor.begin_cut_out();
        let frame = |editor: &mut Editor, events| {
            let mut viewport = egui::Rect::NOTHING;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
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
            out.textures_delta.clear();
            editor
                .view
                .transform(viewport, egui::vec2(640.0, 400.0), ctx.pixels_per_point())
        };
        let t = frame(&mut editor, vec![]);
        let start = t.screen_point([300.0, 260.0]);
        let end = t.screen_point([300.0, 100.0]);
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![Event::PointerMoved(start), button(start, true)],
        );
        frame(&mut editor, vec![Event::PointerMoved(end)]);
        assert_eq!(editor.cut_out.as_ref().unwrap().band, Some([100, 260]));
        assert!(editor.cut_out.as_ref().unwrap().dragging());
        assert!(!editor.editor_shortcuts_idle());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        frame(&mut editor, vec![button(end, false)]);
        assert!(editor.cut_out.as_ref().unwrap().review);
        assert!(!editor.editor_shortcuts_idle());
        assert!(!editor.session.can_undo());
        editor.apply_cut_out();
        assert_eq!(editor.session.document.height, 240);
        assert!(editor.session.document.annotations.is_empty());
        editor.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        assert!(!editor.session.can_undo());
        // A plain raster needs no flattening review and commits on release.
        editor.session = crate::Session::new(crate::Document::blank(640, 400).unwrap()).unwrap();
        editor.begin_cut_out();
        editor.cut_out.as_mut().unwrap().axis = CutAxis::Vertical;
        let transform = frame(&mut editor, vec![]);
        let a = transform.screen_point([400.0, 200.0]);
        let b = transform.screen_point([250.0, 200.0]);
        frame(&mut editor, vec![Event::PointerMoved(a), button(a, true)]);
        frame(&mut editor, vec![Event::PointerMoved(b)]);
        assert_eq!(editor.session.document.width, 640);
        frame(&mut editor, vec![button(b, false)]);
        assert_eq!(editor.session.document.width, 490);
        assert!(!editor.cut_out.as_ref().unwrap().review);
        assert!(editor.cut_out.as_ref().unwrap().band.is_none());
        assert!(editor.editor_shortcuts_idle());
        editor.execute(Command::Undo);
        assert_eq!(editor.session.document.width, 640);
        assert!(!editor.session.can_undo());
    }
}
