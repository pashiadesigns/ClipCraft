//! Bounded preview sampling and integer box passes avoid per-pixel Windows
//! roundf calls. Exports continue to use the document's exact raster renderer.
use super::Viewport;
use crate::{Annotation, Kind};
use image::RgbaImage;

pub(super) fn bitmap(out: &mut RgbaImage, a: &Annotation, v: Viewport) -> bool {
    let Some(b) = &a.image else { return false };
    let Ok(src) = b.pixels() else { return false };
    if a.rotation_degrees != 0.
        || b.is_cursor()
        || b.vector().is_some()
        || !b.opaque_transient()
        || !matches!(a.kind, Kind::Image | Kind::Rectangle | Kind::Ellipse)
    {
        return false;
    }
    let (lo, hi) = a.body_bounds();
    let extent = [hi[0] - lo[0], hi[1] - lo[1]];
    if extent.iter().any(|n| *n <= 0.) {
        return true;
    }
    let view = b.view().rect;
    let dimensions = [src.width(), src.height()];
    let mapping = |n: u32, axis: usize| {
        let local =
            (v.origin[axis] + (n as f64 + 0.5) / v.scale - lo[axis] as f64) / extent[axis] as f64;
        if !(0. ..1.).contains(&local) {
            return None;
        }
        let fraction = if axis == 0 && a.mirror_x {
            1. - local
        } else {
            local
        };
        let point =
            (fraction * view[axis + 2] as f64 + view[axis] as f64) * dimensions[axis] as f64 - 0.5;
        let floor = point.floor();
        Some((
            floor.clamp(0., dimensions[axis] as f64 - 1.) as usize,
            (floor + 1.).clamp(0., dimensions[axis] as f64 - 1.) as usize,
            ((point - floor) * 1024. + 0.5) as u32,
        ))
    };
    let columns: Vec<_> = (0..out.width()).map(|n| mapping(n, 0)).collect();
    let source = src.as_raw().as_chunks::<4>().0;
    let width = out.width() as usize;
    for (y, row) in out.as_mut().chunks_exact_mut(width * 4).enumerate() {
        let Some((y0, y1, fy)) = mapping(y as u32, 1) else {
            continue;
        };
        for (x, dst) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let Some((x0, x1, fx)) = columns[x] else {
                continue;
            };
            let points = [
                source[y0 * src.width() as usize + x0],
                source[y0 * src.width() as usize + x1],
                source[y1 * src.width() as usize + x0],
                source[y1 * src.width() as usize + x1],
            ];
            let coverage = if a.corner_radius > 0. || a.kind == Kind::Ellipse {
                crate::image_frame::coverage(
                    a,
                    [
                        v.origin[0] + (x as f64 + 0.5) / v.scale - lo[0] as f64,
                        v.origin[1] + (y as f64 + 0.5) / v.scale - lo[1] as f64,
                    ],
                    extent,
                    v.scale,
                )
            } else {
                1.
            };
            let alpha = (a.image_opacity() as f64 * coverage + 0.5) as u32;
            let old = dst[3] as u32 * (255 - alpha);
            let total = alpha * 255 + old;
            if total == 0 {
                continue;
            }
            for c in 0..3 {
                let top = points[0][c] as u32 * (1024 - fx) + points[1][c] as u32 * fx;
                let bottom = points[2][c] as u32 * (1024 - fx) + points[3][c] as u32 * fx;
                let pixel = (top * (1024 - fy) + bottom * fy + (1 << 19)) >> 20;
                dst[c] = ((pixel * alpha * 255 + dst[c] as u32 * old + total / 2) / total) as u8;
            }
            dst[3] = ((total + 127) / 255) as u8;
        }
    }
    true
}

/// Three separable box passes approximate a Gaussian. Odd kernels and clamped
/// edges match image::fast_blur; integer rounding keeps each pass byte-exact.
pub(super) fn blur(source: &RgbaImage, sigma: f32) -> RgbaImage {
    let width = source.width() as usize;
    let height = source.height() as usize;
    if width == 0 || height == 0 {
        return source.clone();
    }
    let ideal = (4. * sigma * sigma + 1.).sqrt();
    let lower = ((ideal.floor() as usize).saturating_sub(1) / 2) * 2 + 1;
    let count = (0.75 * (lower as f32 + 3.) - 3. * sigma * sigma / (lower as f32 + 1.))
        .round()
        .clamp(0., 3.) as usize;
    let mut pixels = source.as_raw().clone();
    let mut scratch = vec![0u8; pixels.len()];
    for pass in 0..3 {
        let radius = if pass < count {
            lower / 2
        } else {
            lower / 2 + 1
        } | 1;
        let kernel = radius * 2 + 1;
        let reciprocal = ((1u64 << 24) + kernel as u64 / 2) / kernel as u64;
        let rounded = |sum: u32| ((sum as u64 * reciprocal + (1 << 23)) >> 24) as u8;
        for y in 0..height {
            let row = &pixels[y * width * 4..(y + 1) * width * 4];
            let mut sum = [0u32; 4];
            for dx in -(radius as isize)..=radius as isize {
                let x = dx.clamp(0, width as isize - 1) as usize;
                for c in 0..4 {
                    sum[c] += row[x * 4 + c] as u32;
                }
            }
            for x in 0..width {
                for c in 0..4 {
                    scratch[(y * width + x) * 4 + c] = rounded(sum[c]);
                    sum[c] = sum[c] + row[(x + radius + 1).min(width - 1) * 4 + c] as u32
                        - row[x.saturating_sub(radius) * 4 + c] as u32;
                }
            }
        }
        let mut sums = vec![0u32; width * 4];
        for dy in -(radius as isize)..=radius as isize {
            let y = dy.clamp(0, height as isize - 1) as usize;
            for (sum, p) in sums
                .iter_mut()
                .zip(&scratch[y * width * 4..(y + 1) * width * 4])
            {
                *sum += *p as u32;
            }
        }
        for y in 0..height {
            let enter = (y + radius + 1).min(height - 1) * width * 4;
            let leave = y.saturating_sub(radius) * width * 4;
            for (x, sum) in sums.iter_mut().enumerate() {
                pixels[y * width * 4 + x] = rounded(*sum);
                *sum = *sum + scratch[enter + x] as u32 - scratch[leave + x] as u32;
            }
        }
    }
    RgbaImage::from_raw(source.width(), source.height(), pixels).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_sampler_preserves_crop_mask_opacity_and_layer_order() {
        let pixels = std::sync::Arc::new(RgbaImage::from_fn(180, 100, |x, y| {
            image::Rgba([(x * 7 % 256) as u8, (y * 11 % 256) as u8, 80, 255])
        }));
        for kind in [Kind::Image, Kind::Rectangle, Kind::Ellipse] {
            for mirror in [false, true] {
                for opacity in [120, 255] {
                    let source = crate::bitmap::Bitmap::transient(pixels.clone())
                        .unwrap()
                        .with_view(Some(crate::image_frame::ImageView {
                            rect: [0.1, 0.05, 0.8, 0.9],
                        }))
                        .unwrap()
                        .with_frame_opacity(opacity);
                    let mut a = std::sync::Arc::new(source).annotation(1);
                    a.kind = kind;
                    a.start = [15.3, 12.7];
                    a.end = [160.3, 97.7];
                    a.mirror_x = mirror;
                    a.width = 0;
                    a.corner_radius = if kind == Kind::Rectangle { 12. } else { 0. };
                    if kind == Kind::Image {
                        a.color[3] = opacity;
                    }
                    let v = Viewport {
                        size: [100, 65],
                        origin: [0., 0.],
                        scale: 0.55,
                    };
                    let mut fast = RgbaImage::from_pixel(100, 65, image::Rgba([20, 40, 80, 180]));
                    let mut exact = fast.clone();
                    assert!(bitmap(&mut fast, &a, v));
                    crate::bitmap::render(&mut exact, &a, v);
                    let worst = fast
                        .as_raw()
                        .iter()
                        .zip(exact.as_raw())
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    assert!(
                        worst <= 2,
                        "{kind:?} mirror={mirror} opacity={opacity} difference={worst}"
                    );
                }
            }
        }
    }
    #[test]
    fn integer_focus_blur_matches_existing_kernel() {
        for size in [[3, 2], [80, 50], [160, 90]] {
            for sigma in [0.5, 2., 3.75, 8., 32.] {
                let source = RgbaImage::from_fn(size[0], size[1], |x, y| {
                    image::Rgba([
                        (x * 37 % 256) as u8,
                        (y * 53 % 256) as u8,
                        ((x + y) * 97 % 256) as u8,
                        ((x * y + 75) % 256) as u8,
                    ])
                });
                assert_eq!(
                    blur(&source, sigma),
                    image::imageops::fast_blur(&source, sigma),
                    "size {size:?}, sigma {sigma}"
                );
            }
        }
    }
}
