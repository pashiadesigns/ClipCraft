//! Shared stroke coverage with round joins and a one-pixel smooth edge.
//! All segments share one coverage mask so opacity is applied only once.
use crate::{Annotation, Kind, text};
use image::{GrayImage, Luma, RgbaImage};

fn point(p: [f32; 2]) -> [f64; 2] {
    // Stable subpixel rendering grid. Document geometry remains untouched.
    p.map(|v| (f64::from(v) * 1024.0).round() / 1024.0)
}
fn distance_squared(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 0.0 {
        (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * d[0]).powi(2) + (p[1] - a[1] - t * d[1]).powi(2)
}
fn segment(
    mask: &mut GrayImage,
    a: [f64; 2],
    b: [f64; 2],
    radius: f64,
    feather: f64,
    origin: [u32; 2],
) {
    let extent = radius + 0.5;
    let top = (a[1].min(b[1]) - extent).floor().max(f64::from(origin[1])) as u32;
    let bottom = (a[1].max(b[1]) + extent)
        .ceil()
        .min(f64::from(origin[1] + mask.height())) as u32;
    for y in top..bottom {
        let dy = b[1] - a[1];
        let (x0, x1) = if dy.abs() > f64::EPSILON {
            let t0 = ((f64::from(y) - extent - a[1]) / dy).clamp(0.0, 1.0);
            let t1 = ((f64::from(y) + extent - a[1]) / dy).clamp(0.0, 1.0);
            (a[0] + t0 * (b[0] - a[0]), a[0] + t1 * (b[0] - a[0]))
        } else {
            (a[0], b[0])
        };
        let left = (x0.min(x1) - extent).floor().max(f64::from(origin[0])) as u32;
        let right = (x0.max(x1) + extent)
            .ceil()
            .min(f64::from(origin[0] + mask.width())) as u32;
        for x in left..right {
            let distance = distance_squared([f64::from(x), f64::from(y)], a, b).sqrt();
            let coverage = (((extent - distance) / feather).clamp(0.0, 1.0) * 255.0).round() as u8;
            let pixel = mask.get_pixel_mut(x - origin[0], y - origin[1]);
            pixel[0] = pixel[0].max(coverage);
        }
    }
}
pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    if annotation.color[3] == 0 {
        return;
    }
    // Compute head shape relative to the start to retain translation stability.
    let offset = annotation.start.map(f32::floor);
    let mut local = annotation.clone();
    for (axis, amount) in offset.into_iter().enumerate() {
        local.start[axis] -= amount;
        local.end[axis] -= amount;
    }
    let mut segments = if matches!(
        annotation.kind,
        Kind::Pen | Kind::Highlighter | Kind::Eraser
    ) {
        annotation
            .points
            .windows(2)
            .map(|pair| (point(pair[0]), point(pair[1])))
            .collect::<Vec<_>>()
    } else {
        vec![(point(annotation.start), point(annotation.end))]
    };
    if segments.is_empty() {
        return;
    }
    if annotation.kind == Kind::Arrow
        && let Some(wings) = local.arrow_head()
    {
        for wing in wings {
            segments.push((
                point(annotation.end),
                point([wing[0] + offset[0], wing[1] + offset[1]]),
            ));
        }
    }
    let radius = f64::from(annotation.width) / 2.0;
    let mut min = segments[0].0;
    let mut max = min;
    for &(a, b) in &segments {
        for p in [a, b] {
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
    }
    let left = (min[0] - radius - 1.0)
        .floor()
        .clamp(0.0, f64::from(image.width())) as u32;
    let top = (min[1] - radius - 1.0)
        .floor()
        .clamp(0.0, f64::from(image.height())) as u32;
    let right = (max[0] + radius + 1.0)
        .ceil()
        .clamp(f64::from(left), f64::from(image.width())) as u32;
    let bottom = (max[1] + radius + 1.0)
        .ceil()
        .clamp(f64::from(top), f64::from(image.height())) as u32;
    let mut mask = GrayImage::new(right - left, bottom - top);
    // A soft eraser fades over most of its radius; Hard edge cuts cleanly.
    let feather = if annotation.kind == Kind::Eraser && !annotation.hard_edge {
        (radius * 0.6).max(1.0)
    } else {
        1.0
    };
    for (a, b) in segments {
        segment(&mut mask, a, b, radius, feather, [left, top]);
    }
    for (x, y, Luma([coverage])) in mask.enumerate_pixels() {
        if *coverage == 0 {
            continue;
        }
        let coverage = if annotation.kind == Kind::Eraser && annotation.hard_edge {
            if *coverage < 128 {
                continue;
            }
            &255
        } else {
            coverage
        };
        let mut color = annotation.color;
        color[3] = ((u16::from(color[3]) * u16::from(*coverage) + 127) / 255) as u8;
        let pixel = image.get_pixel_mut(left + x, top + y);
        if annotation.kind == Kind::Eraser {
            // Destination-out: opacity is applied once per gesture, including
            // self-crossings. RGB stays straight-alpha until fully transparent.
            pixel[3] = ((u16::from(pixel[3]) * (255 - u16::from(color[3])) + 127) / 255) as u8;
            if pixel[3] == 0 {
                *pixel = image::Rgba([0; 4]);
            }
        } else {
            text::composite(pixel, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Document, Kind};
    fn arrow(start: [f32; 2], end: [f32; 2], width: u32) -> Annotation {
        Annotation {
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
            kind: Kind::Arrow,
            start,
            end,
            color: [30, 120, 190, 128],
            width,
            text: None,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: None,
        }
    }
    #[test]
    fn shaft_coverage_has_no_seams_in_every_direction_and_at_maximum_width() {
        for end in [
            [110.0, 70.0],
            [70.0, 110.0],
            [15.0, 70.0],
            [70.0, 15.0],
            [110.0, 40.0],
            [40.0, 110.0],
            [15.0, 15.0],
            [110.0, 110.0],
        ] {
            for width in [3, 8, 32] {
                let a = arrow([60.0, 60.0], end, width);
                let mut image = RgbaImage::new(130, 130);
                render(&mut image, &a);
                for step in 0..=10 {
                    let t = step as f64 / 20.0;
                    let p = [
                        60.0 + (f64::from(end[0]) - 60.0) * t,
                        60.0 + (f64::from(end[1]) - 60.0) * t,
                    ];
                    for y in p[1].floor() as u32 - 1..=p[1].ceil() as u32 + 1 {
                        for x in p[0].floor() as u32 - 1..=p[0].ceil() as u32 + 1 {
                            if distance_squared(
                                [f64::from(x), f64::from(y)],
                                [60.0, 60.0],
                                end.map(f64::from),
                            )
                            .sqrt()
                                <= f64::from(width) / 2.0 - 0.5
                            {
                                assert_eq!(
                                    image.get_pixel(x, y).0,
                                    [30, 120, 190, 128],
                                    "missing or double-painted shaft pixel"
                                );
                            }
                        }
                    }
                }
                assert!(image.pixels().all(|p| p[3] <= 128));
                assert!(image.pixels().any(|p| p[3] > 0 && p[3] < 128));
            }
        }
    }
    #[test]
    fn horizontal_width_profile_smooths_edges_without_darkening_joins() {
        let mut image = RgbaImage::new(120, 80);
        let a = arrow([10.0, 30.0], [100.0, 30.0], 8);
        render(&mut image, &a);
        for y in 0..80 {
            let expected = match y {
                26 | 34 => 64,
                27..=33 => 128,
                _ => 0,
            };
            assert_eq!(image.get_pixel(40, y)[3], expected);
        }
        assert_eq!(image.get_pixel(100, 30)[3], 128);
    }
    #[test]
    fn clipping_and_integer_translation_preserve_exact_coverage_and_project_geometry() {
        let mut doc = Document::canvas(130, 110, [0, 0, 0, 0]).unwrap();
        doc.annotations.push(arrow([10.5, 10.5], [95.5, 80.5], 11));
        let before = doc.annotations.clone();
        let original = doc.render().unwrap();
        let mut moved = doc.clone();
        let a = &mut moved.annotations[0];
        for p in [&mut a.start, &mut a.end] {
            p[0] += 10.0;
            p[1] += 10.0;
        }
        let moved = moved.render().unwrap();
        for y in 0..100 {
            for x in 0..120 {
                assert_eq!(original.get_pixel(x, y), moved.get_pixel(x + 10, y + 10));
            }
        }
        assert_eq!(doc.annotations, before);
        let mut edge = RgbaImage::new(10, 10);
        render(&mut edge, &arrow([0.0, 0.0], [10.0, 10.0], 32));
        assert!(edge.pixels().all(|p| p[3] == 128));
    }
}
