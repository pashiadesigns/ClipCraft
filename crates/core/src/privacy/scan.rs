//! The video privacy scan, independent of how frames and text are read:
//! the desktop app feeds decoded frames and Windows OCR, tests feed synthetic
//! ones. Forward tracking, then a known-value sweep and a backward pass.
use super::tracking::{Follow, Patch, changed_rows, page_motion, scene_cut};
use super::{Options, Word, detect, stable};
use crate::video::motion_path::{Path, Purpose};
use image::RgbaImage;

/// Findings tracked separately, before the sweep adds its extra copies.
const MAX_FOUND: usize = 96;
const MAX_TOTAL: usize = 192;
const TOO_MANY: &str = "Too many tracked regions. Scan one clip or fewer categories.";
/// A fragment is at least this long; shorter pieces match ordinary words.
const MIN_FRAGMENT: usize = 4;
/// The backward pass stops where the page sat still this long: OCR read
/// those frames properly, so the value was not there.
const STILL_MS: u64 = 500;
/// Reflowing frames read back to back. Text that keeps changing in place, such
/// as a video playing on the page, returns to the normal rate after this.
const REFLOW_FRAMES: u32 = 12;

/// Which parts of a frame hold any detail, coarsely. A box the tracker wasn't
/// sure of, over a part with none, is covering nothing.
#[derive(PartialEq)]
struct Ink {
    columns: u32,
    rows: u32,
    bits: Vec<u8>,
}
/// Reduced-frame pixels per [`Ink`] cell.
const INK_CELL: u32 = 6;
/// Brightness difference inside a cell that counts as detail.
const INK_CONTRAST: i32 = 40;
impl Ink {
    fn new(frame: &RgbaImage) -> Self {
        let columns = frame.width() / INK_CELL;
        let rows = frame.height() / INK_CELL;
        let mut bits = vec![0; (columns * rows).div_ceil(8) as usize];
        for row in 0..rows {
            for column in 0..columns {
                let (mut lo, mut hi) = (255, 0);
                for y in 0..INK_CELL {
                    for x in 0..INK_CELL {
                        let p = frame.get_pixel(column * INK_CELL + x, row * INK_CELL + y);
                        let v = (p[0] as i32 * 77 + p[1] as i32 * 150 + p[2] as i32 * 29) >> 8;
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                }
                if hi - lo > INK_CONTRAST {
                    let i = (row * columns + column) as usize;
                    bits[i / 8] |= 1 << (i % 8);
                }
            }
        }
        Self {
            columns,
            rows,
            bits,
        }
    }
    /// Is there no detail at all inside `rect`, in reduced-frame pixels?
    fn blank(&self, rect: [f32; 4]) -> bool {
        let cell = INK_CELL as f32;
        let x0 = (rect[0] / cell).max(0.) as u32;
        let y0 = (rect[1] / cell).max(0.) as u32;
        let x1 = (((rect[0] + rect[2]) / cell).ceil().max(0.) as u32).min(self.columns);
        let y1 = (((rect[1] + rect[3]) / cell).ceil().max(0.) as u32).min(self.rows);
        (y0..y1).all(|y| {
            (x0..x1).all(|x| {
                let i = (y * self.columns + x) as usize;
                self.bits[i / 8] & (1 << (i % 8)) == 0
            })
        })
    }
}

pub struct Tracked {
    pub label: String,
    pub key: String,
    pub path: Path,
    pub follow: Follow,
    patch: Patch,
    rect: [f32; 4],
}
impl Tracked {
    pub fn end(&self, scan_end: u64) -> u64 {
        self.follow.ended.unwrap_or(scan_end)
    }
    pub fn review_note(&self) -> Option<String> {
        let (a, b) = *self.follow.needs_review.first()?;
        let more = self.follow.needs_review.len() - 1;
        Some(format!(
            "Needs review {:.2}–{:.2} s{}",
            a as f64 / 1000.,
            b as f64 / 1000.,
            if more > 0 {
                format!(" (+{more} more)")
            } else {
                String::new()
            }
        ))
    }
}

/// Are these two readings of one value? A blurred frame can return only part
/// of it, so a piece of at least four characters counts.
pub fn same_value(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    short.chars().count() >= MIN_FRAGMENT && long.contains(short)
}
/// The box for a new reading of a value: never narrower than the widest seen.
/// A shorter reading is placed by where it falls inside the whole value.
pub fn keep_widest(
    known: [f32; 4],
    found: [f32; 4],
    value: &str,
    reading: &str,
    width: f32,
) -> [f32; 4] {
    // After a page zoom the text is shorter as well as narrower, and the old
    // width would cover the rest of the line.
    let zoomed = reading == value && found[3] < known[3] * 0.8;
    if found[2] >= known[2] || zoomed {
        return found;
    }
    let before = value
        .find(reading)
        .map_or(0, |i| value[..i].chars().count());
    let total = value.chars().count().max(1);
    let x = found[0] - before as f32 / total as f32 * known[2];
    let x = x.clamp(0., (width - known[2]).max(0.));
    [x, found[1], known[2].min(width - x), found[3].max(known[3])]
}
/// Two readings that may differ by a misread character or two.
fn close_reading(a: &str, b: &str) -> bool {
    if same_value(a, b) {
        return true;
    }
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().min(b.len()) < MIN_READING || a.len().abs_diff(b.len()) > MISREADS {
        return false;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (current + 1)
                .min(row[j] + 1)
                .min(previous + usize::from(ca != cb));
            previous = current;
        }
    }
    row[b.len()] <= MISREADS
}
/// Is `later` a fresh reading of the same text as the box at `earlier`?
fn twin(earlier: [f32; 4], later: [f32; 4], usual_width: f32) -> bool {
    let shared = (earlier[1] + earlier[3]).min(later[1] + later[3]) - earlier[1].max(later[1]);
    shared >= earlier[3].min(later[3]) * 0.5
        && earlier[0] < later[0] + later[2]
        && later[0] < earlier[0] + earlier[2]
        && later[2] >= usual_width * 0.8
}
/// Does `outer` hold `inner` across its width and at its center height?
fn holds(outer: [f32; 4], inner: [f32; 4]) -> bool {
    let middle = inner[1] + inner[3] / 2.;
    inner[0] >= outer[0] - 4.
        && inner[0] + inner[2] <= outer[0] + outer[2] + 4.
        && middle >= outer[1]
        && middle <= outer[1] + outer[3]
}
/// A track's typical width; a box mid-reflow can be much wider.
fn usual_width(t: &Tracked) -> f32 {
    let mut widths: Vec<f32> = t.path.nodes.iter().map(|n| n.rect[2]).collect();
    widths.sort_by(f32::total_cmp);
    widths.get(widths.len() / 2).copied().unwrap_or(0.)
}
fn rect_at(t: &Tracked, ms: u64) -> Option<[f32; 4]> {
    let age = ms.checked_sub(t.follow.start)?;
    let i = t.path.nodes.partition_point(|n| n.at <= age);
    t.path.nodes.get(i.checked_sub(1)?).map(|n| n.rect)
}
/// Words in `words` that are a known value or a piece of one.
pub fn fragments(words: &[Word], known: &[String]) -> Vec<(usize, [f32; 4])> {
    let mut out = vec![];
    for word in words {
        let text = word
            .text
            .trim_matches(|c: char| ",.;:()[]{}<>\"'".contains(c))
            .to_lowercase();
        if text.chars().count() < MIN_FRAGMENT {
            continue;
        }
        if let Some(value) = known.iter().position(|k| k.contains(&text)) {
            out.push((value, word.bounds));
        }
    }
    out
}
fn scroll_at(log: &[(u64, f32)], ms: u64) -> f32 {
    let i = log.partition_point(|(at, _)| *at <= ms);
    if i == 0 { 0. } else { log[i - 1].1 }
}
/// A run of sightings of one value at one place on the page.
#[derive(Debug, PartialEq)]
pub struct Extra {
    pub value: usize,
    pub nodes: Vec<(u64, [f32; 4])>,
}
/// Joins sightings no track covers into runs that follow the page's scroll.
pub fn gather(sightings: &[(u64, usize, [f32; 4])], log: &[(u64, f32)]) -> Vec<Extra> {
    let mut out: Vec<Extra> = vec![];
    for &(ms, value, rect) in sightings {
        let open = out.iter_mut().find(|e| {
            let &(at, r) = e.nodes.last().unwrap();
            let moved = scroll_at(log, ms) - scroll_at(log, at);
            e.value == value
                && ms > at
                && ms - at <= 1200
                && (r[0] - rect[0]).abs() < 32.
                && (r[1] + moved - rect[1]).abs() < 24.
        });
        match open {
            Some(e) => e.nodes.push((ms, rect)),
            None => out.push(Extra {
                value,
                nodes: vec![(ms, rect)],
            }),
        }
    }
    out
}
/// Where a value first seen at `start` was on the frames before it, from the
/// page's scroll alone: while it was on screen and the page was moving.
pub fn backfill(
    log: &[(u64, f32)],
    start: u64,
    rect: [f32; 4],
    height: f32,
) -> Vec<(u64, [f32; 4])> {
    let at = log.partition_point(|(ms, _)| *ms <= start);
    if at < 2 {
        return vec![];
    }
    let anchor = log[at - 1].1;
    let mut out = vec![];
    let mut still = 0;
    let mut moved = false;
    for j in (0..at - 1).rev() {
        let (ms, y) = log[j];
        let (next_ms, next_y) = log[j + 1];
        if next_y != y {
            still = 0;
            moved = true;
        } else {
            still += next_ms - ms;
            if still >= STILL_MS {
                break;
            }
        }
        let top = rect[1] - (anchor - y);
        if top + rect[3] <= 0. || top >= height {
            break;
        }
        let visible = top.max(0.);
        out.push((
            ms,
            [
                rect[0],
                visible,
                rect[2],
                (top + rect[3]).min(height) - visible,
            ],
        ));
    }
    if !moved {
        return vec![];
    }
    out.reverse();
    out
}

/// Where a value last seen at `end` went next, from the page's scroll alone:
/// it can no longer be read once the edge cuts it off, but it is still there.
pub fn carry_forward(
    log: &[(u64, f32)],
    end: u64,
    rect: [f32; 4],
    height: f32,
) -> Vec<(u64, [f32; 4])> {
    let at = log.partition_point(|(ms, _)| *ms < end);
    if at == 0 || at >= log.len() {
        return vec![];
    }
    // `rect` is where the value was on the frame the track ended.
    let anchor = log[at].1;
    let mut out: Vec<(u64, [f32; 4])> = vec![];
    let (mut still, mut keep) = (0, 0);
    for j in at..log.len() {
        let (ms, y) = log[j];
        if y != log[j - 1].1 {
            still = 0;
        } else {
            still += ms - log[j - 1].0;
            if still >= STILL_MS {
                break;
            }
        }
        let top = rect[1] + (y - anchor);
        if top + rect[3] <= 0. || top >= height {
            break;
        }
        let visible = top.max(0.);
        out.push((
            ms,
            [
                rect[0],
                visible,
                rect[2],
                (top + rect[3]).min(height) - visible,
            ],
        ));
        if still == 0 {
            keep = out.len();
        }
    }
    out.truncate(keep);
    out
}

/// How early a fresh reading of a moved value starts, in ms.
const TWIN_LEAD_MS: u64 = 150;
/// Readings at least this long may differ by `MISREADS` characters.
const MIN_READING: usize = 8;
const MISREADS: usize = 2;
/// A track this short may be just a piece of another.
const FRAGMENT_NODES: usize = 30;

pub struct Scanner {
    size: [u32; 2],
    density: f32,
    options: Options,
    tracks: Vec<Tracked>,
    next_ocr: u64,
    last_ocr: u64,
    /// Reduced frames since the last OCR pass, to follow newly found regions
    /// back to where they were in those frames (BE6).
    recent: Vec<(u64, RgbaImage, f32)>,
    previous: Option<RgbaImage>,
    last_shift: i32,
    scrolled: f32,
    scrolled_since_ocr: f32,
    /// The page's scroll at each frame, in output pixels.
    log: Vec<(u64, f32)>,
    /// What OCR read, kept for the sweep once every value is known.
    read: Vec<(u64, Vec<Word>)>,
    /// Times of scene changes: nothing is tracked or followed back across one.
    cuts: Vec<u64>,
    /// Frames where the page rearranged in place, such as a zoom.
    reflows: Vec<u64>,
    /// Scrolling frames on which each row changed, to find the page area.
    row_moves: Vec<u32>,
    scroll_frames: u32,
    reflow_run: u32,
    /// Each frame's detail. Unchanged frames share one entry.
    ink: Vec<(u64, std::sync::Arc<Ink>)>,
}
impl Scanner {
    pub fn new(size: [u32; 2], density: f32, options: Options, start: u64) -> Self {
        Self {
            size,
            density,
            options,
            tracks: vec![],
            next_ocr: start,
            last_ocr: start,
            recent: vec![],
            previous: None,
            last_shift: 0,
            scrolled: 0.,
            scrolled_since_ocr: 0.,
            log: vec![],
            read: vec![],
            cuts: vec![],
            reflows: vec![],
            row_moves: vec![],
            scroll_frames: 0,
            reflow_run: 0,
            ink: vec![],
        }
    }
    /// One frame at `ms`, reduced by the scanner's density. `ocr` reads the
    /// full-size frame when this frame needs reading.
    pub fn frame(
        &mut self,
        ms: u64,
        reduced: RgbaImage,
        ocr: impl FnOnce() -> Result<Vec<Word>, String>,
    ) -> Result<(), String> {
        let (density, size) = (self.density, self.size);
        let height = reduced.height() as f32;
        let ink = Ink::new(&reduced);
        let ink = match self.ink.last() {
            Some((_, last)) if **last == ink => last.clone(),
            _ => std::sync::Arc::new(ink),
        };
        self.ink.push((ms, ink));
        if self
            .previous
            .as_ref()
            .is_some_and(|p| scene_cut(p, &reduced))
        {
            self.scene_changed(ms);
        }
        let expected = (self.last_shift != 0).then_some(self.last_shift);
        let motion = self
            .previous
            .as_ref()
            .and_then(|p| page_motion(p, &reduced, 240, expected))
            .unwrap_or_default();
        if motion.shift != 0 {
            self.last_shift = motion.shift;
            self.count_page_rows(&reduced);
        }
        let shift = motion.shift as f32;
        self.scrolled += shift;
        self.scrolled_since_ocr += shift.abs();
        self.log.push((ms, self.scrolled / density));
        let hint = (shift != 0.).then_some([0., shift]);
        for t in self.tracks.iter_mut().filter(|t| t.follow.active()) {
            let found = t.patch.track_hinted(&reduced, 72, hint).map(|(r, c)| {
                t.rect = r;
                (r.map(|x| x / density), c)
            });
            if found.is_none() && shift != 0. {
                // Lost while the page scrolls: the cover moves with it.
                t.rect[1] += shift;
                t.patch.rect = t.rect;
                t.follow.follow_scroll([0., shift / density], size);
            }
            t.follow.observe(&mut t.path, ms, found, size);
            if motion.reflow {
                t.follow.reflowed();
            }
        }
        // A zoom changes the page over a few frames. Each of them is read, and
        // so is the first settled frame.
        let settled = self.reflow_run > 0 && !motion.reflow;
        if motion.reflow {
            self.reflows.push(ms);
        }
        self.reflow_run = if motion.reflow {
            self.reflow_run + 1
        } else {
            0
        };
        if settled || (motion.reflow && self.reflow_run <= REFLOW_FRAMES) {
            self.next_ocr = self.next_ocr.min(ms);
        }
        // Every frame while the page moves fast: this is what catches a
        // value entering at the edge before it has a box.
        if shift.abs() >= height * 0.02
            || (self.scrolled_since_ocr > height * 0.1 && ms >= self.last_ocr + 66)
        {
            self.next_ocr = self.next_ocr.min(ms);
        }
        if ms >= self.next_ocr {
            let words = ocr()?;
            self.read_words(ms, &words, &reduced)?;
            self.read.push((ms, words));
            self.last_ocr = ms;
            self.next_ocr = ms + 500;
            self.scrolled_since_ocr = 0.;
            self.recent.clear();
        }
        self.previous = Some(reduced.clone());
        self.recent.push((ms, reduced, self.scrolled));
        Ok(())
    }
    fn read_words(&mut self, ms: u64, words: &[Word], reduced: &RgbaImage) -> Result<(), String> {
        let (density, size) = (self.density, self.size);
        let width = reduced.width() as f32;
        let findings = detect(words, &self.options, size).map_err(|e| e.to_string())?;
        let mut seen = vec![false; self.tracks.len()];
        for f in findings {
            for rect in f.bounds {
                let r = rect.map(|x| x * density);
                let key = f.text.to_lowercase();
                let existing = self
                    .tracks
                    .iter_mut()
                    .enumerate()
                    // One finding confirms one region: identical text on two
                    // lines must keep two redactions (BE6).
                    .filter(|(i, t)| {
                        same_value(&t.key, &key)
                            && t.follow.active()
                            && !seen.get(*i).copied().unwrap_or(false)
                    })
                    .min_by(|(_, a), (_, b)| distance(a.rect, r).total_cmp(&distance(b.rect, r)))
                    // Only where the track is expected now. A copy far from a
                    // lost track is a new region that needs its own history.
                    .filter(|(_, t)| distance(t.rect, r) < 100.);
                if let Some((i, t)) = existing {
                    let kept = keep_widest(t.rect, r, &t.key, &key, width);
                    if key.len() > t.key.len() {
                        t.label = format!(
                            "{} · {}",
                            f.category.label(),
                            f.text.chars().take(40).collect::<String>()
                        );
                        t.key = key;
                    }
                    t.rect = kept;
                    t.patch.refresh(reduced, kept);
                    t.follow
                        .confirm(&mut t.path, ms, kept.map(|x| x / density), size);
                    seen[i] = true;
                } else {
                    if self.tracks.len() >= MAX_FOUND {
                        return Err(TOO_MANY.into());
                    }
                    let mut path = blank_path(rect);
                    let mut back = Patch::new(reduced, r);
                    let height = reduced.height() as f32;
                    let scrolled = self.scrolled;
                    let mut earlier: Vec<(u64, Option<[f32; 4]>)> = self
                        .recent
                        .iter()
                        .rev()
                        .map(|(at, frame, then)| {
                            // While the page scrolled, the scroll says exactly
                            // where the text was, even cut off at an edge, and a
                            // repeated line can't be mistaken for it.
                            let dy = scrolled - then;
                            let placed = if dy != 0. {
                                let pos = [r[0], r[1] - dy, r[2], r[3]];
                                (pos[1] + pos[3] > 0. && pos[1] < height).then(|| {
                                    let top = pos[1].max(0.);
                                    let bottom = (pos[1] + pos[3]).min(height);
                                    [pos[0], top, pos[2], bottom - top]
                                })
                            } else {
                                back.track(frame, 72).map(|(r, _)| r)
                            };
                            (*at, placed.map(|r| r.map(|x| x / density)))
                        })
                        .collect();
                    earlier.reverse();
                    let follow =
                        Follow::with_history(&mut path, self.last_ocr, ms, rect, size, &earlier);
                    self.tracks.push(Tracked {
                        label: format!(
                            "{} · {}",
                            f.category.label(),
                            f.text.chars().take(40).collect::<String>()
                        ),
                        key,
                        rect: r,
                        patch: Patch::new(reduced, r),
                        follow,
                        path,
                    });
                    seen.push(true);
                }
            }
        }
        for (t, seen) in self.tracks.iter_mut().zip(seen) {
            t.follow.ocr(&mut t.path, ms, seen, size);
        }
        Ok(())
    }
    /// The page's scroll at each frame so far, in output pixels.
    pub fn scroll_log(&self) -> &[(u64, f32)] {
        &self.log
    }
    /// Closes the scan: the sweep, the backward pass and the final padding.
    pub fn finish(mut self, end: u64) -> Result<Vec<Tracked>, String> {
        let size = self.size;
        for t in &mut self.tracks {
            t.follow.finish(end);
        }
        self.sweep(end)?;
        let height = size[1] as f32;
        let page = self.page_rows();
        for t in &mut self.tracks {
            if let Some(first) = t.path.nodes.first().map(|n| n.rect) {
                // The page's scroll says nothing about frames before a cut.
                let cut = self
                    .cuts
                    .iter()
                    .rfind(|c| **c <= t.follow.start)
                    .copied()
                    .unwrap_or(0);
                let log = &self.log[self.log.partition_point(|(ms, _)| *ms < cut)..];
                let earlier = backfill(log, t.follow.start, first, height);
                t.follow.extend_back(&mut t.path, &earlier, size);
                let until = t.end(end);
                if until < end {
                    let cut = self.cuts.partition_point(|c| *c <= until);
                    let stop = self.cuts.get(cut).copied().unwrap_or(u64::MAX);
                    let log = &self.log[..self.log.partition_point(|(ms, _)| *ms < stop)];
                    let later = carry_forward(log, until, t.follow.last, height);
                    t.follow.extend_forward(&mut t.path, &later, size);
                }
            }
            for n in &mut t.path.nodes {
                let at = self
                    .ink
                    .partition_point(|(ms, _)| *ms <= t.follow.start + n.at);
                n.visible = n.confidence > 0.
                    || at == 0
                    || !self.ink[at - 1]
                        .1
                        .blank(inner(n.rect).map(|v| v * self.density));
            }
            let inside = page.filter(|p| on_page(&t.path.nodes, *p));
            stable::apply(&mut t.path.nodes, t.path.reference[3], size, inside);
        }
        self.tracks.retain_mut(trim_hidden);
        self.settle_twins(end, size);
        self.recall(end, page)?;
        for t in &self.tracks {
            t.path.validate().map_err(|e| e.to_string())?;
        }
        Ok(self.tracks)
    }
    /// One value can end up with two tracks after the page zooms: the first
    /// keeps its old place until OCR reads the new one. The fresh reading takes
    /// over from the stale box and starts a moment early, since the value had
    /// already moved by the time it could be read. A piece of a value that
    /// another track already covers is dropped.
    fn settle_twins(&mut self, end: u64, size: [u32; 2]) {
        let covered = |tracks: &[Tracked], i: usize| {
            let t = &tracks[i];
            t.path.nodes.len() <= FRAGMENT_NODES
                && tracks.iter().enumerate().any(|(j, s)| {
                    j != i
                        && close_reading(&s.key, &t.key)
                        && t.path.nodes.iter().all(|n| {
                            rect_at(s, t.follow.start + n.at).is_some_and(|r| holds(r, n.rect))
                        })
                })
        };
        let mut i = 0;
        while i < self.tracks.len() {
            if covered(&self.tracks, i) {
                self.tracks.remove(i);
            } else {
                i += 1;
            }
        }
        let mut order: Vec<usize> = (0..self.tracks.len()).collect();
        order.sort_by_key(|i| self.tracks[*i].follow.start);
        for i in order {
            let start = self.tracks[i].follow.start;
            let Some(first) = self.tracks[i].path.nodes.first().map(|n| n.rect) else {
                continue;
            };
            let stale = (0..self.tracks.len()).find(|j| {
                let s = &self.tracks[*j];
                *j != i
                    && s.follow.start < start
                    && start < s.end(end)
                    && close_reading(&s.key, &self.tracks[i].key)
                    && rect_at(s, start).is_some_and(|r| twin(r, first, usual_width(s)))
            });
            let Some(j) = stale else { continue };
            let earliest = start
                .saturating_sub(TWIN_LEAD_MS)
                .max(self.tracks[j].follow.start);
            let s = &mut self.tracks[j];
            s.follow.shorten(&mut s.path, start);
            let at = self.log.partition_point(|(ms, _)| *ms <= start);
            let Some(anchor) = at.checked_sub(1).map(|a| self.log[a].1) else {
                continue;
            };
            let earlier: Vec<(u64, [f32; 4])> = self.log[..at]
                .iter()
                .filter(|(ms, _)| *ms >= earliest && *ms < start)
                .filter_map(|&(ms, y)| {
                    let top = first[1] - (anchor - y);
                    let bottom = (top + first[3]).min(size[1] as f32);
                    let visible = top.max(0.);
                    (bottom - visible >= 1.)
                        .then_some((ms, [first[0], visible, first[2], bottom - visible]))
                })
                .collect();
            let t = &mut self.tracks[i];
            t.follow.extend_back(&mut t.path, &earlier, size);
        }
    }
    /// A value read once is still there when the page comes back to it, or
    /// when tracking loses it, or when OCR misses it. Every place a value was
    /// read is followed through the whole scene from the page's scroll, and
    /// each frame where it has text there and no box gets one, flagged for
    /// review.
    fn recall(&mut self, end: u64, page: Option<[f32; 2]>) -> Result<(), String> {
        let size = self.size;
        let [top, bottom] = page.unwrap_or([0., size[1] as f32]);
        for place in self.places() {
            // A value is read again and again while it is on screen. Something
            // read only in passing, such as a misread during a window change,
            // keeps its own box but isn't carried through the scene.
            if !self.read_often(&place, top, bottom) {
                continue;
            }
            let mut missing: Vec<(u64, [f32; 4])> = vec![];
            for &(ms, y) in &self.log {
                if self.scene_of(ms) != place.scene {
                    continue;
                }
                let at = place.on_page(ms) + y;
                let (y0, y1) = (at.max(top), (at + place.rect[3]).min(bottom));
                if y1 - y0 < 2. {
                    continue;
                }
                let rect = [place.rect[0], y0, place.rect[2], y1 - y0];
                let ink = self.ink.partition_point(|(when, _)| *when <= ms);
                let blank = ink == 0 || self.ink[ink - 1].1.blank(rect.map(|v| v * self.density));
                let covered = self.tracks.iter().any(|t| {
                    ms >= t.follow.start
                        && ms < t.end(end)
                        && overlap_visible(t, ms, rect) >= COVERED
                });
                if !blank && !covered {
                    missing.push((ms, rect));
                }
            }
            for run in self.runs(missing) {
                if self.tracks.len() >= MAX_TOTAL {
                    return Err(TOO_MANY.into());
                }
                let (&(first, rect), &(last, final_rect)) =
                    (run.first().unwrap(), run.last().unwrap());
                let after = self.log.partition_point(|(ms, _)| *ms <= last);
                let until = self.log.get(after).map_or(end, |e| e.0).max(last + 1);
                let mut path = blank_path(rect);
                path.nodes = run
                    .iter()
                    .map(|&(ms, r)| crate::video::motion_path::Node {
                        at: ms - first,
                        rect: super::tracking::cover(r, r, size),
                        visible: true,
                        sprite: 0,
                        confidence: 0.,
                    })
                    .collect();
                let label = self
                    .tracks
                    .iter()
                    .find(|t| t.key == place.value)
                    .map_or_else(|| "Redaction".to_owned(), |t| t.label.clone());
                self.tracks.push(Tracked {
                    label,
                    key: place.value.clone(),
                    path,
                    follow: Follow::recalled(first, final_rect, until),
                    patch: Patch::new(&RgbaImage::new(4, 4), [0., 0., 4., 4.]),
                    rect: [0.; 4],
                });
            }
        }
        Ok(())
    }
    /// Was this place read on enough of the passes that had it on screen?
    fn read_often(&self, place: &Place, top: f32, bottom: f32) -> bool {
        let mut times: Vec<u64> = place.seen.iter().map(|(ms, _)| *ms).collect();
        times.dedup();
        let passes = self
            .read
            .iter()
            .filter(|(ms, _)| {
                let y = place.on_page(*ms) + scroll_at(&self.log, *ms);
                self.scene_of(*ms) == place.scene && y >= top && y + place.rect[3] <= bottom
            })
            .count();
        times.len() >= 2 && times.len() as f32 >= passes as f32 * MIN_READ_SHARE
    }
    /// Which stretch between scene changes and page zooms `ms` falls in.
    fn scene_of(&self, ms: u64) -> usize {
        self.cuts.partition_point(|c| *c <= ms) + self.reflows.partition_point(|r| *r <= ms)
    }
    /// Every place on the page where OCR read a value.
    fn places(&self) -> Vec<Place> {
        let mut places: Vec<Place> = vec![];
        for (ms, words) in &self.read {
            let found = detect(words, &self.options, self.size).unwrap_or_default();
            let keys: Vec<String> = found.iter().map(|f| f.text.to_lowercase()).collect();
            let whole = found
                .iter()
                .zip(&keys)
                .flat_map(|(f, key)| f.bounds.iter().map(move |r| (key.clone(), *r, true)));
            let pieces = fragments(words, &keys)
                .into_iter()
                .map(|(value, r)| (keys[value].clone(), r, false));
            let scene = self.scene_of(*ms);
            let scroll = scroll_at(&self.log, *ms);
            for (value, rect, full) in whole.chain(pieces) {
                let on_page = rect[1] - scroll;
                let same = places.iter_mut().find(|p| {
                    p.scene == scene
                        && close_reading(&p.value, &value)
                        && p.rect[0] < rect[0] + rect[2]
                        && rect[0] < p.rect[0] + p.rect[2]
                        && p.seen
                            .iter()
                            .any(|(_, y)| (y - on_page).abs() < rect[3].max(p.rect[3]) * 0.75)
                });
                match same {
                    Some(p) => {
                        if rect[2] > p.rect[2] {
                            p.rect = rect;
                            p.value = value;
                        }
                        p.seen.push((*ms, on_page));
                    }
                    // A word that is only part of a value can be an ordinary
                    // word elsewhere; it confirms a place but never starts one.
                    None if full => places.push(Place {
                        value,
                        scene,
                        rect,
                        seen: vec![(*ms, on_page)],
                    }),
                    None => {}
                }
            }
        }
        places
    }
    /// Joins a place's uncovered frames into redactions. Frames between two
    /// close runs are included so one value doesn't become many objects.
    fn runs(&self, missing: Vec<(u64, [f32; 4])>) -> Vec<Vec<(u64, [f32; 4])>> {
        let mut runs: Vec<Vec<(u64, [f32; 4])>> = vec![];
        for (ms, rect) in missing {
            match runs.last_mut() {
                Some(run) if ms - run.last().unwrap().0 <= JOIN_MS => {
                    let last = run.last().unwrap().0;
                    let (from, to) = (
                        self.log.partition_point(|(at, _)| *at <= last),
                        self.log.partition_point(|(at, _)| *at < ms),
                    );
                    let scroll = scroll_at(&self.log, ms);
                    let top = rect[1];
                    for &(at, y) in &self.log[from..to] {
                        run.push((at, [rect[0], top + y - scroll, rect[2], rect[3]]));
                    }
                    run.push((ms, rect));
                }
                _ => runs.push(vec![(ms, rect)]),
            }
        }
        let height = self.size[1] as f32;
        for run in &mut runs {
            run.retain(|(_, r)| r[1] + r[3] > 0. && r[1] < height);
            for (_, r) in run.iter_mut() {
                let y = r[1].max(0.);
                *r = [r[0], y, r[2], (r[1] + r[3]).min(height) - y];
            }
        }
        runs.retain(|r| !r.is_empty());
        runs
    }
    /// A new scene: tracks end here and reading starts over.
    fn scene_changed(&mut self, ms: u64) {
        let size = self.size;
        for t in &mut self.tracks {
            t.follow.cut(&mut t.path, ms, size);
        }
        self.cuts.push(ms);
        self.recent.clear();
        self.previous = None;
        self.last_shift = 0;
        self.scrolled_since_ocr = 0.;
        self.last_ocr = ms;
        self.next_ocr = ms;
    }
    fn count_page_rows(&mut self, reduced: &RgbaImage) {
        let Some(previous) = &self.previous else {
            return;
        };
        let rows = changed_rows(previous, reduced);
        self.row_moves.resize(rows.len(), 0);
        for (count, changed) in self.row_moves.iter_mut().zip(rows) {
            *count += changed as u32;
        }
        self.scroll_frames += 1;
    }
    /// The top and bottom of the area that scrolls, in output pixels: the rows
    /// that keep changing while the page moves. Bars and the taskbar don't.
    fn page_rows(&self) -> Option<[f32; 2]> {
        if self.scroll_frames < 2 {
            return None;
        }
        // Sparse lines pass each row only now and then, so a few times is
        // enough; bars and the taskbar don't change while the page scrolls.
        let enough = (self.scroll_frames / 40).max(2);
        let top = self.row_moves.iter().position(|c| *c >= enough)?;
        let bottom = self.row_moves.iter().rposition(|c| *c >= enough)? + 1;
        let height = self.row_moves.len();
        ((bottom - top) * 10 >= height * 3).then(|| {
            let scale = 1. / self.density;
            [
                ((top as f32 - 3.) * scale).max(0.),
                ((bottom as f32 + 3.) * scale).min(self.size[1] as f32),
            ]
        })
    }
    /// Boxes every copy and piece of a known value that OCR read on some frame
    /// but that no track covers there: tails beside a window, copies cut off
    /// at the edge, values seen in full only later.
    fn sweep(&mut self, end: u64) -> Result<(), String> {
        let mut known: Vec<String> = self.tracks.iter().map(|t| t.key.clone()).collect();
        known.sort();
        known.dedup();
        let mut sightings = vec![];
        for (ms, words) in &self.read {
            for (value, rect) in fragments(words, &known) {
                let covered = self.tracks.iter().any(|t| {
                    *ms >= t.follow.start
                        && *ms < t.end(end)
                        && overlap(t.path.sample(*ms - t.follow.start).rect, rect) >= 0.7
                });
                if !covered {
                    sightings.push((*ms, value, rect));
                }
            }
        }
        let times: Vec<u64> = self.read.iter().map(|(ms, _)| *ms).collect();
        for extra in gather(&sightings, &self.log) {
            if self.tracks.len() >= MAX_TOTAL {
                return Err(TOO_MANY.into());
            }
            let &(last, rect) = extra.nodes.last().unwrap();
            let first = extra.nodes[0].0;
            let until = times[times.partition_point(|t| *t <= last)..]
                .first()
                .copied()
                .unwrap_or(last + STILL_MS)
                .min(end)
                .max(last + 1);
            let earlier: Vec<_> = extra.nodes[..extra.nodes.len() - 1]
                .iter()
                .map(|(ms, r)| (*ms, Some(*r)))
                .collect();
            let mut path = blank_path(extra.nodes[0].1);
            let mut follow =
                Follow::with_history(&mut path, first, last, rect, self.size, &earlier);
            follow.finish(until);
            let label = self
                .tracks
                .iter()
                .find(|t| t.key == known[extra.value])
                .map_or_else(|| "Redaction".to_owned(), |t| t.label.clone());
            self.tracks.push(Tracked {
                label,
                key: known[extra.value].clone(),
                path,
                follow,
                patch: Patch::new(&RgbaImage::new(4, 4), [0., 0., 4., 4.]),
                rect: [0.; 4],
            });
        }
        Ok(())
    }
}
/// One place on the page where a value was read.
struct Place {
    value: String,
    scene: usize,
    /// The widest reading, in output pixels.
    rect: [f32; 4],
    /// When it was read, and its top relative to the page's scroll.
    seen: Vec<(u64, f32)>,
}
impl Place {
    /// The value's top relative to the page's scroll at `ms`. The scroll adds
    /// up small errors, so between two readings it is corrected in proportion.
    fn on_page(&self, ms: u64) -> f32 {
        let next = self.seen.partition_point(|(when, _)| *when <= ms);
        match (
            next.checked_sub(1).map(|i| self.seen[i]),
            self.seen.get(next),
        ) {
            (Some((a, y0)), Some(&(b, y1))) => {
                y0 + (y1 - y0) * ((ms - a) as f32 / (b - a).max(1) as f32)
            }
            (Some((_, y)), None) => y,
            (None, Some(&(_, y))) => y,
            (None, None) => 0.,
        }
    }
}
/// How much of a value a box must hold to count as covering it.
const COVERED: f32 = 0.9;
/// The share of on-screen OCR passes that must read a place for it to be recalled.
const MIN_READ_SHARE: f32 = 0.12;
/// Uncovered frames this close join into one redaction.
const JOIN_MS: u64 = 300;
/// How much of `rect` the track's box at `ms` holds; nothing if it is hidden.
fn overlap_visible(t: &Tracked, ms: u64, rect: [f32; 4]) -> f32 {
    let node = t.path.sample(ms - t.follow.start);
    if node.visible {
        overlap(node.rect, rect)
    } else {
        0.
    }
}
/// A stored cover without the margin it was given, to test what it really holds.
fn inner(r: [f32; 4]) -> [f32; 4] {
    [
        r[0] + 3.,
        r[1] + 3.,
        (r[2] - 6.).max(1.),
        (r[3] - 6.).max(1.),
    ]
}
/// Does this track's value live on the page? Text on a fixed bar stays
/// where it is and isn't limited to the scrolling area.
fn on_page(nodes: &[crate::video::motion_path::Node], page: [f32; 2]) -> bool {
    let confirmed: Vec<_> = nodes.iter().filter(|n| n.confidence >= 0.5).collect();
    let inside = confirmed
        .iter()
        .filter(|n| {
            let middle = n.rect[1] + n.rect[3] / 2.;
            middle >= page[0] && middle <= page[1]
        })
        .count();
    !confirmed.is_empty() && inside * 5 >= confirmed.len() * 4
}
/// Removes a track's hidden start and end. False when nothing is left.
fn trim_hidden(t: &mut Tracked) -> bool {
    let nodes = &mut t.path.nodes;
    let Some(first) = nodes.iter().position(|n| n.visible) else {
        return false;
    };
    let last = nodes.iter().rposition(|n| n.visible).unwrap();
    if let Some(next) = nodes.get(last + 1) {
        t.follow.ended = Some(t.follow.start + next.at);
    }
    nodes.truncate(last + 1);
    if first > 0 {
        let delta = nodes[first].at;
        nodes.drain(..first);
        for n in nodes.iter_mut() {
            n.at -= delta;
        }
        t.follow.start += delta;
    }
    let (from, until) = (t.follow.start, t.follow.ended.unwrap_or(u64::MAX));
    t.follow.needs_review.retain_mut(|r| {
        (r.0, r.1) = (r.0.max(from), r.1.min(until));
        r.0 < r.1
    });
    true
}
fn blank_path(reference: [f32; 4]) -> Path {
    Path {
        purpose: Purpose::Redaction,
        reference,
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
fn distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
/// How much of `inner` lies inside `outer`, 0 to 1.
fn overlap(outer: [f32; 4], inner: [f32; 4]) -> f32 {
    let w = (outer[0] + outer[2]).min(inner[0] + inner[2]) - outer[0].max(inner[0]);
    let h = (outer[1] + outer[3]).min(inner[1] + inner[3]) - outer[1].max(inner[1]);
    (w.max(0.) * h.max(0.)) / (inner[2] * inner[3]).max(1.)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_fresh_reading_at_the_same_spot_is_the_same_value() {
        let old = [561., 940., 232., 46.];
        let width = old[2];
        assert!(twin(old, [530., 940., 210., 46.], width));
        assert!(
            !twin(old, [530., 1000., 210., 46.], width),
            "the next line is a copy"
        );
        assert!(
            !twin(old, [900., 940., 210., 46.], width),
            "further along the line is a copy"
        );
        assert!(
            !twin(old, [561., 940., 80., 46.], width),
            "a piece doesn't replace the whole"
        );
    }
    #[test]
    fn a_misread_character_is_still_the_same_reading() {
        assert!(close_reading("123-45-6789", "123-45-6189"));
        assert!(close_reading("123-45-6789", "23-45-6189"));
        assert!(!close_reading("123-45-6789", "987-65-4321"));
        assert!(
            !close_reading("a@b.co", "c@d.co"),
            "short values must match exactly"
        );
    }
    #[test]
    fn a_piece_inside_a_box_counts_as_held() {
        let value = [530., 940., 210., 46.];
        assert!(holds(value, [538., 948., 73., 30.]));
        assert!(!holds(value, [700., 948., 73., 30.]));
        assert!(!holds(value, [538., 1000., 73., 30.]));
    }
    #[test]
    fn the_whole_value_read_again_gives_its_real_size() {
        let known = [100., 60., 600., 40.];
        let found = [90., 62., 200., 14.];
        let same_size = [100., 60., 200., 38.];
        assert_eq!(
            keep_widest(known, same_size, "sam@mail.test", "sam@mail.test", 640.)[2],
            known[2],
            "a blurred reading at the same text size keeps the width"
        );
        assert_eq!(
            keep_widest(known, found, "sam@mail.test", "sam@mail.test", 640.),
            found
        );
        assert_eq!(
            keep_widest(known, found, "sam@mail.test", "sam@mail", 640.)[2],
            known[2],
            "a piece keeps the full width"
        );
    }
    use super::*;
    const W: u32 = 640;
    const H: u32 = 360;
    const VALUE: &str = "sam@mail.test";
    const SPACING: i32 = 40;
    /// Every fourth line of the page holds the value, at a varied x.
    fn value_lines() -> Vec<(i32, i32)> {
        (1..70)
            .map(|l| (l * 4 * SPACING, 90 + l * 37 % 160))
            .collect()
    }
    const VALUE_W: i32 = 130;
    const VALUE_H: i32 = 14;
    fn glyph(x: i32, y: i32) -> bool {
        (x / 3) % 2 == 0 && (y / 2) % 3 != 0
    }
    /// The window that hides most of one line on the first frames.
    const WINDOW_FRAMES: usize = 24;
    const WINDOW_EDGE: i32 = 280;
    fn window_hides(frame: usize, x: i32) -> bool {
        frame < WINDOW_FRAMES && x < WINDOW_EDGE
    }
    fn draw(scroll: i32, frame: usize) -> RgbaImage {
        let lines = value_lines();
        RgbaImage::from_fn(W, H, |x, y| {
            let (x, y) = (x as i32, y as i32);
            let py = y + scroll;
            if window_hides(frame, x) {
                return image::Rgba([70, 70, 90, 255]);
            }
            for (line, vx) in &lines {
                if py >= *line && py < line + VALUE_H && x >= *vx && x < vx + VALUE_W {
                    let v = if glyph(x - vx, py - line) { 20 } else { 235 };
                    return image::Rgba([v, v, v, 255]);
                }
            }
            // The same text-like row on every line: the page repeats.
            let row = py.rem_euclid(SPACING);
            let v = if row < VALUE_H && (20..600).contains(&x) && (x % 7 < 4) {
                120
            } else {
                250
            };
            image::Rgba([v, v, v, 255])
        })
    }
    /// Scroll position per frame: still, heavy flicks, settles.
    fn motion() -> Vec<(i32, i32)> {
        let mut y = 200;
        let mut out = vec![];
        let speeds: Vec<i32> = [
            vec![0; 32],
            vec![30, 70, 110, 110, 90, 60, 30, 10],
            vec![0; 14],
            vec![-20, -60, -100, -100, -60, -20],
            vec![0; 12],
            vec![50, 120, 120, 120, 80, 40],
            vec![0; 16],
        ]
        .concat();
        for v in speeds {
            y += v;
            out.push((y, v));
        }
        out
    }
    /// What OCR returns: nothing on the fastest frames, a shortened reading
    /// on other fast frames, the visible tail beside the window, and nothing
    /// for a value cut off by the frame edge.
    fn read(scroll: i32, speed: i32, frame: usize) -> Vec<Word> {
        let mut out = vec![];
        if speed.abs() >= 100 && frame.is_multiple_of(2) {
            return out;
        }
        for (i, (line, vx)) in value_lines().iter().enumerate() {
            let y = line - scroll;
            if y < 0 || y + VALUE_H > H as i32 {
                continue;
            }
            let bounds = |x: i32, w: i32| [x as f32, y as f32, w as f32, VALUE_H as f32];
            if window_hides(frame, *vx) && window_hides(frame, vx + VALUE_W / 2) {
                let tail = vx + VALUE_W - WINDOW_EDGE;
                if tail > 0 {
                    out.push(Word {
                        text: if tail >= 40 { "mail.test" } else { "test" }.into(),
                        line: i as u32,
                        bounds: bounds(WINDOW_EDGE, tail),
                    });
                }
                continue;
            }
            // A shortened reading can also persist across a slower stretch.
            let (text, width) =
                if (39..54).contains(&frame) || speed.abs() >= 30 && frame.is_multiple_of(3) {
                    ("sam@mail.te", VALUE_W * 9 / 13)
                } else {
                    (VALUE, VALUE_W)
                };
            out.push(Word {
                text: text.into(),
                line: i as u32,
                bounds: bounds(*vx, width),
            });
        }
        out
    }
    fn options() -> Options {
        Options {
            names: false,
            user_ids: false,
            emails: false,
            phones: false,
            phrases: vec![VALUE.into(), "sam@mail.te".into()],
            payment_cards: false,
            us_ssns: false,
            ip_addresses: false,
            custom: Vec::new(),
        }
    }
    #[test]
    fn repeating_page_with_heavy_flicks_exposes_no_glyph_pixels() {
        let frames = motion();
        let end = frames.len() as u64 * 33 + 33;
        let mut scan = Scanner::new([W, H], 1., options(), 0);
        for (i, (y, v)) in frames.iter().enumerate() {
            scan.frame(i as u64 * 33, draw(*y, i), || Ok(read(*y, *v, i)))
                .unwrap();
        }
        let tracks = scan.finish(end).unwrap();
        assert!(!tracks.is_empty());
        let mut exposed = vec![];
        for (i, (y, _)) in frames.iter().enumerate() {
            let ms = i as u64 * 33;
            let boxes: Vec<[f32; 4]> = tracks
                .iter()
                .filter(|t| ms >= t.follow.start && ms < t.end(end))
                .map(|t| t.path.sample(ms - t.follow.start).rect)
                .collect();
            for (line, vx) in value_lines() {
                let top = line - y;
                if top + VALUE_H <= 0 || top >= H as i32 {
                    continue;
                }
                let mut bare = 0;
                for py in 0..VALUE_H {
                    for px in 0..VALUE_W {
                        let (sx, sy) = (vx + px, top + py);
                        if sy < 0 || sy >= H as i32 || window_hides(i, sx) || !glyph(px, py) {
                            continue;
                        }
                        let hit = boxes.iter().any(|b| {
                            (sx as f32) >= b[0]
                                && (sx as f32) < b[0] + b[2]
                                && (sy as f32) >= b[1]
                                && (sy as f32) < b[1] + b[3]
                        });
                        if !hit {
                            bare += 1;
                        }
                    }
                }
                if bare > 0 {
                    exposed.push((i, line, bare));
                }
            }
        }
        assert!(
            exposed.is_empty(),
            "{} frames/lines exposed: {:?}",
            exposed.len(),
            &exposed[..exposed.len().min(24)]
        );
    }
    /// BL: frames before a scene cut get no boxes for what follows it, and
    /// tracks from before the cut end there.
    #[test]
    fn a_scene_cut_ends_tracks_and_stops_following_back() {
        let page = |y: u32| {
            RgbaImage::from_fn(W, H, |px, py| {
                let ink = py >= y && py < y + 14 && (100..230).contains(&px) && px % 5 < 3;
                let v = if ink { 235 } else { 30 };
                image::Rgba([v, v, v, 255])
            })
        };
        let other = RgbaImage::from_fn(W, H, |x, y| {
            let v = ((x * 3 + y * 5) % 256) as u8;
            image::Rgba([v, v / 2, 255 - v, 255])
        });
        let word = |y: u32| Word {
            text: VALUE.into(),
            line: 0,
            bounds: [100., y as f32, 130., 14.],
        };
        let mut scan = Scanner::new([W, H], 1., options(), 0);
        for i in 0..8u64 {
            scan.frame(i * 33, other.clone(), || Ok(vec![])).unwrap();
        }
        for i in 8..40u64 {
            scan.frame(i * 33, page(100), || Ok(vec![word(100)]))
                .unwrap();
        }
        let tracks = scan.finish(40 * 33).unwrap();
        assert!(!tracks.is_empty());
        assert!(tracks.iter().all(|t| t.follow.start == 8 * 33));
    }
    /// BL: a value entering past the page's edge gets no box on the bars.
    #[test]
    fn page_values_stay_inside_the_scrolling_area() {
        // Rows 0-29 and 330-359 never change; the page between them scrolls.
        let draw = |scroll: i32| {
            RgbaImage::from_fn(W, H, |x, y| {
                let y = y as i32;
                if !(30..330).contains(&y) {
                    return image::Rgba([90, 90, 110, 255]);
                }
                let doc = y + scroll;
                let line = doc.rem_euclid(40) < 14 && (x as i32 + doc / 40 * 13) % 7 < 4;
                let v = if line { 230 } else { 30 };
                image::Rgba([v, v, v, 255])
            })
        };
        let at = |scroll: i32| 300 - scroll;
        let mut scan = Scanner::new([W, H], 1., options(), 0);
        for i in 0..24 {
            let scroll = i * 9;
            let y = at(scroll);
            scan.frame(i as u64 * 33, draw(scroll), || {
                Ok(if (30..316).contains(&y) {
                    vec![Word {
                        text: VALUE.into(),
                        line: 0,
                        bounds: [100., y as f32, 130., 14.],
                    }]
                } else {
                    vec![]
                })
            })
            .unwrap();
        }
        let tracks = scan.finish(24 * 33).unwrap();
        assert!(!tracks.is_empty());
        for t in &tracks {
            for n in t.path.nodes.iter().filter(|n| n.visible) {
                assert!(
                    n.rect[1] >= 28. && n.rect[1] + n.rect[3] <= 332.,
                    "{:?}",
                    n.rect
                );
            }
        }
    }
    #[test]
    fn a_value_leaving_the_page_is_carried_until_it_is_gone() {
        let log: Vec<(u64, f32)> = (0..20).map(|i| (i * 33, i as f32 * -40.)).collect();
        let later = carry_forward(&log, 330, [10., 80., 50., 20.], 360.);
        assert_eq!(later.first().unwrap().0, 330);
        // Two more frames of 40 px and the 20 px value has left the top.
        assert_eq!(later.len(), 3, "{later:?}");
        assert!(later.iter().all(|(_, r)| r[1] >= 0.));
        // Nothing to carry once the page has stopped.
        let still: Vec<(u64, f32)> = (0..30).map(|i| (i * 33, 0.)).collect();
        assert!(carry_forward(&still, 330, [10., 80., 50., 20.], 360.).is_empty());
    }
    #[test]
    fn ink_finds_detail_and_blank_areas() {
        let frame = RgbaImage::from_fn(120, 60, |x, y| {
            let v = if (30..60).contains(&x) && (12..24).contains(&y) && x % 4 < 2 {
                235
            } else {
                30
            };
            image::Rgba([v, v, v, 255])
        });
        let ink = Ink::new(&frame);
        assert!(!ink.blank([28., 10., 40., 16.]));
        assert!(ink.blank([70., 30., 40., 20.]));
    }
    #[test]
    fn a_shorter_reading_keeps_the_widest_box() {
        let known = [100., 50., 130., 14.];
        assert_eq!(
            keep_widest(known, [100., 62., 90., 14.], VALUE, "sam@mail.te", 640.),
            [100., 62., 130., 14.]
        );
        let tail = keep_widest(known, [160., 50., 70., 14.], VALUE, "mail.test", 640.);
        assert!((tail[0] - 120.).abs() < 0.5 && tail[2] == 130., "{tail:?}");
        assert!(same_value(VALUE, "mail.test"));
        assert!(!same_value(VALUE, "sam"));
        assert!(!same_value(VALUE, "other@mail.test"));
    }
    #[test]
    fn fragments_ignore_short_pieces_and_unrelated_words() {
        let word = |t: &str| Word {
            text: t.into(),
            line: 0,
            bounds: [0., 0., 10., 10.],
        };
        let known = vec![VALUE.to_owned()];
        let hits = fragments(
            &[word("test,"), word("sam"), word("hello"), word("mail.")],
            &known,
        );
        assert_eq!(hits.len(), 2);
    }
    #[test]
    fn backfill_follows_the_scroll_and_stops_on_a_still_page_or_the_edge() {
        let mut log: Vec<(u64, f32)> = (0..30).map(|i| (i * 33, 0.)).collect();
        log.extend((30..40).map(|i| (i * 33, (i as f32 - 29.) * 20.)));
        let start = 39 * 33;
        let r = [10., 300., 50., 14.];
        let back = backfill(&log, start, r, 360.);
        assert!(!back.is_empty());
        assert_eq!(back.last().unwrap().1[1], 280.);
        assert!(back.first().unwrap().0 >= 29 * 33 - STILL_MS);
        assert!(back.iter().all(|(_, b)| b[1] >= 0.));
        assert!(
            backfill(
                &(0..30).map(|i| (i * 33, 0.)).collect::<Vec<_>>(),
                900,
                r,
                360.
            )
            .is_empty()
        );
    }
}
