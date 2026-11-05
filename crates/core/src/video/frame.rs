//! Video samples use the same editable shape, crop, stroke and shadow renderer as images.
use crate::{Annotation, Error, Kind, Result, bitmap::Bitmap, image_frame::ImageView};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub shape: Kind,
    pub sides: u8,
    pub radius: f32,
    pub view: ImageView,
    pub fill: bool,
    pub rotation: f32,
    pub mirror: bool,
    pub flip_y: bool,
    pub outline: [u8; 4],
    pub width: u32,
    pub opacity: u8,
    pub shadow: Option<crate::shadow::Shadow>,
    pub line_style: Option<crate::line_style::Style>,
}
impl Default for Frame {
    fn default() -> Self {
        Self {
            shape: Kind::Rectangle,
            sides: 5,
            radius: 20.,
            view: ImageView::default(),
            fill: true,
            rotation: 0.,
            mirror: false,
            flip_y: false,
            outline: [255; 4],
            width: 0,
            opacity: 255,
            shadow: None,
            line_style: None,
        }
    }
}
impl Frame {
    pub fn unmodified_pixels(&self) -> bool {
        self.shape == Kind::Rectangle
            && self.radius == 0.
            && self.view == ImageView::default()
            && self.rotation == 0.
            && !self.mirror
            && !self.flip_y
            && self.width == 0
            && self.opacity == 255
            && self.shadow.is_none()
    }
    pub fn render(
        &self,
        source: Arc<image::RgbaImage>,
        rect: [f32; 4],
        size: [u32; 2],
        id: u64,
    ) -> Result<image::RgbaImage> {
        let annotation = self.annotation(source, rect, size, id)?;
        let mut output = image::RgbaImage::new(size[0], size[1]);
        crate::render_annotation(&mut output, &annotation);
        Ok(output)
    }
    pub fn validate(&self) -> Result<()> {
        self.view.validate()?;
        if !matches!(self.shape, Kind::Rectangle | Kind::Ellipse | Kind::Polygon)
            || !(3..=12).contains(&self.sides)
            || !self.radius.is_finite()
            || !(0. ..=2048.).contains(&self.radius)
            || !self.rotation.is_finite()
            || !(-360. ..=360.).contains(&self.rotation)
            || self.width > 128
        {
            return Err(Error::Invalid(
                "Choose a valid video frame shape, rotation and outline.".into(),
            ));
        }
        if let Some(s) = self.shadow {
            s.validate()?;
        }
        if let Some(s) = &self.line_style {
            s.validate()?;
        }
        Ok(())
    }
    pub fn annotation(
        &self,
        source: Arc<image::RgbaImage>,
        rect: [f32; 4],
        size: [u32; 2],
        id: u64,
    ) -> Result<Annotation> {
        let [x, y, w, h] = [
            rect[0] * size[0] as f32,
            rect[1] * size[1] as f32,
            rect[2] * size[0] as f32,
            rect[3] * size[1] as f32,
        ];
        let mut pixels = source;
        let mut view = self.view;
        if self.fill {
            let source_aspect =
                pixels.width() as f32 * view.rect[2] / (pixels.height() as f32 * view.rect[3]);
            let target = w / h;
            if source_aspect > target {
                let width = view.rect[2] * target / source_aspect;
                view.rect[0] += (view.rect[2] - width) * 0.5;
                view.rect[2] = width;
            } else {
                let height = view.rect[3] * source_aspect / target;
                view.rect[1] += (view.rect[3] - height) * 0.5;
                view.rect[3] = height;
            }
        } else {
            let [sx, sy, sw, sh] = view.rect;
            let crop = image::imageops::crop_imm(
                pixels.as_ref(),
                (sx * pixels.width() as f32) as u32,
                (sy * pixels.height() as f32) as u32,
                (sw * pixels.width() as f32).floor().max(1.) as u32,
                (sh * pixels.height() as f32).floor().max(1.) as u32,
            )
            .to_image();
            let fit = image::imageops::thumbnail(
                &crop,
                w.round().max(1.) as u32,
                h.round().max(1.) as u32,
            );
            let mut padded =
                image::RgbaImage::new(w.round().max(1.) as u32, h.round().max(1.) as u32);
            let offset = [
                (padded.width() - fit.width()) / 2,
                (padded.height() - fit.height()) / 2,
            ];
            image::imageops::overlay(&mut padded, &fit, offset[0] as i64, offset[1] as i64);
            pixels = Arc::new(padded);
            view = ImageView::default();
        }
        if self.flip_y {
            pixels = Arc::new(image::imageops::flip_vertical(pixels.as_ref()));
            view.rect[1] = (1. - view.rect[1] - view.rect[3]).max(0.);
        }
        let bitmap = Bitmap::transient(pixels)?
            .with_view(Some(view))?
            .with_frame_opacity(self.opacity);
        let mut a = Arc::new(bitmap).annotation(id);
        a.kind = self.shape;
        a.start = [x, y];
        a.end = [x + w, y + h];
        a.color = self.outline;
        a.width = self.width;
        a.corner_radius = self.radius;
        a.rotation_degrees = self.rotation;
        a.mirror_x = self.mirror;
        a.shadow = self.shadow;
        a.line_style = self.line_style;
        if a.kind == Kind::Polygon {
            a.points = (0..self.sides)
                .map(|n| {
                    let t = std::f32::consts::TAU * n as f32 / self.sides as f32
                        - std::f32::consts::FRAC_PI_2;
                    [
                        x + w * 0.5 + t.cos() * w * 0.5,
                        y + h * 0.5 + t.sin() * h * 0.5,
                    ]
                })
                .collect();
        }
        Ok(a)
    }
}

/// Coextensive opaque pictures can dissolve directly as GPU textures. Other
/// overlaps retain the exact additive raster compositor.
pub fn texture_dissolve(p: &super::editor::Project, size: [u32; 2], time: u64, track: u64) -> bool {
    let clips: Vec<_> = p
        .segments
        .iter()
        .filter(|c| {
            c.track == track
                && c.active(time)
                && !c.hidden
                && c.kind() != super::editor::MediaKind::Audio
        })
        .collect();
    clips.len() > 1
        && clips.iter().all(|c| {
            c.kind() == super::editor::MediaKind::Video
                && c.frame.as_ref().is_none_or(Frame::unmodified_pixels)
                && c.rect == clips[0].rect
                && c.source.as_ref().map_or(p.source_size, |s| s.size) == size
                && (c.source.is_some() || p.crop == [0, 0, size[0], size[1]])
        })
}

/// Decoded image clips can share the video dissolve path when their alpha and
/// fitted bounds prove that source-over compensation equals additive mixing.
pub fn texture_dissolve_sources(
    p: &super::editor::Project,
    _size: [u32; 2],
    time: u64,
    track: u64,
    sources: &[(u64, Arc<Bitmap>)],
) -> bool {
    let clips: Vec<_> = p
        .segments
        .iter()
        .filter(|c| {
            c.track == track
                && c.active(time)
                && !c.hidden
                && c.kind() != super::editor::MediaKind::Audio
        })
        .collect();
    matching_framed_pair(p, &clips, time, sources)
        || clips.len() == 2
            && clips.iter().all(|c| {
                if c.frame.as_ref().is_some_and(|f| !f.unmodified_pixels()) {
                    return false;
                }
                let Some((_, bitmap)) = sources.iter().find(|(id, _)| *id == c.id) else {
                    return false;
                };
                if !bitmap.opaque_transient() {
                    return false;
                }
                true
            })
}

// Equal opaque sources with identical frame geometry can be mixed before
// clipping. Draw the shared outline/shadow once, rather than rasterizing two
// full-canvas layers. Restrict this to complementary weights: extra fades and
// different frames still use the exact general compositor.
fn matching_framed_pair(
    p: &super::editor::Project,
    clips: &[&super::editor::Segment],
    time: u64,
    sources: &[(u64, Arc<Bitmap>)],
) -> bool {
    if clips.len() != 2
        || clips[0].rect != clips[1].rect
        || clips[0].frame != clips[1].frame
        || !clips[0]
            .frame
            .as_ref()
            .is_some_and(|s| s.fill && !s.unmodified_pixels())
        || (p.opacity(clips[0], time) + p.opacity(clips[1], time) - 1.).abs() > 0.000001
    {
        return false;
    }
    let images: Vec<_> = clips
        .iter()
        .filter_map(|c| {
            sources
                .iter()
                .find(|(id, b)| *id == c.id && b.opaque_transient())
                .and_then(|(_, b)| b.pixels().ok())
        })
        .collect();
    images.len() == 2 && images[0].dimensions() == images[1].dimensions()
}

/// Combine cached transparent media layers and live vector annotations in one order.
pub fn ordered_objects(
    doc: &crate::Document,
    layers: &[(u64, Arc<Bitmap>)],
    time: u64,
    hide_annotations: bool,
) -> Vec<Annotation> {
    let Some(p) = &doc.video else { return vec![] };
    let objects = super::editor::objects_at(doc, time);
    let mut out = vec![];
    for layer in p.layers().iter().rev() {
        if p.layer_hidden(*layer) {
            continue;
        }
        match *layer {
            super::links::Layer::Annotation(track) if !hide_annotations => out.extend(
                objects
                    .iter()
                    .filter(|a| p.objects.get(&a.id).is_some_and(|t| t.track == track))
                    .cloned(),
            ),
            super::links::Layer::Zoom(id) => {
                if let Some(z) = p.zooms.iter().find(|z| z.id == id) {
                    z.transform(&mut out, [doc.width, doc.height], time);
                }
            }
            super::links::Layer::Media(track) => {
                if let Some((_, b)) = layers.iter().find(|(id, _)| *id == track) {
                    let mut layer = compact_layer(b, u64::MAX - track);
                    // Live CPU overlap textures match display resolution. The
                    // document and its editable overlays stay in source pixels.
                    let size = b.size();
                    for point in [&mut layer.start, &mut layer.end] {
                        point[0] *= doc.width as f32 / size[0] as f32;
                        point[1] *= doc.height as f32 / size[1] as f32;
                    }
                    out.push(layer);
                }
            }
            _ => (),
        }
    }
    out
}

// A framed webcam occupies only a small part of its transparent layer. Carry
// that region into Zoom compositing instead of sampling a full output-sized
// transparent bitmap for every frame. One transparent pixel preserves bilinear
// edge sampling; full-frame sources keep the existing zero-copy path.
fn compact_layer(bitmap: &Arc<Bitmap>, id: u64) -> Annotation {
    let original = || bitmap.clone().annotation(id);
    let Ok(pixels) = bitmap.pixels() else {
        return original();
    };
    if bitmap.opaque_transient() {
        return original();
    }
    let mut bounds = [pixels.width(), pixels.height(), 0, 0];
    for (y, row) in pixels
        .as_raw()
        .chunks_exact(pixels.width() as usize * 4)
        .enumerate()
    {
        let row = row.as_chunks::<4>().0;
        if let Some(left) = row.iter().position(|p| p[3] != 0) {
            let right = row.iter().rposition(|p| p[3] != 0).unwrap() + 1;
            bounds[0] = bounds[0].min(left as u32);
            bounds[1] = bounds[1].min(y as u32);
            bounds[2] = bounds[2].max(right as u32);
            bounds[3] = bounds[3].max(y as u32 + 1);
        }
    }
    bounds[0] = bounds[0].saturating_sub(1);
    bounds[1] = bounds[1].saturating_sub(1);
    bounds[2] = (bounds[2] + 1).min(pixels.width());
    bounds[3] = (bounds[3] + 1).min(pixels.height());
    if bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || (bounds[2] - bounds[0]) as u64 * (bounds[3] - bounds[1]) as u64
            > pixels.width() as u64 * pixels.height() as u64 / 2
    {
        return original();
    }
    let crop = image::imageops::crop_imm(
        pixels.as_ref(),
        bounds[0],
        bounds[1],
        bounds[2] - bounds[0],
        bounds[3] - bounds[1],
    )
    .to_image();
    let Ok(cropped) = Bitmap::transient(Arc::new(crop)) else {
        return original();
    };
    let mut a = Arc::new(cropped).annotation(id);
    a.start = [bounds[0] as f32, bounds[1] as f32];
    a.end = [bounds[2] as f32, bounds[3] as f32];
    a
}

#[test]
fn compact_video_layer_preserves_transparent_edges_during_zoom() {
    let pixels = image::RgbaImage::from_fn(240, 160, |x, y| {
        image::Rgba(if (165..205).contains(&x) && (90..130).contains(&y) {
            [40, 160, 220, ((x + y) % 160 + 90) as u8]
        } else {
            [0; 4]
        })
    });
    let bitmap = Arc::new(Bitmap::transient(Arc::new(pixels)).unwrap());
    let compact = compact_layer(&bitmap, 1);
    for scale in [1., 1.35, 2.3] {
        let mut original = bitmap.clone().annotation(1);
        let mut cropped = compact.clone();
        for a in [&mut original, &mut cropped] {
            for p in [&mut a.start, &mut a.end] {
                p[0] = p[0] * scale - 150.;
                p[1] = p[1] * scale - 70.;
            }
        }
        let mut expected = image::RgbaImage::from_pixel(300, 200, image::Rgba([60, 60, 60, 255]));
        let mut actual = expected.clone();
        crate::render_annotation(&mut expected, &original);
        crate::render_annotation(&mut actual, &cropped);
        assert!(
            expected
                .as_raw()
                .iter()
                .zip(actual.as_raw())
                .all(|(a, b)| a.abs_diff(*b) <= 1)
        );
    }
}
pub fn composite(
    doc: &crate::Document,
    layers: &[(u64, Arc<Bitmap>)],
    time: u64,
) -> Result<image::RgbaImage> {
    let background = doc.video.as_ref().map_or([0; 4], |p| p.background);
    let mut output = image::RgbaImage::from_pixel(doc.width, doc.height, image::Rgba(background));
    for a in ordered_objects(doc, layers, time, false) {
        crate::render_annotation(&mut output, &a);
    }
    Ok(output)
}

/// Export-local bounded cache for unchanged annotation artwork. Animated objects
/// and backdrop-dependent obfuscation always render against the current frame.
#[derive(Default)]
pub struct CompositeCache {
    size: [u32; 2],
    entries: std::collections::VecDeque<(Annotation, Annotation)>,
}
impl CompositeCache {
    pub fn composite(
        &mut self,
        doc: &crate::Document,
        layers: &[(u64, Arc<Bitmap>)],
        time: u64,
    ) -> Result<image::RgbaImage> {
        let size = [doc.width, doc.height];
        if size != self.size {
            self.entries.clear();
            self.size = size;
        }
        let background = doc.video.as_ref().map_or([0; 4], |p| p.background);
        let mut out = image::RgbaImage::from_pixel(size[0], size[1], image::Rgba(background));
        for a in ordered_objects(doc, layers, time, false) {
            let cacheable = a.focus.is_none()
                && !matches!(a.kind, Kind::Blur | Kind::Pixelate | Kind::Eraser)
                && !a.image.as_ref().is_some_and(|b| b.is_cursor())
                && doc.annotations.iter().any(|original| original == &a);
            if !cacheable {
                crate::render_annotation(&mut out, &a);
                continue;
            }
            let index = self.entries.iter().position(|(key, _)| key == &a);
            let patch = if let Some(i) = index {
                let entry = self.entries.remove(i).unwrap();
                let patch = entry.1.clone();
                self.entries.push_back(entry);
                patch
            } else {
                let mut pixels = image::RgbaImage::new(size[0], size[1]);
                crate::render_annotation(&mut pixels, &a);
                let mut bounds = [size[0], size[1], 0, 0];
                for (x, y, p) in pixels.enumerate_pixels() {
                    if p[3] != 0 {
                        bounds[0] = bounds[0].min(x);
                        bounds[1] = bounds[1].min(y);
                        bounds[2] = bounds[2].max(x + 1);
                        bounds[3] = bounds[3].max(y + 1);
                    }
                }
                if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
                    continue;
                }
                let cropped = image::imageops::crop_imm(
                    &pixels,
                    bounds[0],
                    bounds[1],
                    bounds[2] - bounds[0],
                    bounds[3] - bounds[1],
                )
                .to_image();
                let mut patch = Arc::new(Bitmap::transient(Arc::new(cropped))?).annotation(a.id);
                patch.start = [bounds[0] as f32, bounds[1] as f32];
                patch.end = [bounds[2] as f32, bounds[3] as f32];
                self.entries.push_back((a, patch.clone()));
                while self.entries.len() > 32
                    || self
                        .entries
                        .iter()
                        .map(|(a, p)| {
                            a.image.as_ref().map_or(0, |b| b.memory_bytes())
                                + p.image.as_ref().map_or(0, |b| b.memory_bytes())
                        })
                        .sum::<usize>()
                        > 64 * 1024 * 1024
                {
                    self.entries.pop_front();
                }
                patch
            };
            crate::render_annotation(&mut out, &patch);
        }
        Ok(out)
    }
}

/// The live canvas keeps decoded pixels separate from editable frame geometry.
pub fn source_objects(
    doc: &crate::Document,
    sources: &[(u64, Arc<Bitmap>)],
    time: u64,
    hide_annotations: bool,
) -> Vec<Annotation> {
    let Some(p) = &doc.video else { return vec![] };
    let objects = super::editor::objects_at(doc, time);
    let mut out = vec![];
    for layer in p.layers().iter().rev() {
        if p.layer_hidden(*layer) {
            continue;
        }
        match *layer {
            super::links::Layer::Annotation(track) if !hide_annotations => out.extend(
                objects
                    .iter()
                    .filter(|a| p.objects.get(&a.id).is_some_and(|t| t.track == track))
                    .cloned(),
            ),
            super::links::Layer::Zoom(id) => {
                if let Some(z) = p.zooms.iter().find(|z| z.id == id) {
                    z.transform(&mut out, [doc.width, doc.height], time);
                }
            }
            super::links::Layer::Media(track) => {
                let clips: Vec<_> = p
                    .segments
                    .iter()
                    .filter(|c| c.track == track && c.active(time) && !c.hidden)
                    .collect();
                if matching_framed_pair(p, &clips, time, sources) {
                    let first = sources
                        .iter()
                        .find(|(id, _)| *id == clips[0].id)
                        .unwrap()
                        .1
                        .pixels()
                        .unwrap();
                    let second = sources
                        .iter()
                        .find(|(id, _)| *id == clips[1].id)
                        .unwrap()
                        .1
                        .pixels()
                        .unwrap();
                    let weight = p.opacity(clips[1], time) as f32;
                    let bytes: Vec<u8> = first
                        .as_raw()
                        .iter()
                        .zip(second.as_raw())
                        .map(|(&a, &b)| {
                            (a as f32 * (1. - weight) + b as f32 * weight).round() as u8
                        })
                        .collect();
                    let pixels =
                        image::RgbaImage::from_raw(first.width(), first.height(), bytes).unwrap();
                    if let Ok(a) = clips[0].frame.as_ref().unwrap().annotation(
                        Arc::new(pixels),
                        clips[0].rect,
                        [doc.width, doc.height],
                        u64::MAX - clips[0].id,
                    ) {
                        out.push(a);
                    }
                    continue;
                }
                // A dissolve's weights are additive, while the canvas paints
                // source-over. Coextensive opaque video can use equivalent
                // source-over alphas, keeping preview on the GPU without the
                // dark dip from fading both pictures independently.
                let dissolve =
                    texture_dissolve_sources(p, [doc.width, doc.height], time, track, sources);
                let mut media_objects = Vec::new();
                for c in &clips {
                    let Some((_, bitmap)) = sources.iter().find(|(id, _)| *id == c.id) else {
                        continue;
                    };
                    let Ok(pixels) = bitmap.pixels() else {
                        continue;
                    };
                    let mut rect = c.rect;
                    let mut style = c.frame.clone().unwrap_or(Frame {
                        radius: 0.,
                        fill: p.fill,
                        ..Default::default()
                    });
                    if c.frame.is_none() && c.source.is_none() {
                        let [x, y, w, h] = p.crop;
                        style.view.rect = [
                            x as f32 / pixels.width() as f32,
                            y as f32 / pixels.height() as f32,
                            w as f32 / pixels.width() as f32,
                            h as f32 / pixels.height() as f32,
                        ];
                    }
                    if !style.fill && (c.frame.is_none() || dissolve) {
                        let aspect = pixels.width() as f32 * style.view.rect[2]
                            / (pixels.height() as f32 * style.view.rect[3]);
                        let target = rect[2] * doc.width as f32 / (rect[3] * doc.height as f32);
                        if aspect > target {
                            let h = rect[3] * target / aspect;
                            rect[1] += (rect[3] - h) / 2.;
                            rect[3] = h;
                        } else {
                            let w = rect[2] * aspect / target;
                            rect[0] += (rect[2] - w) / 2.;
                            rect[2] = w;
                        }
                        style.fill = true;
                    }
                    let gain = p.opacity(c, time);
                    style.opacity = (style.opacity as f64 * gain).round() as u8;
                    style.outline[3] = (style.outline[3] as f64 * gain).round() as u8;
                    if let Some(shadow) = style.shadow.as_mut() {
                        shadow.color[3] = (shadow.color[3] as f64 * gain).round() as u8;
                    }
                    if let Ok(a) = style.annotation(
                        pixels.clone(),
                        rect,
                        [doc.width, doc.height],
                        u64::MAX - c.id,
                    ) {
                        media_objects.push((a, gain));
                    }
                }
                if dissolve && media_objects.len() == 2 {
                    let (top, top_gain) = media_objects.pop().unwrap();
                    let (bottom, gain) = media_objects.pop().unwrap();
                    out.extend(dissolve_regions(bottom, gain, &top, top_gain));
                    out.push(top);
                } else {
                    out.extend(media_objects.into_iter().map(|(a, _)| a));
                }
            }
            _ => (),
        }
    }
    out
}

/// Partition the lower opaque rectangle at the upper picture's coverage edges.
/// Source-over alpha compensation is needed only in their intersection; the
/// letterbox margins retain the normal gain and reveal the layers below them.
fn dissolve_regions(
    bottom: Annotation,
    gain: f64,
    top: &Annotation,
    top_gain: f64,
) -> Vec<Annotation> {
    let (lo, hi) = bottom.body_bounds();
    let (tl, th) = top.body_bounds();
    let il = [lo[0].max(tl[0]), lo[1].max(tl[1])];
    let ih = [hi[0].min(th[0]), hi[1].min(th[1])];
    if il[0] >= ih[0] || il[1] >= ih[1] {
        return vec![bottom];
    }
    let bounds = [
        (lo, [hi[0], il[1]], gain),
        ([lo[0], ih[1]], hi, gain),
        ([lo[0], il[1]], [il[0], ih[1]], gain),
        ([ih[0], il[1]], [hi[0], ih[1]], gain),
        (
            il,
            ih,
            (gain / (1. - top_gain).max(f64::EPSILON)).clamp(0., 1.),
        ),
    ];
    let bitmap = bottom.image.as_ref().unwrap();
    let view = bitmap.view().rect;
    bounds
        .into_iter()
        .enumerate()
        .filter_map(|(i, (a, b, opacity))| {
            if a[0] >= b[0] || a[1] >= b[1] {
                return None;
            }
            let mut part = bottom.clone();
            part.id -= 1_000_000 * (i as u64 + 1);
            part.start = a;
            part.end = b;
            part.image = Some(Arc::new(
                bitmap
                    .with_view(Some(ImageView {
                        rect: [
                            view[0] + (a[0] - lo[0]) / (hi[0] - lo[0]) * view[2],
                            view[1] + (a[1] - lo[1]) / (hi[1] - lo[1]) * view[3],
                            (b[0] - a[0]) / (hi[0] - lo[0]) * view[2],
                            (b[1] - a[1]) / (hi[1] - lo[1]) * view[3],
                        ],
                    }))
                    .ok()?
                    .with_frame_opacity((opacity * 255.).round() as u8),
            ));
            Some(part)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matching_shaped_dissolve_matches_additive_frame_pixels_and_keeps_unequal_frames_on_fallback()
    {
        let mut doc = crate::Document::blank(160, 90).unwrap();
        let mut p = super::super::editor::Project::new("camera.mp4".into(), [80, 60], 2000, 30);
        p.segments[0].end = 1500;
        p.segments[0].rect = [0.1, 0.1, 0.7, 0.7];
        p.segments[0].frame = Some(Frame {
            shape: Kind::Ellipse,
            width: 4,
            outline: [25, 155, 220, 255],
            shadow: Some(crate::shadow::Shadow {
                color: [0, 0, 0, 140],
                angle: 90.,
                distance: 3.,
                blur: 2.,
            }),
            ..Frame::default()
        });
        let mut second = p.segments[0].clone();
        second.id = 2;
        second.at = 1000;
        second.end = 1000;
        p.segments.push(second);
        let a = Arc::new(image::RgbaImage::from_fn(80, 60, |x, y| {
            image::Rgba([(x * 3) as u8, (y * 3) as u8, 40, 255])
        }));
        let b = Arc::new(image::RgbaImage::from_fn(80, 60, |x, y| {
            image::Rgba([30, (x * 2) as u8, (y * 4) as u8, 255])
        }));
        let sources = vec![
            (1, Arc::new(Bitmap::transient(a.clone()).unwrap())),
            (2, Arc::new(Bitmap::transient(b.clone()).unwrap())),
        ];
        let style = p.segments[0].frame.as_ref().unwrap();
        let ar = style.render(a, p.segments[0].rect, [160, 90], 1).unwrap();
        let br = style.render(b, p.segments[0].rect, [160, 90], 2).unwrap();
        doc.video = Some(p.clone());
        for time in [1010, 1125, 1250, 1375, 1490] {
            assert!(texture_dissolve_sources(&p, [160, 90], time, 1, &sources));
            let wa = p.opacity(&p.segments[0], time) as f32;
            let wb = p.opacity(&p.segments[1], time) as f32;
            let mut actual = image::RgbaImage::from_pixel(160, 90, image::Rgba([60, 60, 60, 255]));
            let objects = source_objects(&doc, &sources, time, false);
            assert_eq!(objects.len(), 1);
            crate::render_annotation(&mut actual, &objects[0]);
            for ((a, b), out) in ar.pixels().zip(br.pixels()).zip(actual.pixels()) {
                let aa = a[3] as f32 / 255. * wa;
                let ba = b[3] as f32 / 255. * wb;
                for ch in 0..3 {
                    let expected = (a[ch] as f32 * aa + b[ch] as f32 * ba + 60. * (1. - aa - ba))
                        .round() as u8;
                    assert!(
                        out[ch].abs_diff(expected) <= 3,
                        "at {time}: {out:?} vs {expected}"
                    );
                }
            }
        }
        p.segments[1].frame.as_mut().unwrap().width = 5;
        assert!(!texture_dissolve_sources(&p, [160, 90], 1250, 1, &sources));
    }
    #[test]
    fn different_aspect_dissolve_preserves_margins_and_brightness() {
        use super::super::editor::{MediaKind, Project, Segment, Source};
        let mut d = crate::Document::blank(160, 90).unwrap();
        let mut p = Project::new("screen.mp4".into(), [160, 90], 2000, 30);
        p.segments[0].end = 1500;
        let mut second = Segment::new(
            2,
            1000,
            1,
            Some(Source {
                path: "second.mp4".into(),
                size: [160, 80],
                duration: 1000,
                kind: MediaKind::Video,
            }),
            1000,
        );
        second.frame = None;
        p.segments.push(second);
        let sources = vec![
            (
                1,
                Arc::new(
                    Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                        160,
                        90,
                        image::Rgba([240, 0, 0, 255]),
                    )))
                    .unwrap(),
                ),
            ),
            (
                2,
                Arc::new(
                    Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                        160,
                        80,
                        image::Rgba([0, 0, 240, 255]),
                    )))
                    .unwrap(),
                ),
            ),
        ];
        assert!(texture_dissolve_sources(&p, [160, 90], 1250, 1, &sources));
        d.video = Some(p);
        let mut output = image::RgbaImage::from_pixel(160, 90, image::Rgba([20, 20, 20, 255]));
        for a in source_objects(&d, &sources, 1250, false) {
            crate::render_annotation(&mut output, &a);
        }
        for (point, expected) in [
            ((80, 45), [120u8, 0, 120, 255]),
            ((80, 1), [130, 10, 10, 255]),
            ((80, 88), [130, 10, 10, 255]),
        ] {
            assert!(
                output
                    .get_pixel(point.0, point.1)
                    .0
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| a.abs_diff(b) <= 2),
                "{:?}",
                output.get_pixel(point.0, point.1)
            );
        }
    }
    #[test]
    fn opaque_imported_image_dissolve_uses_fast_path_but_transparency_does_not() {
        use super::super::editor::{MediaKind, Project, Source};
        let mut d = crate::Document::blank(160, 90).unwrap();
        let mut p = Project::new("screen.mp4".into(), [160, 90], 2000, 30);
        let mut next = p.segments[0].clone();
        next.id = 2;
        next.at = 500;
        next.end = 1000;
        next.source = Some(Source {
            path: "image.png".into(),
            size: [4, 3],
            duration: 1000,
            kind: MediaKind::Image,
        });
        next.frame = Some(Frame {
            radius: 0.,
            fill: true,
            ..Default::default()
        });
        p.segments.push(next);
        let bitmap = |size: [u32; 2], color| {
            Arc::new(
                Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                    size[0],
                    size[1],
                    image::Rgba(color),
                )))
                .unwrap(),
            )
        };
        let mut sources = vec![
            (1, bitmap([160, 90], [240, 0, 0, 255])),
            (2, bitmap([4, 3], [0, 0, 240, 255])),
        ];
        assert!(texture_dissolve_sources(&p, [160, 90], 750, 1, &sources));
        d.video = Some(p.clone());
        let mut pixels = image::RgbaImage::new(160, 90);
        for a in source_objects(&d, &sources, 750, false) {
            crate::render_annotation(&mut pixels, &a);
        }
        let px = pixels.get_pixel(80, 45);
        assert!((px[0] as i16 + px[2] as i16 - 240).abs() <= 1);
        sources[1].1 = bitmap([4, 3], [0, 0, 240, 100]);
        assert!(!texture_dissolve_sources(&p, [160, 90], 750, 1, &sources));
    }
    #[test]
    fn live_full_frame_crossfade_keeps_constant_luminance_and_matches_export_weights() {
        let mut d = crate::Document::blank(160, 90).unwrap();
        let mut p = super::super::editor::Project::new("screen.mp4".into(), [160, 90], 2000, 30);
        p.segments[0].end = 1500;
        let mut next = p.segments[0].clone();
        next.id = 2;
        next.at = 500;
        next.end = 1500;
        p.segments.push(next);
        d.video = Some(p);
        let sources = vec![
            (
                1,
                Arc::new(
                    Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                        160,
                        90,
                        image::Rgba([240, 0, 0, 255]),
                    )))
                    .unwrap(),
                ),
            ),
            (
                2,
                Arc::new(
                    Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                        160,
                        90,
                        image::Rgba([0, 0, 240, 255]),
                    )))
                    .unwrap(),
                ),
            ),
        ];
        for time in (500..1500).step_by(50) {
            let mut out = image::RgbaImage::new(160, 90);
            for a in source_objects(&d, &sources, time, false) {
                crate::render_annotation(&mut out, &a);
            }
            let pixel = out.get_pixel(80, 45);
            let p = d.video.as_ref().unwrap();
            let red = (p.opacity(&p.segments[0], time) * 240.).round() as i16;
            let blue = (p.opacity(&p.segments[1], time) * 240.).round() as i16;
            assert!(
                (pixel[0] as i16 - red).abs() <= 1 && (pixel[2] as i16 - blue).abs() <= 1,
                "{time}: {pixel:?}, expected {red}/{blue}"
            );
            assert!(pixel[3] >= 254);
            assert!((pixel[0] as i16 + pixel[2] as i16 - 240).abs() <= 1);
        }
    }
    #[test]
    fn video_frame_masks_crop_rotation_and_original_pixels_remain_independent() {
        let mut source = image::RgbaImage::from_pixel(100, 100, image::Rgba([250, 0, 0, 255]));
        for y in 0..100 {
            for x in 50..100 {
                source.put_pixel(x, y, image::Rgba([0, 0, 250, 255]));
            }
        }
        let source = Arc::new(source);
        let mut f = Frame {
            shape: Kind::Ellipse,
            ..Default::default()
        };
        let a = f
            .render(source.clone(), [0.25, 0.25, 0.5, 0.5], [200, 200], 1)
            .unwrap();
        assert_eq!(a.get_pixel(50, 50)[3], 0);
        assert!(a.get_pixel(100, 100)[3] > 250);
        f.view.rect = [0.5, 0., 0.5, 1.];
        f.rotation = 30.;
        let b = f
            .render(source.clone(), [0.25, 0.25, 0.5, 0.5], [200, 200], 1)
            .unwrap();
        assert!(b.get_pixel(100, 100)[2] > 240);
        assert_eq!(source.get_pixel(0, 0)[0], 250);
        f.shape = Kind::Polygon;
        f.sides = 5;
        let c = f
            .render(source, [0.1, 0.1, 0.8, 0.8], [200, 200], 1)
            .unwrap();
        assert_eq!(c.get_pixel(0, 0)[3], 0);
        assert!(c.get_pixel(100, 100)[3] > 240);
    }
    #[test]
    fn visual_order_interleaves_camera_and_annotations_and_hidden_annotations_keep_camera() {
        use super::super::{editor::*, links::Layer};
        let mut d = crate::Document::blank(320, 180).unwrap();
        d.video = Some(Project::new("screen.mp4".into(), [320, 180], 1000, 30));
        let source = Arc::new(image::RgbaImage::from_pixel(
            320,
            180,
            image::Rgba([0, 0, 240, 255]),
        ));
        let bitmap = Arc::new(Bitmap::transient(source).unwrap());
        let mut a =
            crate::steps::marker(1, [80., 80.], 44, [240, 0, 0, 255], (1, false), [320, 180])
                .unwrap();
        a.text = None;
        d.annotations.push(a);
        d.video.as_mut().unwrap().sync(&d.annotations);
        let layers = vec![(1, bitmap)];
        let front = super::composite(&d, &layers, 100).unwrap();
        assert!(front.get_pixel(80, 80)[0] > 200);
        d.video.as_mut().unwrap().move_layer(Layer::Media(1), -1);
        let back = super::composite(&d, &layers, 100).unwrap();
        assert_eq!(back.get_pixel(80, 80)[2], 240);
        assert_eq!(ordered_objects(&d, &layers, 100, true).len(), 1);
    }
    #[test]
    fn transient_video_samples_have_distinct_cache_identity() {
        let a = Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255; 4]),
        )))
        .unwrap();
        let b = Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([0; 4]),
        )))
        .unwrap();
        assert_ne!(a, b);
        assert_eq!(a, a.clone());
    }
}
