//! Four-connected, seed-relative raster fill. Hidden transparent RGB is ignored
//! when matching colors. The queue and visited bitset are bounded by image size.
use crate::{Document, Error, RasterImage, Result};
use image::{Rgba, RgbaImage};
use std::{collections::VecDeque, sync::Arc};

fn visible(color: Rgba<u8>) -> [u8; 4] {
    let alpha = u16::from(color[3]);
    [
        ((u16::from(color[0]) * alpha + 127) / 255) as u8,
        ((u16::from(color[1]) * alpha + 127) / 255) as u8,
        ((u16::from(color[2]) * alpha + 127) / 255) as u8,
        color[3],
    ]
}

fn fill(image: &mut RgbaImage, x: u32, y: u32, color: [u8; 4], tolerance: u8) -> bool {
    let width = image.width();
    let height = image.height();
    let seed = visible(*image.get_pixel(x, y));
    let replacement = Rgba(if color[3] == 0 { [0; 4] } else { color });
    let mut seen = vec![0u8; (width as usize * height as usize).div_ceil(8)];
    let mut queue = VecDeque::new();
    let first = y * width + x;
    seen[first as usize / 8] |= 1 << (first % 8);
    queue.push_back(first);
    let mut changed = false;
    while let Some(index) = queue.pop_front() {
        let x = index % width;
        let y = index / width;
        let pixel = image.get_pixel_mut(x, y);
        changed |= *pixel != replacement;
        *pixel = replacement;
        let neighbors = [
            (x > 0).then(|| index - 1),
            (x + 1 < width).then(|| index + 1),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ];
        for next in neighbors.into_iter().flatten() {
            let byte = &mut seen[next as usize / 8];
            let bit = 1 << (next % 8);
            if *byte & bit != 0 {
                continue;
            }
            *byte |= bit;
            if visible(*image.get_pixel(next % width, next / width))
                .into_iter()
                .zip(seed)
                .all(|(value, seed)| value.abs_diff(seed) <= tolerance)
            {
                queue.push_back(next);
            }
        }
    }
    changed
}

impl Document {
    pub(crate) fn flood_fill(
        &mut self,
        x: u32,
        y: u32,
        color: [u8; 4],
        tolerance: u8,
    ) -> Result<bool> {
        if x >= self.width || y >= self.height {
            return Err(Error::Invalid("Fill seed must be inside the image.".into()));
        }
        let mut image = self.render()?;
        if !fill(&mut image, x, y, color, tolerance) {
            return Ok(false);
        }
        // Finish allocation/encoding before changing the document or history.
        let raster = Arc::new(RasterImage::from_image(&image)?);
        self.source_size = Some(self.source_size.unwrap_or([self.width, self.height]));
        self.raster = Some(raster);
        self.annotations.clear();
        self.auto_canvas_base = None;
        self.version = 2;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};

    #[test]
    fn fill_respects_barriers_diagonals_and_fixed_seed_tolerance() {
        let white = Rgba([255; 4]);
        let black = Rgba([0, 0, 0, 255]);
        let red = [220, 30, 10, 255];
        let mut image = RgbaImage::from_pixel(7, 7, white);
        for y in 0..7 {
            image.put_pixel(3, y, black);
        }
        image.put_pixel(1, 1, black);
        assert!(fill(&mut image, 0, 0, red, 0));
        assert_eq!(image.get_pixel(2, 6).0, red);
        assert_eq!(*image.get_pixel(4, 0), white);
        assert_eq!(*image.get_pixel(1, 1), black);
        let mut diagonal = RgbaImage::from_pixel(2, 2, black);
        diagonal.put_pixel(0, 0, white);
        diagonal.put_pixel(1, 1, white);
        fill(&mut diagonal, 0, 0, red, 0);
        assert_eq!(*diagonal.get_pixel(1, 1), white);
        let mut ramp = RgbaImage::from_fn(10, 1, |x, _| Rgba([x as u8 * 10, 0, 0, 255]));
        fill(&mut ramp, 0, 0, red, 20);
        assert_eq!(ramp.get_pixel(2, 0).0, red);
        assert_eq!(ramp.get_pixel(3, 0).0, [30, 0, 0, 255]);
    }

    #[test]
    fn transparent_matching_ignores_hidden_rgb_and_replacement_is_exact_rgba() {
        let mut image = RgbaImage::from_fn(5, 2, |x, _| Rgba([x as u8 * 40, 90, 110, 0]));
        image.put_pixel(4, 1, Rgba([0, 0, 0, 1]));
        fill(&mut image, 0, 0, [10, 80, 140, 128], 0);
        assert_eq!(image.get_pixel(4, 0).0, [10, 80, 140, 128]);
        assert_eq!(image.get_pixel(4, 1).0, [0, 0, 0, 1]);
        fill(&mut image, 0, 0, [255, 50, 80, 0], 255);
        assert!(image.pixels().all(|p| p.0 == [0; 4]));
    }

    #[test]
    fn fill_history_roundtrip_and_invalid_noop_preserve_editable_state() {
        let original: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let before = original.render().unwrap();
        let annotations = original.annotations.clone();
        let mut session = Session::new(original.clone()).unwrap();
        let command: Command = serde_json::from_str(
            r#"{"command":"flood_fill","x":0,"y":0,"color":[10,80,140,128],"tolerance":0}"#,
        )
        .unwrap();
        session.execute(command.clone()).unwrap();
        let filled = session.document.render().unwrap();
        assert_eq!(filled.get_pixel(0, 0).0, [10, 80, 140, 128]);
        assert!(session.document.annotations.is_empty());
        assert_eq!(session.document.source_png, original.source_png);
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("filled.capture.json");
        session.save(&project).unwrap();
        assert_eq!(Document::load(&project).unwrap().render().unwrap(), filled);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, annotations);
        assert_eq!(session.document.render().unwrap(), before);
        assert!(!session.can_undo());
        let unchanged = Command::FloodFill {
            x: 0,
            y: 0,
            color: before.get_pixel(0, 0).0,
            tolerance: 0,
        };
        session.execute(unchanged).unwrap();
        assert_eq!(session.document.annotations, annotations);
        assert!(session.can_redo());
        let bad = Command::FloodFill {
            x: original.width,
            y: 0,
            color: [0; 4],
            tolerance: 0,
        };
        assert!(session.execute(bad).is_err());
        assert!(session.can_redo());
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.render().unwrap(), filled);
        session.execute(Command::RevertOriginal).unwrap();
        assert_eq!(
            session.document.render().unwrap(),
            original.base_image().unwrap()
        );
    }

    #[test]
    fn large_solid_fill_uses_iterative_traversal_and_same_seed_color_still_fills_tolerant_neighbors()
     {
        let mut image = RgbaImage::from_pixel(1024, 1024, Rgba([250, 250, 250, 255]));
        image.put_pixel(0, 0, Rgba([255; 4]));
        assert!(fill(&mut image, 0, 0, [255; 4], 5));
        assert!(image.pixels().all(|p| p.0 == [255; 4]));
        assert!(!fill(&mut image, 1023, 1023, [255; 4], 0));
    }
}
