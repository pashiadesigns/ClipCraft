//! Does the scan leave any value bare? Generated pages are scrolled in
//! varied ways while a simulated OCR misses readings, the way real OCR does on
//! moving text. No scenario is tied to one recording, and each seed changes
//! the movement and which readings fail.
use super::scan::{Scanner, Tracked};
use super::{Options, Word};
use crate::video::motion_path::NEAREST_FRAME_MS;
use image::{Rgba, RgbaImage};

const W: u32 = 640;
const H: u32 = 360;
/// Fixed bars above and below the part that scrolls.
const BAR: i32 = 24;
const LINE_H: i32 = 14;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
    fn chance(&mut self, percent: u32) -> bool {
        self.next() % 100 < percent
    }
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.next() % (hi - lo + 1) as u32) as i32
    }
}

#[derive(Clone)]
struct Item {
    x: i32,
    y: i32,
    text: String,
    sensitive: bool,
}
impl Item {
    fn width(&self) -> i32 {
        self.text.chars().count() as i32 * 8
    }
}
struct Page {
    pixels: RgbaImage,
    items: Vec<Item>,
}
/// A page of plain lines with values in some of them. `period` repeats the
/// same lines all the way down, as a document pasted several times does.
fn page(rng: &mut Rng, period: Option<usize>) -> Page {
    let words = ["could", "be", "worse", "depending", "what", "you", "find"];
    let values = ["sam@mail.test", "415-555-0134", "ann@site.test"];
    let mut items = vec![];
    let mut y = 40;
    let mut rows: Vec<(i32, Vec<Item>)> = vec![];
    for row in 0..90 {
        let spacing = rng.range(36, 90);
        let source = period.filter(|p| row >= *p).map(|p| row % p);
        let line: Vec<Item> = match source {
            Some(s) => rows[s].1.iter().map(|i| Item { y, ..i.clone() }).collect(),
            None => {
                let mut x = 12;
                let mut line = vec![];
                for _ in 0..rng.range(2, 5) {
                    let text = words[rng.next() as usize % words.len()].to_owned();
                    let w = text.chars().count() as i32 * 8;
                    line.push(Item {
                        x,
                        y,
                        text,
                        sensitive: false,
                    });
                    x += w + 10;
                }
                if rng.chance(45) {
                    line.push(Item {
                        x,
                        y,
                        text: values[rng.next() as usize % values.len()].to_owned(),
                        sensitive: true,
                    });
                }
                line
            }
        };
        rows.push((y, line.clone()));
        items.extend(line);
        y += spacing;
    }
    let mut pixels = RgbaImage::from_pixel(W, y as u32 + H, Rgba([40, 40, 40, 255]));
    for item in &items {
        let seed = item
            .text
            .bytes()
            .fold(7u32, |a, b| a.wrapping_mul(31) ^ b as u32);
        for dy in 0..LINE_H {
            for dx in 0..item.width() {
                let column = dx / 8;
                let bit = seed
                    .rotate_left((column % 31) as u32)
                    .wrapping_mul(dy as u32 * 2 + 1)
                    >> 7
                    & 1;
                if bit == 1 && dx % 8 < 6 {
                    pixels.put_pixel(
                        (item.x + dx) as u32,
                        (item.y + dy) as u32,
                        Rgba([225, 225, 225, 255]),
                    );
                }
            }
        }
    }
    Page { pixels, items }
}
fn frame(page: &Page, scroll: i32) -> RgbaImage {
    RgbaImage::from_fn(W, H, |x, y| {
        let y = y as i32;
        if y < BAR || y >= H as i32 - BAR {
            return Rgba([24, 24, 70, 255]);
        }
        *page.pixels.get_pixel(x, (y + scroll) as u32)
    })
}
/// Scroll per frame from moves of (frames, pixels per frame); -1 frames returns to the top.
fn script(moves: &[(i32, i32)], limit: i32) -> Vec<i32> {
    let mut scroll = vec![0];
    let mut at = 0;
    for &(frames, speed) in moves {
        if frames < 0 {
            while at > 0 {
                at = (at - speed).max(0);
                scroll.push(at);
            }
            continue;
        }
        for _ in 0..frames {
            at = (at + speed).clamp(0, limit);
            scroll.push(at);
        }
    }
    scroll
}
struct Score {
    label: String,
    exposed: usize,
    appearances: usize,
    tall: usize,
    tracks: usize,
    first: Option<(u64, String)>,
}
fn rect_in_view(item: &Item, scroll: i32) -> Option<[f32; 4]> {
    let top = (item.y - scroll).max(BAR);
    let bottom = (item.y - scroll + LINE_H).min(H as i32 - BAR);
    (bottom - top >= 4).then_some([
        item.x as f32,
        top as f32,
        item.width() as f32,
        (bottom - top) as f32,
    ])
}
fn holds(outer: [f32; 4], inner: [f32; 4]) -> bool {
    outer[0] <= inner[0] + 1.
        && outer[1] <= inner[1] + 1.
        && outer[0] + outer[2] >= inner[0] + inner[2] - 1.
        && outer[1] + outer[3] >= inner[1] + inner[3] - 1.
}
fn visible_rects(tracks: &[Tracked], ms: u64, end: u64) -> Vec<[f32; 4]> {
    tracks
        .iter()
        .filter(|t| ms + NEAREST_FRAME_MS >= t.follow.start && ms < t.end(end))
        .map(|t| t.path.sample(ms.saturating_sub(t.follow.start)))
        .filter(|n| n.visible)
        .map(|n| n.rect)
        .collect()
}
fn run(
    label: &str,
    seed: u32,
    moves: &[(i32, i32)],
    period: Option<usize>,
    late_shift: u64,
) -> Score {
    let mut rng = Rng(seed.wrapping_mul(2654435761).max(1));
    let page = page(&mut rng, period);
    let limit = page.pixels.height() as i32 - H as i32 - 1;
    // Each seed varies the speed of every move a little.
    let moves: Vec<(i32, i32)> = moves
        .iter()
        .map(|&(f, v)| {
            (
                f,
                if f > 0 && v != 0 {
                    (v.abs() + rng.range(-3, 3)).max(1) * v.signum()
                } else {
                    v
                },
            )
        })
        .collect();
    let scroll = script(&moves, limit);
    let times: Vec<u64> = (0..scroll.len() as u64).map(|i| i * 1000 / 30).collect();
    let end = times.last().unwrap() + 33;
    let mut scanner = Scanner::new([W, H], 1., Options::default(), 0);
    let mut read = std::collections::HashSet::new();
    for (i, s) in scroll.iter().enumerate() {
        let shift = i.checked_sub(1).map_or(0, |p| (s - scroll[p]).abs());
        let pass = rng.next();
        scanner
            .frame(times[i], frame(&page, *s), || {
                let mut rng = Rng(pass.max(1));
                let miss = if shift >= 12 { 60 } else { 20 };
                let mut words = vec![];
                for (k, item) in page.items.iter().enumerate() {
                    let top = item.y - s;
                    if top < BAR || top + LINE_H > H as i32 - BAR {
                        continue;
                    }
                    if item.sensitive && rng.chance(miss) {
                        continue;
                    }
                    if item.sensitive {
                        read.insert(k);
                    }
                    words.push(Word {
                        text: item.text.clone(),
                        line: item.y as u32,
                        bounds: [
                            item.x as f32,
                            top as f32,
                            item.width() as f32,
                            LINE_H as f32,
                        ],
                    });
                }
                Ok(words)
            })
            .unwrap();
    }
    let mut tracks = scanner.finish(end).unwrap();
    for t in &tracks {
        t.path.validate().unwrap();
    }
    // A project whose stamps are slightly off the frames must stay covered.
    for t in &mut tracks {
        t.follow.start += late_shift;
        if let Some(e) = &mut t.follow.ended {
            *e += late_shift;
        }
    }
    let mut score = Score {
        label: format!("{label} seed {seed}"),
        exposed: 0,
        appearances: 0,
        tall: 0,
        tracks: tracks.len(),
        first: None,
    };
    for (i, s) in scroll.iter().enumerate() {
        let rects = visible_rects(&tracks, times[i], end + late_shift);
        score.tall += rects.iter().filter(|r| r[3] > (LINE_H + 40) as f32).count();
        for (k, item) in page.items.iter().enumerate() {
            if !item.sensitive || !read.contains(&k) {
                continue;
            }
            let Some(rect) = rect_in_view(item, *s) else {
                continue;
            };
            score.appearances += 1;
            if !rects.iter().any(|r| holds(*r, rect)) {
                score.exposed += 1;
                score
                    .first
                    .get_or_insert_with(|| (times[i], format!("{item:?}", item = item.text)));
            }
        }
    }
    score
}
/// A name, its moves, and how often the page's lines repeat.
type Scenario = (&'static str, &'static [(i32, i32)], Option<usize>);
const STRESS: &[Scenario] = &[
    (
        "slow scroll and pauses",
        &[
            (20, 0),
            (45, 4),
            (20, 0),
            (30, -5),
            (18, 0),
            (40, 3),
            (18, 0),
        ],
        None,
    ),
    (
        "fast flicks",
        &[
            (16, 0),
            (6, 34),
            (16, 0),
            (5, -30),
            (16, 0),
            (8, 38),
            (20, 0),
            (7, -36),
            (16, 0),
        ],
        None,
    ),
    (
        "flick then coast",
        &[
            (16, 0),
            (6, 36),
            (8, 7),
            (8, 3),
            (10, 1),
            (14, 0),
            (5, -34),
            (8, -6),
            (10, -1),
            (14, 0),
        ],
        None,
    ),
    (
        "return to the top",
        &[
            (30, 0),
            (10, 30),
            (14, 0),
            (-1, 34),
            (40, 0),
            (12, 26),
            (14, 0),
            (-1, 30),
            (34, 0),
        ],
        None,
    ),
    (
        "repeated lines",
        &[
            (16, 0),
            (8, 30),
            (14, 0),
            (6, -32),
            (14, 0),
            (9, 36),
            (14, 0),
            (-1, 34),
            (30, 0),
        ],
        Some(6),
    ),
];

fn all(late_shift: u64) -> Vec<Score> {
    let mut scores = vec![];
    for (label, moves, period) in STRESS {
        for seed in [11, 29, 53] {
            scores.push(run(label, seed, moves, *period, late_shift));
        }
    }
    scores
}
fn report(scores: &[Score]) -> String {
    scores
        .iter()
        .map(|s| {
            format!(
                "{}: {} of {} appearances bare, {} tall boxes, {} objects, first {:?}",
                s.label, s.exposed, s.appearances, s.tall, s.tracks, s.first
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn stress_no_value_is_left_bare() {
    let scores = all(0);
    let text = report(&scores);
    eprintln!("{text}");
    assert!(scores.iter().all(|s| s.appearances > 0), "{text}");
    assert_eq!(scores.iter().map(|s| s.exposed).sum::<usize>(), 0, "{text}");
    assert_eq!(scores.iter().map(|s| s.tall).sum::<usize>(), 0, "{text}");
}
#[test]
fn stress_values_stay_covered_with_timing_slightly_off() {
    let scores = all(13);
    let text = report(&scores);
    assert_eq!(scores.iter().map(|s| s.exposed).sum::<usize>(), 0, "{text}");
}
