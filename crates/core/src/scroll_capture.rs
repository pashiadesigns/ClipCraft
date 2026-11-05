//! Bounded vertical overlap stitching. Uncertain frames never mutate the image.
//! Select only scrolling content; stationary browser chrome belongs outside it.
use crate::{Error, Result};
use image::RgbaImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Append {
    Unchanged,
    Added(u32),
    NoOverlap,
}
pub struct Stitcher {
    image: std::sync::Arc<RgbaImage>,
    last: RgbaImage,
    frames: usize,
    top: i64,
    view: i64,
    history: Vec<Checkpoint>,
    pending: Option<Checkpoint>,
}
#[derive(Clone, Copy)]
struct Checkpoint {
    top: i64,
    height: u32,
    view: i64,
    frames: usize,
}
impl Stitcher {
    pub fn new(first: RgbaImage) -> Result<Self> {
        crate::validate_dimensions(first.width(), first.height())?;
        if first.width() < 64 || first.height() < 64 {
            return Err(Error::Invalid(
                "Select a scrolling area at least 64 × 64 pixels.".into(),
            ));
        }
        Ok(Self {
            image: std::sync::Arc::new(first.clone()),
            last: first,
            frames: 1,
            top: 0,
            view: 0,
            history: Vec::new(),
            pending: None,
        })
    }
    pub fn image(&self) -> &RgbaImage {
        &self.image
    }
    pub fn shared_image(&self) -> std::sync::Arc<RgbaImage> {
        self.image.clone()
    }
    pub fn frames(&self) -> usize {
        self.frames
    }
    /// A burst stores geometry, not another full screenshot. Accepted pixels
    /// remain immutable; Undo crops back to the exact previous range.
    pub fn end_burst(&mut self) {
        if let Some(before) = self.pending.take()
            && (before.top != self.top || before.height != self.image.height())
        {
            self.history.push(before);
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty() || self.pending.is_some()
    }
    pub fn undo(&mut self) -> bool {
        self.end_burst();
        let Some(before) = self.history.pop() else {
            return false;
        };
        self.image = std::sync::Arc::new(
            image::imageops::crop_imm(
                self.image.as_ref(),
                0,
                (before.top - self.top) as u32,
                self.image.width(),
                before.height,
            )
            .to_image(),
        );
        self.last = image::imageops::crop_imm(
            self.image.as_ref(),
            0,
            (before.view - before.top) as u32,
            self.last.width(),
            self.last.height(),
        )
        .to_image();
        self.top = before.top;
        self.view = before.view;
        self.frames = before.frames;
        true
    }
    pub fn append(&mut self, next: &RgbaImage) -> Result<Append> {
        if next.dimensions() != self.last.dimensions() {
            return Err(Error::Invalid(
                "The capture area changed size. Finish this capture and start another.".into(),
            ));
        }
        if difference(&self.last, next, 0) < 0.7 {
            return Ok(Append::Unchanged);
        }
        let h = next.height();
        let forward = motion_anchors(&self.last, next);
        let backward = motion_anchors(next, &self.last);
        let mut candidates = Vec::new();
        // At least a quarter of the viewport must remain visible after scrolling.
        for shift in 1..=h * 3 / 4 {
            candidates.push((
                anchor_difference(&self.last, next, shift, &forward),
                shift as i64,
            ));
            candidates.push((
                anchor_difference(next, &self.last, shift, &backward),
                -(shift as i64),
            ));
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (error, shift) = candidates[0];
        let ambiguous = candidates
            .iter()
            .skip(1)
            .any(|(e, s)| s.abs_diff(shift) > 3 && *e < error + 0.7);
        if error > 5. || ambiguous {
            return Ok(Append::NoOverlap);
        }
        let view = self.view + shift;
        let top = self.top.min(view);
        let bottom = (self.top + self.image.height() as i64).max(view + h as i64);
        let new_height = (bottom - top) as u32;
        // Retracing captured content is navigation, not a new capture or Undo.
        if top == self.top && new_height == self.image.height() {
            let known = image::imageops::crop_imm(
                self.image.as_ref(),
                0,
                (view - top) as u32,
                next.width(),
                h,
            )
            .to_image();
            if !settled(&known, next) {
                return Ok(Append::NoOverlap);
            }
            self.last = next.clone();
            self.view = view;
            return Ok(Append::Unchanged);
        }
        crate::validate_dimensions(next.width(),new_height).map_err(|_|Error::Invalid("Capture reached the image size limit. Finish it, then capture the next section separately.".into()))?;
        if self.frames >= 120 {
            return Err(Error::Invalid(
                "Capture reached 120 frames. Finish it, then start another.".into(),
            ));
        }
        self.pending.get_or_insert(Checkpoint {
            top: self.top,
            height: self.image.height(),
            view: self.view,
            frames: self.frames,
        });
        let added = new_height - self.image.height();
        let mut merged = RgbaImage::new(next.width(), new_height);
        image::imageops::replace(&mut merged, next, 0, view - top);
        image::imageops::replace(&mut merged, self.image.as_ref(), 0, self.top - top);
        self.image = std::sync::Arc::new(merged);
        self.last = next.clone();
        self.frames += 1;
        self.top = top;
        self.view = view;
        Ok(Append::Added(added))
    }
}
// Fixed informative points make every candidate comparable. A uniform sparse
// grid mostly samples white margins on text documents, and a different grid per
// shift can discard the correct displacement before refinement.
fn motion_anchors(previous: &RgbaImage, next: &RgbaImage) -> Vec<(u32, u32)> {
    let mut points = Vec::new();
    let stride = 1;
    for y in (1..next.height() - 1).step_by(2) {
        for x in (1..next.width() - 1).step_by(stride as usize) {
            let pixel = next.get_pixel(x, y);
            let changed = (0..3).any(|c| previous.get_pixel(x, y)[c].abs_diff(pixel[c]) > 8);
            let edge = (0..3).any(|c| {
                next.get_pixel(x - 1, y)[c].abs_diff(pixel[c]) > 12
                    || next.get_pixel(x, y - 1)[c].abs_diff(pixel[c]) > 12
            });
            if changed && edge {
                points.push((x, y));
            }
        }
    }
    if points.len() <= 768 {
        return points;
    }
    (0..768)
        .map(|i| points[i * (points.len() - 1) / 767])
        .collect()
}
fn anchor_difference(
    previous: &RgbaImage,
    next: &RgbaImage,
    shift: u32,
    points: &[(u32, u32)],
) -> f64 {
    let mut errors = Vec::with_capacity(points.len());
    for &(x, y) in points {
        if y + shift >= previous.height() {
            continue;
        }
        let mut error = 0;
        for (dx, dy) in [(0, 0), (-1, 0), (0, -1)] {
            let a = previous.get_pixel(
                (x as i32 + dx) as u32,
                (y as i32 + shift as i32 + dy) as u32,
            );
            let b = next.get_pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32);
            error += (0..3).map(|c| a[c].abs_diff(b[c]) as u32).sum::<u32>();
        }
        errors.push(error / 3);
    }
    if errors.len() < 12 {
        return 255.;
    }
    // Ignore a small minority of changing pixels (caret, hover, animation),
    // never a competing region of scrolling content.
    errors.sort_unstable();
    let keep = (errors.len() * 98 / 100).max(1);
    errors[..keep].iter().map(|e| *e as f64).sum::<f64>() / (keep * 3) as f64
}
/// Sample the full overlap, including both edges, with bounded work per shift.
fn difference(previous: &RgbaImage, next: &RgbaImage, shift: u32) -> f64 {
    sample_difference(previous, next, shift, 48, 64)
}
fn sample_difference(
    previous: &RgbaImage,
    next: &RgbaImage,
    shift: u32,
    rows: u32,
    columns: u32,
) -> f64 {
    let w = previous.width();
    let h = previous.height() - shift;
    let mut sum = 0u64;
    let mut n = 0u64;
    for ry in 0..rows {
        let y = ry * (h - 1) / (rows - 1);
        for rx in 0..columns {
            let x = rx * (w - 1) / (columns - 1);
            let a = previous.get_pixel(x, y + shift);
            let b = next.get_pixel(x, y);
            // Stationary chrome and blank margins are not evidence of motion.
            // Ignore them rather than letting their mismatch defeat real content.
            if shift > 0 && (0..3).all(|c| previous.get_pixel(x, y)[c].abs_diff(b[c]) <= 2) {
                continue;
            }
            for c in 0..3 {
                sum += a[c].abs_diff(b[c]) as u64;
                n += 1;
            }
        }
    }
    if n < (rows * columns / 12) as u64 {
        255.
    } else {
        sum as f64 / n as f64
    }
}
pub fn settled(previous: &RgbaImage, next: &RgbaImage) -> bool {
    previous.dimensions() == next.dimensions()
        && !previous.is_empty()
        && difference(previous, next, 0) < 0.7
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_text_single_wheel_and_sticky_chrome_align() {
        let mut full = RgbaImage::from_pixel(1200, 1800, image::Rgba([250, 250, 250, 255]));
        // Paragraphs with unique word lengths, wide margins and real-world
        // low ink coverage rather than a full-screen random pixel texture.
        for row in 0..90u32 {
            for word in 0..9u32 {
                let start = 210 + word * 73;
                let width = 15
                    + ((row.wrapping_mul(2654435761) ^ word.wrapping_mul(2246822519)) >> 23) % 51;
                for y in (row * 19 + 6)..(row * 19 + 12) {
                    for x in start..start + width {
                        full.put_pixel(x, y, image::Rgba([35, 39, 42, 255]));
                    }
                }
            }
        }
        let frame = |at| {
            let mut f = image::imageops::crop_imm(&full, 0, at, 1200, 700).to_image();
            for y in 0..35 {
                for x in 0..1200 {
                    f.put_pixel(x, y, image::Rgba([53, 62, 66, 255]));
                }
            }
            f
        };
        let mut stitch = Stitcher::new(frame(0)).unwrap();
        for at in [53, 109, 170, 231, 300] {
            assert!(
                matches!(stitch.append(&frame(at)).unwrap(), Append::Added(_)),
                "failed at {at}"
            );
        }
        assert_eq!(stitch.image().height(), 1000);
    }
    fn page() -> RgbaImage {
        RgbaImage::from_fn(180, 900, |x, y| {
            image::Rgba([
                ((x * 17 + y * 23) % 251) as u8,
                ((x * 11 + y * 31) % 253) as u8,
                ((x * 7 + y * 19) % 247) as u8,
                255,
            ])
        })
    }
    #[test]
    fn forward_stitch_duplicate_reverse_and_discontinuity_do_not_corrupt_content() {
        let full = page();
        let frame = |y| image::imageops::crop_imm(&full, 0, y, 180, 240).to_image();
        let mut s = Stitcher::new(frame(0)).unwrap();
        assert_eq!(s.append(&frame(0)).unwrap(), Append::Unchanged);
        for y in [110, 230, 350, 490, 660] {
            assert!(matches!(s.append(&frame(y)).unwrap(), Append::Added(_)));
        }
        assert_eq!(s.image(), &full);
        let before = s.image().clone();
        assert_eq!(s.append(&frame(490)).unwrap(), Append::Unchanged);
        assert_eq!(s.image(), &before);
        assert!(s.append(&RgbaImage::new(200, 240)).is_err());
        assert_eq!(s.image(), &before);
    }
    #[test]
    fn repeated_patterns_are_rejected_instead_of_guessing() {
        let first = RgbaImage::from_fn(96, 240, |_, y| {
            image::Rgba([((y % 20) * 10) as u8, 0, 0, 255])
        });
        let next = RgbaImage::from_fn(96, 240, |_, y| {
            image::Rgba([(((y + 7) % 20) * 10) as u8, 0, 0, 255])
        });
        let mut s = Stitcher::new(first.clone()).unwrap();
        assert_eq!(s.append(&next).unwrap(), Append::NoOverlap);
        assert_eq!(s.image(), &first);
    }
    #[test]
    fn burst_undo_restores_pixels_and_anchor_for_up_down_and_retries() {
        let full = page();
        let frame = |y| image::imageops::crop_imm(&full, 0, y, 180, 240).to_image();
        let mut s = Stitcher::new(frame(330)).unwrap();
        for y in [360, 390, 420, 450, 480] {
            assert!(matches!(s.append(&frame(y)).unwrap(), Append::Added(_)));
        }
        s.end_burst();
        let first = s.image().clone();
        assert!(matches!(s.append(&frame(510)).unwrap(), Append::Added(_)));
        assert!(s.undo());
        assert_eq!(s.image(), &first);
        assert!(s.undo());
        assert_eq!(s.image(), &frame(330));
        assert!(!s.can_undo());
        for y in [300, 270, 240] {
            assert!(matches!(s.append(&frame(y)).unwrap(), Append::Added(_)));
        }
        s.end_burst();
        assert_eq!(
            s.image(),
            &image::imageops::crop_imm(&full, 0, 240, 180, 330).to_image()
        );
        assert_eq!(s.append(&frame(300)).unwrap(), Append::Unchanged);
        assert!(s.undo());
        assert_eq!(s.image(), &frame(330));
        assert!(matches!(s.append(&frame(360)).unwrap(), Append::Added(30)));
    }
}
#[test]
fn small_wheel_moves_with_stationary_margins_are_not_alignment_failures() {
    let full = image::RgbaImage::from_fn(240, 800, |x, y| {
        if !(35..205).contains(&x) {
            image::Rgba([240, 240, 240, 255])
        } else {
            image::Rgba([
                ((x * 31 + y * 17) % 251) as u8,
                ((x * 17 + y * 29) % 253) as u8,
                ((x * 11 + y * 13) % 247) as u8,
                255,
            ])
        }
    });
    let view = |y| image::imageops::crop_imm(&full, 0, y, 240, 300).to_image();
    let mut s = Stitcher::new(view(0)).unwrap();
    for y in [1, 4, 13, 31, 84, 170, 275, 400] {
        assert!(
            matches!(s.append(&view(y)).unwrap(), Append::Added(_)),
            "offset {y}"
        );
    }
    assert_eq!(
        s.image(),
        &image::imageops::crop_imm(&full, 0, 0, 240, 700).to_image()
    );
}
