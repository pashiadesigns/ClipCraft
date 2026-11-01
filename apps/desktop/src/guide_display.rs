//! Retained capture/guide textures: screenshots are sampled by the GPU, while vector
//! overlays retain the shared renderer. Translation never regenerates pixels.
use crate::{Annotation, Document, Kind, canvas_view::Transform, egui};
use capture_core::display::effect_patch::{self, Plan};
use capture_core::display::{Viewport, render_annotation_preview};
use std::{collections::HashMap, sync::Arc};
#[cfg(test)]
#[path = "guide_display_effect_tests.rs"]
mod effect_tests;
#[derive(Default)]
pub(super) struct Scene {
    background: Option<Background>,
    images: HashMap<usize, (Arc<image::RgbaImage>, egui::TextureHandle)>,
    vectors: HashMap<u64, Vector>,
    effects: HashMap<u64, Effect>,
}
struct Effect {
    plan: Plan,
    offset: [i32; 2],
    tiles: Vec<([u32; 4], egui::TextureHandle)>,
}
struct Background {
    raster: Option<Arc<capture_core::RasterImage>>,
    source: String,
    pixels: image::RgbaImage,
    tiles: HashMap<[u32; 2], egui::TextureHandle>,
}
struct Vector {
    local: Annotation,
    scale: f64,
    texture: egui::TextureHandle,
    size: [u32; 2],
}
pub(super) fn supported(d: &Document) -> bool {
    d.guide.is_some()
        || d.raster.is_some()
        || !d.source_png.is_empty()
        || d.annotations.iter().any(|a| a.kind == Kind::Image)
        || d.annotations.iter().any(effect_patch::needs_pixels)
}
// Replacement patches must remove earlier ink, including alpha. Clip the
// earlier layers around each later patch; never paint erased pixels over ink.
fn with_holes(
    p: &egui::Painter,
    bounds: egui::Rect,
    holes: &[egui::Rect],
    mut paint: impl FnMut(&egui::Painter),
) {
    let mut clips = vec![p.clip_rect().intersect(bounds)];
    for hole in holes {
        clips = clips
            .into_iter()
            .flat_map(|r| {
                let h = r.intersect(*hole);
                if !h.is_positive() {
                    return vec![r];
                }
                [
                    egui::Rect::from_min_max(r.min, egui::pos2(r.right(), h.top())),
                    egui::Rect::from_min_max(egui::pos2(r.left(), h.bottom()), r.max),
                    egui::Rect::from_min_max(
                        egui::pos2(r.left(), h.top()),
                        egui::pos2(h.left(), h.bottom()),
                    ),
                    egui::Rect::from_min_max(
                        egui::pos2(h.right(), h.top()),
                        egui::pos2(r.right(), h.bottom()),
                    ),
                ]
                .into_iter()
                .filter(|r| r.is_positive())
                .collect()
            })
            .collect();
    }
    for r in clips.into_iter().filter(|r| r.is_positive()) {
        paint(&p.with_clip_rect(r));
    }
}
impl Scene {
    pub(super) fn paint(
        &mut self,
        ui: &egui::Ui,
        d: &Document,
        t: Transform,
    ) -> capture_core::Result<()> {
        self.paint_with_base(ui, d, t, d, [0, 0])
    }
    pub(super) fn paint_with_base(
        &mut self,
        ui: &egui::Ui,
        d: &Document,
        t: Transform,
        base: &Document,
        offset: [i32; 2],
    ) -> capture_core::Result<()> {
        let painter = ui.painter().with_clip_rect(t.viewport.intersect(t.image));
        let shift = offset.map(|n| n as f32);
        let objects: Vec<_> = d
            .annotations
            .iter()
            .filter(|a| d.object_visible(a.id) && a.analysis.is_none())
            .cloned()
            .collect();
        let plans: Vec<_> = objects
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let plan = Plan::new(&objects, i, [d.width, d.height])?;
                let [x, y, w, h] = plan.output;
                let rect = egui::Rect::from_min_max(
                    t.screen_point([x as f32, y as f32]),
                    t.screen_point([(x + w) as f32, (y + h) as f32]),
                );
                rect.intersects(t.viewport).then_some((i, a.id, rect, plan))
            })
            .collect();
        let all_holes: Vec<_> = plans.iter().map(|p| p.2).collect();
        if base.raster.is_some() || !base.source_png.is_empty() || !plans.is_empty() {
            if self.background.as_ref().is_none_or(|b| {
                !match (&b.raster, &base.raster) {
                    (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                } || b.source != base.source_png
                    || b.pixels.dimensions() != (base.width, base.height)
            }) {
                self.effects.clear();
                self.background = Some(Background {
                    raster: base.raster.clone(),
                    source: base.source_png.clone(),
                    pixels: base.base_image()?,
                    tiles: HashMap::new(),
                });
            }
            let background = self.background.as_mut().unwrap();
            let tile_size = (ui.input(|i| i.max_texture_side) as u32).clamp(1, 1024);
            let mut visible = Vec::new();
            for y in (0..base.height).step_by(tile_size as usize) {
                for x in (0..base.width).step_by(tile_size as usize) {
                    let w = tile_size.min(base.width - x);
                    let h = tile_size.min(base.height - y);
                    let rect = egui::Rect::from_min_max(
                        t.screen_point([x as f32 + shift[0], y as f32 + shift[1]]),
                        t.screen_point([(x + w) as f32 + shift[0], (y + h) as f32 + shift[1]]),
                    );
                    if !rect.intersects(t.viewport) {
                        continue;
                    }
                    visible.push([x, y]);
                    let texture = background.tiles.entry([x, y]).or_insert_with(|| {
                        let pixels =
                            image::imageops::crop_imm(&background.pixels, x, y, w, h).to_image();
                        ui.ctx().load_texture(
                            "guide-base",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [w as usize, h as usize],
                                pixels.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        )
                    });
                    with_holes(&painter, rect, &all_holes, |p| {
                        p.image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                            egui::Color32::WHITE,
                        );
                    });
                }
            }
            background.tiles.retain(|key, _| visible.contains(key));
        } else {
            self.background = None;
            let rect = egui::Rect::from_min_max(
                t.screen_point(shift),
                t.screen_point([base.width as f32 + shift[0], base.height as f32 + shift[1]]),
            );
            with_holes(&painter, rect, &all_holes, |p| {
                p.rect_filled(rect, 0., egui::Color32::WHITE);
            });
        }
        for (_, id, _, plan) in &plans {
            if self
                .effects
                .get(id)
                .is_none_or(|e| e.plan != *plan || e.offset != offset)
            {
                let pixels = plan.render(&self.background.as_ref().unwrap().pixels, offset)?;
                let limit = (ui.input(|i| i.max_texture_side) as u32).clamp(1, 1024);
                let mut tiles = Vec::new();
                for y in (0..pixels.height()).step_by(limit as usize) {
                    for x in (0..pixels.width()).step_by(limit as usize) {
                        let w = limit.min(pixels.width() - x);
                        let h = limit.min(pixels.height() - y);
                        let patch = image::imageops::crop_imm(&pixels, x, y, w, h).to_image();
                        tiles.push((
                            [x, y, w, h],
                            ui.ctx().load_texture(
                                "effect-region",
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [w as usize, h as usize],
                                    patch.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            ),
                        ));
                    }
                }
                self.effects.insert(
                    *id,
                    Effect {
                        plan: plan.clone(),
                        offset,
                        tiles,
                    },
                );
            }
        }
        let mut retained = Vec::new();
        let mut live_vectors = Vec::new();
        let ppp = ui.ctx().pixels_per_point();
        for (index, a) in objects.iter().enumerate() {
            let holes: Vec<_> = plans.iter().filter(|p| p.0 > index).map(|p| p.2).collect();
            if effect_patch::needs_pixels(a) {
                if let Some(e) = self
                    .effects
                    .get(&a.id)
                    .filter(|_| plans.iter().any(|p| p.1 == a.id))
                {
                    for ([x, y, w, h], texture) in &e.tiles {
                        let [ox, oy, _, _] = e.plan.output;
                        let rect = egui::Rect::from_min_max(
                            t.screen_point([(ox + x) as f32, (oy + y) as f32]),
                            t.screen_point([(ox + x + w) as f32, (oy + y + h) as f32]),
                        );
                        with_holes(&painter, rect, &holes, |p| {
                            p.image(
                                texture.id(),
                                rect,
                                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                                egui::Color32::WHITE,
                            );
                        });
                    }
                }
                continue;
            }
            let (mut lo, mut hi) = a.bounds();
            if let Some((slo, shi)) = capture_core::shadow::bounds(a) {
                for i in 0..2 {
                    lo[i] = lo[i].min(slo[i]);
                    hi[i] = hi[i].max(shi[i]);
                }
            }
            let margin = if a.kind == Kind::Arrow {
                a.width as f32 * 4. + 2.
            } else {
                a.width as f32 * 0.5 + 2.
            };
            lo = lo.map(|x| x - margin);
            hi = hi.map(|x| x + margin);
            let screen = egui::Rect::from_min_max(t.screen_point(lo), t.screen_point(hi));
            if !screen.intersects(t.viewport) {
                continue;
            }
            if a.kind == Kind::Image
                && a.image.as_ref().is_some_and(|b| b.vector().is_none())
                && a.rotation_degrees == 0.
                && a.shadow.is_none()
                && a.image
                    .as_ref()
                    .unwrap()
                    .size()
                    .into_iter()
                    .all(|n| n <= ui.input(|i| i.max_texture_side) as u32)
            {
                let b = a.image.as_ref().unwrap();
                let pixels = b.pixels()?;
                let key = Arc::as_ptr(pixels) as usize;
                retained.push(key);
                let (_, texture) = self.images.entry(key).or_insert_with(|| {
                    (
                        pixels.clone(),
                        ui.ctx().load_texture(
                            "guide-source",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [pixels.width() as usize, pixels.height() as usize],
                                pixels.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        ),
                    )
                });
                let (min, max) = a.body_bounds();
                let rect = egui::Rect::from_min_max(t.screen_point(min), t.screen_point(max));
                let [x, y, w, h] = b.view().rect;
                with_holes(&painter, rect, &holes, |p| {
                    p.add(
                        egui::epaint::RectShape::filled(
                            rect,
                            (a.corner_radius * t.scale).round().clamp(0., 255.) as u8,
                            egui::Color32::from_white_alpha(a.color[3]),
                        )
                        .with_texture(
                            texture.id(),
                            egui::Rect::from_min_max(egui::pos2(x, y), egui::pos2(x + w, y + h)),
                        ),
                    );
                });
                continue;
            }
            live_vectors.push(a.id);
            let mut local = capture_core::canvas_expansion::translated(
                std::slice::from_ref(a),
                lo.map(|x| -x),
            )?
            .remove(0);
            // Translation subtraction can leave tiny f32 differences in local
            // geometry. Quantize the display cache only (never the document).
            let stable_point = |p: &mut [f32; 2]| {
                for v in p {
                    *v = (*v * 256.).round() / 256.;
                }
            };
            stable_point(&mut local.start);
            stable_point(&mut local.end);
            for p in &mut local.points {
                stable_point(p);
            }
            if let Some(c) = &mut local.callout {
                stable_point(&mut c.pointer);
            }
            let scale = f64::from(t.zoom);
            let extent = |n: f32| {
                let n = n * t.zoom;
                (if (n - n.round()).abs() < 0.002 {
                    n.round()
                } else {
                    n.ceil()
                })
                .max(1.) as u32
            };
            let size = [extent(hi[0] - lo[0]), extent(hi[1] - lo[1])];
            if self
                .vectors
                .get(&a.id)
                .is_none_or(|v| v.local != local || v.scale != scale || v.size != size)
            {
                let pixels = render_annotation_preview(
                    &local,
                    Viewport {
                        size,
                        origin: [0.; 2],
                        scale,
                    },
                )?;
                let texture = ui.ctx().load_texture(
                    "guide-vector",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [size[0] as usize, size[1] as usize],
                        pixels.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                );
                self.vectors.insert(
                    a.id,
                    Vector {
                        local,
                        scale,
                        texture,
                        size,
                    },
                );
            }
            let v = &self.vectors[&a.id];
            let rect = egui::Rect::from_min_size(
                t.screen_point(lo),
                egui::vec2(size[0] as f32, size[1] as f32) / ppp,
            );
            with_holes(&painter, rect, &holes, |p| {
                p.image(
                    v.texture.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    egui::Color32::WHITE,
                );
            });
        }
        // Only visible source textures remain resident; source PNGs stay shared
        // with the document, so guide length cannot grow GPU memory unchecked.
        self.images.retain(|id, _| retained.contains(id));
        self.vectors.retain(|id, _| live_vectors.contains(id));
        self.effects
            .retain(|id, _| plans.iter().any(|p| p.1 == *id));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::property_layout::tests::{PaintedOutput, dump_meshes};
    #[test]
    fn blank_canvas_raster_and_vector_stamps_reuse_pixels_during_fractional_movement() {
        for id in ["builtin-raster-sample", "builtin-check-circle"] {
            let mut session =
                capture_core::Session::new(Document::blank(3000, 2000).unwrap()).unwrap();
            session
                .execute(capture_core::Command::PasteObjects {
                    payload: capture_core::stamps::builtin::find(id)
                        .unwrap()
                        .payload()
                        .unwrap(),
                    center: [100., 100.],
                })
                .unwrap();
            assert!(supported(&session.document));
            let baseline = session.document.annotations.clone();
            let ctx = egui::Context::default();
            let mut scene = Scene::default();
            let paint = |scene: &mut Scene, doc: &Document, scale: f32| {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000., 800.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        scene
                            .paint(
                                ui,
                                doc,
                                Transform {
                                    viewport: ui.clip_rect(),
                                    image: egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(doc.width as f32, doc.height as f32) * scale,
                                    ),
                                    scale,
                                    zoom: scale,
                                },
                            )
                            .unwrap()
                    },
                ))
            };
            for scale in [0.35, 0.79, 1., 1.5, 2., 4., 8.] {
                let _ = paint(&mut scene, &session.document, scale);
                let _ = paint(&mut scene, &session.document, scale);
                for n in 1..8 {
                    session.document.annotations = capture_core::canvas_expansion::translated(
                        &baseline,
                        [n as f32 * 0.37, n as f32 * 0.23],
                    )
                    .unwrap();
                    let output = paint(&mut scene, &session.document, scale);
                    assert!(
                        output.0.textures_delta.set.is_empty(),
                        "{id} movement at {scale} uploaded pixels again"
                    );
                }
            }
        }
    }
    #[test]
    fn edited_expanded_guides_keep_cached_background_and_screenshots_during_movement() {
        use capture_core::{Command, Session, guide_sections::Action, guide_sections::SectionKind};
        let mut guide = capture_core::step_capture::Guide::new([320, 180]).unwrap();
        for _ in 0..5 {
            guide
                .push(
                    image::RgbaImage::from_pixel(320, 180, image::Rgba([80, 150, 200, 255])),
                    capture_core::step_capture::Gesture {
                        start: [90, 60],
                        end: [90, 60],
                        button: capture_core::step_capture::Button::Left,
                    },
                )
                .unwrap();
        }
        let mut s = Session::new(guide.document().unwrap()).unwrap();
        let moved =
            capture_core::canvas_expansion::translated(&s.document.annotations[..1], [100., 0.])
                .unwrap();
        s.execute(Command::MoveAndExpand {
            annotations: moved,
            duplicate: false,
        })
        .unwrap();
        assert!(s.document.raster.is_some());
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        for action in [
            Action::Insert {
                after: Some(1),
                kind: SectionKind::Text,
            },
            Action::Remove { id: 6 },
            Action::Remove { id: 5 },
            Action::Remove { id: 4 },
        ] {
            s.execute(Command::Guide { action }).unwrap();
            assert!(supported(&s.document));
            let mut paint = |d: &Document, offset: [i32; 2], zoom: f32| {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(650., 450.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        scene
                            .paint_with_base(
                                ui,
                                d,
                                Transform {
                                    viewport: ui.clip_rect(),
                                    image: egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(d.width as f32, d.height as f32) * zoom,
                                    ),
                                    scale: zoom,
                                    zoom,
                                },
                                &s.document,
                                offset,
                            )
                            .unwrap();
                    },
                ))
            };
            paint(&s.document, [0, 0], 1.);
            assert!(
                paint(&s.document, [0, 0], 1.)
                    .0
                    .textures_delta
                    .set
                    .is_empty()
            );
            for n in 0..10 {
                let mut d = s.document.clone();
                d.annotations[0] = capture_core::canvas_expansion::translated(
                    &d.annotations[..1],
                    [n as f32 * 0.37, 0.],
                )
                .unwrap()
                .remove(0);
                let o = paint(&d, [0, 0], 1.);
                assert!(
                    o.0.textures_delta.set.is_empty(),
                    "drag rebuilt retained pixels after section edit"
                );
            }
            // Further canvas growth translates the cached background, too.
            let mut grown = s.document.clone();
            grown.width += 20;
            let output = paint(&grown, [20, 0], 1.);
            assert!(output.0.textures_delta.set.is_empty());
            let background = scene
                .background
                .as_ref()
                .unwrap()
                .tiles
                .values()
                .map(|t| t.id())
                .collect::<Vec<_>>();
            assert!(!background.is_empty());
        }
    }
    #[test]
    fn guide_sources_reuse_textures_during_zoom_and_translation_without_cpu_resampling() {
        let mut guide = capture_core::step_capture::Guide::new([640, 360]).unwrap();
        for _ in 0..5 {
            guide
                .push(
                    image::RgbaImage::from_fn(640, 360, |x, y| {
                        image::Rgba([
                            ((x / 16) % 2 * 150 + 50) as u8,
                            ((y / 12) % 2 * 140 + 70) as u8,
                            190,
                            255,
                        ])
                    }),
                    capture_core::step_capture::Gesture {
                        start: [200, 180],
                        end: [200, 180],
                        button: capture_core::step_capture::Button::Left,
                    },
                )
                .unwrap();
        }
        let mut doc = guide.document().unwrap();
        let original = doc.render().unwrap();
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut scene = Scene::default();
        let paint = |scene: &mut Scene, doc: &Document, scale: f32| {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 600.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    scene
                        .paint(
                            ui,
                            doc,
                            Transform {
                                viewport: ui.clip_rect(),
                                image: egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(doc.width as f32, doc.height as f32) * scale,
                                ),
                                scale,
                                zoom: scale,
                            },
                        )
                        .unwrap();
                },
            ))
        };
        let mut first = paint(&mut scene, &doc, 1.);
        let still = paint(&mut scene, &doc, 1.);
        assert!(still.0.textures_delta.set.is_empty());
        assert!(!scene.images.is_empty());
        let sources: Vec<_> = scene.images.values().map(|(_, t)| t.id()).collect();
        for zoom in [1.05, 1.2, 1.5, 2., 1.] {
            let output = paint(&mut scene, &doc, zoom);
            assert!(
                output
                    .0
                    .textures_delta
                    .set
                    .iter()
                    .all(|(id, _)| !sources.contains(id)),
                "zoom must not upload screenshot pixels again"
            );
        }
        // The actual screenshot and its click marker move without a single
        // replacement texture; vector coverage moves with the cached pixels.
        let ids: Vec<_> = doc.annotations.iter().take(4).map(|a| a.id).collect();
        for _ in 0..12 {
            for a in doc.annotations.iter_mut().filter(|a| ids.contains(&a.id)) {
                *a = capture_core::canvas_expansion::translated(std::slice::from_ref(a), [1., 0.])
                    .unwrap()
                    .remove(0);
            }
            let o = paint(&mut scene, &doc, 1.);
            assert!(
                o.0.textures_delta.set.is_empty(),
                "translation rebuilt textures"
            );
        }
        for a in doc.annotations.iter_mut().filter(|a| ids.contains(&a.id)) {
            *a = capture_core::canvas_expansion::translated(std::slice::from_ref(a), [-12., 0.])
                .unwrap()
                .remove(0);
        }
        assert_eq!(doc.render().unwrap(), original);
        let baseline = doc.annotations.clone();
        for n in 1..8 {
            for a in doc.annotations.iter_mut().filter(|a| ids.contains(&a.id)) {
                *a = capture_core::canvas_expansion::translated(
                    std::slice::from_ref(baseline.iter().find(|b| b.id == a.id).unwrap()),
                    [n as f32 * 0.37, n as f32 * 0.23],
                )
                .unwrap()
                .remove(0);
            }
            let o = paint(&mut scene, &doc, 1.);
            assert!(
                o.0.textures_delta.set.is_empty(),
                "fractional translation rebuilt textures"
            );
        }
        doc.annotations = baseline;
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_INTERACTIONS_DIR") {
            scene = Scene::default();
            let uploaded = paint(&mut scene, &doc, 1.);
            first
                .0
                .textures_delta
                .set
                .extend(uploaded.0.textures_delta.set.clone());
            let last = paint(&mut scene, &doc, 1.);
            dump_meshes(&ctx, &root, "guide-retained", first, last);
        }
    }
}
