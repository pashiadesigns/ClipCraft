//! Original, bounded screenshot analysis. Color components identify interface
//! regions; nearby glyph components merge into editable line blocks. No OCR,
//! network service or alteration of the source pixels is involved.
use image::RgbaImage;

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub color: [u8; 4],
}
#[derive(Clone)]
struct Region {
    lo: [usize; 2],
    hi: [usize; 2],
    count: usize,
    color: [u8; 4],
}
impl Region {
    fn width(&self) -> usize {
        self.hi[0] - self.lo[0]
    }
    fn height(&self) -> usize {
        self.hi[1] - self.lo[1]
    }
    fn area(&self) -> usize {
        self.width() * self.height()
    }
}

/// Detail 0..100 controls the minimum feature size. Work is bounded to one
/// million sampled pixels and at most 400 output objects, even for huge images.
pub fn analyze(image: &RgbaImage, detail: u8) -> Vec<Block> {
    if image.width() == 0 || image.height() == 0 {
        return vec![];
    }
    let scale = (1024. / image.width().max(image.height()) as f32).min(1.);
    let w = (image.width() as f32 * scale).ceil() as usize;
    let h = (image.height() as f32 * scale).ceil() as usize;
    let sx = image.width() as f32 / w as f32;
    let sy = image.height() as f32 / h as f32;
    let pixels: Vec<_> = (0..w * h)
        .map(|i| {
            image
                .get_pixel(((i % w) as f32 * sx) as u32, ((i / w) as f32 * sy) as u32)
                .0
        })
        .collect();
    // Cluster common screenshot colors; assigning antialiased pixels to the
    // nearest retained color keeps thin glyph strokes connected.
    let mut bins = std::collections::BTreeMap::<[u8; 3], (usize, [u64; 3])>::new();
    for p in &pixels {
        if p[3] < 128 {
            continue;
        }
        let (count, sum) = bins.entry([p[0] / 16, p[1] / 16, p[2] / 16]).or_default();
        *count += 1;
        for (s, c) in sum.iter_mut().zip(p) {
            *s += *c as u64;
        }
    }
    let mut bins: Vec<_> = bins.into_values().collect();
    bins.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    let distance = |a: [u8; 3], b: [u8; 3]| {
        (0..3)
            .map(|i| (a[i] as i32 - b[i] as i32).pow(2))
            .sum::<i32>()
    };
    let mut colors: Vec<[u8; 3]> = vec![];
    for (count, sum) in bins {
        if count < (w * h / 5000).max(3) {
            continue;
        }
        let c = sum.map(|s| (s / count as u64) as u8);
        // Sparse colors lying between two common colors are usually AA blends,
        // not independent UI fills. Do not split a glyph into separate shades.
        let blend = count < (w * h / 200).max(20)
            && colors.iter().enumerate().any(|(i, a)| {
                colors[..i].iter().any(|b| {
                    let v = [0, 1, 2].map(|j| a[j] as f32 - b[j] as f32);
                    let q = [0, 1, 2].map(|j| c[j] as f32 - b[j] as f32);
                    let t = (0..3).map(|j| q[j] * v[j]).sum::<f32>()
                        / v.iter().map(|v| v * v).sum::<f32>();
                    t > 0.
                        && t < 1.
                        && (0..3).map(|j| (q[j] - t * v[j]).powi(2)).sum::<f32>() < 32. * 32.
                })
            });
        if blend {
            continue;
        }
        if colors.iter().all(|p| distance(*p, c) > 55 * 55) {
            colors.push(c);
        }
        if colors.len() == 12 {
            break;
        }
    }
    if colors.is_empty() {
        return vec![];
    }
    let keys: Vec<_> = pixels
        .iter()
        .map(|p| {
            colors
                .iter()
                .enumerate()
                .min_by_key(|(_, c)| distance(**c, [p[0], p[1], p[2]]))
                .unwrap()
                .0
        })
        .collect();
    let mut seen = vec![false; w * h];
    let mut stack = Vec::new();
    let mut regions = Vec::new();
    let minimum = 3 + (100 - detail.min(100)) as usize / 12;
    for seed in 0..pixels.len() {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        if pixels[seed][3] < 128 {
            continue;
        }
        let k = keys[seed];
        stack.push(seed);
        let mut r = Region {
            lo: [w, h],
            hi: [0, 0],
            count: 0,
            color: [0, 0, 0, 255],
        };

        while let Some(i) = stack.pop() {
            let [x, y] = [i % w, i / w];
            r.lo = [r.lo[0].min(x), r.lo[1].min(y)];
            r.hi = [r.hi[0].max(x + 1), r.hi[1].max(y + 1)];
            r.count += 1;
            for n in [
                x.checked_sub(1).map(|x| y * w + x),
                (x + 1 < w).then_some(i + 1),
                y.checked_sub(1).map(|y| y * w + x),
                (y + 1 < h).then_some(i + w),
            ]
            .into_iter()
            .flatten()
            {
                if !seen[n] && pixels[n][3] >= 128 && keys[n] == k {
                    seen[n] = true;
                    stack.push(n);
                }
            }
        }
        if r.count < 2
            || r.count * 100 > w * h * 45
            || (r.width() * 10 > w * 9 && r.height() * 10 > h * 9)
            || r.count * 10 < r.area()
        {
            continue;
        }
        r.color[..3].copy_from_slice(&colors[k]);
        regions.push(r);
    }
    // Ignore antialias fragments/noise before the bounded line grouping pass.
    regions.sort_by_key(|r| std::cmp::Reverse(r.count));
    regions.truncate(4000);
    // Attach detached i/j dots and accents to their stems before line grouping.
    // Only a small component may join a taller one, so adjacent text rows remain separate.
    let mut i = 0;
    while i < regions.len() {
        if regions[i].height() <= 4 {
            let dot = &regions[i];
            let nearest = regions
                .iter()
                .enumerate()
                .filter(|(j, stem)| {
                    *j != i
                        && stem.height() >= dot.height() * 3
                        && stem.height() <= 48
                        && stem.color == dot.color
                        && stem.lo[0] < dot.hi[0]
                        && dot.lo[0] < stem.hi[0]
                        && stem.lo[1]
                            .saturating_sub(dot.hi[1])
                            .max(dot.lo[1].saturating_sub(stem.hi[1]))
                            <= stem.height() / 2
                })
                .min_by_key(|(_, stem)| stem.lo[1].abs_diff(dot.lo[1]))
                .map(|(j, _)| j);
            if let Some(j) = nearest {
                let dot = regions[i].clone();
                let stem = &mut regions[j];
                for axis in 0..2 {
                    stem.lo[axis] = stem.lo[axis].min(dot.lo[axis]);
                    stem.hi[axis] = stem.hi[axis].max(dot.hi[axis]);
                }
                stem.count += dot.count;
                regions.remove(i);
                continue;
            }
        }
        i += 1;
    }
    regions.sort_by_key(|r| (r.lo[1], r.lo[0]));
    let mut merged: Vec<Region> = Vec::new();
    for r in regions {
        let is_glyph = r.height() <= 64 && r.width() <= r.height() * 3;
        let target = is_glyph
            .then(|| {
                merged.iter_mut().rev().find(|line| {
                    let overlap = line.hi[1]
                        .min(r.hi[1])
                        .saturating_sub(line.lo[1].max(r.lo[1]));
                    let gap = r.lo[0]
                        .saturating_sub(line.hi[0])
                        .max(line.lo[0].saturating_sub(r.hi[0]));
                    line.height() <= 64
                        && overlap * 2 >= line.height().min(r.height())
                        && line.height().max(r.height()) <= line.height().min(r.height()) * 6
                        && gap <= line.height().max(r.height())
                        && line.color[..3]
                            .iter()
                            .zip(r.color)
                            .all(|(a, b)| a.abs_diff(b) <= 36)
                })
            })
            .flatten();
        if let Some(line) = target {
            for c in 0..3 {
                line.color[c] = ((line.color[c] as usize * line.count
                    + r.color[c] as usize * r.count)
                    / (line.count + r.count)) as u8;
            }
            line.count += r.count;
            line.lo = [line.lo[0].min(r.lo[0]), line.lo[1].min(r.lo[1])];
            line.hi = [line.hi[0].max(r.hi[0]), line.hi[1].max(r.hi[1])];
        } else {
            merged.push(r);
        }
    }
    // Joining descenders and differently aligned glyph tops can connect runs
    // that were separate earlier in the scan. Resolve those bridges as well.
    for _ in 0..4 {
        let mut joined = false;
        let mut i = 0;
        while i < merged.len() {
            let mut j = i + 1;
            while j < merged.len() {
                let (a, b) = (&merged[i], &merged[j]);
                let overlap = a.hi[1].min(b.hi[1]).saturating_sub(a.lo[1].max(b.lo[1]));
                let gap = a.lo[0]
                    .saturating_sub(b.hi[0])
                    .max(b.lo[0].saturating_sub(a.hi[0]));
                if a.height().max(b.height()) <= 64
                    && overlap * 2 >= a.height().min(b.height())
                    && gap <= a.height().max(b.height())
                    && a.color == b.color
                {
                    let b = merged.remove(j);
                    let a = &mut merged[i];
                    a.lo = [a.lo[0].min(b.lo[0]), a.lo[1].min(b.lo[1])];
                    a.hi = [a.hi[0].max(b.hi[0]), a.hi[1].max(b.hi[1])];
                    a.count += b.count;
                    joined = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
        if !joined {
            break;
        }
    }
    let text_regions = merged.clone();
    merged.retain(|r| {
        r.count >= minimum
            && !(r.color[..3] == colors[0]
                && text_regions.iter().any(|t| {
                    t.color[..3] != colors[0]
                        && t.height() <= 64
                        && t.width() > t.height() * 2
                        && t.lo[0] <= r.lo[0]
                        && t.lo[1] <= r.lo[1]
                        && t.hi[0] >= r.hi[0]
                        && t.hi[1] >= r.hi[1]
                }))
    });
    merged.sort_by_key(|r| std::cmp::Reverse(r.area()));
    merged.truncate(400);
    merged
        .into_iter()
        .map(|r| {
            // One sampling pixel covers antialiased glyph fringes without changing
            // the proportions/placement of the detected region.
            let pad = usize::from(r.count * 10 < r.area() * 8);
            Block {
                start: [
                    r.lo[0].saturating_sub(pad) as f32 * sx,
                    r.lo[1].saturating_sub(pad) as f32 * sy,
                ],
                end: [
                    (r.hi[0] + pad).min(w) as f32 * sx,
                    (r.hi[1] + pad).min(h) as f32 * sy,
                ],
                color: r.color,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn antialiased_small_text_dots_low_contrast_and_separate_columns_keep_coverage() {
        let fixture: crate::Document =
            serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json"))
                .unwrap();
        let template = fixture
            .annotations
            .iter()
            .find(|a| a.kind == crate::Kind::Text)
            .unwrap();
        let mut document = crate::Document::blank(500, 170).unwrap();
        for (id, x, y, color, size) in [
            (1, 20., 20., [30, 30, 30, 255], 10.),
            (2, 285., 20., [30, 30, 30, 255], 10.),
            (3, 20., 90., [200, 200, 200, 255], 12.),
        ] {
            let mut a = template.clone();
            a.id = id;
            a.start = [x, y];
            a.end = [x + 180., y + 35.];
            a.color = color;
            let t = a.text.as_mut().unwrap();
            t.content = "minimum i.j. ill".into();
            t.size_points = size;
            t.color = None;
            document.annotations.push(a);
        }
        document.promote_features();
        let image = document.render().unwrap();
        let blocks = analyze(&image, 70);
        assert!(
            blocks.iter().any(|b| b.color[0] > 150 && b.start[1] > 80.),
            "low contrast label: {blocks:?}"
        );
        assert!(
            !blocks.iter().any(|b| b.start[0] < 200. && b.end[0] > 280.),
            "separate columns must not merge"
        );
        let mut ink = 0;
        let mut covered = 0;
        for (x, y, pixel) in image.enumerate_pixels() {
            if pixel[0] < 240 {
                ink += 1;
                if blocks.iter().any(|b| {
                    x as f32 >= b.start[0]
                        && (x as f32) < b.end[0]
                        && y as f32 >= b.start[1]
                        && (y as f32) < b.end[1]
                }) {
                    covered += 1;
                }
            }
        }
        assert!(ink > 200);
        assert!(
            covered * 100 >= ink * 97,
            "{covered}/{ink} ink pixels covered: {blocks:?}"
        );
        assert!(
            blocks.len() <= 8,
            "text should consolidate into rows: {blocks:?}"
        );
    }
    #[test]
    fn regions_keep_sampled_colors_dimensions_and_bounded_work() {
        let image = RgbaImage::from_fn(480, 240, |x, y| {
            image::Rgba(if (30..200).contains(&x) && (25..65).contains(&y) {
                [25, 100, 200, 255]
            } else if (40..160).contains(&x) && x % 12 < 7 && (95..113).contains(&y) {
                [35, 35, 35, 255]
            } else {
                [245, 245, 245, 255]
            })
        });
        let blocks = analyze(&image, 50);
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        assert!(blocks.iter().any(|b| b.start == [30., 25.]
            && b.end == [200., 65.]
            && b.color == [25, 100, 200, 255]));
        assert!(
            blocks
                .iter()
                .any(|b| b.color == [35, 35, 35, 255] && b.end[0] - b.start[0] > 110.)
        );
        assert_eq!(analyze(&image, 50), blocks);
        assert!(
            analyze(
                &RgbaImage::from_pixel(100, 100, image::Rgba([50, 50, 50, 255])),
                50
            )
            .is_empty()
        );
        assert!(analyze(&RgbaImage::new(100, 100), 50).is_empty());
        let noisy = RgbaImage::from_fn(2048, 1200, |x, y| {
            image::Rgba([
                ((x * 71 + y * 17) % 256) as u8,
                ((x + y * 5) % 256) as u8,
                90,
                255,
            ])
        });
        for detail in [0, 50, 100] {
            let found = analyze(&noisy, detail);
            assert!(found.len() <= 400);
            assert!(found.iter().all(|b| b.start[0] >= 0.
                && b.start[1] >= 0.
                && b.end[0] <= 2048.
                && b.end[1] <= 1200.));
        }
    }
}
