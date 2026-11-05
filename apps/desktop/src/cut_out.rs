use crate::{Command, Editor, canvas_view::Transform, egui};
use capture_core::cut_out::CutAxis;

pub(super) struct CutOut {
    axis: CutAxis,
    origin: Option<u32>,
    band: Option<[u32; 2]>,
    review: bool,
    all_pages: bool,
    page: usize,
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

        self.select_only(None);
        self.tool = None;
        self.cut_out = Some(CutOut {
            axis: CutAxis::Horizontal,
            origin: None,
            band: None,
            review: false,
            all_pages: false,
            page: 0,
        });
        self.status="Cut Out: drag across a horizontal strip. The joined image previews live; release applies or reviews flattening.".into();
    }
    fn apply_cut_out(&mut self) {
        let Some(cut) = self.cut_out.as_ref() else {
            return;
        };
        let Some([start, end]) = cut.band else { return };
        let command = if self
            .session
            .document
            .guide
            .as_ref()
            .is_some_and(|g| g.pages.is_some())
        {
            Command::CutOutPages {
                axis: cut.axis,
                start,
                end,
                page: cut.page,
                all_pages: cut.all_pages,
            }
        } else {
            Command::CutOut {
                axis: cut.axis,
                start,
                end,
            }
        };
        let result = self.session.execute(command);
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
            "Drag a strip on the page to cut. Choose only that page or the same strip across all pages. Cut pages become images; other pages stay editable. Page Setup headers and footers remain live. Undo restores the original layout and objects.",
        );

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
        if crate::controls::toggle(ui, &mut cut.all_pages, "Cut across all pages").changed() {
            cut.reset();
        }
        ui.label(if cut.all_pages {
            "Cut across all pages"
        } else {
            "Cut out only selected page"
        });
        ui.small("Start dragging on the page you want to cut.");
        ui.label("Straight edge");
        let (_, size) = page_geometry(&self.session.document, cut.page);
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
                "This merges objects on {} into images. Other pages and live headers/footers remain editable.",
                if cut.all_pages { "all pages".into() } else { format!("page {}",cut.page + 1) }
            ));
            apply = ui.button("Flatten and cut out").clicked();
            if ui.button("Cancel cut out").clicked() {
                cut.reset();
                self.status = "Cut Out cancelled. Image unchanged.".into();
            }
        }
        ui.separator();

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
        let document = &self.session.document;
        let Some(cut) = self.cut_out.as_mut() else {
            return;
        };
        let axis = cut.axis.dimension();
        let visible = transform.image.intersect(transform.viewport);
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
                    let point = transform.image_point(pos);
                    cut.page = document.guide.as_ref().map_or(0, |g| g.page_at(point));
                    let (offset, size) = page_geometry(document, cut.page);
                    if (0..2)
                        .all(|i| point[i] >= offset[i] && point[i] <= offset[i] + size[i] as f32)
                    {
                        cut.origin = Some(coordinate(transform, pos, offset, size, axis));
                    }
                }
                if let Some(origin) = cut.origin
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    let (offset, size) = page_geometry(document, cut.page);
                    let end = coordinate(transform, pos, offset, size, axis);
                    cut.band = Some([origin.min(end), origin.max(end)]);
                    ui.ctx().request_repaint();
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && cut.origin.take().is_some()
                {
                    if let Some([start, end]) = cut.band
                        && cut
                            .axis
                            .output_size(page_geometry(document, cut.page).1, start, end)
                            .is_ok()
                    {
                        let validation = if document
                            .guide
                            .as_ref()
                            .is_some_and(|g| g.pages.is_some())
                        {
                            document
                                .cut_out_page_layout(cut.axis, start, end, cut.page, cut.all_pages)
                                .map(|_| ())
                        } else {
                            Ok(())
                        };
                        if let Err(error) = validation {
                            self.status = error.to_string();
                            cut.reset();
                        } else {
                            cut.review = !document.annotations.is_empty();
                            apply = !cut.review;
                        }
                    } else {
                        cut.reset();
                    }
                }
            }
        }
        if let Some([start, end]) = cut.band {
            // Preview uses two slices of the already rendered texture, without
            // raster encodes or history writes on pointer movement. Coordinates
            // remain anchored to the original image throughout the gesture.
            let count = document.guide.as_ref().map_or(1, |g| g.page_count());
            for page in 0..count {
                if !cut.all_pages && page != cut.page {
                    continue;
                }
                let (offset, size) = page_geometry(document, page);
                if let Ok(new) = cut.axis.output_size(size, start, end) {
                    let image = egui::Rect::from_min_size(
                        transform.screen_point(offset),
                        egui::vec2(size[0] as f32, size[1] as f32) * transform.scale,
                    );
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(
                            offset[0] / document.width as f32,
                            offset[1] / document.height as f32,
                        ),
                        egui::pos2(
                            (offset[0] + size[0] as f32) / document.width as f32,
                            (offset[1] + size[1] as f32) / document.height as f32,
                        ),
                    );
                    paint_joined_preview(
                        ui,
                        Transform { image, ..transform },
                        (texture, uv),
                        size,
                        new,
                        cut.axis,
                        [start, end],
                    );
                }
            }
        } else if response.hovered()
            && let Some(pos) = response.hover_pos()
        {
            let point = transform.image_point(pos);
            let page = document.guide.as_ref().map_or(0, |g| g.page_at(point));
            let (offset, size) = page_geometry(document, page);
            let n = coordinate(transform, pos, offset, size, axis) as f32;
            let mut a = offset;
            let mut b = [offset[0] + size[0] as f32, offset[1] + size[1] as f32];
            a[axis] += n;
            b[axis] = a[axis];
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
    (texture, source_uv): (egui::TextureId, egui::Rect),
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
            egui::Rect::from_min_max(
                source_uv.min + egui::vec2(uvlo[0], uvlo[1]) * source_uv.size(),
                source_uv.min + egui::vec2(uvhi[0], uvhi[1]) * source_uv.size(),
            ),
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

fn page_geometry(document: &crate::Document, page: usize) -> ([f32; 2], [u32; 2]) {
    let size = [document.width, document.height];
    document
        .guide
        .as_ref()
        .filter(|g| g.pages.is_some())
        .map_or(([0.; 2], size), |g| {
            (g.page_offset(page), g.page_size(page, size))
        })
}
fn coordinate(
    transform: Transform,
    p: egui::Pos2,
    offset: [f32; 2],
    size: [u32; 2],
    axis: usize,
) -> u32 {
    (transform.image_point(p)[axis] - offset[axis])
        .round()
        .clamp(0., size[axis] as f32) as u32
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
