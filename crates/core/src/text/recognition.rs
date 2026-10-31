//! Approximate OCR appearance from explicit image pixels and known word text.
//! No font identity is supplied by Windows OCR. Compare only our bundled faces.
use super::{Font, ScaleFont, TextFont, TextStyle, point};
use image::{GrayImage, Luma, RgbaImage};

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub style: TextStyle,
    pub background: [u8; 3],
}

/// Bounded word matching; uncertain/empty crops produce no inferred formatting.
pub fn estimate(image: &RgbaImage, word: &str, bounds: [f32; 4]) -> Option<Appearance> {
    if word.chars().count() < 3
        || word.chars().count() > 48
        || bounds.iter().any(|v| !v.is_finite())
        || bounds[2] < 2.
        || bounds[3] < 2.
    {
        return None;
    }
    let x0 = bounds[0].floor().max(0.) as u32;
    let y0 = bounds[1].floor().max(0.) as u32;
    let x1 = (bounds[0] + bounds[2])
        .ceil()
        .clamp(0., image.width() as f32) as u32;
    let y1 = (bounds[1] + bounds[3])
        .ceil()
        .clamp(0., image.height() as f32) as u32;
    if x1 <= x0 || y1 <= y0 || u64::from(x1 - x0) * u64::from(y1 - y0) > 1_000_000 {
        return None;
    }
    let crop = image::imageops::crop_imm(image, x0, y0, x1 - x0, y1 - y0).to_image();
    // Dominant quantized color is normally the paper. Composite transparent
    // sources on white exactly as OCR does, avoiding invisible RGB pollution.
    let mut bins = vec![(0u32, [0u64; 3]); 4096];
    let rgb = |p: &image::Rgba<u8>| -> [u8; 3] {
        std::array::from_fn(|i| {
            ((u32::from(p[i]) * u32::from(p[3]) + 255 * (255 - u32::from(p[3])) + 127) / 255) as u8
        })
    };
    for p in crop.pixels() {
        let c = rgb(p);
        let key =
            (usize::from(c[0] >> 4) << 8) | (usize::from(c[1] >> 4) << 4) | usize::from(c[2] >> 4);
        bins[key].0 += 1;
        for (i, v) in c.iter().enumerate() {
            bins[key].1[i] += u64::from(*v);
        }
    }
    let average = |b: &(u32, [u64; 3])| -> [u8; 3] {
        std::array::from_fn(|i| (b.1[i] / u64::from(b.0.max(1))) as u8)
    };
    let bg = average(bins.iter().max_by_key(|b| b.0)?);
    let distance = |c: [u8; 3]| -> f32 {
        (0..3)
            .map(|i| (f32::from(c[i]) - f32::from(bg[i])).powi(2))
            .sum::<f32>()
    };
    let ink = bins
        .iter()
        .filter(|b| b.0 > 0 && distance(average(b)) > 1600.)
        .max_by(|a, b| {
            (a.0 as f32 * distance(average(a)).sqrt())
                .total_cmp(&(b.0 as f32 * distance(average(b)).sqrt()))
        })?;
    let fg = average(ink);
    let delta: [f32; 3] = std::array::from_fn(|i| f32::from(fg[i]) - f32::from(bg[i]));
    let norm = distance(fg);
    let mask = GrayImage::from_fn(crop.width(), crop.height(), |x, y| {
        let c = rgb(crop.get_pixel(x, y));
        let coverage = (0..3)
            .map(|i| (f32::from(c[i]) - f32::from(bg[i])) * delta[i])
            .sum::<f32>()
            / norm;
        Luma([(coverage.clamp(0., 1.) * 255.).round() as u8])
    });
    let source = tight(&mask)?;
    let target = image::imageops::resize(&source, 64, 24, image::imageops::FilterType::Triangle);
    let aspect = source.width() as f32 / source.height() as f32;
    let mut best = None;
    for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let reference = word_mask(word, font, bold, italic)?;
            let sample =
                image::imageops::resize(&reference, 64, 24, image::imageops::FilterType::Triangle);
            let shape = target
                .pixels()
                .zip(sample.pixels())
                .map(|(a, b)| (f32::from(a[0]) - f32::from(b[0])).abs() / 255.)
                .sum::<f32>()
                / 1536.;
            let ratio = reference.width() as f32 / reference.height() as f32;
            let score = shape + (aspect / ratio).ln().abs() * 0.45;
            if best.as_ref().is_none_or(|(s, _)| score < *s) {
                best = Some((
                    score,
                    TextStyle {
                        content: String::new(),
                        font,
                        bold,
                        italic,
                        size_points: (48. * source.height() as f32 / reference.height() as f32
                            * 0.75)
                            .clamp(6., 108.),
                        color: Some([fg[0], fg[1], fg[2], 255]),
                        ..Default::default()
                    },
                ));
            }
        }
    }
    best.map(|(_, style)| Appearance {
        style,
        background: bg,
    })
}

fn tight(mask: &GrayImage) -> Option<GrayImage> {
    let mut bounds = [mask.width(), mask.height(), 0, 0];
    let mut any = false;
    for (x, y, p) in mask.enumerate_pixels() {
        if p[0] > 32 {
            any = true;
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x + 1);
            bounds[3] = bounds[3].max(y + 1);
        }
    }
    any.then(|| {
        image::imageops::crop_imm(
            mask,
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        )
        .to_image()
    })
}

fn word_mask(word: &str, font: TextFont, bold: bool, italic: bool) -> Option<GrayImage> {
    let face = font.face(bold, italic);
    let scaled = face.as_scaled(48.);
    let mut mask = GrayImage::new(2600, 80);
    let mut x = 4.;
    let mut previous = None;
    for ch in word.chars() {
        let id = face.glyph_id(ch);
        if let Some(prev) = previous {
            x += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(48., point(x, 55.));
        if let Some(outline) = face.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|px, py, c| {
                let x = b.min.x as i32 + px as i32;
                let y = b.min.y as i32 + py as i32;
                if x >= 0 && y >= 0 && x < mask.width() as i32 && y < mask.height() as i32 {
                    mask.put_pixel(x as u32, y as u32, Luma([(c * 255.).round() as u8]));
                }
            });
        }
        x += scaled.h_advance(id);
        previous = Some(id);
    }
    tight(&mask)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn appearance_matches_bundled_faces_and_color_on_light_dark_and_transparent_paper() {
        for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                let mask = word_mask("Capture", font, bold, italic).unwrap();
                for bg in [[255, 255, 255, 255], [20, 25, 30, 255], [0, 0, 0, 0]] {
                    let fg = [49, 99, 185];
                    let mut source = RgbaImage::from_pixel(
                        mask.width() + 12,
                        mask.height() + 12,
                        image::Rgba(bg),
                    );
                    for (x, y, p) in mask.enumerate_pixels() {
                        let a = u32::from(p[0]);
                        source.put_pixel(
                            x + 6,
                            y + 6,
                            if bg[3] == 0 {
                                image::Rgba([fg[0], fg[1], fg[2], p[0]])
                            } else {
                                image::Rgba(std::array::from_fn(|i| {
                                    if i == 3 {
                                        255
                                    } else {
                                        ((u32::from(fg[i]) * a
                                            + u32::from(bg[i]) * (255 - a)
                                            + 127)
                                            / 255) as u8
                                    }
                                }))
                            },
                        );
                    }
                    let before = source.clone();
                    let estimate = estimate(
                        &source,
                        "Capture",
                        [0., 0., source.width() as f32, source.height() as f32],
                    )
                    .unwrap();
                    assert_eq!(estimate.style.font, font);
                    assert_eq!((estimate.style.bold, estimate.style.italic), (bold, italic));
                    assert!((estimate.style.size_points - 36.).abs() < 1.);
                    for (actual, expected) in estimate.style.color.unwrap().iter().zip(fg) {
                        assert!((i16::from(*actual) - i16::from(expected)).abs() < 12);
                    }
                    assert_eq!(source, before);
                }
            }
        }
    }
    #[test]
    fn blank_and_invalid_word_regions_have_no_guessed_style() {
        let source = RgbaImage::from_pixel(100, 100, image::Rgba([255; 4]));
        for bounds in [
            [0., 0., 100., 100.],
            [f32::NAN, 0., 1., 1.],
            [500., 0., 10., 10.],
            [0., 0., -1., 2.],
        ] {
            assert!(estimate(&source, "Blank", bounds).is_none());
        }
    }
}
