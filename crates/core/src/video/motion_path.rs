//! Editable timed geometry shared by recorded cursors and reviewed redactions.
use crate::{Annotation, Error, Result, bitmap::Bitmap};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Purpose {
    Cursor,
    Redaction,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub at: u64,
    pub rect: [f32; 4],
    pub visible: bool,
    pub sprite: usize,
    pub confidence: f32,
}
/// How a recorded cursor path is simplified. The recording itself is kept in
/// [`Path::recorded`], so every mode can be switched or retuned without loss.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reduction {
    /// Use every recorded movement sample.
    #[default]
    Off,
    /// Rest at each click, then travel directly to the next click, starting this
    /// many milliseconds before it. Drags keep their full movement.
    Clicks(u16),
    /// Drop movement samples closer than this many source pixels to the previous kept one.
    Distance(u16),
}
impl Reduction {
    pub const DEFAULT_DISTANCE: u16 = 24;
    pub const MIN_DISTANCE: u16 = 4;
    pub const MAX_DISTANCE: u16 = 200;
    /// "Move before click" lead time (approved Capture Video mockup).
    pub const DEFAULT_LEAD: u16 = 600;
    pub const MIN_LEAD: u16 = 200;
    pub const MAX_LEAD: u16 = 2000;
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Clicks(_) => "Direct paths between clicks",
            Self::Distance(_) => "Distance-based reduction",
        }
    }
    fn is_off(&self) -> bool {
        *self == Self::Off
    }
}
/// A recorded press, in path time. A drag keeps its movement until `release`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub at: u64,
    pub dragging: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Path {
    pub purpose: Purpose,
    pub reference: [f32; 4],
    pub nodes: Vec<Node>,
    pub sprites: Vec<Arc<Bitmap>>,
    pub smooth: bool,
    pub enabled: bool,
    pub offset_ms: u64,
    pub reviewed: bool,
    #[serde(default, skip_serializing_if = "Reduction::is_off")]
    pub reduction: Reduction,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Input>,
    /// The unreduced recording. `nodes` is derived from it while a reduction is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recorded: Option<Arc<Vec<Node>>>,
}
/// Position between two visible nodes. A cursor-shape change between them
/// still glides; only the shape switches at the later node.
/// How far ahead a redaction looks for the node of the frame being drawn:
/// half a frame at 25 fps, the slowest project rate.
pub const NEAREST_FRAME_MS: u64 = 20;
fn interpolate(nodes: &[Node], at: u64) -> Node {
    let index = nodes.partition_point(|n| n.at <= at).saturating_sub(1);
    let mut out = nodes[index].clone();
    if let Some(next) = nodes.get(index + 1)
        && out.visible
        && next.visible
        && at > out.at
    {
        let t = (at - out.at) as f32 / (next.at - out.at) as f32;
        let fields = if out.sprite == next.sprite { 4 } else { 2 };
        for (i, value) in out.rect.iter_mut().enumerate().take(fields) {
            *value += (next.rect[i] - *value) * t;
        }
    }
    out.at = at;
    out
}
/// Recomputes visible cursor nodes from the recording. Click anchors keep their
/// recorded times and positions; drag spans and visibility changes are always
/// preserved. Direct paths also drop release and cursor-shape keys and add a
/// hold key so the cursor rests at each click until shortly before the next.
pub fn reduce(recorded: &[Node], inputs: &[Input], reduction: Reduction) -> Vec<Node> {
    let direct = matches!(reduction, Reduction::Clicks(_));
    // A recording without click data has nothing to keep between its ends.
    if recorded.len() < 2 || reduction == Reduction::Off || (direct && inputs.is_empty()) {
        return recorded.to_vec();
    }
    let first = recorded[0].at;
    let last = recorded[recorded.len() - 1].at;
    let mut anchors = vec![first, last];
    let mut drags = vec![];
    for (i, input) in inputs.iter().enumerate() {
        anchors.push(input.at);
        if input.dragging {
            let until = input
                .release
                .or_else(|| inputs.get(i + 1).map(|n| n.at))
                .unwrap_or(last);
            anchors.push(until);
            drags.push((input.at, until));
        }
        if let Some(release) = input.release
            && !direct
        {
            anchors.push(release);
        }
    }
    anchors.retain(|at| (first..=last).contains(at));
    anchors.sort_unstable();
    anchors.dedup();
    let in_drag = |at: u64| drags.iter().any(|(a, b)| (*a..=*b).contains(&at));
    let mut out: Vec<Node> = anchors
        .iter()
        .map(|at| interpolate(recorded, *at))
        .collect();
    let mut kept: Option<[f32; 2]> = None;
    for (i, node) in recorded.iter().enumerate() {
        // Visibility changes always keep their keys; shape changes only outside direct paths.
        let changed =
            |a: &Node, b: &Node| a.visible != b.visible || !direct && a.sprite != b.sprite;
        let state_change = i > 0 && changed(&recorded[i - 1], node);
        let before_change = recorded.get(i + 1).is_some_and(|n| changed(node, n));
        let keep = in_drag(node.at)
            || state_change
            || before_change
            || match reduction {
                Reduction::Off => true,
                Reduction::Clicks(_) => false,
                Reduction::Distance(px) => {
                    let anchor = anchors.binary_search(&node.at).is_ok();
                    let far = kept.is_none_or(|k| {
                        (node.rect[0] - k[0]).hypot(node.rect[1] - k[1]) >= f32::from(px)
                    });
                    anchor || far
                }
            };
        if keep {
            kept = Some([node.rect[0], node.rect[1]]);
            out.push(node.clone());
        } else if anchors.binary_search(&node.at).is_ok() {
            kept = Some([node.rect[0], node.rect[1]]);
        }
    }
    out.sort_by_key(|n| n.at);
    out.dedup_by_key(|n| n.at);
    if let Reduction::Clicks(lead) = reduction {
        // Rest at each key until `lead` ms before the next one, then travel.
        // Drags and visibility changes keep their own timing.
        let lead = u64::from(lead);
        let mut holds = vec![];
        for pair in out.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            let hold = b.at.saturating_sub(lead);
            if hold > a.at
                && a.visible
                && b.visible
                && !drags
                    .iter()
                    .any(|(s, e)| (*s..=*e).contains(&a.at) && (*s..=*e).contains(&b.at))
                && a.rect[..2] != b.rect[..2]
            {
                let mut h = a.clone();
                h.at = hold;
                holds.push(h);
            }
        }
        out.extend(holds);
        out.sort_by_key(|n| n.at);
    }
    out
}
impl Path {
    /// Applies `reduction` to a cursor path, keeping the original recording.
    pub fn set_reduction(&mut self, reduction: Reduction) {
        if self.purpose != Purpose::Cursor {
            return;
        }
        let reduction = match reduction {
            Reduction::Distance(px) => {
                Reduction::Distance(px.clamp(Reduction::MIN_DISTANCE, Reduction::MAX_DISTANCE))
            }
            Reduction::Clicks(lead) => {
                Reduction::Clicks(lead.clamp(Reduction::MIN_LEAD, Reduction::MAX_LEAD))
            }
            other => other,
        };
        let recorded = self
            .recorded
            .get_or_insert_with(|| Arc::new(self.nodes.clone()))
            .clone();
        self.reduction = reduction;
        self.nodes = reduce(&recorded, &self.inputs, reduction);
    }
    pub fn remap_time(&mut self, base: u64, map: impl Fn(u64) -> u64, anchors: &[u64]) {
        let mut nodes = vec![{
            let mut n = self.sample(0);
            n.at = self.offset_ms;
            n
        }];
        nodes.extend(
            self.nodes
                .iter()
                .filter(|n| n.at >= self.offset_ms)
                .cloned(),
        );
        for at in anchors.iter().copied().filter(|at| *at >= base) {
            let mut n = self.sample(at - base);
            n.at = at - base + self.offset_ms;
            nodes.push(n);
        }
        nodes.sort_by_key(|n| n.at);
        let offset = self.offset_ms;
        let time = |at: u64| map(base + at - offset).saturating_sub(map(base));
        let mut mapped = std::collections::BTreeMap::new();
        for mut n in nodes {
            n.at = time(n.at);
            mapped.insert(n.at, n);
        }
        self.nodes = mapped.into_values().collect();
        if let Some(recorded) = &self.recorded {
            let mut start = interpolate(recorded, offset);
            start.at = offset;
            let mut mapped = std::collections::BTreeMap::new();
            for mut n in
                std::iter::once(start).chain(recorded.iter().filter(|n| n.at > offset).cloned())
            {
                n.at = time(n.at);
                mapped.insert(n.at, n);
            }
            self.recorded = Some(Arc::new(mapped.into_values().collect()));
        }
        self.inputs = self
            .inputs
            .iter()
            .filter(|i| i.at >= offset)
            .map(|i| Input {
                at: time(i.at),
                dragging: i.dragging,
                release: i.release.filter(|r| *r >= offset).map(time),
            })
            .collect();
        self.offset_ms = 0;
        if self.purpose == Purpose::Redaction {
            self.reviewed = false;
        }
    }
    pub fn validate(&self) -> Result<()> {
        let valid_rect = |r: [f32; 4]| {
            r.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.) && r[2] > 0. && r[3] > 0.
        };
        let valid_nodes = |nodes: &[Node]| {
            !nodes.is_empty()
                && nodes.len() <= 220_000
                && nodes.windows(2).all(|n| n[0].at < n[1].at)
                && nodes.iter().all(|n| {
                    n.at <= 14_400_000
                        && valid_rect(n.rect)
                        && n.confidence.is_finite()
                        && (0. ..=1.).contains(&n.confidence)
                        && (self.sprites.is_empty() || n.sprite < self.sprites.len())
                })
        };
        if self.recorded.as_ref().is_some_and(|r| !valid_nodes(r))
            || self.inputs.len() > 100_000
            || self.inputs.windows(2).any(|i| i[0].at > i[1].at)
            || self
                .inputs
                .iter()
                .any(|i| i.at > 14_400_000 || i.release.is_some_and(|r| r < i.at || r > 14_400_000))
            || matches!(self.reduction, Reduction::Distance(px) if !(Reduction::MIN_DISTANCE..=Reduction::MAX_DISTANCE).contains(&px))
            || matches!(self.reduction, Reduction::Clicks(lead) if !(Reduction::MIN_LEAD..=Reduction::MAX_LEAD).contains(&lead))
            || self.reduction != Reduction::Off && self.purpose != Purpose::Cursor
        {
            return Err(Error::Invalid("Invalid cursor recording or path reduction. Keep ordered inputs and a distance between 4 and 200 px.".into()));
        }
        if self.nodes.is_empty()
            || self.nodes.len() > 220_000
            || self.sprites.len() > 64
            || !valid_rect(self.reference)
            || self.offset_ms > 14_400_000
            || self.nodes.windows(2).any(|n| n[0].at >= n[1].at)
            || self.nodes.iter().any(|n| {
                n.at > 14_400_000
                    || !valid_rect(n.rect)
                    || !n.confidence.is_finite()
                    || !(0. ..=1.).contains(&n.confidence)
                    || !self.sprites.is_empty() && n.sprite >= self.sprites.len()
            })
            || self
                .sprites
                .iter()
                .any(|b| b.size().iter().any(|v| *v > 512))
        {
            return Err(Error::Invalid("Invalid cursor or tracking path. Keep valid, ordered keyframes and local cursor artwork.".into()));
        }
        Ok(())
    }
    /// Samples the unreduced recording (or the nodes when nothing was reduced).
    pub fn sample_recorded(&self, age: u64) -> Node {
        match &self.recorded {
            Some(recorded) if !recorded.is_empty() => self.sample_in(recorded, age),
            _ => self.sample(age),
        }
    }
    pub fn sample(&self, age: u64) -> Node {
        self.sample_in(&self.nodes, age)
    }
    fn sample_in(&self, nodes: &[Node], age: u64) -> Node {
        let at = age.saturating_add(self.offset_ms);
        let mut index = nodes.partition_point(|n| n.at <= at).saturating_sub(1);
        // A redaction node belongs to one frame. If its stamp is slightly off
        // the frame being drawn, it still covers that frame; the box before it
        // is a frame out of date and would leave the value bare while moving.
        if self.purpose == Purpose::Redaction
            && let Some(next) = nodes.get(index + 1)
            && next.at - at <= NEAREST_FRAME_MS
            && next.at - at < at.saturating_sub(nodes[index].at)
        {
            index += 1;
        }
        let mut out = nodes[index].clone();
        if let Some(next) = nodes.get(index + 1)
            && self.purpose != Purpose::Redaction
            && out.visible
            && next.visible
            && at >= out.at
        {
            let mut t = (at - out.at) as f32 / (next.at - out.at) as f32;
            if self.smooth {
                t = t * t * (3. - 2. * t);
            }
            // A shape change still glides in position; the shape switches at the next key.
            let fields = if out.sprite == next.sprite { 4 } else { 2 };
            for (i, value) in out.rect.iter_mut().enumerate().take(fields) {
                *value += (next.rect[i] - *value) * t;
            }
            out.confidence = out.confidence.min(next.confidence);
        }
        out
    }
    pub fn apply(&self, a: &Annotation, age: u64) -> Option<Annotation> {
        if !self.enabled {
            return Some(a.clone());
        }
        let node = self.sample(age);
        if !node.visible {
            return None;
        }
        let mut out = a.clone();
        let (lo, hi) = a.body_bounds();
        let scale = [
            (hi[0] - lo[0]) / self.reference[2],
            (hi[1] - lo[1]) / self.reference[3],
        ];
        out.start = [0, 1].map(|i| lo[i] + (node.rect[i] - self.reference[i]) * scale[i]);
        out.end = [0, 1].map(|i| out.start[i] + node.rect[i + 2] * scale[i]);
        if let Some(bitmap) = self.sprites.get(node.sprite) {
            out.image = Some(bitmap.clone());
        }
        Some(out)
    }
    /// Remove idle samples without altering a click/drag anchor or cursor shape.
    pub fn push(&mut self, node: Node) {
        if self.nodes.last().is_some_and(|n| n.at >= node.at) {
            return;
        }
        if self.nodes.len() >= 2 {
            let a = &self.nodes[self.nodes.len() - 2];
            let b = self.nodes.last().unwrap();
            if a.rect == b.rect
                && b.rect == node.rect
                && a.visible == b.visible
                && b.visible == node.visible
                && a.sprite == b.sprite
                && b.sprite == node.sprite
            {
                self.nodes.pop();
            }
        }
        if self.nodes.len() < 220_000 {
            self.nodes.push(node);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> Path {
        Path {
            purpose: Purpose::Cursor,
            reference: [0., 0., 20., 20.],
            nodes: vec![
                Node {
                    at: 0,
                    rect: [0., 0., 20., 20.],
                    visible: true,
                    sprite: 0,
                    confidence: 1.,
                },
                Node {
                    at: 1000,
                    rect: [100., 40., 20., 20.],
                    visible: true,
                    sprite: 0,
                    confidence: 1.,
                },
            ],
            sprites: vec![],
            smooth: true,
            enabled: true,
            offset_ms: 0,
            reviewed: true,
            reduction: Reduction::Off,
            inputs: vec![],
            recorded: None,
        }
    }
    fn node(at: u64, x: f32, y: f32) -> Node {
        Node {
            at,
            rect: [x, y, 20., 20.],
            visible: true,
            sprite: 0,
            confidence: 1.,
        }
    }
    /// A wandering recording: clicks at 1000 and 3000, a drag from 4000 to 5000.
    fn recording() -> Path {
        let mut p = path();
        p.nodes = (0..=60)
            .map(|i| {
                let at = i * 100;
                node(
                    at,
                    at as f32 * 0.1 + (i % 3) as f32 * 4.,
                    (i % 5) as f32 * 3.,
                )
            })
            .collect();
        p.inputs = vec![
            Input {
                at: 1000,
                dragging: false,
                release: Some(1080),
            },
            Input {
                at: 3000,
                dragging: false,
                release: None,
            },
            Input {
                at: 4000,
                dragging: true,
                release: Some(5000),
            },
        ];
        p
    }
    #[test]
    fn direct_paths_rest_at_each_click_then_travel_before_the_next() {
        let mut p = recording();
        let original = p.nodes.clone();
        p.set_reduction(Reduction::Clicks(600));
        p.validate().unwrap();
        // Between the clicks at 1000 and 3000 only the hold key remains: no
        // release key, no shape key, no movement samples (AZ1).
        let between: Vec<_> = p
            .nodes
            .iter()
            .filter(|n| n.at > 1000 && n.at < 3000)
            .collect();
        assert_eq!(
            between.len(),
            1,
            "{:?}",
            between.iter().map(|n| n.at).collect::<Vec<_>>()
        );
        let click = p.nodes.iter().find(|n| n.at == 1000).unwrap();
        assert_eq!(between[0].at, 2400);
        assert_eq!(
            between[0].rect, click.rect,
            "the cursor rests at the click until 600 ms before the next"
        );
        // So halfway between, the cursor is still at the first click.
        assert_eq!(p.sample(1800).rect[..2], click.rect[..2]);
        // Click positions and times are unchanged.
        for at in [1000, 3000, 4000] {
            let n = p.nodes.iter().find(|n| n.at == at).unwrap();
            assert_eq!(n.rect, original.iter().find(|o| o.at == at).unwrap().rect);
        }
        // A cursor-shape change between clicks adds no key.
        let mut shaped = recording();
        for n in shaped
            .nodes
            .iter_mut()
            .filter(|n| (1500..2000).contains(&n.at))
        {
            n.sprite = 1;
        }
        shaped.sprites = vec![];
        shaped.set_reduction(Reduction::Clicks(600));
        assert_eq!(
            shaped
                .nodes
                .iter()
                .filter(|n| n.at > 1000 && n.at < 3000)
                .count(),
            1
        );
        // Lead is clamped to the supported range.
        p.set_reduction(Reduction::Clicks(50));
        assert_eq!(p.reduction, Reduction::Clicks(Reduction::MIN_LEAD));
        // The drag keeps every recorded sample.
        let drag = |nodes: &[Node]| {
            nodes
                .iter()
                .filter(|n| (4000..=5000).contains(&n.at))
                .count()
        };
        p.set_reduction(Reduction::Clicks(600));
        assert_eq!(drag(&p.nodes), drag(&original));
        // Reversible: Off restores the exact recording.
        p.set_reduction(Reduction::Off);
        assert_eq!(p.nodes, original);
    }
    #[test]
    fn deleting_a_key_glides_even_across_a_shape_change() {
        let mut p = path();
        p.nodes[1].sprite = 1;
        p.sprites = vec![];
        // AZ3: the cursor moves smoothly between the remaining keys.
        let mid = p.sample(500);
        assert!(
            (mid.rect[0] - 50.).abs() < 0.01 && (mid.rect[1] - 20.).abs() < 0.01,
            "{:?}",
            mid.rect
        );
    }
    #[test]
    fn distance_reduction_follows_threshold_and_keeps_anchors() {
        let mut p = recording();
        let original = p.nodes.len();
        p.set_reduction(Reduction::Distance(8));
        let small = p.nodes.len();
        p.set_reduction(Reduction::Distance(40));
        let large = p.nodes.len();
        assert!(
            large < small && small < original,
            "{large} {small} {original}"
        );
        for at in [1000, 3000, 4000, 5000] {
            assert!(p.nodes.iter().any(|n| n.at == at), "anchor {at} kept");
        }
        p.set_reduction(Reduction::Distance(1));
        assert_eq!(p.reduction, Reduction::Distance(Reduction::MIN_DISTANCE));
        p.validate().unwrap();
        let q: Path = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(q, p);
    }
    #[test]
    fn retiming_keeps_recording_and_inputs_aligned() {
        let mut p = recording();
        p.set_reduction(Reduction::Clicks(600));
        p.offset_ms = 500;
        p.remap_time(10_000, |t| t * 2, &[]);
        assert_eq!(p.inputs[0].at, 1000);
        assert_eq!(p.inputs[0].release, Some(1160));
        p.set_reduction(Reduction::Clicks(600));
        p.validate().unwrap();
        assert!(p.nodes.iter().any(|n| n.at == 1000));
    }
    #[test]
    fn cursor_path_moves_and_resizes_without_baking_pixels() {
        let p = path();
        p.validate().unwrap();
        let mut a =
            crate::steps::marker(1, [30., 30.], 40, [255; 4], (1, false), [320, 180]).unwrap();
        a.kind = crate::Kind::Rectangle;
        a.text = None;
        let out = p.apply(&a, 500).unwrap();
        assert_eq!(out.start, [110., 50.]);
        assert_eq!(out.end, [150., 90.]);
        a.start = [0., 0.];
        a.end = [20., 20.];
        assert_eq!(p.apply(&a, 1000).unwrap().start, [100., 40.]);
    }
    #[test]
    fn path_trim_and_visibility_roundtrip() {
        let mut p = path();
        p.offset_ms = 500;
        assert_eq!(p.sample(0).rect[0], 50.);
        p.nodes[1].visible = false;
        assert!(!p.sample(500).visible);
        let q: Path = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(q, p);
        p.nodes[1].at = 0;
        assert!(p.validate().is_err());
    }
}
