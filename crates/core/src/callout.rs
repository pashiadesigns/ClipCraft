//! Original rectangular callout geometry shared by rendering and selection.
use crate::{Annotation, Error, Result};
use image::{GrayImage, Luma, RgbaImage};
use imageproc::{
    drawing::{draw_line_segment_mut, draw_polygon_mut},
    point::Point,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalloutShape {
    #[default]
    Box,
    Oval,
    Cloud,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalloutTail {
    #[default]
    Wedge,
    Thin,
    Bubbles,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CalloutStyle {
    #[serde(default)]
    pub shape: CalloutShape,
    #[serde(default)]
    pub tail: CalloutTail,
    pub fill: [u8; 4],
    pub outline: [u8; 4],
    pub pointer: [f32; 2],
    pub padding: f32,
}

impl CalloutStyle {
    pub fn for_box(start: [f32; 2], end: [f32; 2], canvas: [u32; 2]) -> Self {
        let min = [start[0].min(end[0]), start[1].min(end[1])];
        let max = [start[0].max(end[0]), start[1].max(end[1])];
        let length = ((max[1] - min[1]) * 0.4).clamp(12.0, 35.0);
        let below = canvas[1] as f32 - max[1];
        let y = if below >= length || below >= min[1] {
            (max[1] + length).min(canvas[1] as f32)
        } else {
            (min[1] - length).max(0.0)
        };
        Self {
            shape: CalloutShape::Box,
            tail: CalloutTail::Wedge,
            fill: [255, 240, 180, 255],
            outline: [80, 65, 35, 255],
            pointer: [min[0] + (max[0] - min[0]) * 0.3, y],
            padding: 8.0,
        }
    }

    pub fn validate(
        &self,
        annotation: &Annotation,
        canvas: [u32; 2],
        overflow: bool,
    ) -> Result<()> {
        if !self.padding.is_finite() || !(0.0..=128.0).contains(&self.padding) {
            return Err(Error::Invalid(
                "Callout padding must be 0–128 pixels".into(),
            ));
        }
        if !self
            .pointer
            .iter()
            .all(|p| p.is_finite() && p.abs() <= 1_000_000.)
            || (!overflow
                && (self.pointer[0] < 0.0
                    || self.pointer[1] < 0.0
                    || self.pointer[0] > canvas[0] as f32
                    || self.pointer[1] > canvas[1] as f32))
        {
            return Err(Error::Invalid(
                "Callout pointer must be inside the image".into(),
            ));
        }
        let (min, max) = annotation.body_bounds();
        if max[0] <= min[0] || max[1] <= min[1] {
            return Err(Error::Invalid(
                "Callout body must have positive width and height".into(),
            ));
        }
        Ok(())
    }
}

impl Annotation {
    /// Clockwise body outline, with a triangular pointer on the facing side.
    pub(crate) fn callout_polygon(&self) -> Vec<[f32; 2]> {
        let (min, max) = self.body_bounds();
        let corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
        let Some(style) = &self.callout else {
            return self.rounded_polygon(corners.to_vec());
        };
        if style.shape != CalloutShape::Box {
            return round_outline(style, min, max);
        }
        let tip = style.pointer;
        if style.tail == CalloutTail::Bubbles
            || (tip[0] >= min[0] && tip[0] <= max[0] && tip[1] >= min[1] && tip[1] <= max[1])
        {
            return self.rounded_polygon(corners.to_vec());
        }
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        let dx = (tip[0] - center[0]) / (max[0] - min[0]).max(0.01);
        let dy = (tip[1] - center[1]) / (max[1] - min[1]).max(0.01);
        let edge = if dx.abs() > dy.abs() {
            if dx > 0.0 { 1 } else { 3 }
        } else if dy > 0.0 {
            2
        } else {
            0
        };
        let axis = if edge % 2 == 0 { 0 } else { 1 };
        let widest = if style.tail == CalloutTail::Thin {
            4.0
        } else {
            12.0
        };
        let half = ((max[axis] - min[axis]) * 0.18).min(widest);
        let middle = tip[axis].clamp(min[axis] + half, max[axis] - half);
        let mut first = corners[edge];
        let mut second = first;
        let direction = if edge < 2 { 1.0 } else { -1.0 };
        first[axis] = middle - half * direction;
        second[axis] = middle + half * direction;
        let mut polygon = Vec::with_capacity(7);
        for (index, point) in corners.into_iter().enumerate() {
            polygon.push(point);
            if index == edge {
                polygon.extend([first, tip, second]);
            }
        }
        self.rounded_polygon(polygon)
    }
    /// The trail of a thought bubble, as circle centers and radii.
    pub(crate) fn callout_bubbles(&self) -> Vec<([f32; 2], f32)> {
        let Some(style) = self
            .callout
            .as_ref()
            .filter(|s| s.tail == CalloutTail::Bubbles)
        else {
            return vec![];
        };
        let (min, max) = self.body_bounds();
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        let half = [
            ((max[0] - min[0]) * 0.5).max(0.01),
            ((max[1] - min[1]) * 0.5).max(0.01),
        ];
        let tip = style.pointer;
        let reach = [
            (tip[0] - center[0]) / half[0],
            (tip[1] - center[1]) / half[1],
        ];
        let out = if style.shape == CalloutShape::Box {
            reach[0].abs().max(reach[1].abs())
        } else {
            reach[0].hypot(reach[1])
        };
        if out <= 1. {
            return vec![];
        }
        let edge = [0, 1].map(|a| center[a] + (tip[a] - center[a]) / out);
        let room = (tip[0] - edge[0]).hypot(tip[1] - edge[1]);
        let scale = (room / 40.).min(1.);
        [(0.3, 9.), (0.62, 6.), (0.9, 3.5)]
            .map(|(t, r)| ([0, 1].map(|a| edge[a] + (tip[a] - edge[a]) * t), r * scale))
            .to_vec()
    }
}

/// An oval or cloud body, opened where the pointer leaves it.
fn round_outline(style: &CalloutStyle, min: [f32; 2], max: [f32; 2]) -> Vec<[f32; 2]> {
    use std::f32::consts::{FRAC_PI_2, TAU};
    const SAMPLES: usize = 132;
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    let half = [
        ((max[0] - min[0]) * 0.5).max(0.01),
        ((max[1] - min[1]) * 0.5).max(0.01),
    ];
    let at = |t: f32| {
        let radius = match style.shape {
            CalloutShape::Cloud => 0.87 + 0.13 * (t * 5.5).sin().abs(),
            _ => 1.,
        };
        [
            center[0] + t.cos() * half[0] * radius,
            center[1] + t.sin() * half[1] * radius,
        ]
    };
    let tip = style.pointer;
    let reach = [
        (tip[0] - center[0]) / half[0],
        (tip[1] - center[1]) / half[1],
    ];
    if style.tail == CalloutTail::Bubbles || reach[0].hypot(reach[1]) <= 1. {
        return (0..SAMPLES)
            .map(|i| at(-FRAC_PI_2 + TAU * i as f32 / SAMPLES as f32))
            .collect();
    }
    let toward = reach[1].atan2(reach[0]);
    let spread = if style.tail == CalloutTail::Thin {
        0.09
    } else {
        0.22
    };
    let mut ring: Vec<_> = (0..SAMPLES)
        .map(|i| at(toward + spread + (TAU - 2. * spread) * i as f32 / (SAMPLES - 1) as f32))
        .collect();
    ring.push(tip);
    ring
}

pub(crate) fn hit_test(annotation: &Annotation, point: [f32; 2], tolerance: f32) -> bool {
    hit_polygon(&annotation.callout_polygon(), point, tolerance)
        || annotation
            .callout_bubbles()
            .iter()
            .any(|(c, r)| (point[0] - c[0]).hypot(point[1] - c[1]) <= r + tolerance)
}

pub(crate) fn hit_polygon(polygon: &[[f32; 2]], point: [f32; 2], tolerance: f32) -> bool {
    let mut inside = false;
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        if crate::selection::segment_distance(point, *a, *b) <= tolerance {
            return true;
        }
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}

pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let Some(style) = &annotation.callout else {
        return;
    };
    let polygon = annotation.callout_polygon();
    let (min, max) = annotation.bounds();
    let margin = annotation.width as f32;
    let left = (min[0] - margin).floor().max(0.0) as u32;
    let top = (min[1] - margin).floor().max(0.0) as u32;
    let right = (max[0] + margin).ceil().min(image.width() as f32) as u32;
    let bottom = (max[1] + margin).ceil().min(image.height() as f32) as u32;
    if right <= left || bottom <= top {
        return;
    }
    let mut points: Vec<_> = polygon
        .iter()
        .map(|p| {
            Point::new(
                (p[0] - left as f32).round() as i32,
                (p[1] - top as f32).round() as i32,
            )
        })
        .collect();
    points.dedup();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    let mut mask = GrayImage::new(right - left, bottom - top);
    if points.len() >= 3 {
        draw_polygon_mut(&mut mask, &points, Luma([255]));
    }
    composite_mask(image, &mask, left, top, style.fill);
    mask.fill(0);
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length = dx.hypot(dy).max(1.0);
        for step in 0..annotation.width {
            let offset = step as f32 - (annotation.width as f32 - 1.0) * 0.5;
            let x = -dy / length * offset;
            let y = dx / length * offset;
            draw_line_segment_mut(
                &mut mask,
                ((a[0] - left as f32) + x, (a[1] - top as f32) + y),
                ((b[0] - left as f32) + x, (b[1] - top as f32) + y),
                Luma([255]),
            );
        }
    }
    composite_mask(image, &mask, left, top, style.outline);
    let (min, max) = annotation.body_bounds();
    let inset = style.padding + annotation.width as f32 * 0.5;
    if max[0] - min[0] <= 2.0 * inset || max[1] - min[1] <= 2.0 * inset {
        return;
    }
    crate::text::render_in_bounds(
        image,
        annotation,
        [min[0] + inset, min[1] + inset],
        [max[0] - inset, max[1] - inset],
    );
}

fn composite_mask(image: &mut RgbaImage, mask: &GrayImage, left: u32, top: u32, color: [u8; 4]) {
    if color[3] == 0 {
        return;
    }
    for (x, y, pixel) in mask.enumerate_pixels() {
        if pixel[0] != 0 {
            crate::text::composite(image.get_pixel_mut(left + x, top + y), color);
        }
    }
}
