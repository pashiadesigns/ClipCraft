//! Editable rectangular visual effects. These are not solid redaction.
//! Reuses the cached image crate's box-filter Gaussian approximation.
use crate::{Annotation, Kind};
use image::{Rgba, RgbaImage};

pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let (min, max) = annotation.body_bounds();
    let [x, y] = min.map(|v| v.floor().max(0.0) as u32);
    let right = (max[0].ceil() as u32).min(image.width());
    let bottom = (max[1].ceil() as u32).min(image.height());
    if x >= right || y >= bottom {
        return;
    }
    if annotation.kind == Kind::Pixelate {
        let block = annotation.width;
        if block == 1 {
            return;
        }
        for top in (y..bottom).step_by(block as usize) {
            for left in (x..right).step_by(block as usize) {
                let r = (left + block).min(right);
                let b = (top + block).min(bottom);
                let mut sum = [0u64; 4];
                for py in top..b {
                    for px in left..r {
                        let p = image.get_pixel(px, py).0;
                        for c in 0..3 {
                            sum[c] += u64::from(p[c]) * u64::from(p[3]);
                        }
                        sum[3] += u64::from(p[3]);
                    }
                }
                let count = u64::from((r - left) * (b - top));
                let color = Rgba([
                    (sum[0] + sum[3] / 2).checked_div(sum[3]).unwrap_or(0) as u8,
                    (sum[1] + sum[3] / 2).checked_div(sum[3]).unwrap_or(0) as u8,
                    (sum[2] + sum[3] / 2).checked_div(sum[3]).unwrap_or(0) as u8,
                    ((sum[3] + count / 2) / count) as u8,
                ]);
                for py in top..b {
                    for px in left..r {
                        image.put_pixel(px, py, color);
                    }
                }
            }
        }
    } else {
        // Include neighbors outside the effect region, but write only inside.
        let reach = annotation.width * 3;
        let left = x.saturating_sub(reach);
        let top = y.saturating_sub(reach);
        let r = (right + reach).min(image.width());
        let b = (bottom + reach).min(image.height());
        let region = image::imageops::crop_imm(image, left, top, r - left, b - top).to_image();
        let mut premultiplied = image::DynamicImage::ImageRgba8(region).into_rgba32f();
        for p in premultiplied.pixels_mut() {
            for c in 0..3 {
                let v = p.0[c];
                p.0[c] = (if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }) * p.0[3];
            }
        }
        let blurred = image::imageops::fast_blur(&premultiplied, annotation.width as f32);
        for py in y..bottom {
            for px in x..right {
                let p = blurred.get_pixel(px - left, py - top).0;
                let alpha = p[3].clamp(0.0, 1.0);
                let mut color = [0; 4];
                if alpha > 0.0 {
                    for c in 0..3 {
                        let v = (p[c] / alpha).clamp(0.0, 1.0);
                        let srgb = if v <= 0.0031308 {
                            v * 12.92
                        } else {
                            1.055 * v.powf(1.0 / 2.4) - 0.055
                        };
                        color[c] = (srgb * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
                color[3] = (alpha * 255.0).round() as u8;
                image.put_pixel(px, py, Rgba(color));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session, StackOrder};
    fn effect(kind: Kind) -> Annotation {
        Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,
            analysis: None,
            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            id: 1,
            kind,
            start: [2.0, 1.0],
            end: [7.0, 6.0],
            color: [0; 4],
            width: 2,
            corner_radius: 0.0,
            fill: None,
            points: vec![],
            text: None,
            callout: None,
        }
    }

    #[test]
    fn pixelation_matches_independent_alpha_weighted_blocks_and_preserves_outside() {
        let source = RgbaImage::from_fn(9, 7, |x, y| {
            Rgba([
                (x * 20) as u8,
                (y * 30) as u8,
                90,
                if x % 2 == 0 { 0 } else { 200 },
            ])
        });
        let mut doc = Document::from_image(source.clone()).unwrap();
        let mut a = effect(Kind::Pixelate);
        a.start = [7.0, 6.0];
        a.end = [2.0, 1.0];
        doc.annotations.push(a);
        let output = doc.render().unwrap();
        for (x, y, p) in output.enumerate_pixels() {
            if !(2..7).contains(&x) || !(1..6).contains(&y) {
                assert_eq!(p, source.get_pixel(x, y));
                continue;
            }
            let l = 2 + (x - 2) / 2 * 2;
            let t = 1 + (y - 1) / 2 * 2;
            let pixels: Vec<_> = (t..(t + 2).min(6))
                .flat_map(|y| (l..(l + 2).min(7)).map(move |x| (x, y)))
                .map(|(x, y)| source.get_pixel(x, y).0)
                .collect();
            let alpha: u32 = pixels.iter().map(|p| p[3] as u32).sum();
            let mut expected = [0u8; 4];
            for c in 0..3 {
                expected[c] = (pixels
                    .iter()
                    .map(|p| u32::from(p[c]) * u32::from(p[3]))
                    .sum::<u32>()
                    + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or(0) as u8;
            }
            expected[3] = ((alpha + pixels.len() as u32 / 2) / pixels.len() as u32) as u8;
            assert_eq!(p.0, expected);
        }
        doc.annotations[0].width = 1;
        assert_eq!(doc.render().unwrap(), source);
    }

    #[test]
    fn blur_ignores_hidden_rgb_and_leaves_pixels_outside_selection_unchanged() {
        let source = RgbaImage::from_fn(11, 9, |x, y| {
            if x == 5 && y == 4 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 255, 255, 0])
            }
        });
        let mut doc = Document::from_image(source.clone()).unwrap();
        let mut a = effect(Kind::Blur);
        a.start = [3.0, 2.0];
        a.end = [8.0, 7.0];
        doc.annotations.push(a);
        let output = doc.render().unwrap();
        assert!(output.get_pixel(5, 4)[3] > 0 && output.get_pixel(5, 4)[3] < 255);
        for (x, y, p) in output.enumerate_pixels() {
            if (3..8).contains(&x) && (2..7).contains(&y) {
                if p[3] > 0 {
                    assert_eq!([p[0], p[1], p[2]], [255, 0, 0]);
                }
            } else {
                assert_eq!(p, source.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn effects_follow_stacking_and_remain_editable_through_history_and_persistence() {
        for kind in [Kind::Blur, Kind::Pixelate] {
            let pixels =
                RgbaImage::from_fn(9, 7, |x, y| Rgba([(x * 20) as u8, (y * 30) as u8, 90, 255]));
            let mut session = Session::new(Document::from_image(pixels.clone()).unwrap()).unwrap();
            let a = effect(kind);
            session
                .execute(Command::Add {
                    annotation: a.clone(),
                })
                .unwrap();
            let changed = session.document.render().unwrap();
            assert_ne!(changed, pixels);
            session.execute(Command::Undo).unwrap();
            assert_eq!(session.document.render().unwrap(), pixels);
            session.execute(Command::Redo).unwrap();
            assert_eq!(session.document.render().unwrap(), changed);
            let moved = a.translated([1.0, 0.0], [9, 7]).unwrap();
            session
                .execute(Command::Update { annotation: moved })
                .unwrap();
            assert_ne!(session.document.render().unwrap(), changed);
            session.execute(Command::Undo).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("effects.capture.json");
            session.save(&path).unwrap();
            assert_eq!(Document::load(&path).unwrap().render().unwrap(), changed);
            let mut invalid = a.clone();
            invalid.width = 33;
            assert!(
                session
                    .execute(Command::Update {
                        annotation: invalid
                    })
                    .is_err()
            );
            assert_eq!(session.document.render().unwrap(), changed);
            let mut cover = a;
            cover.id = 2;
            cover.kind = Kind::Redact;
            cover.start = [4.0, 2.0];
            cover.end = [5.0, 5.0];
            session.execute(Command::Add { annotation: cover }).unwrap();
            let on_top = session.document.render().unwrap();
            session
                .execute(Command::Arrange {
                    id: 1,
                    order: StackOrder::BringToFront,
                })
                .unwrap();
            assert_ne!(session.document.render().unwrap(), on_top);
        }
    }
}
