//! Retained capture/guide textures: screenshots are sampled by the GPU, while vector
//! overlays retain the shared renderer. Translation never regenerates pixels.
use crate::{Annotation, Document, Kind, canvas_view::Transform, egui};
use capture_core::display::effect_patch::{self, Plan};
use capture_core::display::{Viewport, render_annotation_preview};
use std::{collections::HashMap, sync::Arc};
#[cfg(test)]
#[path = "guide_display_effect_tests.rs"]
mod effect_tests;
#[path = "guide_display_focus.rs"]
mod focus;
#[derive(Default)]
pub(super) struct Scene {
    background: Option<Background>,
    images: HashMap<usize, (Arc<image::RgbaImage>, egui::TextureHandle)>,
    vectors: HashMap<u64, Vector>,
    shadows: HashMap<u64, (Annotation, capture_core::shadow::Patch, egui::TextureHandle)>,
    effects: HashMap<u64, Effect>,
    video_density: Option<f32>,
    /// A canvas gesture is in progress. Reduced-density previews then reuse the
    /// retained lens surface instead of re-rendering full-resolution detail
    /// on every pointer move; detail is rebuilt once the gesture ends.
    pub(super) interacting: bool,
}
struct Effect {
    plan: Plan,
    offset: [i32; 2],
    tiles: Vec<([u32; 4], egui::TextureHandle)>,
    clear: Option<Vec<([u32; 4], egui::TextureHandle)>>,
    density: f32,
    detail: Option<([u32; 4], focus::Tiles)>,
    /// Sharp Magnify sources for the other Focus areas sharing this backdrop.
    member_detail: HashMap<u64, ([u32; 4], focus::Tiles)>,
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
    origin: [f64; 2],
}
#[cfg(test)]
pub(super) fn supported(d: &Document) -> bool {
    supported_with(d, false)
}
/// `warp_preview` admits perspective images while a corner is dragged: the
/// scene then warps them as a GPU mesh. Otherwise perspective (on an image or
/// the whole picture) goes through the engine's display list, which warps
/// exactly as export does (BI8, BI9).
pub(super) fn supported_with(d: &Document, warp_preview: bool) -> bool {
    if d.canvas_behavior.perspective.is_some() || d.canvas_behavior.finish.is_some() {
        return false;
    }
    let warped = d
        .annotations
        .iter()
        .any(|a| a.image.as_ref().is_some_and(|b| b.perspective().is_some()));
    if warped {
        return warp_preview;
    }
    d.canvas_behavior.allow_overflow
        || d.guide.is_some()
        || d.raster.is_some()
        || !d.source_png.is_empty()
        || d.annotations.iter().any(|a| a.image.is_some())
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
    pub(super) fn paint_video(
        &mut self,
        ui: &egui::Ui,
        d: &Document,
        t: Transform,
        frame: Option<&image::RgbaImage>,
        revision: u64,
        project: &capture_core::video::editor::Project,
    ) -> capture_core::Result<()> {
        let key = if frame.is_none_or(|f| f.dimensions() == (1, 1)) {
            format!(
                "video-background:{:?}:{}:{}",
                project.background, d.width, d.height
            )
        } else {
            format!(
                "video:{revision}:{}:{:?}:{}:{}:{}:{:?}",
                project.source, project.crop, project.fill, d.width, d.height, project.background
            )
        };
        let mut base = d.clone();
        base.annotations.clear();
        base.raster = None;
        base.source_png = key.clone();
        base.source_size = None;
        if self.background.as_ref().is_none_or(|b| b.source != key) {
            base.video = Some(project.clone());
            let pixels = if frame.is_none_or(|f| f.dimensions() == (1, 1)) {
                image::RgbaImage::from_pixel(d.width, d.height, image::Rgba(project.background))
            } else {
                capture_core::video::editor::composite(&base, frame.unwrap(), 0)?
            };
            self.background = Some(Background {
                raster: None,
                source: key,
                pixels,
                tiles: HashMap::new(),
            });
            self.effects.clear();
        }
        base.video = None;
        self.video_density = Some((t.scale * ui.ctx().pixels_per_point()).clamp(0.05, 1.));
        let result = self.paint_with_base(ui, d, t, &base, [0, 0]);
        self.video_density = None;
        result
    }
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
        let painter = ui
            .painter()
            .with_clip_rect(if d.canvas_behavior.allow_overflow {
                t.viewport
            } else {
                t.viewport.intersect(t.image)
            });
        let shift = offset.map(|n| n as f32);
        let mut objects: Vec<_> = d
            .annotations
            .iter()
            .filter(|a| d.object_visible(a.id))
            .cloned()
            .collect();
        objects.extend(d.page_furniture());
        // Focus effects drawn together share one backdrop; on paged guides,
        // those on the same page.
        let paged = !d.canvas_behavior.page_owners.is_empty() || d.guide.is_some();
        let backdrop_for = |a: &Annotation| {
            if paged {
                let owner = d.object_page(a);
                let page: Vec<_> = objects
                    .iter()
                    .filter(|o| d.object_page(o) == owner)
                    .cloned()
                    .collect();
                capture_core::focus::Backdrop::new(&page)
            } else {
                capture_core::focus::Backdrop::new(&objects)
            }
        };
        // A later member needs no surfaces of its own. The exception is a
        // Magnify view whose window shows objects drawn after the backdrop,
        // because the backdrop's picture does not include those objects.
        let draws_alone = |i: usize, a: &Annotation, g: &capture_core::focus::Backdrop| {
            g.is_quiet(a)
                && (a.focus.is_none_or(|f| f.magnify.is_none())
                    || !objects
                        .iter()
                        .position(|o| Some(o.id) == g.lead())
                        .is_some_and(|lead| {
                            let (lo, hi) = a.bounds();
                            objects[lead + 1..i].iter().any(|o| {
                                let (olo, ohi) = o.bounds();
                                !g.members().iter().any(|m| m.id == o.id)
                                    && olo[0] < hi[0]
                                    && lo[0] < ohi[0]
                                    && olo[1] < hi[1]
                                    && lo[1] < ohi[1]
                            })
                        }))
        };
        let plans: Vec<_> = objects
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                let group = a.focus.is_some().then(|| backdrop_for(a));
                if group.as_ref().is_some_and(|g| draws_alone(i, a, g)) {
                    return None;
                }
                let plan = if !d.canvas_behavior.page_owners.is_empty()
                    || (a.focus.is_some() && d.guide.is_some())
                {
                    if !effect_patch::needs_pixels(a) {
                        return None;
                    }
                    let owner = d.object_page(a);
                    let own: Vec<_> = objects[..=i]
                        .iter()
                        .filter(|o| d.object_page(o) == owner)
                        .cloned()
                        .collect();
                    let plan = Plan::new(&own, own.len() - 1, [d.width, d.height])?;
                    match group {
                        Some(group) => plan.with_backdrop(group),
                        None => plan,
                    }
                    .clipped(d.page_rect(owner).map(|n| n as u32))?
                } else {
                    Plan::new(&objects, i, [d.width, d.height])?
                };
                let [x, y, w, h] = plan.output;
                let rect = egui::Rect::from_min_max(
                    t.screen_point([x as f32, y as f32]),
                    t.screen_point([(x + w) as f32, (y + h) as f32]),
                );
                rect.intersects(t.viewport).then_some((i, a.id, rect, plan))
            })
            .collect();
        let mut all_holes: Vec<_> = plans.iter().map(|p| p.2).collect();
        if d.canvas_behavior.allow_overflow
            && let Some(g) = &d.guide
        {
            let mut gaps = vec![t.image.intersect(t.viewport)];
            for i in 0..g.page_count() {
                let r = d.page_rect(i);
                let screen = egui::Rect::from_min_size(
                    t.screen_point([r[0], r[1]]),
                    egui::vec2(r[2], r[3]) * t.scale,
                );
                gaps = gaps
                    .into_iter()
                    .flat_map(|r| crate::crop::outside(r, screen))
                    .collect();
            }
            all_holes.extend(gaps);
        }
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
            let surfaces = plan.focus_surfaces();
            let density = if surfaces.is_some() {
                self.video_density.unwrap_or(1.)
            } else {
                1.
            };
            let key = surfaces.as_ref().map_or(plan, |(_, outside)| outside);
            if self
                .effects
                .get(id)
                .is_none_or(|e| e.plan != *key || e.offset != offset || e.density != density)
            {
                let base = &self.background.as_ref().unwrap().pixels;
                let (clear, pixels) = if surfaces.is_some() {
                    let (clear, affected) = plan.render_focus_surfaces(base, offset, density)?;
                    (Some(clear), affected)
                } else {
                    (None, key.render(base, offset)?)
                };
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
                        plan: key.clone(),
                        offset,
                        tiles,
                        clear: clear.map(|pixels| focus::textures(ui, &pixels, limit)),
                        density,
                        detail: None,
                        member_detail: HashMap::new(),
                    },
                );
            }
            let effect = self.effects.get_mut(id).unwrap();
            let region = (density < 1. && !self.interacting)
                .then(|| plan.magnified_region())
                .flatten();
            if effect.detail.as_ref().map(|(r, _)| *r) != region {
                effect.detail = if let Some(region) = region {
                    let pixels = plan.render_magnified_region(
                        &self.background.as_ref().unwrap().pixels,
                        offset,
                        region,
                    )?;
                    let limit = (ui.input(|i| i.max_texture_side) as u32).clamp(1, 1024);
                    Some((region, focus::textures(ui, &pixels, limit)))
                } else {
                    None
                };
            }
            let wanted: Vec<_> = if density < 1. && !self.interacting {
                plan.member_lens_regions()
                    .into_iter()
                    .filter(|(member, _)| !plans.iter().any(|p| p.1 == *member))
                    .collect()
            } else {
                vec![]
            };
            effect
                .member_detail
                .retain(|member, (region, _)| wanted.contains(&(*member, *region)));
            for (member, region) in wanted {
                if let std::collections::hash_map::Entry::Vacant(slot) =
                    effect.member_detail.entry(member)
                {
                    let pixels = plan.render_magnified_region(
                        &self.background.as_ref().unwrap().pixels,
                        offset,
                        region,
                    )?;
                    let limit = (ui.input(|i| i.max_texture_side) as u32).clamp(1, 1024);
                    slot.insert((region, focus::textures(ui, &pixels, limit)));
                }
            }
        }
        let mut retained = Vec::new();
        let mut live_vectors = Vec::new();
        let ppp = ui.ctx().pixels_per_point();
        for (index, a) in objects.iter().enumerate() {
            let mut outline;
            let mut holes: Vec<_> = plans.iter().filter(|p| p.0 > index).map(|p| p.2).collect();
            if !d.canvas_behavior.page_owners.is_empty()
                && let Some(g) = &d.guide
            {
                for page in 0..g.page_count() {
                    if page == d.object_page(a) {
                        continue;
                    }
                    let r = d.page_rect(page);
                    holes.push(egui::Rect::from_min_size(
                        t.screen_point([r[0], r[1]]),
                        egui::vec2(r[2], r[3]) * t.scale,
                    ));
                }
            }
            let group = a.focus.is_some().then(|| backdrop_for(a));
            if let Some(g) = group
                .as_ref()
                .filter(|g| g.is_quiet(a) && !plans.iter().any(|p| p.1 == a.id))
            {
                if let Some(e) = g.lead().and_then(|id| self.effects.get(&id))
                    && let Some(clear) = &e.clear
                {
                    focus::paint_member(
                        &painter,
                        t,
                        a,
                        e.plan.output,
                        clear,
                        &holes,
                        e.density,
                        e.member_detail.get(&a.id),
                    );
                }
                outline = a.clone();
                outline.focus = None;
                outline.fill = None;
                outline.text = None;
                outline.shadow = None;
            } else if effect_patch::needs_pixels(a) {
                if let Some(e) = self
                    .effects
                    .get(&a.id)
                    .filter(|_| plans.iter().any(|p| p.1 == a.id))
                {
                    if let Some(clear) = &e.clear {
                        focus::paint(
                            &painter,
                            t,
                            a,
                            e.plan.output,
                            clear,
                            &e.tiles,
                            &holes,
                            e.density,
                            e.detail.as_ref(),
                            &group
                                .iter()
                                .flat_map(|g| g.members())
                                .filter(|m| m.id != a.id)
                                .cloned()
                                .collect::<Vec<_>>(),
                        );
                        // Only the stroke remains a normal retained vector.
                        outline = a.clone();
                        outline.focus = None;
                        outline.fill = None;
                        outline.text = None;
                        outline.shadow = None;
                    } else {
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
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                outline = a.clone();
            }
            let a = &outline;
            let (mut lo, mut hi) = a.bounds();
            if let Some((slo, shi)) = capture_core::shadow::bounds(a) {
                for i in 0..2 {
                    lo[i] = lo[i].min(slo[i]);
                    hi[i] = hi[i].max(shi[i]);
                }
            }
            let margin = if matches!(a.kind, Kind::Arrow | Kind::Polygon) {
                a.width as f32 * 4. + 2.
            } else {
                capture_core::line_style::stroke_reach(a) + 2.
            };
            lo = lo.map(|x| x - margin);
            hi = hi.map(|x| x + margin);
            let screen = egui::Rect::from_min_max(t.screen_point(lo), t.screen_point(hi));
            if !screen.intersects(t.viewport) {
                continue;
            }
            // A later pixel effect has already composed fully covered layers.
            // Do not upload/rasterize them again underneath its replacement.
            if holes
                .iter()
                .any(|h| h.contains_rect(screen.intersect(painter.clip_rect())))
            {
                continue;
            }
            let patterned_frame = a.is_image_frame()
                && a.line_style
                    .is_some_and(|s| s.pattern != capture_core::line_style::Pattern::Solid);
            let mut frame_outline_only = false;
            if (a.kind == Kind::Image || a.is_image_frame())
                && a.convex_frame()
                && a.image.as_ref().is_some_and(|b| b.vector().is_none())
                && a.image
                    .as_ref()
                    .unwrap()
                    .size()
                    .into_iter()
                    .all(|n| n <= ui.input(|i| i.max_texture_side) as u32)
            {
                if a.shadow.is_some() {
                    let anchor = a.start;
                    let mut local = capture_core::canvas_expansion::translated(
                        std::slice::from_ref(a),
                        anchor.map(|n| -n),
                    )?
                    .remove(0);
                    for p in std::iter::once(&mut local.start)
                        .chain(std::iter::once(&mut local.end))
                        .chain(local.points.iter_mut())
                    {
                        for n in p {
                            *n = (*n * 256.).round() / 256.;
                        }
                    }
                    capture_core::shadow::normalize_opaque_source(&mut local);
                    if self
                        .shadows
                        .get(&a.id)
                        .is_none_or(|(old, _, _)| old != &local)
                        && let Some(patch) = capture_core::shadow::patch(&local)
                    {
                        let texture = ui.ctx().load_texture(
                            "frame-shadow",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [
                                    patch.pixels.width() as usize,
                                    patch.pixels.height() as usize,
                                ],
                                patch.pixels.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        );
                        self.shadows.insert(a.id, (local, patch, texture));
                    }
                    if let Some((_, patch, texture)) = self.shadows.get(&a.id) {
                        let origin = [0, 1].map(|i| anchor[i] + patch.origin[i] as f32);
                        let rect = egui::Rect::from_min_size(
                            t.screen_point(origin),
                            egui::vec2(patch.pixels.width() as f32, patch.pixels.height() as f32)
                                * t.scale,
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
                let b = a.image.as_ref().unwrap();
                let (lo, hi) = a.body_bounds();
                // A border, edge or filter is part of the texture, so moving the
                // object only moves the texture (the finish is cached).
                let pixels = b.display_pixels([hi[0] - lo[0], hi[1] - lo[1]])?;
                let pixels = &pixels;
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
                // The image mask bounds exclude the outer half of the outline.
                // Clip against expanded ink bounds, never the frame's body rectangle.
                with_holes(&painter, screen, &holes, |p| {
                    if let Some(quad) = b.perspective() {
                        // A fine grid keeps the warp straight along each line.
                        const CELLS: u32 = 24;
                        let mut mesh = egui::Mesh::with_texture(texture.id());
                        for row in 0..=CELLS {
                            for column in 0..=CELLS {
                                let s = [column as f32 / CELLS as f32, row as f32 / CELLS as f32];
                                let at = capture_core::bitmap::warp_point(quad, s);
                                let across = if a.mirror_x { 1. - at[0] } else { at[0] };
                                let point = a.rotation().point([
                                    min[0] + across * (max[0] - min[0]),
                                    min[1] + at[1] * (max[1] - min[1]),
                                ]);
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos: t.screen_point(point),
                                    uv: egui::pos2(x + s[0] * w, y + s[1] * h),
                                    color: egui::Color32::from_white_alpha(a.image_opacity()),
                                });
                            }
                        }
                        for row in 0..CELLS {
                            for column in 0..CELLS {
                                let i = row * (CELLS + 1) + column;
                                let n = i + CELLS + 1;
                                mesh.indices.extend([i, i + 1, n, i + 1, n + 1, n]);
                            }
                        }
                        p.add(mesh);
                    } else if a.is_shape() || a.rotation_degrees != 0. || a.mirror_x {
                        let boundary = a.frame_boundary();
                        let points: Vec<_> = boundary
                            .iter()
                            .map(|p| t.screen_point(a.rotation().point(*p)))
                            .collect();
                        if let Some(fill) = a.fill {
                            p.add(egui::Shape::convex_polygon(
                                points.clone(),
                                egui::Color32::from_rgba_unmultiplied(
                                    fill[0], fill[1], fill[2], fill[3],
                                ),
                                egui::Stroke::NONE,
                            ));
                        }
                        let mut mesh = egui::Mesh::with_texture(texture.id());
                        for (local, screen) in boundary.iter().zip(&points) {
                            mesh.vertices.push(egui::epaint::Vertex {
                                pos: *screen,
                                uv: egui::pos2(
                                    x + (local[0] - min[0]) / (max[0] - min[0]) * w,
                                    y + (local[1] - min[1]) / (max[1] - min[1]) * h,
                                ),
                                color: egui::Color32::from_white_alpha(a.image_opacity()),
                            });
                        }
                        for i in 1..points.len().saturating_sub(1) {
                            mesh.indices.extend([0, i as u32, i as u32 + 1]);
                        }
                        p.add(mesh);
                        if a.is_shape() && !patterned_frame {
                            p.add(egui::Shape::closed_line(
                                points,
                                egui::Stroke::new(
                                    a.width as f32 * t.scale,
                                    egui::Color32::from_rgba_unmultiplied(
                                        a.color[0], a.color[1], a.color[2], a.color[3],
                                    ),
                                ),
                            ));
                        }
                    } else {
                        p.add(
                            egui::epaint::RectShape::filled(
                                rect,
                                (a.corner_radius * t.scale).round().clamp(0., 255.) as u8,
                                egui::Color32::from_white_alpha(a.image_opacity()),
                            )
                            .with_texture(
                                texture.id(),
                                egui::Rect::from_min_max(
                                    egui::pos2(x, y),
                                    egui::pos2(x + w, y + h),
                                ),
                            ),
                        );
                    }
                });
                if !patterned_frame {
                    continue;
                }
                frame_outline_only = true;
            }
            live_vectors.push(a.id);
            let mut local = capture_core::canvas_expansion::translated(
                std::slice::from_ref(a),
                lo.map(|x| -x),
            )?
            .remove(0);
            if frame_outline_only {
                local.image = None;
                local.fill = None;
                local.text = None;
                local.shadow = None;
            }
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
            let prior = self
                .vectors
                .get(&a.id)
                .filter(|v| v.local == local)
                .map(|v| Viewport {
                    size: v.size,
                    origin: v.origin,
                    scale: v.scale,
                });
            let key = crate::canvas_display::cached_viewport(
                Transform {
                    image: screen,
                    viewport: t.viewport,
                    scale: t.scale,
                    zoom: t.zoom,
                },
                ppp,
                ui.input(|i| i.max_texture_side) as u32,
                prior,
            );
            let scale = key.scale;
            let size = key.size;
            if self.vectors.get(&a.id).is_none_or(|v| {
                v.local != local || v.scale != scale || v.size != size || v.origin != key.origin
            }) {
                let pixels = render_annotation_preview(&local, key)?;
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
                        origin: key.origin,
                    },
                );
            }
            let v = &self.vectors[&a.id];
            let rect = egui::Rect::from_min_size(
                t.screen_point([0, 1].map(|i| lo[i] + key.origin[i] as f32)),
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
        self.shadows
            .retain(|id, _| objects.iter().any(|a| a.id == *id && a.shadow.is_some()));
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
    /// BI8, BI9: perspective must leave the flat retained scene, so the canvas
    /// shows the same warp as export.
    #[test]
    fn perspective_documents_use_the_engine_display() {
        let pixels = image::RgbaImage::from_pixel(64, 48, image::Rgba([200, 40, 40, 255]));
        let bitmap = capture_core::bitmap::Bitmap::new(
            capture_core::clipboard_payload::ImagePayload::from_rgba(64, 48, pixels.into_raw())
                .unwrap()
                .image,
        )
        .unwrap();
        let mut d = Document::canvas(300, 200, [255; 4]).unwrap();
        let mut a = Arc::new(bitmap.clone()).annotation(1);
        a.start = [20., 20.];
        a.end = [180., 140.];
        d.annotations.push(a);
        assert!(supported(&d), "a flat image keeps the fast scene");
        let quad = capture_core::bitmap::Perspective::TiltLeft.quad(0.6, 0.5);
        d.annotations[0].image = Some(Arc::new(bitmap.with_perspective(Some(quad)).unwrap()));
        assert!(!supported(&d), "a tilted image uses the display list");
        let flat = bitmap.with_perspective(None).unwrap();
        d.annotations[0].image = Some(Arc::new(flat));
        assert!(supported(&d));
        d.canvas_behavior.perspective = Some(capture_core::tilt::Tilt {
            quad,
            background: None,
        });
        assert!(!supported(&d), "a tilted picture uses the display list");
        // And that path really tilts what is drawn.
        let list = d.display_list().unwrap();
        let view = capture_core::display::Viewport {
            size: [300, 200],
            origin: [0., 0.],
            scale: 1.,
        };
        let tilted = list.render_viewport(view).unwrap();
        d.canvas_behavior.perspective = None;
        let flat = d.display_list().unwrap().render_viewport(view).unwrap();
        assert_ne!(tilted, flat);
    }
    #[test]
    fn outlined_rotated_shadow_frames_retain_pixels_and_do_not_clip_ink_at_high_zoom() {
        for kind in [Kind::Image, Kind::Ellipse, Kind::Polygon] {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let pixels = image::RgbaImage::from_fn(1920, 1080, |x, y| {
                image::Rgba([(x % 255) as u8, (y % 255) as u8, 140, 255])
            });
            let bitmap = Arc::new(
                capture_core::bitmap::Bitmap::new(
                    capture_core::clipboard_payload::ImagePayload::from_rgba(
                        1920,
                        1080,
                        pixels.into_raw(),
                    )
                    .unwrap()
                    .image,
                )
                .unwrap(),
            );
            let mut a = bitmap.annotation(1);
            a.kind = kind;
            a.start = [40., 40.];
            a.end = [340., 280.];
            a.width = 14;
            if kind == Kind::Polygon {
                a.points = capture_core::polygon::outline(a.start, a.end, 5);
            }
            a.shadow = Some(Default::default());
            let mut d = Document::canvas(700, 500, [255; 4]).unwrap();
            d.annotations.push(a.clone());
            d.promote_features();
            assert!(supported(&d));
            let mut scene = Scene::default();
            let paint = |scene: &mut Scene, d: &Document, zoom: f32| {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800., 600.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let origin = egui::pos2(400. - 190. * zoom, 300. - 160. * zoom);
                        scene
                            .paint(
                                ui,
                                d,
                                Transform {
                                    viewport: ui.max_rect(),
                                    image: egui::Rect::from_min_size(
                                        origin,
                                        egui::vec2(700., 500.) * zoom,
                                    ),
                                    scale: zoom,
                                    zoom,
                                },
                            )
                            .unwrap();
                    },
                ))
            };
            for (iteration, rotation) in [0., 25., 0.].into_iter().enumerate() {
                d.annotations[0].rotation_degrees = rotation;
                let first = paint(&mut scene, &d, 1.);
                let start = std::time::Instant::now();
                for zoom in [1., 1.25, 2., 4., 8., 12., 16., 4., 1.] {
                    d.annotations =
                        capture_core::canvas_expansion::translated(&d.annotations, [0.25, 0.125])
                            .unwrap();
                    let out = paint(&mut scene, &d, zoom);
                    assert!(
                        scene.vectors.is_empty(),
                        "raster frame fell back to CPU at {zoom}"
                    );
                    assert!(
                        out.textures_delta.set.is_empty(),
                        "frame pixels regenerated at {zoom}"
                    );
                    if zoom == 1. && kind != Kind::Image && rotation == 0. {
                        let ink=out.shapes.iter().find(|s|matches!(&s.shape,egui::Shape::Path(p) if p.closed && p.stroke.width>0.)).unwrap();
                        let egui::Shape::Path(path) = &ink.shape else {
                            unreachable!()
                        };
                        let left = path
                            .points
                            .iter()
                            .map(|p| p.x)
                            .fold(f32::INFINITY, f32::min);
                        assert!(
                            ink.clip_rect.left() <= left - 7.,
                            "outline clipped at body boundary"
                        );
                    }
                }
                eprintln!(
                    "retained-frame {kind:?} rotation={rotation} nine zoom/drag frames {:?}",
                    start.elapsed()
                );
                if iteration == 0
                    && kind == Kind::Polygon
                    && let Ok(dir) = std::env::var("CLIPDESIGNER_FRAME_PAGES_UI")
                {
                    let last = paint(&mut scene, &d, 1.);
                    dump_meshes(&ctx, &dir, "frame-ink", first, last);
                }
            }
        }
    }
    #[test]
    fn shape_frames_reuse_source_texture_during_movement_and_zoom() {
        let ctx = egui::Context::default();
        for kind in [Kind::Rectangle, Kind::Ellipse, Kind::Polygon] {
            let mut d = Document::canvas(800, 600, [255; 4]).unwrap();
            let mut a = capture_core::templates::placeholder()
                .unwrap()
                .annotation(1);
            a.kind = kind;
            a.start = [100., 100.];
            a.end = [400., 400.];
            a.corner_radius = if kind == Kind::Rectangle { 20. } else { 0. };
            if kind == Kind::Polygon {
                a.points = capture_core::polygon::outline(a.start, a.end, 6);
            }
            d.annotations.push(a);
            d.promote_features();
            let mut scene = Scene::default();
            let paint = |scene: &mut Scene, d: &Document, zoom| {
                PaintedOutput(ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(700., 500.),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        let t = Transform {
                            image: ui.max_rect(),
                            viewport: ui.max_rect(),
                            scale: zoom,
                            zoom,
                        };
                        scene.paint(ui, d, t).unwrap();
                    },
                ))
            };
            paint(&mut scene, &d, 1.);
            for zoom in [0.25, 0.5, 0.75, 1., 1.25, 2., 4.] {
                d.annotations =
                    capture_core::canvas_expansion::translated(&d.annotations, [0.3, 0.2]).unwrap();
                let output = paint(&mut scene, &d, zoom);
                assert!(
                    output.textures_delta.set.is_empty(),
                    "{kind:?} at {zoom} reuploaded its pixels"
                );
                assert!(
                    scene.vectors.is_empty(),
                    "shape image should use source texture and geometry"
                );
            }
        }
    }
    #[test]
    fn blank_canvas_raster_and_vector_icons_reuse_pixels_during_fractional_movement() {
        for id in ["builtin-raster-sample", "builtin-check-circle"] {
            let mut session =
                capture_core::Session::new(Document::blank(3000, 2000).unwrap()).unwrap();
            session
                .execute(capture_core::Command::PasteObjects {
                    payload: capture_core::icon_library::builtin::find(id)
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
        if let Ok(root) = std::env::var("CLIPDESIGNER_HEADLESS_INTERACTIONS_DIR") {
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
