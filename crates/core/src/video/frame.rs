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
    /// Filters and color for this clip.
    #[serde(default)]
    pub color: crate::picture_finish::ColorAdjust,
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
            color: Default::default(),
        }
    }
}
impl Frame {
    /// Takes only what changed between `before` and `after` on another frame.
    pub fn copy_change(&mut self, before: &Frame, after: &Frame) {
        macro_rules! copy {
            ($($field:ident),+) => {$(
                if after.$field != before.$field {
                    self.$field = after.$field.clone();
                }
            )+};
        }
        copy!(
            shape, sides, radius, view, fill, rotation, mirror, flip_y, outline, width, opacity,
            shadow, line_style, color
        );
    }
    pub fn unmodified_pixels(&self) -> bool {
        self.shape == Kind::Rectangle
            && self.color.is_neutral()
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
        if let Some((plain, colored)) = self.color_first(&source) {
            return plain.render(colored, rect, size, id);
        }
        match self.plain_fit(&source, rect, size) {
            Some(rendered) => Ok(rendered),
            None => self.render_general(source, rect, size, id),
        }
    }
    /// With a color change: this frame without it, and the recolored source.
    pub(super) fn color_first(
        &self,
        source: &image::RgbaImage,
    ) -> Option<(Frame, Arc<image::RgbaImage>)> {
        if self.color.is_neutral() {
            return None;
        }
        let mut colored = source.clone();
        self.color.apply(&mut colored);
        let plain = Frame {
            color: Default::default(),
            ..self.clone()
        };
        Some((plain, Arc::new(colored)))
    }
    /// A frame held still through an export: a plain picture lands on whole
    /// pixels, a shift of under half a pixel that a moving frame would show.
    pub(super) fn plain_snapped(
        &self,
        source: &image::RgbaImage,
        rect: [f32; 4],
        size: [u32; 2],
    ) -> Option<image::RgbaImage> {
        let left = (rect[0] * size[0] as f32).round();
        let top = (rect[1] * size[1] as f32).round();
        let right = ((rect[0] + rect[2]) * size[0] as f32).round();
        let bottom = ((rect[1] + rect[3]) * size[1] as f32).round();
        self.plain_box(source, [left, top, right - left, bottom - top], size)
    }
    /// The picture for Fit: the view shrunk to fit the frame, centered on a
    /// transparent image of the frame's size. Returns that image and where the
    /// picture sits in it, as x, y, width, height.
    pub(super) fn padded_fit(
        &self,
        source: &image::RgbaImage,
        rect: [f32; 4],
        size: [u32; 2],
    ) -> (image::RgbaImage, [u32; 4]) {
        let w = (rect[2] * size[0] as f32).round().max(1.) as u32;
        let h = (rect[3] * size[1] as f32).round().max(1.) as u32;
        let [sx, sy, sw, sh] = self.view.rect;
        let crop = image::imageops::crop_imm(
            source,
            (sx * source.width() as f32) as u32,
            (sy * source.height() as f32) as u32,
            (sw * source.width() as f32).floor().max(1.) as u32,
            (sh * source.height() as f32).floor().max(1.) as u32,
        )
        .to_image();
        let (fit_w, fit_h) = contained(crop.dimensions(), (w, h));
        let fit = fit_pixels(&crop, fit_w, fit_h);
        let mut padded = image::RgbaImage::new(w, h);
        let offset = [(w - fit_w) / 2, (h - fit_h) / 2];
        image::imageops::overlay(&mut padded, &fit, offset[0] as i64, offset[1] as i64);
        (padded, [offset[0], offset[1], fit_w, fit_h])
    }
    /// A plain rectangle on whole pixels is only a resize and a copy, the same
    /// pixels the shape renderer gives. Export reaches this for every frame of a
    /// clip that does not match the canvas size.
    fn plain_fit(
        &self,
        source: &image::RgbaImage,
        rect: [f32; 4],
        size: [u32; 2],
    ) -> Option<image::RgbaImage> {
        self.plain_box(
            source,
            [
                rect[0] * size[0] as f32,
                rect[1] * size[1] as f32,
                rect[2] * size[0] as f32,
                rect[3] * size[1] as f32,
            ],
            size,
        )
    }
    fn plain_box(
        &self,
        source: &image::RgbaImage,
        [x, y, w, h]: [f32; 4],
        size: [u32; 2],
    ) -> Option<image::RgbaImage> {
        if self.fill || self.line_style.is_some() || !self.unmodified_pixels() {
            return None;
        }
        if [x, y, w, h].iter().any(|n| n.fract() != 0. || *n < 0.)
            || w < 1.
            || h < 1.
            || x + w > size[0] as f32
            || y + h > size[1] as f32
        {
            return None;
        }
        let (fit_w, fit_h) = contained(source.dimensions(), (w as u32, h as u32));
        let fit = fit_pixels(source, fit_w, fit_h);
        if fit.as_raw().as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
            return None;
        }
        let mut output = image::RgbaImage::new(size[0], size[1]);
        let stride = size[0] as usize * 4;
        let start = (y as usize + (h as usize - fit_h as usize) / 2) * stride
            + (x as usize + (w as usize - fit_w as usize) / 2) * 4;
        let width = fit_w as usize * 4;
        for (index, row) in fit.as_raw().chunks_exact(width).enumerate() {
            output.as_mut()[start + index * stride..][..width].copy_from_slice(row);
        }
        Some(output)
    }
    fn render_general(
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
            pixels = Arc::new(self.padded_fit(&pixels, rect, size).0);
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

/// `image::imageops::thumbnail`, spread across the CPU's cores. Video export runs
/// it for every frame, so it gives the same values as that function.
/// The largest size inside `target` with the shape of `source`. Within a pixel
/// of `target` it is `target`, so a rounding difference never leaves a line.
fn contained(source: (u32, u32), target: (u32, u32)) -> (u32, u32) {
    let scale =
        (target.0 as f64 / source.0.max(1) as f64).min(target.1 as f64 / source.1.max(1) as f64);
    let width = ((source.0 as f64 * scale).round() as u32).clamp(1, target.0);
    let height = ((source.1 as f64 * scale).round() as u32).clamp(1, target.1);
    if target.0.abs_diff(width) <= 1 && target.1.abs_diff(height) <= 1 {
        target
    } else {
        (width, height)
    }
}
fn fit_pixels(source: &image::RgbaImage, new_width: u32, new_height: u32) -> image::RgbaImage {
    let (width, height) = source.dimensions();
    let stride = new_width as usize * 4;
    let mut out = vec![0; stride * new_height as usize];
    if width > 0 && height > 0 && stride > 0 && new_height > 0 {
        let ratio = [
            width as f32 / new_width as f32,
            height as f32 / new_height as f32,
        ];
        let columns: Vec<_> = (0..new_width)
            .map(|x| Span::new(x, ratio[0], width))
            .collect();
        let threads = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(8);
        let band = (new_height as usize).div_ceil(threads);
        let columns = &columns;
        std::thread::scope(|scope| {
            for (index, rows) in out.chunks_mut(stride * band).enumerate() {
                scope.spawn(move || {
                    for (offset, row) in rows.chunks_exact_mut(stride).enumerate() {
                        let y = (index * band + offset) as u32;
                        fit_row(source, Span::new(y, ratio[1], height), columns, row);
                    }
                });
            }
        });
    }
    image::RgbaImage::from_raw(new_width, new_height, out).expect("sized for the output")
}

/// The source pixels that one output row or column covers.
#[derive(Clone, Copy)]
struct Span {
    near: f32,
    far: f32,
    start: u32,
    end: u32,
}
impl Span {
    fn new(index: u32, ratio: f32, length: u32) -> Self {
        let near = index as f32 * ratio;
        let far = near + ratio;
        let start = (near.ceil() as u32).min(length - 1);
        let end = (far.ceil() as u32).clamp(start, length);
        Self {
            near,
            far,
            start,
            end,
        }
    }
    fn is_empty(self) -> bool {
        self.start == self.end
    }
    fn fraction(self) -> f32 {
        (self.near.fract() + self.far.fract()) / 2.
    }
}

fn fit_row(source: &image::RgbaImage, row: Span, columns: &[Span], out: &mut [u8]) {
    let width = source.width() as usize;
    let raw = source.as_raw();
    let at = |x: u32, y: u32| -> [u32; 4] {
        let i = (y as usize * width + x as usize) * 4;
        [
            raw[i] as u32,
            raw[i + 1] as u32,
            raw[i + 2] as u32,
            raw[i + 3] as u32,
        ]
    };
    let add = |sum: &mut [u32; 4], p: [u32; 4]| {
        for c in 0..4 {
            sum[c] += p[c];
        }
    };
    let (bottom, top) = (row.start, row.end);
    for (column, target) in columns.iter().zip(out.as_chunks_mut::<4>().0) {
        let (left, right) = (column.start, column.end);
        *target = match (row.is_empty(), column.is_empty()) {
            (false, false) => {
                let mut sum = [0; 4];
                for y in bottom..top {
                    for x in left..right {
                        add(&mut sum, at(x, y));
                    }
                }
                let n = (right - left) * (top - bottom);
                sum.map(|v| ((v + n / 2) / n).min(255) as u8)
            }
            (false, true) => {
                let (mut near, mut far) = ([0; 4], [0; 4]);
                for y in bottom..top {
                    add(&mut near, at(right - 1, y));
                    add(&mut far, at(right, y));
                }
                mix_pair(near, far, column.fraction(), top - bottom)
            }
            (true, false) => {
                let (mut near, mut far) = ([0; 4], [0; 4]);
                for x in left..right {
                    add(&mut near, at(x, top - 1));
                    add(&mut far, at(x, top));
                }
                mix_pair(near, far, row.fraction(), right - left)
            }
            (true, true) => {
                let (v, h) = (row.fraction(), column.fraction());
                let (bl, tl) = (at(right - 1, top - 1), at(right - 1, top));
                let (br, tr) = (at(right, top - 1), at(right, top));
                std::array::from_fn(|c| {
                    ((1. - v) * h * br[c] as f32
                        + v * h * tr[c] as f32
                        + (1. - v) * (1. - h) * bl[c] as f32
                        + v * (1. - h) * tl[c] as f32) as u8
                })
            }
        };
    }
}

fn mix_pair(near: [u32; 4], far: [u32; 4], fraction: f32, count: u32) -> [u8; 4] {
    let (far_part, near_part) = (fraction / count as f32, (1. - fraction) / count as f32);
    std::array::from_fn(|c| (near_part * near[c] as f32 + far_part * far[c] as f32) as u8)
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
    let sampled = p.sampled_media(time);
    let p = sampled.as_ref();
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
            super::links::Layer::Zoom(track) => {
                // A shared zoom track applies whichever of its zooms is active.
                if let Some(z) = p
                    .zooms
                    .iter()
                    .find(|z| z.track() == track && z.active(time))
                    .or_else(|| p.zooms.iter().find(|z| z.track() == track))
                {
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
    let objects = ordered_objects(doc, layers, time, false);
    // Focus effects on screen together share one backdrop (BJ).
    let backdrop = crate::focus::Backdrop::new(&objects);
    for a in &objects {
        if !backdrop.render(&mut output, a) {
            crate::render_annotation(&mut output, a);
        }
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
        let objects = ordered_objects(doc, layers, time, false);
        let backdrop = crate::focus::Backdrop::new(&objects);
        for a in objects {
            if backdrop.render(&mut out, &a) {
                continue;
            }
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
    let sampled = p.sampled_media(time);
    let p = sampled.as_ref();
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
            super::links::Layer::Zoom(track) => {
                // A shared zoom track applies whichever of its zooms is active.
                if let Some(z) = p
                    .zooms
                    .iter()
                    .find(|z| z.track() == track && z.active(time))
                    .or_else(|| p.zooms.iter().find(|z| z.track() == track))
                {
                    z.transform(&mut out, [doc.width, doc.height], time);
                }
            }
            super::links::Layer::Media(track) => {
                // A clip whose picture hasn't arrived from the decoder yet is
                // left out of the weights, so the picture already showing
                // holds instead of fading toward the background and jumping
                // back when the new one lands (BW15).
                let has_source = |id: u64| sources.iter().any(|(s, _)| *s == id);
                let held;
                let p =
                    if p.segments.iter().any(|c| {
                        c.track == track && c.active(time) && !c.hidden && !has_source(c.id)
                    }) {
                        let mut only = p.clone();
                        only.segments.retain(|c| {
                            c.track != track || !c.active(time) || c.hidden || has_source(c.id)
                        });
                        held = only;
                        &held
                    } else {
                        p
                    };
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
    fn the_faster_resize_gives_the_same_pixels_as_the_image_crate() {
        for (from, to) in [
            ((64, 40), (64, 40)),
            ((96, 51), (96, 54)),
            ((96, 54), (96, 51)),
            ((50, 37), (113, 91)),
            ((113, 91), (50, 37)),
            ((80, 60), (33, 120)),
            ((33, 120), (80, 60)),
            ((7, 5), (64, 64)),
        ] {
            let source = noisy(from.0, from.1, 255);
            assert_eq!(
                super::fit_pixels(&source, to.0, to.1).as_raw(),
                image::imageops::thumbnail(&source, to.0, to.1).as_raw(),
                "{from:?} to {to:?}"
            );
            let clear = noisy(from.0, from.1, 0);
            assert_eq!(
                super::fit_pixels(&clear, to.0, to.1).as_raw(),
                image::imageops::thumbnail(&clear, to.0, to.1).as_raw()
            );
        }
    }

    #[test]
    fn a_plain_frame_renders_the_same_pixels_on_the_quick_path() {
        let plain = Frame {
            radius: 0.,
            fill: false,
            ..Default::default()
        };
        for (source, rect, size) in [
            ((192, 102), [0., 0., 1., 1.], [192, 108]),
            ((192, 108), [0., 0., 0.5, 0.5], [192, 108]),
            ((100, 75), [0.25, 0.5, 0.5, 0.5], [200, 100]),
            ((100, 75), [0.5, 0., 0.5, 1.], [64, 64]),
        ] {
            let image = std::sync::Arc::new(noisy(source.0, source.1, 255));
            let quick = plain.plain_fit(&image, rect, size).expect("quick path");
            let general = plain.render_general(image, rect, size, 9).unwrap();
            assert_eq!(quick.as_raw(), general.as_raw(), "{source:?} {rect:?}");
        }
        let image = noisy(32, 32, 255);
        for other in [
            Frame {
                fill: true,
                ..plain.clone()
            },
            Frame {
                radius: 4.,
                ..plain.clone()
            },
            Frame {
                width: 2,
                ..plain.clone()
            },
            Frame {
                opacity: 128,
                ..plain.clone()
            },
        ] {
            assert!(
                other
                    .plain_fit(&image, [0., 0., 1., 1.], [64, 64])
                    .is_none()
            );
        }
        assert!(
            plain
                .plain_fit(&image, [0.1, 0., 0.5, 0.5], [64, 64])
                .is_none(),
            "a fractional position uses the shape renderer"
        );
        assert!(
            plain
                .plain_fit(&noisy(32, 32, 200), [0., 0., 1., 1.], [64, 64])
                .is_none(),
            "transparent pictures use the shape renderer"
        );
    }

    #[test]
    fn fit_keeps_the_shape_of_a_clip_that_differs_from_its_frame() {
        let plain = Frame {
            radius: 0.,
            fill: false,
            ..Default::default()
        };
        let image = noisy(192, 102, 255);
        let out = plain
            .plain_fit(&image, [0., 0., 1., 1.], [192, 108])
            .expect("quick path");
        assert_eq!(out.get_pixel(5, 1)[3], 0, "a band above the picture");
        assert_eq!(out.get_pixel(5, 106)[3], 0, "a band below the picture");
        assert_eq!(out.get_pixel(5, 3)[3], 255);
        assert_eq!(super::contained((192, 107), (192, 108)), (192, 108));
    }
    fn noisy(width: u32, height: u32, alpha: u8) -> image::RgbaImage {
        let mut state = 7u32;
        image::RgbaImage::from_fn(width, height, |_, _| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let [a, b, c, d] = state.to_le_bytes();
            image::Rgba([b, c, d, if alpha == 0 { a } else { alpha }])
        })
    }
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
    /// BW15: PDF pages added to a video are picture clips of different page
    /// sizes, fitted, overlapping 600 ms. Steps through the crossfade as the
    /// live canvas draws it and prints the middle pixel.
    #[test]
    fn pdf_page_clips_crossfade_steadily() {
        use super::super::editor::{MediaKind, Source};
        let mut d = crate::Document::blank(1280, 720).unwrap();
        let mut p = super::super::editor::Project::new("bg.png".into(), [1280, 720], 9000, 30);
        let page = |id: u64, at: u64, size: [u32; 2]| {
            let mut c = p.segments[0].clone();
            c.id = id;
            c.track = 7;
            c.at = at;
            c.start = 0;
            c.end = 4000;
            c.source = Some(Source {
                path: format!("page-{id}.png"),
                size,
                duration: 3_600_000,
                kind: MediaKind::Image,
            });
            c.frame = Some(Frame {
                fill: false,
                radius: 0.,
                ..Default::default()
            });
            c
        };
        let first = page(11, 0, [1190, 1684]);
        let second = page(12, 3400, [1224, 1584]);
        p.media_tracks.insert(0, 7);
        p.segments.push(first);
        p.segments.push(second);
        p.layer_order = vec![];
        p.layer_order = p.layers();
        d.video = Some(p);
        let picture = |size: [u32; 2], color: [u8; 4]| {
            Arc::new(
                Bitmap::transient(Arc::new(image::RgbaImage::from_pixel(
                    size[0],
                    size[1],
                    image::Rgba(color),
                )))
                .unwrap(),
            )
        };
        let sources = vec![
            (11, picture([1190, 1684], [240, 0, 0, 255])),
            (12, picture([1224, 1584], [0, 0, 240, 255])),
        ];
        let mut trace = vec![];
        for time in (3300..=4100).step_by(25) {
            let mut out = image::RgbaImage::from_pixel(1280, 720, image::Rgba([0, 0, 0, 255]));
            for a in source_objects(&d, &sources, time, false) {
                crate::render_annotation(&mut out, &a);
            }
            let px = out.get_pixel(640, 360);
            trace.push((time, px[0], px[2]));
        }
        // Plain page frames dissolve on the GPU: no dip in the middle.
        assert!(texture_dissolve_sources(
            d.video.as_ref().unwrap(),
            [1280, 720],
            3700,
            7,
            &sources
        ));
        for w in trace.windows(2) {
            let ((t0, r0, b0), (t1, r1, b1)) = (w[0], w[1]);
            assert!(
                r1 <= r0 + 1 && b1 + 1 >= b0,
                "{t0}->{t1}: red {r0}->{r1}, blue {b0}->{b1}"
            );
        }
        for (time, r, b) in &trace {
            assert!(
                (i16::from(*r) + i16::from(*b) - 240).abs() <= 2,
                "{time}: {r}+{b}"
            );
        }
        // The next page's picture still decoding: the first page holds
        // instead of fading toward the background.
        let only_first = &sources[..1];
        for time in [3450, 3600, 3900] {
            let mut out = image::RgbaImage::from_pixel(1280, 720, image::Rgba([0, 0, 0, 255]));
            for a in source_objects(&d, only_first, time, false) {
                crate::render_annotation(&mut out, &a);
            }
            assert_eq!(out.get_pixel(640, 360)[0], 240, "{time}");
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
