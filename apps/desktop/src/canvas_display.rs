//! A display cache scoped to the visible physical viewport, not zoomed document
//! dimensions. Idle frames reuse the texture; edits invalidate both representations.
use crate::{Editor, canvas_view::Transform, egui};
use capture_core::display::{DisplayList, Viewport};

#[derive(Default)]
pub(super) struct CanvasDisplay {
    scene: crate::guide_display::Scene,
    list: Option<DisplayList>,
    texture: Option<egui::TextureHandle>,
    key: Option<Viewport>,
    failed_key: Option<Viewport>,
}

#[cfg(test)]
impl CanvasDisplay {
    pub(super) fn uses_retained_scene(&self) -> bool {
        self.list.is_none() && self.texture.is_none()
    }
}

impl Editor {
    pub(super) fn toggle_raster_preview(&mut self) {
        if !self.commit_text() {
            return;
        }
        self.commit_property_preview();
        self.cancel_gesture();
        self.shell.raster_preview = !self.shell.raster_preview;
        if self.shell.raster_preview {
            self.shell.annotations_hidden = false;
            self.status = format!(
                "Raster Preview · {} × {} PNG pixels · zoom in to inspect pixels; toggle off to edit.",
                self.session.document.width, self.session.document.height
            );
        } else {
            self.status = "Editable vector view restored.".into();
        }
        self.needs_render = true;
    }

    pub(super) fn refresh_canvas_display(&mut self, ctx: &egui::Context) {
        if !self.shell.raster_preview && self.guide_scene_document().is_some() {
            self.texture = None;
            self.display.list = None;
            self.display.texture = None;
            self.display.key = None;
            self.needs_render = false;
            return;
        }
        self.display = CanvasDisplay::default();
        self.texture = None;
        if self.shell.raster_preview {
            self.ensure_raster_texture(ctx);
            self.needs_render = false;
            return;
        }
        let list = if let Some(plan) = self.expanding_move_plan() {
            plan.display_list(&self.session.document)
        } else {
            self.canvas_document().display_list()
        };
        match list {
            Ok(list) => self.display.list = Some(list),
            Err(error) => {
                // Budget errors degrade display only. Never flatten the project.
                self.status = format!("Standard raster preview: {error}");
                self.ensure_raster_texture(ctx);
            }
        }
        self.needs_render = false;
    }

    pub(super) fn ensure_raster_texture(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        if self.texture.is_none() {
            match self.canvas_document().render() {
                Ok(image) => self.texture = Some(upload(ctx, "canvas-raster", &image)),
                Err(error) => self.status = error.to_string(),
            }
        }
        self.texture.as_ref().map(egui::TextureHandle::id)
    }

    pub(super) fn paint_canvas_display(&mut self, ui: &egui::Ui, transform: Transform) {
        let visible = transform.image.intersect(transform.viewport);
        if !visible.is_positive() {
            return;
        }
        if !self.shell.raster_preview
            && let Some((document, offset)) = self.guide_scene_document()
        {
            let result = if let Some(offset) = offset {
                self.display.scene.paint_with_base(
                    ui,
                    &document,
                    transform,
                    &self.session.document,
                    offset,
                )
            } else {
                self.display.scene.paint(ui, &document, transform)
            };
            match result {
                Ok(()) => return,
                Err(error) => self.status = format!("Guide display: {error}"),
            }
        }
        let ppp = ui.ctx().pixels_per_point();
        let key = cached_viewport(
            transform,
            ppp,
            ui.input(|i| i.max_texture_side) as u32,
            self.display.key,
        );
        let screen = egui::Rect::from_min_size(
            transform.screen_point(key.origin.map(|n| n as f32)),
            egui::vec2(key.size[0] as f32, key.size[1] as f32) / ppp,
        );
        let vector_enabled =
            self.display.list.is_some() && self.cut_out.is_none() && !self.shell.raster_preview;
        if vector_enabled && self.display.key != Some(key) && self.display.failed_key != Some(key) {
            match self.display.list.as_ref().unwrap().render_viewport(key) {
                Ok(image) => {
                    self.display.texture = Some(upload(ui.ctx(), "canvas-vector", &image));
                    self.display.key = Some(key);
                    self.display.failed_key = None;
                }
                Err(error) => {
                    self.display.failed_key = Some(key);
                    self.status = format!("Standard raster preview: {error}");
                }
            }
        }
        let painter = ui.painter().with_clip_rect(visible);
        let vector_texture = self
            .display
            .texture
            .as_ref()
            .filter(|_| vector_enabled && self.display.key == Some(key));
        let (texture, bounds) = if let Some(texture) = vector_texture {
            (texture.id(), screen)
        } else if let Some(texture) = self.ensure_raster_texture(ui.ctx()) {
            (texture, transform.image)
        } else {
            return;
        };
        painter.image(
            texture,
            bounds,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
    fn guide_scene_document(&self) -> Option<(crate::Document, Option<[i32; 2]>)> {
        if !crate::guide_display::supported(&self.session.document) {
            return None;
        }
        if let Some(plan) = self.expanding_move_plan() {
            let mut d = self.session.document.clone();
            [d.width, d.height] = plan.dimensions;
            d.annotations = plan.annotations;
            if self.shell.annotations_hidden {
                d.annotations.clear();
            }
            crate::guide_display::supported(&d).then_some((d, Some(plan.offset)))
        } else {
            let d = self.canvas_document();
            crate::guide_display::supported(&d).then_some((d, None))
        }
    }
}

/// Reuse image-coordinate pixels as the GPU translates them during a pan.
/// Small images get one bounded texture; large zoomed images get a padded tile.
/// Edits/zoom/DPI invalidate it, but fractional motion does not rerasterize.
pub(super) fn cached_viewport(
    transform: Transform,
    ppp: f32,
    max_texture_side: u32,
    previous: Option<Viewport>,
) -> Viewport {
    // zoom is the intended physical scale; dividing and multiplying by DPI can
    // drift across 1.0 and select a different raster sampling path at startup.
    let scale = f64::from(transform.zoom);
    let visible = transform.image.intersect(transform.viewport);
    let low = transform.image_point(visible.min).map(f64::from);
    let high = transform.image_point(visible.max).map(f64::from);
    if let Some(key) = previous
        && (key.scale - scale).abs() < 1e-9
        && (0..2).all(|i| {
            low[i] + 0.001 >= key.origin[i]
                && high[i] - 0.001 <= key.origin[i] + f64::from(key.size[i]) / scale
        })
    {
        return key;
    }
    // Rect subtraction at fractional DPI can produce 400.00003 for 400
    // physical pixels. Ceil would append a transparent row and alter sampling.
    let extent = |points: f32| {
        let pixels = points * ppp;
        if (pixels - pixels.round()).abs() < 0.002 {
            pixels.round() as u32
        } else {
            pixels.ceil() as u32
        }
    };
    let whole = [
        extent(transform.image.width()),
        extent(transform.image.height()),
    ];
    let visible_pixels = f64::from(visible.width() * ppp) * f64::from(visible.height() * ppp);
    // A tall guide can fit the old four-megapixel budget while most screenshots
    // are off screen. Do not rasterize all those sections on every zoom/drag.
    let whole_budget = (visible_pixels * 1.25).max(262_144.0) as u64;
    if u64::from(whole[0]) * u64::from(whole[1]) <= whole_budget.min(4_194_304)
        && whole.iter().all(|n| *n <= max_texture_side.min(8192))
    {
        return Viewport {
            size: whole,
            origin: [0.0, 0.0],
            scale,
        };
    }
    let margin = if (visible.width() * ppp + 320.0) * (visible.height() * ppp + 320.0)
        <= 8_388_608.0
        && visible.width() * ppp + 320.0 <= max_texture_side as f32
        && visible.height() * ppp + 320.0 <= max_texture_side as f32
    {
        128.0
    } else {
        0.0
    };
    let mut origin = [0.0; 2];
    let mut size = [1; 2];
    for i in 0..2 {
        let quantum = if margin > 0.0 { 64.0 } else { 1.0 };
        let lo = ((low[i] * scale - margin) / quantum).floor().max(0.0) * quantum;
        let hi = (high[i] * scale + margin).ceil().min(f64::from(whole[i]));
        origin[i] = lo / scale;
        size[i] = (hi - lo).max(1.0) as u32;
    }
    Viewport {
        size,
        origin,
        scale,
    }
}

fn upload(ctx: &egui::Context, name: &str, image: &image::RgbaImage) -> egui::TextureHandle {
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            ..egui::TextureOptions::LINEAR
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use capture_core::{Command, Document, Session};

    #[test]
    fn raster_preview_uploads_exact_png_pixels_reuses_cache_and_restores_vectors() {
        let ctx = egui::Context::default();
        let mut editor = fixture(&ctx);
        let original = serde_json::to_vec(&editor.session.document).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("export.png");
        editor.session.document.export_png(&file).unwrap();
        let png = image::open(&file).unwrap().to_rgba8();
        editor.shell.annotations_hidden = true;
        editor.toggle_raster_preview();
        assert!(!editor.shell.annotations_hidden);
        editor.refresh_canvas_display(&ctx);
        assert!(editor.display.list.is_none());
        let first = paint(&ctx, &mut editor, 0.0, 4.0);
        let id = editor.texture.as_ref().unwrap().id();
        let delta = first.textures_delta.set.get(&id).unwrap().last().unwrap();
        let egui::ImageData::Color(actual) = &delta.image;
        assert_eq!(
            actual.as_ref(),
            &egui::ColorImage::from_rgba_unmultiplied([640, 400], png.as_raw())
        );
        assert_eq!(delta.options.magnification, egui::TextureFilter::Nearest);
        for zoom in [0.5, 1.0, 4.0, 8.0] {
            let output = paint(&ctx, &mut editor, -12.25, zoom);
            assert_eq!(editor.texture.as_ref().unwrap().id(), id);
            assert!(
                !output
                    .textures_delta
                    .set
                    .iter()
                    .any(|(texture, _)| *texture == id)
            );
            assert!(editor.display.texture.is_none());
        }
        assert!(!editor.session.can_undo());
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_RASTER_DIR") {
            let root = std::path::Path::new(&root);
            std::fs::create_dir_all(root).unwrap();
            png.save(root.join("export-native.png")).unwrap();
            image::imageops::resize(&png, 2560, 1600, image::imageops::FilterType::Nearest)
                .save(root.join("export-400-percent.png"))
                .unwrap();
            editor
                .session
                .document
                .display_list()
                .unwrap()
                .render_viewport(Viewport {
                    size: [2560, 1600],
                    origin: [0.0, 0.0],
                    scale: 4.0,
                })
                .unwrap()
                .save(root.join("vector-400-percent.png"))
                .unwrap();
        }
        let mut changed = editor.session.document.annotations[0].clone();
        changed.color = [230, 40, 80, 255];
        editor.execute(Command::Update {
            annotation: changed,
        });
        editor.refresh_canvas_display(&ctx);
        paint(&ctx, &mut editor, 0.0, 4.0);
        assert_ne!(editor.texture.as_ref().unwrap().id(), id);
        editor.toggle_raster_preview();
        editor.refresh_canvas_display(&ctx);
        paint(&ctx, &mut editor, 0.0, 4.0);
        assert!(editor.display.texture.is_some());
        assert!(editor.texture.is_none());
        editor.execute(Command::Undo);
        assert_eq!(
            serde_json::to_vec(&editor.session.document).unwrap(),
            original
        );
        editor.toggle_raster_preview();
        editor.toggle_annotations();
        assert!(editor.shell.annotations_hidden && !editor.shell.raster_preview);
        editor.toggle_raster_preview();
        editor.select_only(Some(1));
        editor.begin_text_edit();
        assert!(!editor.shell.raster_preview);
        assert!(editor.text_draft.is_some());
        assert!(editor.commit_text());
        assert!(!editor.session.can_undo());
    }

    #[test]
    fn fractional_zoom_pans_reuse_one_texture_without_rerender_or_upload() {
        for scale in [0.79, 0.87, 0.97] {
            let ctx = egui::Context::default();
            let mut editor = fixture(&ctx);
            paint(&ctx, &mut editor, 0.0, scale);
            let id = editor.display.texture.as_ref().unwrap().id();
            let key = editor.display.key.unwrap();
            let started = std::time::Instant::now();
            for i in 0..120 {
                paint(&ctx, &mut editor, -i as f32 * 0.37, scale);
                assert_eq!(editor.display.key, Some(key));
                assert_eq!(editor.display.texture.as_ref().unwrap().id(), id);
            }
            eprintln!(
                "{scale:.2} zoom:120 cached fractional pans in {:?}; 0 rerenders/uploads",
                started.elapsed()
            );
            assert!(editor.texture.is_none());
            assert!(!editor.session.can_undo());
        }
    }
    #[test]
    fn large_images_use_bounded_guard_tiles_and_zoom_rebuilds() {
        let make = |x: f32| Transform {
            image: egui::Rect::from_min_size(egui::pos2(x, 0.0), egui::vec2(6000.0, 4000.0)),
            viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0)),
            scale: 1.0,
            zoom: 1.0,
        };
        let first = cached_viewport(make(-500.0), 1.0, 8192, None);
        let pan = cached_viewport(make(-560.25), 1.0, 8192, Some(first));
        assert_eq!(first, pan);
        assert!(first.size[0] <= 1320 && first.size[1] <= 956);
        assert_ne!(first, cached_viewport(make(-900.0), 1.0, 8192, Some(first)));
        let mut zoom = make(-560.25);
        zoom.scale = 2.0;
        zoom.zoom = 2.0;
        assert_ne!(first, cached_viewport(zoom, 1.0, 8192, Some(first)));
    }

    #[test]
    fn restored_first_frame_at_actual_size_is_pixel_aligned_and_matches_zoom_roundtrip() {
        use crate::canvas_view::{CanvasView, ZoomAction, ZoomMode};
        let doc: Document = serde_json::from_str(include_str!(
            "../../../crates/core/tests/fixtures/properties.capture.json"
        ))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restored.capture.json");
        Session::new(doc).unwrap().save(&path).unwrap();
        for ppp in [1.0, 1.25, 1.5, 1.75, 2.0] {
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(ppp);
            let mut e = crate::canvas::tests::editor(&ctx);
            e.session = Session::new(Document::load(&path).unwrap()).unwrap();
            let png = e.session.document.render().unwrap();
            let expected = egui::ColorImage::from_rgba_unmultiplied([640, 400], png.as_raw());
            e.refresh_canvas_display(&ctx);
            let mut view = CanvasView::default();
            // Odd/fractional content dimensions are normal after panels and DPI
            // layout. No pointer motion/zoom should be needed for a sharp image.
            let viewport =
                egui::Rect::from_min_size(egui::pos2(12.25, 24.5), egui::vec2(901.0, 603.0));
            let mut first = None;
            for zoom in [None, Some(2.0), Some(1.0)] {
                if let Some(zoom) = zoom {
                    view.apply(
                        ZoomAction::Set(ZoomMode::Manual(zoom)),
                        Some(viewport.center() + egui::vec2(37.25, 19.5)),
                    );
                }
                let output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                    egui::RawInput::default(),
                    |ui| {
                        let t = view.transform(
                            viewport,
                            egui::vec2(640.0, 400.0),
                            ui.ctx().pixels_per_point(),
                        );
                        let physical = t.image.min.to_vec2() * ui.ctx().pixels_per_point();
                        assert!(
                            (physical.x - physical.x.round()).abs() < 0.0001,
                            "x {physical:?} DPI {ppp}"
                        );
                        assert!(
                            (physical.y - physical.y.round()).abs() < 0.0001,
                            "y {physical:?} DPI {ppp}"
                        );
                        e.paint_canvas_display(ui, t);
                    },
                ));
                assert!(
                    e.texture.is_none(),
                    "startup must use the vector display without raster fallback"
                );
                if e.display.key.unwrap().scale == 1.0 {
                    let id = e.display.texture.as_ref().unwrap().id();
                    let delta = output.textures_delta.set.get(&id).unwrap().last().unwrap();
                    let egui::ImageData::Color(actual) = &delta.image;
                    assert_eq!(
                        actual.as_ref(),
                        &expected,
                        "1:1 must be exact native output from first load, DPI {ppp}"
                    );
                    if let Some(first) = &first {
                        assert_eq!(actual, first);
                    } else {
                        first = Some(actual.clone());
                    }
                }
            }
            assert!(!e.session.can_undo());
        }
    }

    fn fixture(ctx: &egui::Context) -> Editor {
        let mut editor = crate::canvas::tests::editor(ctx);
        let mut doc = Document::blank(640, 400).unwrap();
        doc.annotations.push(
            capture_core::steps::marker(
                1,
                [100.0, 100.0],
                64,
                [30, 90, 160, 255],
                (1, false),
                [640, 400],
            )
            .unwrap(),
        );
        editor.session = Session::new(doc).unwrap();
        editor.refresh_canvas_display(ctx);
        editor
    }

    fn paint(
        ctx: &egui::Context,
        editor: &mut Editor,
        offset: f32,
        scale: f32,
    ) -> crate::property_layout::tests::PaintedOutput {
        let viewport = egui::Rect::from_min_size(egui::pos2(12.0, 20.0), egui::vec2(240.0, 180.0));
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.paint_canvas_display(
                ui,
                Transform {
                    image: egui::Rect::from_min_size(
                        viewport.min + egui::vec2(offset, 0.0),
                        egui::vec2(640.0, 400.0) * scale,
                    ),
                    viewport,
                    scale,
                    zoom: scale,
                },
            );
        });
        crate::property_layout::tests::PaintedOutput(output)
    }

    #[test]
    fn unchanged_frames_and_panning_reuse_texture_zoom_and_document_changes_invalidate() {
        let ctx = egui::Context::default();
        let mut editor = fixture(&ctx);
        paint(&ctx, &mut editor, 0.0, 1.0);
        let first = editor.display.texture.as_ref().unwrap().id();
        assert!(
            editor.texture.is_none(),
            "do not rasterize the full export on normal frames"
        );
        paint(&ctx, &mut editor, 0.0, 1.0);
        assert_eq!(first, editor.display.texture.as_ref().unwrap().id());
        paint(&ctx, &mut editor, -20.0, 1.0);
        assert_eq!(first, editor.display.texture.as_ref().unwrap().id());
        paint(&ctx, &mut editor, -20.0, 4.0);
        assert_eq!(editor.display.key.unwrap().scale, 4.0);
        assert!(
            u64::from(editor.display.key.unwrap().size[0])
                * u64::from(editor.display.key.unwrap().size[1])
                <= 8_388_608
        );
        let mut object = editor.session.document.annotations[0].clone();
        object.color = [190, 30, 80, 255];
        editor.execute(Command::Update {
            annotation: object.clone(),
        });
        editor.refresh_canvas_display(&ctx);
        assert!(editor.display.key.is_none());
        assert!(
            matches!(&editor.display.list.as_ref().unwrap().layers[0], capture_core::display::Layer::Vector(a) if a == &object)
        );
        paint(&ctx, &mut editor, 0.0, 4.0);
        assert!(editor.display.texture.is_some());
    }

    #[test]
    fn hidden_view_and_live_property_preview_do_not_change_export_or_undo() {
        let ctx = egui::Context::default();
        let mut editor = fixture(&ctx);
        let original = editor.session.document.render().unwrap();
        editor.select_only(Some(1));
        let mut changed = editor.session.document.annotations[0].clone();
        changed.color = [210, 60, 120, 255];
        editor.preview_property_edit(changed.clone());
        editor.refresh_canvas_display(&ctx);
        assert!(
            matches!(&editor.display.list.as_ref().unwrap().layers[0], capture_core::display::Layer::Vector(a) if a == &changed)
        );
        assert_eq!(editor.session.document.render().unwrap(), original);
        assert!(!editor.session.can_undo());
        editor.cancel_gesture();
        editor.toggle_annotations();
        editor.refresh_canvas_display(&ctx);
        assert!(editor.display.list.as_ref().unwrap().layers.is_empty());
        assert_eq!(editor.session.document.render().unwrap(), original);
        editor.toggle_annotations();
        editor.refresh_canvas_display(&ctx);
        assert_eq!(editor.display.list.as_ref().unwrap().layers.len(), 1);
        assert!(!editor.session.can_undo());
    }

    #[test]
    fn cutout_uses_native_texture_and_returns_to_vectors_without_changing_document() {
        let ctx = egui::Context::default();
        let mut editor = fixture(&ctx);
        let bytes = serde_json::to_vec(&editor.session.document).unwrap();
        editor.begin_cut_out();
        paint(&ctx, &mut editor, 0.0, 4.0);
        assert!(editor.texture.is_some());
        assert!(editor.display.key.is_none());
        editor.cancel_gesture();
        paint(&ctx, &mut editor, 0.0, 4.0);
        assert!(editor.display.texture.is_some());
        assert_eq!(serde_json::to_vec(&editor.session.document).unwrap(), bytes);
    }
}

#[cfg(test)]
mod profile;
#[cfg(test)]
mod guide_bounds_tests {
    use super::*;
    #[test]
    fn offscreen_guide_sections_do_not_multiply_zoom_or_drag_raster_work() {
        for zoom in [0.79, 0.85, 0.97, 1.0, 1.2] {
            let make = |height: f32| Transform {
                image: egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1048., height) * zoom,
                ),
                viewport: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900., 600.)),
                scale: zoom,
                zoom,
            };
            let two = cached_viewport(make(1302.), 1., 8192, None);
            let five = cached_viewport(make(3219.), 1., 8192, None);
            let twenty = cached_viewport(make(12804.), 1., 8192, None);
            assert_eq!(two, five);
            assert_eq!(five, twenty);
            assert!(u64::from(five.size[0]) * u64::from(five.size[1]) < 800_000);
        }
    }
}
