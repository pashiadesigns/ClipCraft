//! Bounded local patch tracking. Ambiguous matches remain explicitly unresolved.
use image::RgbaImage;
#[derive(Clone)]
pub struct Patch {
    pixels: Vec<f32>,
    width: usize,
    height: usize,
    pub rect: [f32; 4],
    velocity: [f32; 2],
}
impl Patch {
    pub fn new(frame: &RgbaImage, rect: [f32; 4]) -> Self {
        let width = (rect[2] as usize).clamp(4, 32);
        let height = (rect[3] as usize).clamp(4, 16);
        let pixels = sample(frame, rect, width, height);
        Self {
            pixels,
            width,
            height,
            rect,
            velocity: [0., 0.],
        }
    }
    /// Re-samples the patch at a confirmed rectangle, keeping its movement.
    pub fn refresh(&mut self, frame: &RgbaImage, rect: [f32; 4]) {
        let velocity = self.velocity;
        *self = Self::new(frame, rect);
        self.velocity = velocity;
    }
    fn error(&self, frame: &RgbaImage, r: [f32; 4], limit: f32) -> f32 {
        let mut total = 0.;
        let bound = limit * self.pixels.len() as f32;
        for (i, expected) in self.pixels.iter().enumerate() {
            let x = (r[0] + (i % self.width) as f32 * r[2] / self.width as f32)
                .clamp(0., frame.width().saturating_sub(1) as f32) as u32;
            let y = (r[1] + (i / self.width) as f32 * r[3] / self.height as f32)
                .clamp(0., frame.height().saturating_sub(1) as f32) as u32;
            let p = frame.get_pixel(x, y);
            let value = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) / 255.;
            total += (value - expected).abs();
            if total > bound {
                return 1.;
            }
        }
        total / self.pixels.len() as f32
    }
    pub fn locate(&self, frame: &RgbaImage, radius: i32) -> Option<([f32; 4], f32)> {
        self.search(frame, self.rect, radius, radius)
    }
    /// Follows the patch into `frame` and moves it there. Fast scrolling can
    /// outrun a fixed window, so the search tries the position predicted from
    /// the last movement, then the window around the last match, then a long
    /// vertical strip for scrolled content.
    pub fn track(&mut self, frame: &RgbaImage, radius: i32) -> Option<([f32; 4], f32)> {
        self.track_hinted(frame, radius, None)
    }
    /// Like [`Patch::track`], trying first where the page's shared scroll
    /// (see [`scroll_shift`]) puts the patch. Scrolled text then stays matched
    /// however far one frame jumps, and a copy of the same text elsewhere on
    /// the page can't win over the scrolled position.
    pub fn track_hinted(
        &mut self,
        frame: &RgbaImage,
        radius: i32,
        scroll: Option<[f32; 2]>,
    ) -> Option<([f32; 4], f32)> {
        if let Some(d) = scroll.filter(|d| d[0].abs() + d[1].abs() >= 1.) {
            let moved = [
                self.rect[0] + d[0],
                self.rect[1] + d[1],
                self.rect[2],
                self.rect[3],
            ];
            if let Some((r, c)) = self.search(frame, moved, 8, 8) {
                self.velocity = [r[0] - self.rect[0], r[1] - self.rect[1]];
                self.rect = r;
                return Some((r, c));
            }
        }
        let predicted = [
            self.rect[0] + self.velocity[0],
            self.rect[1] + self.velocity[1],
            self.rect[2],
            self.rect[3],
        ];
        let moving = self.velocity[0].abs() + self.velocity[1].abs() >= 2.;
        let found = moving
            .then(|| self.search(frame, predicted, radius / 2, radius / 2))
            .flatten()
            .or_else(|| self.search(frame, self.rect, radius, radius))
            .or_else(|| self.search(frame, self.rect, 12, radius * 4))
            // BA15: panned content moves sideways, and zooming the page scales
            // the text; search a horizontal strip, then scaled sizes.
            .or_else(|| self.search(frame, self.rect, radius * 4, 12))
            .or_else(|| {
                [1.1, 0.9, 1.25, 0.8, 1.5, 0.67]
                    .into_iter()
                    .find_map(|scale| {
                        let [x, y, w, h] = self.rect;
                        let (sw, sh) = (w * scale, h * scale);
                        let scaled = [x + (w - sw) * 0.5, y + (h - sh) * 0.5, sw, sh];
                        self.search(frame, scaled, radius, radius)
                    })
            });
        match found {
            Some((r, c)) => {
                self.velocity = [r[0] - self.rect[0], r[1] - self.rect[1]];
                self.rect = r;
                Some((r, c))
            }
            None => None,
        }
    }
    /// Movement per frame of the last confirmed match, in frame pixels.
    pub fn velocity(&self) -> [f32; 2] {
        self.velocity
    }
    fn search(
        &self,
        frame: &RgbaImage,
        origin: [f32; 4],
        radius_x: i32,
        radius_y: i32,
    ) -> Option<([f32; 4], f32)> {
        let inside = |r: [f32; 4]| {
            r[0] >= 0.
                && r[1] >= 0.
                && r[0] + r[2] <= frame.width() as f32
                && r[1] + r[3] <= frame.height() as f32
        };
        let error = if inside(origin) {
            self.error(frame, origin, 1.)
        } else {
            1.
        };
        if error < 0.003 {
            return Some((origin, 1.));
        }
        // Candidates are ranked by match error plus a small distance cost, so
        // when the same text appears on several lines (an email repeated down a
        // page) the tracker stays on its own copy instead of jumping to another
        // identical one (BE6). The coarse grid keeps several promising spots and
        // each is refined before choosing, so a copy that happens to sit on the
        // grid can't beat the nearer copy that sits between grid points.
        let cost = |dx: i32, dy: i32| (dx as f32).hypot(dy as f32) * DISTANCE_COST;
        let fits = |dx: i32, dy: i32| {
            let r = [origin[0] + dx as f32, origin[1] + dy as f32];
            r[0] >= 0.
                && r[1] >= 0.
                && r[0] + origin[2] <= frame.width() as f32
                && r[1] + origin[3] <= frame.height() as f32
        };
        let at = |dx: i32, dy: i32| {
            [
                origin[0] + dx as f32,
                origin[1] + dy as f32,
                origin[2],
                origin[3],
            ]
        };
        let mut coarse: Vec<(f32, i32, i32)> = vec![(error, 0, 0)];
        for y in (-radius_y..=radius_y).step_by(6) {
            for x in (-radius_x..=radius_x).step_by(6) {
                if fits(x, y) {
                    coarse.push((self.error(frame, at(x, y), 0.6), x, y));
                }
            }
        }
        coarse.sort_by(|a, b| a.0.total_cmp(&b.0));
        let floor = coarse[0].0;
        let mut seeds: Vec<(i32, i32)> = vec![];
        for (e, x, y) in &coarse {
            if *e > floor + 0.3 || seeds.len() >= 8 {
                break;
            }
            if seeds
                .iter()
                .all(|(sx, sy)| (sx - x).abs() > 8 || (sy - y).abs() > 8)
            {
                seeds.push((*x, *y));
            }
        }
        // (score, error, rect) per refined seed.
        let mut refined: Vec<(f32, f32, [f32; 4])> = vec![];
        for (cx, cy) in seeds {
            let mut best = (f32::INFINITY, 1f32, at(cx, cy));
            for y in cy - 5..=cy + 5 {
                for x in cx - 5..=cx + 5 {
                    if !fits(x, y) {
                        continue;
                    }
                    let error = self.error(frame, at(x, y), best.1.min(0.2));
                    let score = error + cost(x, y);
                    if score < best.0 {
                        best = (score, error, at(x, y));
                    }
                }
            }
            refined.push(best);
        }
        refined.sort_by(|a, b| a.0.total_cmp(&b.0));
        let &best = refined.first()?;
        // The best error among all other spots, for the ambiguity check.
        let second = refined
            .iter()
            .skip(1)
            .map(|r| r.1)
            .chain(
                coarse
                    .iter()
                    .filter(|(_, x, y)| {
                        (origin[0] + *x as f32 - best.2[0]).abs() > 8.
                            || (origin[1] + *y as f32 - best.2[1]).abs() > 8.
                    })
                    .map(|c| c.0),
            )
            .fold(f32::INFINITY, f32::min);
        let mean = self.pixels.iter().sum::<f32>() / self.pixels.len() as f32;
        let variance =
            self.pixels.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / self.pixels.len() as f32;
        let error = best.1;
        if variance < 0.002 || error > 0.12 || second - error < 0.002 && error > 0.035 {
            return None;
        }
        Some((best.2, (1. - error * 4.).clamp(0., 1.)))
    }
}
/// The page's shared vertical scroll from `prev` to `next` in pixels
/// (positive: content moved down), or `None` when the frames don't show a
/// clear scroll. Rows are compared as coarse column profiles, so static strips
/// such as a title bar or taskbar don't outvote the scrolling content.
pub fn scroll_shift(prev: &RgbaImage, next: &RgbaImage, max: i32) -> Option<i32> {
    scroll_shift_near(prev, next, max, None)
}
/// [`scroll_shift`], for a page whose lines repeat: one line spacing can match
/// about as well as the true scroll, so among near-equal shifts this takes the
/// one closest to the recent movement `expected`.
pub fn scroll_shift_near(
    prev: &RgbaImage,
    next: &RgbaImage,
    max: i32,
    expected: Option<i32>,
) -> Option<i32> {
    page_motion(prev, next, max, expected).map(|m| m.shift)
}
/// What happened to the page between two frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    /// The page's vertical scroll, in pixels of these frames.
    pub shift: i32,
    /// Much of the page changed without scrolling: text moved or resized
    /// in place, as a page zoom does.
    pub reflow: bool,
}
/// A row must match this closely, summed over its column profile, to count
/// as the same row after the scroll.
const ROW_MATCH: f32 = 0.7;
/// The share of changed rows a real scroll explains. A page zoom moves every
/// row by a different amount, so one shift explains few of them.
const SCROLL_EXPLAINS: f32 = 0.55;
/// What a long one-frame jump must explain to be believed.
const FAR_JUMP_EXPLAINS: f32 = 0.7;
/// How closely a row must match for a far jump: scrolled rows are the same
/// pixels, while repeated or resized lines are only similar.
const EXACT_ROW: f32 = 0.35;
/// How closely a row must match to choose among nearby shifts.
const NEAR_ROW: f32 = 1.4;
/// A scene change: the frames' brightness histograms barely overlap.
const SCENE_CUT: f32 = 0.6;
/// Are these two frames from different scenes? Scrolling and zooming a page
/// keep its overall brightness mix, so they never count.
pub fn scene_cut(prev: &RgbaImage, next: &RgbaImage) -> bool {
    if prev.dimensions() != next.dimensions() {
        return true;
    }
    let histogram = |img: &RgbaImage| {
        let mut bins = [0f32; 16];
        let mut total = 0f32;
        for (_, _, p) in img.enumerate_pixels().step_by(3) {
            let luma = p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114;
            bins[(luma as usize / 16).min(15)] += 1.;
            total += 1.;
        }
        bins.map(|b| b / total.max(1.))
    };
    let (a, b) = (histogram(prev), histogram(next));
    a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f32>() > SCENE_CUT
}
/// Which rows differ between two frames.
pub fn changed_rows(prev: &RgbaImage, next: &RgbaImage) -> Vec<bool> {
    (0..prev.height().min(next.height()))
        .map(|y| {
            (0..prev.width().min(next.width())).step_by(4).any(|x| {
                let (p, q) = (prev.get_pixel(x, y), next.get_pixel(x, y));
                (0..3).any(|c| p[c].abs_diff(q[c]) > 24)
            })
        })
        .collect()
}
/// How the page moved from `prev` to `next`: see [`scroll_shift_near`].
pub fn page_motion(
    prev: &RgbaImage,
    next: &RgbaImage,
    max: i32,
    expected: Option<i32>,
) -> Option<Motion> {
    if prev.dimensions() != next.dimensions() || prev.height() < 16 {
        return None;
    }
    const BINS: usize = 48;
    let (w, h) = (prev.width() as usize, prev.height() as usize);
    let rows = |img: &RgbaImage| -> Vec<[f32; BINS]> {
        (0..h)
            .map(|y| {
                let mut row = [0f32; BINS];
                for (b, v) in row.iter_mut().enumerate() {
                    let (x0, x1) = (b * w / BINS, ((b + 1) * w / BINS).max(b * w / BINS + 1));
                    let mut sum = 0.;
                    for x in x0..x1 {
                        let p = img.get_pixel(x as u32, y as u32);
                        sum += p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114;
                    }
                    *v = sum / (x1 - x0) as f32 / 255.;
                }
                row
            })
            .collect()
    };
    let (a, b) = (rows(prev), rows(next));
    // Rows that change between the frames; a still frame has none. Only
    // these are scored, so static strips (title bar, taskbar) can't vote
    // for "no scroll".
    let changed: Vec<usize> = (0..h)
        .filter(|&y| a[y].iter().zip(&b[y]).any(|(p, q)| (p - q).abs() > 0.02))
        .collect();
    // Rows that change. A row revealed from under a fixed bar has nothing to
    // match, so only rows that came from a changing row count.
    let mut moved = vec![false; h];
    for &y in &changed {
        moved[y] = true;
    }
    // A handful of rows is a caret or a tooltip, not the page moving.
    if changed.len() < 6.max(h / 60) {
        return Some(Motion::default());
    }
    let max = max.min(h as i32 / 2);
    let score = |dy: i32| -> f32 {
        let mut total = 0.;
        let mut n = 0;
        for &y in &changed {
            let y = y as i32;
            let from = y - dy;
            if from < 0 || from >= h as i32 {
                continue;
            }
            let (p, q) = (&a[from as usize], &b[y as usize]);
            // Only rows with detail say anything about the scroll.
            if p.iter()
                .zip(&q[..])
                .all(|(x, z)| (x - p[0]).abs() < 0.01 && (z - q[0]).abs() < 0.01)
            {
                continue;
            }
            total += p
                .iter()
                .zip(&q[..])
                .map(|(x, z)| (x - z).abs())
                .sum::<f32>();
            n += 1;
        }
        if n < 4 {
            f32::INFINITY
        } else {
            total / n as f32
        }
    };
    // How many of the changed rows this one shift explains.
    let explains = |dy: i32, close: f32| -> f32 {
        let (mut matched, mut n) = (0, 0);
        for &y in &changed {
            let y = y as i32;
            let from = y - dy;
            if from < 0 || from >= h as i32 || !moved[from as usize] {
                continue;
            }
            let (p, q) = (&a[from as usize], &b[y as usize]);
            if p.iter()
                .zip(&q[..])
                .all(|(x, z)| (x - p[0]).abs() < 0.01 && (z - q[0]).abs() < 0.01)
            {
                continue;
            }
            n += 1;
            if p.iter()
                .zip(&q[..])
                .map(|(x, z)| (x - z).abs())
                .sum::<f32>()
                <= close
            {
                matched += 1;
            }
        }
        if n == 0 {
            0.
        } else {
            matched as f32 / n as f32
        }
    };
    let still = score(0);
    let scores: Vec<(i32, f32)> = (-max..=max).map(|dy| (dy, score(dy))).collect();
    let (mut best, mut error) = *scores.iter().min_by(|x, y| x.1.total_cmp(&y.1))?;
    if let (Some(near), true) = (expected, error.is_finite()) {
        let tie = error * 1.1 + 0.002;
        if let Some(pick) = scores
            .iter()
            .filter(|(_, s)| *s <= tie)
            .min_by_key(|(dy, _)| (dy - near).abs())
        {
            (best, error) = *pick;
        }
    }
    // Lines that repeat down a page can make a far shift match best. Prefer a
    // near one that also works; a far jump alone must explain nearly everything.
    let reach = expected.map_or(30, i32::abs).max(30) * 3;
    let mut close = ROW_MATCH;
    let far =
        best.abs() > reach && (expected.is_some() || explains(best, EXACT_ROW) < FAR_JUMP_EXPLAINS);
    if far || explains(best, ROW_MATCH) < SCROLL_EXPLAINS {
        // A fast flick blurs the picture, so rows match only loosely here.
        close = NEAR_ROW;
        // Among the shifts that work, the best match decides. The last speed
        // only breaks a tie: a scroll slows to a crawl right after a flick.
        let working: Vec<(i32, f32)> = scores
            .iter()
            .filter(|(dy, _)| *dy != 0 && dy.abs() <= reach)
            .filter(|(dy, _)| explains(*dy, NEAR_ROW) >= SCROLL_EXPLAINS)
            .copied()
            .collect();
        let lowest = working
            .iter()
            .map(|(_, s)| *s)
            .fold(f32::INFINITY, f32::min);
        let near = working
            .iter()
            .filter(|(_, s)| *s <= lowest * 1.1 + 0.002)
            .min_by_key(|(dy, _)| (dy - expected.unwrap_or(0)).abs());
        match near {
            Some(pick) => (best, error) = *pick,
            None if explains(best, EXACT_ROW) < FAR_JUMP_EXPLAINS => {
                return Some(Motion {
                    shift: 0,
                    reflow: changed.len() >= h / 6,
                });
            }
            None => (),
        }
    }
    // A clear winner only: much better than staying put, and a real scroll.
    let explained = explains(best, close);
    if best != 0
        && error.is_finite()
        && explained >= SCROLL_EXPLAINS
        && (error < still * 0.6 || explained > explains(0, close) + 0.3)
    {
        return Some(Motion {
            shift: best,
            reflow: false,
        });
    }
    Some(Motion {
        shift: 0,
        reflow: changed.len() >= h / 6,
    })
}
/// Match-error cost per pixel of distance from the expected position: 100 px
/// away costs as much as a 0.03 difference in match error.
const DISTANCE_COST: f32 = 0.0003;
/// A redaction rectangle grown on every side so recognized-text bounds that run
/// a little short never leave the first or last letters showing.
pub fn pad(r: [f32; 4], size: [u32; 2]) -> [f32; 4] {
    let grow = [2. + r[3] * 0.12, 2. + r[3] * 0.15];
    let x = (r[0] - grow[0]).max(0.);
    let y = (r[1] - grow[1]).max(0.);
    let right = (r[0] + r[2] + grow[0]).min(size[0] as f32);
    let bottom = (r[1] + r[3] + grow[1]).min(size[1] as f32);
    [x, y, (right - x).max(1.), (bottom - y).max(1.)]
}
fn sample(frame: &RgbaImage, r: [f32; 4], w: usize, h: usize) -> Vec<f32> {
    (0..w * h)
        .map(|i| {
            let x = (r[0] + (i % w) as f32 * r[2] / w as f32)
                .clamp(0., frame.width().saturating_sub(1) as f32) as u32;
            let y = (r[1] + (i / w) as f32 * r[3] / h as f32)
                .clamp(0., frame.height().saturating_sub(1) as f32) as u32;
            let p = frame.get_pixel(x, y);
            (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114) / 255.
        })
        .collect()
}
pub fn cover(a: [f32; 4], b: [f32; 4], size: [u32; 2]) -> [f32; 4] {
    let x = (a[0].min(b[0]) - 3.).max(0.);
    let y = (a[1].min(b[1]) - 3.).max(0.);
    let right = (a[0] + a[2]).max(b[0] + b[2]) + 3.;
    let bottom = (a[1] + a[3]).max(b[1] + b[3]) + 3.;
    [
        x,
        y,
        right.min(size[0] as f32) - x,
        bottom.min(size[1] as f32) - y,
    ]
}
/// How long a lost region keeps its last confirmed rectangle before the redaction ends.
pub const LOST_GRACE_MS: u64 = 1000;
/// Consecutive OCR passes that must miss a region's text before the redaction ends.
pub const OCR_MISS_LIMIT: u32 = 3;

/// Follows one detected region through a scan and writes its redaction path.
///
/// A lost region never grows to the full frame: it holds its last confirmed
/// rectangle for [`LOST_GRACE_MS`], then the redaction ends. Every held interval
/// is recorded in `needs_review` so the user can check it before export.
#[derive(Clone, Debug)]
pub struct Follow {
    /// Scan time (ms) at which the path's node 0 sits.
    pub start: u64,
    /// Last confirmed rectangle in output coordinates.
    pub last: [f32; 4],
    /// Scan time at which the redaction ends, once it has.
    pub ended: Option<u64>,
    /// Intervals (scan ms, start..end) that use held, unconfirmed coverage.
    pub needs_review: Vec<(u64, u64)>,
    lost_since: Option<u64>,
    ocr_misses: u32,
    first_miss: Option<u64>,
    /// The lost region has scrolled out of the frame; end it now.
    gone: bool,
    /// The page has moved since the region was lost, so its old place is
    /// known to be wrong.
    moved_while_lost: bool,
}
impl Follow {
    /// A redaction the recall pass adds: from `first` until `until`, placed
    /// from where the value was read and the page's scroll, so all of it is
    /// flagged for review.
    pub fn recalled(first: u64, last: [f32; 4], until: u64) -> Self {
        Self {
            start: first,
            last,
            ended: Some(until),
            needs_review: vec![(first, until)],
            lost_since: None,
            ocr_misses: 0,
            first_miss: None,
            gone: false,
            moved_while_lost: false,
        }
    }
    /// Starts following at `found_at`. When the scan only noticed the region at
    /// an OCR pass after `begins`, the gap before it is covered with the detected
    /// rectangle (padded) and flagged for review, never with the full frame.
    pub fn new(
        path: &mut crate::video::motion_path::Path,
        begins: u64,
        found_at: u64,
        rect: [f32; 4],
        size: [u32; 2],
    ) -> Self {
        Self::with_history(path, begins, found_at, rect, size, &[])
    }
    /// Like [`Follow::new`], with the region already followed back through the
    /// frames between `begins` and `found_at` (`earlier`, ascending scan ms and
    /// the rectangle found there, if any). Content that scrolled in between two
    /// OCR passes is then covered where it actually was, frame by frame (BE6).
    /// Frames where it could not be followed hold the nearest known rectangle
    /// and are flagged for review.
    pub fn with_history(
        path: &mut crate::video::motion_path::Path,
        begins: u64,
        found_at: u64,
        rect: [f32; 4],
        size: [u32; 2],
        earlier: &[(u64, Option<[f32; 4]>)],
    ) -> Self {
        use crate::video::motion_path::Node;
        let earlier: Vec<_> = earlier
            .iter()
            .filter(|(ms, _)| (begins..found_at).contains(ms))
            .copied()
            .collect();
        if !earlier.is_empty() && earlier.iter().any(|(_, r)| r.is_some()) {
            let mut follow = Self::with_history(path, found_at, found_at, rect, size, &[]);
            follow.start = begins;
            path.nodes.clear();
            let mut known = earlier.iter().find_map(|(_, r)| *r).unwrap_or(rect);
            if earlier[0].0 > begins {
                follow.needs_review.push((begins, earlier[0].0));
            }
            let push =
                |path: &mut crate::video::motion_path::Path, at: u64, r: [f32; 4], c: f32| {
                    path.push(Node {
                        at: at - begins,
                        rect: cover(r, r, size),
                        visible: true,
                        sprite: 0,
                        confidence: c,
                    });
                };
            for (i, (ms, r)) in earlier.iter().enumerate() {
                let at = if i == 0 { begins } else { *ms };
                match r {
                    Some(r) => {
                        known = *r;
                        push(path, at, *r, 0.5);
                    }
                    None => {
                        push(path, at, cover(known, known, size), 0.);
                        let next = earlier.get(i + 1).map_or(found_at, |e| e.0);
                        follow.needs_review.push((*ms, next));
                    }
                }
            }
            push(path, found_at, rect, 1.);
            follow.merge_review();
            return follow;
        }
        let mut follow = Self {
            start: begins,
            last: rect,
            ended: None,
            needs_review: vec![],
            lost_since: None,
            ocr_misses: 0,
            first_miss: None,
            gone: false,
            moved_while_lost: false,
        };
        path.nodes.clear();
        if begins < found_at {
            path.nodes.push(Node {
                at: 0,
                rect: cover(rect, rect, size),
                visible: true,
                sprite: 0,
                confidence: 0.,
            });
            follow.needs_review.push((begins, found_at));
        }
        path.nodes.push(Node {
            at: found_at - begins,
            rect,
            visible: true,
            sprite: 0,
            confidence: 1.,
        });
        follow
    }
    pub fn active(&self) -> bool {
        self.ended.is_none()
    }
    /// While the region is lost, its held cover moves with the page's scroll
    /// `d` (output pixels) instead of staying where the text was, and the
    /// redaction ends once the region has scrolled out of the frame.
    pub fn follow_scroll(&mut self, d: [f32; 2], size: [u32; 2]) {
        if self.ended.is_some() {
            return;
        }
        self.last[0] += d[0];
        self.last[1] += d[1];
        self.moved_while_lost = true;
        let [x, y, w, h] = self.last;
        self.gone = x + w <= 0. || y + h <= 0. || x >= size[0] as f32 || y >= size[1] as f32;
    }
    /// Where the region is expected now, in output coordinates.
    pub fn expected(&self) -> [f32; 4] {
        self.last
    }
    /// Records the tracker's result for the frame at `ms` (output coordinates).
    pub fn observe(
        &mut self,
        path: &mut crate::video::motion_path::Path,
        ms: u64,
        found: Option<([f32; 4], f32)>,
        size: [u32; 2],
    ) {
        use crate::video::motion_path::Node;
        if self.ended.is_some() || ms < self.start {
            return;
        }
        let (rect, confidence) = match found {
            Some((rect, confidence)) => {
                self.moved_while_lost = false;
                if let Some(since) = self.lost_since.take() {
                    self.needs_review.push((since, ms));
                }
                self.last = rect;
                (rect, confidence)
            }
            None => {
                let since = *self.lost_since.get_or_insert(ms);
                if self.gone {
                    self.end(path, ms, size);
                    return;
                }
                if ms.saturating_sub(since) >= LOST_GRACE_MS {
                    self.end(path, since + LOST_GRACE_MS, size);
                    return;
                }
                // Its last place, the same size as ever. Where the value went is
                // worked out afterwards from the page's scroll (`Scanner::recall`).
                (self.last, 0.)
            }
        };
        if let Some(n) = path.nodes.last_mut() {
            n.confidence = n.confidence.min(confidence);
        }
        // Export reads the same frames at the same times as the scan, so each
        // node covers its own frame. Never interpolate a mask off the target.
        path.push(Node {
            at: ms - self.start,
            rect: cover(rect, rect, size),
            visible: true,
            sprite: 0,
            confidence,
        });
    }
    /// An OCR pass at `ms` found (or did not find) this region's text.
    pub fn ocr(
        &mut self,
        path: &mut crate::video::motion_path::Path,
        ms: u64,
        seen: bool,
        size: [u32; 2],
    ) {
        if self.ended.is_some() {
            return;
        }
        if seen {
            self.ocr_misses = 0;
            self.first_miss = None;
            return;
        }
        self.ocr_misses += 1;
        let first = *self.first_miss.get_or_insert(ms);
        // Lost, the page has moved and OCR agrees it isn't there: no box stays
        // behind on empty lines. A later reading starts a new one.
        if self.ocr_misses >= OCR_MISS_LIMIT || (self.moved_while_lost && self.ocr_misses >= 2) {
            // Coverage continues to the confirming pass; the unconfirmed tail is flagged.
            self.needs_review.push((first, ms));
            self.end(path, ms, size);
        }
    }
    /// OCR re-detected the text at `ms` with a fresh rectangle.
    pub fn confirm(
        &mut self,
        path: &mut crate::video::motion_path::Path,
        ms: u64,
        rect: [f32; 4],
        size: [u32; 2],
    ) {
        if self.ended.is_some() {
            return;
        }
        if let Some(since) = self.lost_since.take() {
            self.needs_review.push((since, ms));
        }
        self.ocr_misses = 0;
        self.first_miss = None;
        self.last = rect;
        // The same shape `observe` stores, so a confirmation doesn't change
        // the box.
        if let Some(n) = path.nodes.last_mut() {
            n.rect = cover(rect, rect, size);
            n.confidence = 1.;
        }
    }
    /// The scene changed: nothing tracked so far is on screen any more.
    pub fn cut(&mut self, path: &mut crate::video::motion_path::Path, ms: u64, size: [u32; 2]) {
        if self.ended.is_none() {
            self.end(path, ms, size);
        }
    }
    /// The page reflowed in place, so a lost region's old spot is known wrong.
    pub fn reflowed(&mut self) {
        if self.lost_since.is_some() {
            self.moved_while_lost = true;
        }
    }
    /// Starts the redaction earlier: `earlier` holds positions before `start`
    /// (ascending scan ms). They are unconfirmed, so they are flagged for review.
    pub fn extend_back(
        &mut self,
        path: &mut crate::video::motion_path::Path,
        earlier: &[(u64, [f32; 4])],
        size: [u32; 2],
    ) {
        use crate::video::motion_path::Node;
        let Some(&(first, _)) = earlier.first().filter(|(ms, _)| *ms < self.start) else {
            return;
        };
        let delta = self.start - first;
        for n in &mut path.nodes {
            n.at += delta;
        }
        let nodes: Vec<Node> = earlier
            .iter()
            .map(|(ms, r)| Node {
                at: ms - first,
                rect: cover(*r, *r, size),
                visible: true,
                sprite: 0,
                confidence: 0.,
            })
            .collect();
        path.nodes.splice(0..0, nodes);
        self.needs_review.push((first, self.start));
        self.start = first;
        self.merge_review();
    }
    /// Continues the redaction after its end: `later` holds positions from the
    /// page's scroll alone, so they are flagged for review.
    pub fn extend_forward(
        &mut self,
        path: &mut crate::video::motion_path::Path,
        later: &[(u64, [f32; 4])],
        size: [u32; 2],
    ) {
        use crate::video::motion_path::Node;
        let (Some(&(first, _)), Some(&(last, _))) = (later.first(), later.last()) else {
            return;
        };
        for (ms, r) in later {
            path.push(Node {
                at: ms - self.start,
                rect: cover(*r, *r, size),
                visible: true,
                sprite: 0,
                confidence: 0.,
            });
        }
        self.ended = Some(last + 1);
        self.needs_review.push((first, last + 1));
        self.merge_review();
    }
    /// Ends the redaction sooner than it ended, because something replaced it.
    pub fn shorten(&mut self, path: &mut crate::video::motion_path::Path, at: u64) {
        if at <= self.start || self.ended.is_some_and(|e| at >= e) {
            return;
        }
        path.nodes.retain(|n| n.at < at - self.start);
        self.ended = Some(at);
        self.needs_review.retain_mut(|r| {
            r.1 = r.1.min(at);
            r.0 < r.1
        });
    }
    /// Closes the redaction at the end of the scan if it is still running.
    pub fn finish(&mut self, end: u64) {
        if let Some(since) = self.lost_since.take() {
            self.needs_review.push((since, end));
        }
        if self.ended.is_none() {
            self.ended = Some(end);
        }
        self.merge_review();
    }
    fn end(&mut self, path: &mut crate::video::motion_path::Path, at: u64, size: [u32; 2]) {
        use crate::video::motion_path::Node;
        let at = at.max(self.start + 1);
        if let Some(since) = self.lost_since.take() {
            self.needs_review.push((since, at));
        }
        path.nodes.retain(|n| n.at < at - self.start);
        if path.nodes.is_empty() {
            path.nodes.push(Node {
                at: 0,
                rect: cover(self.last, self.last, size),
                visible: true,
                sprite: 0,
                confidence: 0.,
            });
        }
        self.ended = Some(at);
        self.merge_review();
    }
    fn merge_review(&mut self) {
        self.needs_review.sort_unstable();
        let mut merged: Vec<(u64, u64)> = vec![];
        for (a, b) in self.needs_review.drain(..) {
            match merged.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => merged.push((a, b.max(a))),
            }
        }
        self.needs_review = merged;
    }
}
/// A tracked redaction that still needs review before export.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub id: u64,
    /// What was found, such as "Email" (never the found text itself).
    pub kind: String,
    /// Where it is on the timeline, in ms.
    pub start: u64,
    pub end: u64,
    /// Ranges (timeline ms) covered by held positions where tracking was lost.
    pub uncertain: Vec<(u64, u64)>,
}
/// Tracked redactions not yet reviewed, in timeline order.
pub fn pending_reviews(doc: &crate::Document) -> Vec<Pending> {
    let Some(project) = doc.video.as_ref() else {
        return vec![];
    };
    let mut out: Vec<Pending> = project
        .objects
        .iter()
        .filter_map(|(id, t)| {
            let path = t.motion_path.as_ref()?;
            if path.purpose != crate::video::motion_path::Purpose::Redaction || path.reviewed {
                return None;
            }
            let at = |n: u64| (t.start + n.saturating_sub(path.offset_ms)).min(t.end);
            let mut uncertain: Vec<(u64, u64)> = vec![];
            for (i, n) in path.nodes.iter().enumerate() {
                if n.confidence > 0. {
                    continue;
                }
                let from = at(n.at);
                let to = path.nodes.get(i + 1).map_or(t.end, |next| at(next.at));
                match uncertain.last_mut() {
                    Some(last) if last.1 >= from => last.1 = last.1.max(to),
                    _ => uncertain.push((from, to.max(from))),
                }
            }
            let kind = t
                .label
                .split(" \u{b7} ")
                .next()
                .filter(|k| !k.trim().is_empty())
                .unwrap_or("Redaction")
                .to_owned();
            Some(Pending {
                id: *id,
                kind,
                start: t.start,
                end: t.end,
                uncertain,
            })
        })
        .collect();
    out.sort_by_key(|p| (p.start, p.id));
    out
}
/// One moment to look at while reviewing: a flagged range of a tracked
/// redaction, or the whole redaction when nothing in it was flagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stop {
    pub id: u64,
    pub start: u64,
    pub end: u64,
}
/// Every moment to review, across all redactions, in time order.
pub fn review_stops(doc: &crate::Document) -> Vec<Stop> {
    let mut stops: Vec<Stop> = pending_reviews(doc)
        .into_iter()
        .flat_map(|p| {
            let ranges = if p.uncertain.is_empty() {
                vec![(p.start, p.end)]
            } else {
                p.uncertain
            };
            ranges.into_iter().map(move |(start, end)| Stop {
                id: p.id,
                start,
                end,
            })
        })
        .collect();
    stops.sort_by_key(|s| (s.start, s.id));
    stops
}
/// Where to go once redaction `id` is marked reviewed: the next moment of any
/// other redaction, or the earliest one left.
pub fn next_stop_after(stops: &[Stop], id: u64, after_ms: u64) -> Option<Stop> {
    let others = || stops.iter().filter(|s| s.id != id);
    others()
        .find(|s| s.start >= after_ms)
        .or_else(|| others().next())
        .copied()
}
/// Marks every tracked redaction reviewed (Export > Mark all as reviewed).
/// Returns how many changed. Editing a redaction later clears it again.
pub fn mark_all_reviewed(project: &mut crate::video::editor::Project) -> usize {
    let mut changed = 0;
    for t in project.objects.values_mut() {
        if let Some(path) = t.motion_path.as_mut()
            && path.purpose == crate::video::motion_path::Purpose::Redaction
            && !path.reviewed
        {
            std::sync::Arc::make_mut(path).reviewed = true;
            changed += 1;
        }
    }
    changed
}
/// Export uses only paths that have been explicitly reviewed after edits.
pub fn require_review(doc: &crate::Document) -> crate::Result<()> {
    if doc.video.as_ref().is_some_and(|p| {
        p.objects.values().any(|t| {
            t.motion_path.as_ref().is_some_and(|path| {
                path.purpose == crate::video::motion_path::Purpose::Redaction && !path.reviewed
            })
        })
    }) {
        return Err(crate::Error::Invalid("Some tracked redactions need a look before export. Review each one and mark it reviewed in its Properties.ing.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follows_translated_detail_and_rejects_missing_content() {
        let mut a = RgbaImage::from_pixel(240, 120, image::Rgba([255; 4]));
        for y in 30..50 {
            for x in 50..110 {
                a.put_pixel(
                    x,
                    y,
                    image::Rgba([((x * 17 + y * 31) % 211) as u8, 0, 0, 255]),
                );
            }
        }
        let patch = Patch::new(&a, [50., 30., 60., 20.]);
        let mut b = RgbaImage::from_pixel(240, 120, image::Rgba([255; 4]));
        for y in 30..50 {
            for x in 50..110 {
                b.put_pixel(x + 18, y + 12, *a.get_pixel(x, y));
            }
        }
        let (r, c) = patch.locate(&b, 48).unwrap();
        assert_eq!(r, [68., 42., 60., 20.]);
        assert!(c > 0.9);
        assert!(
            patch
                .locate(&RgbaImage::from_pixel(240, 120, image::Rgba([255; 4])), 48)
                .is_none()
        );
    }

    fn path() -> crate::video::motion_path::Path {
        crate::video::motion_path::Path {
            purpose: crate::video::motion_path::Purpose::Redaction,
            reference: [100., 100., 80., 20.],
            nodes: vec![],
            sprites: vec![],
            smooth: false,
            enabled: true,
            offset_ms: 0,
            reviewed: false,
            reduction: Default::default(),
            inputs: vec![],
            recorded: None,
        }
    }
    const SIZE: [u32; 2] = [1920, 1080];
    const FULL: f32 = 1920. * 1080.;
    fn area(r: [f32; 4]) -> f32 {
        r[2] * r[3]
    }

    #[test]
    fn lost_region_holds_last_rect_then_ends_without_full_frame() {
        let mut p = path();
        let rect = [100., 100., 80., 20.];
        let mut f = Follow::new(&mut p, 0, 0, rect, SIZE);
        for ms in (33..=500).step_by(33) {
            f.observe(&mut p, ms, Some((rect, 0.95)), SIZE);
        }
        for ms in (533..=3000).step_by(33) {
            f.observe(&mut p, ms, None, SIZE);
        }
        let end = f.ended.expect("redaction ends after the grace period");
        assert!((1500..=1600).contains(&end), "ended at {end}");
        assert!(p.nodes.iter().all(|n| area(n.rect) < FULL / 100.));
        assert!(p.nodes.iter().all(|n| n.at < end));
        assert_eq!(f.needs_review.len(), 1);
        assert_eq!(f.needs_review[0].0, 533);
        p.validate().unwrap();
    }

    #[test]
    fn brief_loss_is_flagged_and_tracking_resumes() {
        let mut p = path();
        let rect = [100., 100., 80., 20.];
        let moved = [140., 100., 80., 20.];
        let mut f = Follow::new(&mut p, 0, 0, rect, SIZE);
        f.observe(&mut p, 33, None, SIZE);
        f.observe(&mut p, 66, None, SIZE);
        f.observe(&mut p, 99, Some((moved, 0.9)), SIZE);
        f.finish(5000);
        assert_eq!(f.ended, Some(5000));
        assert_eq!(f.needs_review, vec![(33, 99)]);
        assert_eq!(f.last, moved);
        assert!(p.nodes.iter().all(|n| area(n.rect) < FULL / 100.));
    }

    #[test]
    fn repeated_ocr_misses_end_the_redaction() {
        let mut p = path();
        let rect = [100., 100., 80., 20.];
        let mut f = Follow::new(&mut p, 0, 0, rect, SIZE);
        for (i, ms) in (0..4000).step_by(33).enumerate() {
            f.observe(&mut p, ms, Some((rect, 0.9)), SIZE);
            if i % 15 == 0 && ms >= 1000 {
                f.ocr(&mut p, ms, false, SIZE);
            }
        }
        let end = f.ended.expect("ends after three missed OCR passes");
        assert!(end < 3000, "ended at {end}");
        assert!(!f.needs_review.is_empty());
    }

    fn detail(frame: &mut RgbaImage, at: [u32; 2]) {
        for y in 0..20 {
            for x in 0..60 {
                frame.put_pixel(
                    at[0] + x,
                    at[1] + y,
                    image::Rgba([((x * 17 + y * 31) % 211) as u8, 0, 0, 255]),
                );
            }
        }
    }

    /// A text-like page: lines of detail every 40 px, a static title bar
    /// and taskbar, scrolled by `offset`.
    fn page(offset: i32) -> RgbaImage {
        let mut f = RgbaImage::from_pixel(480, 270, image::Rgba([30, 30, 30, 255]));
        for y in 0..270i32 {
            let doc = y - 20 + offset;
            for x in 0..480i32 {
                let ink = if !(20..250).contains(&y) {
                    // Static chrome.
                    if (x / 9 + y / 5) % 3 == 0 { 200 } else { 60 }
                } else if doc.rem_euclid(40) < 12
                    && x < 40 + (doc.div_euclid(40) * 97).rem_euclid(400)
                {
                    (((x * 7 + doc * 13) % 97) * 2 + 40) as u8 as i32
                } else {
                    30
                };
                f.put_pixel(
                    x as u32,
                    y as u32,
                    image::Rgba([ink as u8, ink as u8, ink as u8, 255]),
                );
            }
        }
        f
    }

    /// BJ: the page's shared scroll is found for slow and fast flicks in
    /// both directions, and a still frame reads as no scroll.
    #[test]
    fn page_scroll_is_measured_despite_static_chrome() {
        assert_eq!(scroll_shift(&page(0), &page(0), 120), Some(0));
        for d in [3, 17, 46, 95, -8, -60, -110] {
            // Content moving up the screen means a negative shift.
            assert_eq!(
                scroll_shift(&page(0), &page(d), 120),
                Some(-d),
                "scrolled {d}"
            );
        }
    }

    /// A page zoom resizes the text in place: not a scroll, but a reflow.
    #[test]
    fn a_page_zoom_is_a_reflow_and_not_a_scroll() {
        let page = |zoom: f32| {
            let mut f = RgbaImage::from_pixel(480, 270, image::Rgba([30, 30, 30, 255]));
            for y in 0..270u32 {
                for x in 0..480u32 {
                    let (dx, dy) = (x as f32 / zoom, y as f32 / zoom);
                    let line = (dy as u32).rem_euclid(26) < 9;
                    let ink = line && (dx as u32 / 3).is_multiple_of(2) && dx < 400. - dy / 2.;
                    if ink {
                        f.put_pixel(x, y, image::Rgba([225, 225, 225, 255]));
                    }
                }
            }
            f
        };
        let m = page_motion(&page(1.0), &page(1.21), 120, None).unwrap();
        assert_eq!(m.shift, 0, "{m:?}");
        assert!(m.reflow, "{m:?}");
    }
    /// BL: a few changed rows on a settled page used to read as a long jump.
    #[test]
    fn a_few_changed_rows_are_not_a_scroll() {
        let a = page(0);
        let mut b = a.clone();
        for y in 200..204 {
            for x in 300..340 {
                b.put_pixel(x, y, image::Rgba([240, 240, 240, 255]));
            }
        }
        assert_eq!(page_motion(&a, &b, 120, None).unwrap(), Motion::default());
    }
    #[test]
    fn only_a_different_scene_is_a_cut() {
        let dark = page(0);
        assert!(!scene_cut(&dark, &page(46)));
        let bright = RgbaImage::from_fn(480, 270, |x, _| {
            let v = (x * 255 / 480) as u8;
            image::Rgba([v, v, v, 255])
        });
        assert!(scene_cut(&dark, &bright));
    }
    /// BL: a confirmation stores the same box shape as tracking, so a still
    /// value doesn't pulse each time OCR reads it again.
    #[test]
    fn a_confirmation_keeps_the_tracked_box_shape() {
        let mut p = path();
        let rect = [100., 100., 80., 20.];
        let mut f = Follow::new(&mut p, 0, 0, rect, SIZE);
        for ms in (33..=132).step_by(33) {
            f.observe(&mut p, ms, Some((rect, 0.9)), SIZE);
        }
        let before = p.nodes.last().unwrap().rect;
        f.confirm(&mut p, 132, rect, SIZE);
        assert_eq!(p.nodes.last().unwrap().rect, before);
    }

    /// BJ: with the page's scroll as a hint, a patch follows its own line
    /// through a big jump even when the same text sits on a nearby line.
    #[test]
    fn scroll_hint_keeps_each_copy_on_its_own_line_through_big_jumps() {
        let mut first = RgbaImage::from_pixel(320, 900, image::Rgba([255; 4]));
        detail(&mut first, [100, 300]);
        detail(&mut first, [100, 380]);
        let mut patch = Patch::new(&first, [100., 380., 60., 20.]);
        // The page jumps 150 px up; the copy now sits 70 px above the old spot,
        // closer than our own line, which moved 150 px.
        let mut next = RgbaImage::from_pixel(320, 900, image::Rgba([255; 4]));
        detail(&mut next, [100, 150]);
        detail(&mut next, [100, 230]);
        let (r, _) = patch.track_hinted(&next, 72, Some([0., -150.])).unwrap();
        assert_eq!(r, [100., 230., 60., 20.]);
    }

    /// BJ: a lost region's cover moves with the scroll instead of staying on
    /// the old spot, and ends as soon as it has scrolled out of the frame.
    #[test]
    fn lost_cover_follows_the_scroll_and_ends_off_screen() {
        let mut p = path();
        let rect = [100., 200., 80., 20.];
        let mut f = Follow::new(&mut p, 0, 0, rect, SIZE);
        f.observe(&mut p, 33, Some((rect, 0.9)), SIZE);
        f.observe(&mut p, 66, None, SIZE);
        f.follow_scroll([0., -150.], SIZE);
        f.observe(&mut p, 99, None, SIZE);
        let last = p.nodes.last().unwrap().rect;
        assert!(
            last[1] < 60.,
            "the held cover moved up with the page: {last:?}"
        );
        assert!(f.active());
        f.follow_scroll([0., -150.], SIZE);
        f.observe(&mut p, 132, None, SIZE);
        assert_eq!(
            f.ended,
            Some(132),
            "ends once off screen, not after a second"
        );
        assert!(!f.needs_review.is_empty());
    }

    /// BL: export reads the frames the scan read, so a box stays the size of
    /// its value however fast the page moves.
    #[test]
    fn each_frame_covers_its_own_position_at_one_size() {
        let mut p = path();
        let mut f = Follow::new(&mut p, 0, 0, [100., 900., 80., 20.], SIZE);
        let mut ys = vec![900f32];
        for (i, step) in [20, 56, 26, 70, 18, 90, 30]
            .into_iter()
            .cycle()
            .take(14)
            .enumerate()
        {
            let y = ys.last().unwrap() - step as f32;
            ys.push(y);
            f.observe(
                &mut p,
                (i as u64 + 1) * 33,
                Some(([100., y, 80., 20.], 0.9)),
                SIZE,
            );
        }
        for (k, y) in ys.iter().enumerate().skip(1) {
            let r = p.nodes[k].rect;
            assert!(r[1] <= *y && r[1] + r[3] >= y + 20., "frame {k}: {r:?}");
            assert_eq!(r[3], 26., "frame {k}: {r:?}");
        }
    }

    #[test]
    fn fast_scrolling_beyond_the_search_window_stays_tracked() {
        let mut first = RgbaImage::from_pixel(320, 900, image::Rgba([255; 4]));
        detail(&mut first, [100, 40]);
        let mut patch = Patch::new(&first, [100., 40., 60., 20.]);
        // 110 px per frame is beyond the 72 px window the scan uses.
        for (i, y) in [150u32, 260, 370, 480, 590].into_iter().enumerate() {
            let mut frame = RgbaImage::from_pixel(320, 900, image::Rgba([255; 4]));
            detail(&mut frame, [100, y]);
            let (r, _) = patch
                .track(&frame, 72)
                .unwrap_or_else(|| panic!("lost at frame {i}"));
            assert_eq!(r, [100., y as f32, 60., 20.]);
        }
        assert_eq!(patch.velocity(), [0., 110.]);
    }

    #[test]
    fn identical_text_on_two_lines_keeps_each_tracker_on_its_own_copy() {
        // The same email on two lines 50 px apart; follow the lower one while
        // the page scrolls up by amounts that don't land on the search grid.
        let page = |top: u32| {
            let mut f = RgbaImage::from_pixel(320, 600, image::Rgba([255; 4]));
            detail(&mut f, [100, top]);
            detail(&mut f, [100, top + 50]);
            f
        };
        let mut lower = Patch::new(&page(200), [100., 250., 60., 20.]);
        let mut upper = Patch::new(&page(200), [100., 200., 60., 20.]);
        for top in [190u32, 177, 163, 150, 139] {
            let frame = page(top);
            let (r, _) = lower.track(&frame, 72).expect("lower copy followed");
            assert_eq!(
                r[1],
                (top + 50) as f32,
                "lower tracker jumped at {top}: {r:?}"
            );
            let (r, _) = upper.track(&frame, 72).expect("upper copy followed");
            assert_eq!(r[1], top as f32, "upper tracker jumped at {top}");
        }
    }

    #[test]
    fn regions_found_late_are_covered_where_they_were_in_earlier_frames() {
        let mut p = path();
        // Found by OCR at 500 ms; followed back through frames at 0..500 ms
        // while scrolling up 30 px per frame; one frame could not be followed.
        let earlier = [
            (0, Some([100., 400., 80., 20.])),
            (100, Some([100., 370., 80., 20.])),
            (200, None),
            (300, Some([100., 310., 80., 20.])),
            (400, Some([100., 280., 80., 20.])),
        ];
        let f = Follow::with_history(&mut p, 0, 500, [100., 250., 80., 20.], SIZE, &earlier);
        assert_eq!(f.start, 0);
        let at = |ms| p.nodes.iter().find(|n| n.at == ms).unwrap().rect;
        // Each node covers the position in its own frame.
        assert!(
            at(0)[1] <= 400. && at(0)[1] + at(0)[3] >= 420.,
            "{:?}",
            at(0)
        );
        assert!(at(300)[1] <= 310. && at(300)[1] + at(300)[3] >= 330.);
        let last = p.nodes.last().unwrap().rect;
        assert!(last[1] <= 250. && last[1] + last[3] >= 270., "{last:?}");
        // The frame that couldn't be followed is flagged for review.
        assert_eq!(f.needs_review, vec![(200, 300)]);
        p.validate().unwrap();
    }

    #[test]
    fn padding_grows_the_cover_without_leaving_the_frame() {
        let r = pad([10., 10., 100., 20.], SIZE);
        assert!(r[0] < 10. && r[1] < 10. && r[2] > 100. && r[3] > 20.);
        let edge = pad([0., 0., 50., 20.], SIZE);
        assert_eq!([edge[0], edge[1]], [0., 0.]);
    }

    #[test]
    fn a_lost_cover_keeps_its_size_and_never_stretches() {
        let mut p = path();
        let mut f = Follow::new(&mut p, 0, 0, [100., 100., 80., 20.], SIZE);
        f.observe(&mut p, 33, Some(([100., 130., 80., 20.], 0.9)), SIZE);
        f.observe(&mut p, 66, Some(([100., 160., 80., 20.], 0.9)), SIZE);
        f.observe(&mut p, 99, None, SIZE);
        let held = p.nodes.last().unwrap().rect;
        assert!(held[1] <= 160. && held[1] + held[3] >= 180., "{held:?}");
        for ms in (132..=900).step_by(33) {
            f.observe(&mut p, ms, None, SIZE);
        }
        assert!(p.nodes.iter().all(|n| n.rect[3] <= 20. + 6.));
        assert_eq!(f.needs_review, vec![]);
        f.finish(1000);
        assert_eq!(f.needs_review, vec![(99, 1000)]);
    }

    #[test]
    fn late_detection_backfills_with_detected_rect_not_full_frame() {
        let mut p = path();
        let rect = [300., 200., 120., 24.];
        let f = Follow::new(&mut p, 1000, 1400, rect, SIZE);
        assert_eq!(p.nodes[0].at, 0);
        assert!(area(p.nodes[0].rect) < FULL / 100.);
        assert_eq!(p.nodes[0].confidence, 0.);
        assert_eq!(f.needs_review, vec![(1000, 1400)]);
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::*;
    fn frame(rect: [f32; 4]) -> RgbaImage {
        RgbaImage::from_fn(640, 360, |x, y| {
            let u = (x as f32 - rect[0]) / rect[2];
            let v = (y as f32 - rect[1]) / rect[3];
            let ink = (0. ..1.).contains(&u)
                && (0. ..1.).contains(&v)
                && ((u * 7.).floor() as i32 + (v * 3.).floor() as i32) % 2 == 0;
            let g = if ink { 20 } else { 235 };
            image::Rgba([g, g, g, 255])
        })
    }
    #[test]
    fn zoomed_and_panned_text_stays_tracked() {
        let rect = [200., 150., 120., 40.];
        let mut patch = Patch::new(&frame(rect), rect);
        // The page zooms in 25% around the text.
        let zoomed = [185., 145., 150., 50.];
        let (found, _) = patch.track(&frame(zoomed), 72).expect("zoomed text found");
        assert!((found[2] - 150.).abs() < 12., "{found:?}");
        assert!((found[0] - 185.).abs() < 12., "{found:?}");
        // Then the page pans sideways farther than the local window.
        let mut patch = Patch::new(&frame(rect), rect);
        let panned = [340., 150., 120., 40.];
        let (found, _) = patch.track(&frame(panned), 40).expect("panned text found");
        assert!((found[0] - 340.).abs() < 6., "{found:?}");
    }
}
