//! Sparse property keys, independent of entrance/exit effects and source pixels.
use crate::{Annotation, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Property {
    Position,
    Size,
    Rotation,
    Opacity,
    Stroke,
    StrokeWidth,
    Fill,
    CornerRadius,
    Text,
    Crop,
}
impl Property {
    /// What one keyframe button press on `self` writes. On a video clip,
    /// position and size are a single node: keying only one of them leaves the
    /// other animating over a different stretch of the clip.
    pub fn node(self, media: bool) -> &'static [Self] {
        match self {
            Self::Position | Self::Size if media => &[Self::Position, Self::Size],
            Self::Position => &[Self::Position],
            Self::Size => &[Self::Size],
            Self::Rotation => &[Self::Rotation],
            Self::Opacity => &[Self::Opacity],
            Self::Stroke => &[Self::Stroke],
            Self::StrokeWidth => &[Self::StrokeWidth],
            Self::Fill => &[Self::Fill],
            Self::CornerRadius => &[Self::CornerRadius],
            Self::Text => &[Self::Text],
            Self::Crop => &[Self::Crop],
        }
    }
    pub const ANNOTATION: [Self; 9] = [
        Self::Position,
        Self::Size,
        Self::Rotation,
        Self::Stroke,
        Self::StrokeWidth,
        Self::Fill,
        Self::CornerRadius,
        Self::Text,
        Self::Opacity,
    ];
    pub const MEDIA: [Self; 8] = [
        Self::Position,
        Self::Size,
        Self::Rotation,
        Self::Opacity,
        Self::Stroke,
        Self::StrokeWidth,
        Self::CornerRadius,
        Self::Crop,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Position => "Position",
            Self::Size => "Size",
            Self::Rotation => "Rotation",
            Self::Opacity => "Opacity",
            Self::Stroke => "Stroke color",
            Self::StrokeWidth => "Stroke width",
            Self::Fill => "Fill color",
            Self::CornerRadius => "Corners",
            Self::Text => "Text & formatting",
            Self::Crop => "Crop footage",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Numbers(Vec<f32>),
    Color(Option<[u8; 4]>),
    Text(Option<Box<crate::text::TextStyle>>),
}
impl Value {
    fn lerp(&self, other: &Self, t: f32) -> Self {
        match (self, other) {
            (Self::Numbers(a), Self::Numbers(b)) => {
                Self::Numbers(a.iter().zip(b).map(|(a, b)| a + (b - a) * t).collect())
            }
            // BA7: colors blend, including from or to none (fades through transparent).
            (Self::Color(a), Self::Color(b)) => Self::Color(mix(*a, *b, t)),
            _ => self.clone(),
        }
    }
    /// Text keys stay discrete, but their colors (text, highlight, text stroke)
    /// blend toward the next key (BA7).
    /// `ink` is the color text without its own color is drawn in (the object's
    /// color), so text colored in only one key still blends from or to it.
    fn blend_text_colors(&self, other: &Self, t: f32, ink: Option<[u8; 4]>) -> Self {
        let (Self::Text(Some(a)), Self::Text(Some(b))) = (self, other) else {
            return self.clone();
        };
        let mut out = a.clone();
        if let (Some(x), Some(y)) = (a.color.or(ink), b.color.or(ink))
            && (a.color.is_some() || b.color.is_some())
        {
            out.color = mix(Some(x), Some(y), t);
        }
        // Text size moves smoothly toward the next key too (BG2 note).
        out.size_points = a.size_points + (b.size_points - a.size_points) * t;
        let mut typography = a.typography();
        typography.highlight = mix(a.typography().highlight, b.typography().highlight, t);
        out.set_typography(typography);
        // Text styled in the inline editor keeps colors per character run. Blend
        // them character by character, whatever runs each key has, so inline
        // colors move like colors set from the pane (BB8, BE3).
        if (a.rich.is_some() || b.rich.is_some()) && a.content == b.content {
            out.rich = Some(blend_runs(a, b, t, ink));
        }
        out.outline = match (&a.outline, &b.outline) {
            (None, None) => None,
            (x, y) => {
                let template = *x.as_ref().or(y.as_ref()).unwrap();
                let color = mix(x.as_ref().map(|o| o.color), y.as_ref().map(|o| o.color), t);
                let width = |o: &Option<crate::text::outline::Outline>| {
                    o.as_ref().map_or(template.width, |o| o.width)
                };
                let w = width(x) + (width(y) - width(x)) * t;
                color.map(|color| crate::text::outline::Outline { color, width: w })
            }
        };
        Self::Text(Some(out))
    }
    fn shifted(&self, old: &Self, new: &Self, property: Property) -> Self {
        match (self, old, new) {
            (Self::Numbers(v), Self::Numbers(a), Self::Numbers(b)) => Self::Numbers(
                v.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        if property == Property::Size && a[i].abs() > 0.0001 {
                            v * b[i] / a[i]
                        } else {
                            v + b[i] - a[i]
                        }
                    })
                    .collect(),
            ),
            (Self::Color(Some(v)), Self::Color(Some(a)), Self::Color(Some(b))) => {
                Self::Color(Some([0, 1, 2, 3].map(|i| {
                    (v[i] as i32 + b[i] as i32 - a[i] as i32).clamp(0, 255) as u8
                })))
            }
            (Self::Text(Some(v)), Self::Text(Some(a)), Self::Text(Some(b))) => {
                // Whole-clip formatting changes must not replace the distinct
                // words saved at other keys. Apply only fields the user edited.
                let mut out = serde_json::to_value(v).expect("serializable text style");
                let old = serde_json::to_value(a).expect("serializable text style");
                let new = serde_json::to_value(b).expect("serializable text style");
                if let (Some(out), Some(old), Some(new)) =
                    (out.as_object_mut(), old.as_object(), new.as_object())
                {
                    for key in old.keys().chain(new.keys()) {
                        if old.get(key) != new.get(key) {
                            if let Some(value) = new.get(key) {
                                out.insert(key.clone(), value.clone());
                            } else {
                                out.remove(key);
                            }
                        }
                    }
                    if a.content == b.content && v.content != b.content && a.rich != b.rich {
                        out.remove("rich");
                    }
                }
                Self::Text(Some(Box::new(
                    serde_json::from_value(out).unwrap_or_else(|_| v.as_ref().clone()),
                )))
            }
            _ => new.clone(),
        }
    }
}
/// Rich-text runs of  with each character's text color, highlight and text
/// stroke color blended toward the same character in  (same content).
fn blend_runs(
    a: &crate::text::TextStyle,
    b: &crate::text::TextStyle,
    t: f32,
    ink: Option<[u8; 4]>,
) -> crate::text::rich::RichText {
    use crate::text::rich::{RichText, Run, format_at};
    let n = a.content.chars().count();
    let mut cuts: Vec<usize> = [0, n]
        .into_iter()
        .chain([a, b].into_iter().flat_map(|s| {
            s.rich
                .iter()
                .flat_map(|r| r.runs.iter().flat_map(|r| [r.start, r.end]))
        }))
        .filter(|c| *c <= n)
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let both = |x: Option<[u8; 4]>, y: Option<[u8; 4]>| match (x.or(ink), y.or(ink)) {
        (Some(c), Some(d)) if x.is_some() || y.is_some() => mix(Some(c), Some(d), t),
        _ => x,
    };
    let mut runs: Vec<Run> = Vec::new();
    for w in cuts.windows(2) {
        let (start, end) = (w[0], w[1]);
        let (x, y) = (format_at(a, start), format_at(b, start));
        let mut format = x.clone();
        format.size = x.size + (y.size - x.size) * t;
        format.color = both(x.color, y.color);
        format.typography.highlight = mix(x.typography.highlight, y.typography.highlight, t);
        if let (Some(o), Some(p)) = (x.outline, y.outline) {
            format.outline = Some(crate::text::outline::Outline {
                color: mix(Some(o.color), Some(p.color), t).unwrap_or(o.color),
                width: o.width + (p.width - o.width) * t,
            });
        }
        match runs.last_mut() {
            Some(last) if last.end == start && last.format == format => last.end = end,
            _ => runs.push(Run { start, end, format }),
        }
    }
    let paragraphs = a
        .rich
        .as_ref()
        .or(b.rich.as_ref())
        .map(|r| r.paragraphs.clone())
        .unwrap_or_default();
    RichText { runs, paragraphs }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Curve {
    Linear,
    #[default]
    Smooth,
    Hold,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub at: u64,
    pub value: Value,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub curve: Curve,
    pub keys: Vec<Key>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Set {
    pub offset_ms: u64,
    pub channels: BTreeMap<Property, Channel>,
}
impl Set {
    pub fn trim_start(&mut self, delta: i64) {
        if !self.is_empty() {
            self.offset_ms = self.offset_ms.saturating_add_signed(delta);
        }
    }
    pub fn transform(&mut self, scale: [f32; 2], offset: [f32; 2]) {
        for (p, c) in &mut self.channels {
            for k in &mut c.keys {
                if let Value::Numbers(v) = &mut k.value {
                    if *p == Property::Position {
                        for i in 0..2 {
                            v[i] = v[i] * scale[i] + offset[i];
                        }
                    } else if *p == Property::Size {
                        for i in 0..2 {
                            v[i] *= scale[i];
                        }
                    }
                }
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
    pub fn sample(&self, p: Property, age: u64) -> Option<Value> {
        self.sample_ink(p, age, None)
    }
    /// [`Set::sample`] where text without its own color is drawn in `ink`.
    pub fn sample_ink(&self, p: Property, age: u64, ink: Option<[u8; 4]>) -> Option<Value> {
        let c = self.channels.get(&p)?;
        let at = age.saturating_add(self.offset_ms);
        let i = c.keys.partition_point(|k| k.at <= at).saturating_sub(1);
        let k = c.keys.get(i)?;
        let Some(next) = c.keys.get(i + 1) else {
            return Some(k.value.clone());
        };
        if at <= k.at {
            return Some(k.value.clone());
        }
        let mut t = (at - k.at) as f32 / (next.at - k.at) as f32;
        if p == Property::Text {
            return Some(
                k.value
                    .blend_text_colors(&next.value, t * t * (3. - 2. * t), ink),
            );
        }
        if c.curve == Curve::Hold {
            return Some(k.value.clone());
        }
        if c.curve == Curve::Smooth {
            t = t * t * (3. - 2. * t)
        }
        Some(k.value.lerp(&next.value, t))
    }
    pub fn insert(&mut self, p: Property, age: u64, initial: Value, value: Value) {
        let at = age.saturating_add(self.offset_ms);
        // A property keyed for the first time holds its old value up to the
        // latest earlier node, so its change starts there and not at the clip
        // start.
        let start = self.offset_ms;
        let earlier = self
            .channels
            .iter()
            .filter(|(q, _)| **q != p)
            .flat_map(|(_, c)| c.keys.iter())
            .map(|k| k.at)
            .filter(|t| *t > start && *t < at)
            .max();
        let c = self.channels.entry(p).or_default();
        if p == Property::Text {
            c.curve = Curve::Hold;
        }
        if c.keys.is_empty() && at > start {
            c.keys.push(Key {
                at: start,
                value: initial.clone(),
            });
            if let Some(at) = earlier {
                c.keys.push(Key { at, value: initial });
            }
        }
        match c.keys.binary_search_by_key(&at, |k| k.at) {
            Ok(i) => c.keys[i].value = value,
            Err(i) => c.keys.insert(i, Key { at, value }),
        }
    }
    /// Copies the nearest keyframe before `age` (every property keyed at that
    /// time, or only `property`) to `age`. Returns the copied key's age.
    pub fn duplicate_previous(&mut self, age: u64, property: Option<Property>) -> Option<u64> {
        let at = age.saturating_add(self.offset_ms);
        let from = self
            .channels
            .iter()
            .filter(|(p, _)| property.is_none_or(|v| v == **p))
            .flat_map(|(_, c)| c.keys.iter())
            .map(|k| k.at)
            .filter(|t| *t < at)
            .max()?;
        let copies: Vec<(Property, Value)> = self
            .channels
            .iter()
            .filter(|(p, _)| property.is_none_or(|v| v == **p))
            .filter_map(|(p, c)| {
                c.keys
                    .iter()
                    .find(|k| k.at == from)
                    .map(|k| (*p, k.value.clone()))
            })
            .collect();
        for (p, value) in copies {
            self.insert(p, age, value.clone(), value);
        }
        Some(from - self.offset_ms)
    }
    pub fn times(&self, duration: u64) -> BTreeSet<u64> {
        self.channels
            .values()
            .flat_map(|c| c.keys.iter())
            .filter(|k| k.at >= self.offset_ms && k.at - self.offset_ms < duration)
            .map(|k| k.at - self.offset_ms)
            .collect()
    }
    pub fn remove(&mut self, age: u64, property: Option<Property>) {
        let at = age + self.offset_ms;
        for (p, c) in &mut self.channels {
            if property.is_none_or(|v| v == *p) {
                c.keys.retain(|k| k.at != at)
            }
        }
        self.channels.retain(|_, c| !c.keys.is_empty());
        if self.channels.is_empty() {
            self.offset_ms = 0;
        }
    }
    pub fn move_key(&mut self, from: u64, to: u64, property: Option<Property>) -> Result<()> {
        let from = from + self.offset_ms;
        let to = to + self.offset_ms;
        if from == to {
            return Ok(());
        }
        if self.channels.iter().any(|(p, c)| {
            property.is_none_or(|v| v == *p)
                && c.keys.iter().any(|k| k.at == from)
                && c.keys.iter().any(|k| k.at == to)
        }) {
            return Err(Error::Invalid(
                "A keyframe already occupies that frame. Move to an empty frame.".into(),
            ));
        }
        for (p, c) in &mut self.channels {
            if property.is_none_or(|v| v == *p) {
                if let Some(k) = c.keys.iter_mut().find(|k| k.at == from) {
                    k.at = to;
                }
                c.keys.sort_by_key(|k| k.at);
            }
        }
        Ok(())
    }
    /// Copies the key at `from` to `to`; the original stays.
    pub fn copy_key(&mut self, from: u64, to: u64, property: Option<Property>) -> Result<()> {
        let from = from + self.offset_ms;
        let to = to + self.offset_ms;
        if from == to {
            return Ok(());
        }
        if self.channels.iter().any(|(p, c)| {
            property.is_none_or(|v| v == *p)
                && c.keys.iter().any(|k| k.at == from)
                && c.keys.iter().any(|k| k.at == to)
        }) {
            return Err(Error::Invalid(
                "A keyframe already occupies that frame. Move to an empty frame.".into(),
            ));
        }
        for (p, c) in &mut self.channels {
            if property.is_none_or(|v| v == *p)
                && let Some(mut copy) = c.keys.iter().find(|k| k.at == from).cloned()
            {
                copy.at = to;
                c.keys.push(copy);
                c.keys.sort_by_key(|k| k.at);
            }
        }
        Ok(())
    }
    pub fn remap_time(&mut self, base: u64, map: impl Fn(u64) -> u64, anchors: &[u64]) {
        let old = self.clone();
        for (p, c) in &mut self.channels {
            let mut keys: BTreeMap<u64, Value> = BTreeMap::new();
            if let Some(value) = old.sample(*p, 0) {
                keys.insert(0, value);
            }
            for k in &c.keys {
                if k.at >= old.offset_ms {
                    keys.insert(
                        map(base + k.at - old.offset_ms).saturating_sub(map(base)),
                        k.value.clone(),
                    );
                }
            }
            for at in anchors.iter().copied().filter(|t| *t >= base) {
                if let Some(value) = old.sample(*p, at - base) {
                    keys.insert(map(at).saturating_sub(map(base)), value);
                }
            }
            c.keys = keys
                .into_iter()
                .map(|(at, value)| Key { at, value })
                .collect();
        }
        self.offset_ms = 0;
    }
    pub fn validate(&self) -> Result<()> {
        let invalid = || {
            Error::Invalid("Keyframes need ordered times and valid property values (maximum 2,048 keys per property).".into())
        };
        if self.offset_ms > 14_400_000
            || self.channels.values().map(|c| c.keys.len()).sum::<usize>() > 8192
        {
            return Err(invalid());
        }
        for (p, c) in &self.channels {
            if c.keys.is_empty()
                || c.keys.len() > 2048
                || c.keys.windows(2).any(|v| v[0].at >= v[1].at)
            {
                return Err(invalid());
            }
            for k in &c.keys {
                if k.at > 14_400_000 {
                    return Err(invalid());
                }
                match (&k.value, p) {
                    (Value::Numbers(v), _) => {
                        let n = match p {
                            Property::Position | Property::Size => 2,
                            Property::Crop => 4,
                            Property::Rotation
                            | Property::Opacity
                            | Property::StrokeWidth
                            | Property::CornerRadius => 1,
                            _ => return Err(invalid()),
                        };
                        if v.len() != n || v.iter().any(|v| !v.is_finite() || v.abs() > 32768.) {
                            return Err(invalid());
                        }
                        if (*p == Property::Size && v.iter().any(|v| *v < 0.))
                            || (*p == Property::Opacity && !(0. ..=1.).contains(&v[0]))
                            || (*p == Property::Rotation && !(-360. ..=360.).contains(&v[0]))
                            || (*p == Property::StrokeWidth && !(0. ..=128.).contains(&v[0]))
                            || (*p == Property::CornerRadius && !(0. ..=2048.).contains(&v[0]))
                        {
                            return Err(invalid());
                        }
                        if *p == Property::Crop {
                            crate::image_frame::ImageView {
                                rect: [v[0], v[1], v[2], v[3]],
                            }
                            .validate()?;
                        }
                    }
                    (Value::Color(_), Property::Fill | Property::Stroke) => (),
                    (Value::Text(t), Property::Text) => {
                        if let Some(t) = t {
                            t.validate()?;
                        }
                    }
                    _ => return Err(invalid()),
                }
            }
        }
        Ok(())
    }
    pub fn annotation(&self, a: &Annotation, age: u64) -> Annotation {
        let mut out = a.clone();
        let ink = Some(text_ink(a));
        for p in Self::geometry_order() {
            if let Some(v) = self.sample_ink(p, age, ink) {
                set_annotation(&mut out, p, &v);
            }
        }
        out
    }
    pub fn editable_annotation(&self, a: &Annotation, age: u64) -> Annotation {
        let mut out = a.clone();
        for p in Property::ANNOTATION
            .into_iter()
            .filter(|p| *p != Property::Opacity)
        {
            if let Some(v) = self.sample_ink(p, age, Some(text_ink(a))) {
                set_annotation(&mut out, p, &v);
            }
        }
        out
    }
    fn geometry_order() -> [Property; 9] {
        Property::ANNOTATION
    }
    /// Incoming values are the editable values at the playhead, before entrance/exit effects.
    pub fn edit_annotation(
        &mut self,
        base: &Annotation,
        edited: &Annotation,
        age: u64,
        auto: bool,
    ) -> Annotation {
        let sampled = self.editable_annotation(base, age);
        let mut out = edited.clone();
        for p in Self::geometry_order()
            .into_iter()
            .filter(|p| *p != Property::Opacity)
        {
            let old = annotation_value(&sampled, p);
            let new = annotation_value(edited, p);
            let original = annotation_value(base, p);
            if old != new {
                if auto {
                    self.insert(p, age, original.clone(), new);
                    set_annotation(&mut out, p, &original);
                } else if let Some(c) = self.channels.get_mut(&p) {
                    for key in &mut c.keys {
                        key.value = key.value.shifted(&old, &new, p)
                    }
                    set_annotation(&mut out, p, &original.shifted(&old, &new, p));
                }
            } else if self.channels.contains_key(&p) {
                set_annotation(&mut out, p, &original);
            }
        }
        out
    }
    pub fn validate_media(&self, base: &super::editor::Segment) -> Result<()> {
        self.validate()?;
        if self.channels.keys().any(|p| !Property::MEDIA.contains(p)) {
            return Err(Error::Invalid(
                "This property cannot animate a video frame.".into(),
            ));
        }
        let mut times = self.times(14_400_000);
        times.insert(0);
        for at in times {
            let c = self.media(base, at);
            let r = c.rect;
            if r[2] <= 0.
                || r[3] <= 0.
                || r[2] > 1.
                || r[3] > 1.
                || r[0].abs() > 8.
                || r[1].abs() > 8.
            {
                return Err(Error::Invalid("Use an animated frame width and height above 0 and at most 1 (100% of the canvas).".into()));
            }
            if let Some(f) = c.frame {
                f.validate()?;
            }
        }
        Ok(())
    }
    pub fn media(&self, base: &super::editor::Segment, age: u64) -> super::editor::Segment {
        let mut out = base.clone();
        for p in Property::MEDIA {
            if let Some(v) = self.sample(p, age) {
                set_media(&mut out, p, &v)
            }
        }
        out
    }
    pub fn edit_media(
        &mut self,
        base: &super::editor::Segment,
        edited: &super::editor::Segment,
        age: u64,
        auto: bool,
    ) -> super::editor::Segment {
        let sampled = self.media(base, age);
        let mut out = edited.clone();
        // A keyed move or resize is one node: position and size stay together.
        let placed = auto && sampled.rect != edited.rect;
        for p in Property::MEDIA {
            let old = media_value(&sampled, p);
            let new = media_value(edited, p);
            let original = media_value(base, p);
            if old != new || (placed && matches!(p, Property::Position | Property::Size)) {
                if auto {
                    self.insert(p, age, original.clone(), new);
                    set_media(&mut out, p, &original)
                } else if let Some(c) = self.channels.get_mut(&p) {
                    for k in &mut c.keys {
                        k.value = k.value.shifted(&old, &new, p)
                    }
                    set_media(&mut out, p, &original.shifted(&old, &new, p));
                }
            } else if self.channels.contains_key(&p) {
                set_media(&mut out, p, &original);
            }
        }
        out.keyframes = self.clone();
        out
    }
}
pub fn media_value(c: &super::editor::Segment, p: Property) -> Value {
    let f = c.frame.clone().unwrap_or(super::frame::Frame {
        radius: 0.,
        ..Default::default()
    });
    match p {
        Property::Position => Value::Numbers(c.rect[..2].to_vec()),
        Property::Size => Value::Numbers(c.rect[2..].to_vec()),
        Property::Rotation => Value::Numbers(vec![f.rotation]),
        Property::Opacity => Value::Numbers(vec![f.opacity as f32 / 255.]),
        Property::Stroke => Value::Color(Some(f.outline)),
        Property::StrokeWidth => Value::Numbers(vec![f.width as f32]),
        Property::CornerRadius => Value::Numbers(vec![f.radius]),
        Property::Crop => Value::Numbers(f.view.rect.to_vec()),
        _ => Value::Numbers(vec![]),
    }
}
fn set_media(c: &mut super::editor::Segment, p: Property, v: &Value) {
    match (p, v) {
        (Property::Position, Value::Numbers(v)) => c.rect[..2].copy_from_slice(v),
        (Property::Size, Value::Numbers(v)) => c.rect[2..].copy_from_slice(v),
        _ => {
            let f = c.frame.get_or_insert_with(|| super::frame::Frame {
                radius: 0.,
                ..Default::default()
            });
            match (p, v) {
                (Property::Rotation, Value::Numbers(v)) => f.rotation = v[0],
                (Property::Opacity, Value::Numbers(v)) => f.opacity = (v[0] * 255.).round() as u8,
                (Property::Stroke, Value::Color(Some(v))) => f.outline = *v,
                (Property::StrokeWidth, Value::Numbers(v)) => f.width = v[0].round() as u32,
                (Property::CornerRadius, Value::Numbers(v)) => f.radius = v[0],
                (Property::Crop, Value::Numbers(v)) => f.view.rect.copy_from_slice(v),
                _ => (),
            }
        }
    }
}
impl super::editor::Segment {
    pub fn keyed(&self, time: u64) -> Self {
        self.keyframes.media(self, time.saturating_sub(self.at))
    }
}
/// Blends two optional colors; none blends through the other color at zero alpha.
fn mix(a: Option<[u8; 4]>, b: Option<[u8; 4]>, t: f32) -> Option<[u8; 4]> {
    let (a, b) = match (a, b) {
        (None, None) => return None,
        (Some(a), None) => (a, [a[0], a[1], a[2], 0]),
        (None, Some(b)) => ([b[0], b[1], b[2], 0], b),
        (Some(a), Some(b)) => (a, b),
    };
    Some([0, 1, 2, 3].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t).round() as u8))
}
/// The color an object's text is drawn in when the text has no color of its own.
fn text_ink(a: &Annotation) -> [u8; 4] {
    if a.kind == crate::Kind::Step {
        crate::steps::foreground(a).color
    } else {
        a.color
    }
}
pub fn annotation_value(a: &Annotation, p: Property) -> Value {
    let (lo, hi) = a.body_bounds();
    // A step marker's fill is its color and its outline is separate (BA7).
    if a.kind == crate::Kind::Step {
        match p {
            Property::Fill => return Value::Color(Some(a.color)),
            Property::Stroke => return Value::Color(a.step_outline.as_ref().map(|o| o.color)),
            Property::StrokeWidth => {
                return Value::Numbers(vec![a.step_outline.as_ref().map_or(0., |o| o.width)]);
            }
            _ => (),
        }
    }
    match p {
        Property::Position => Value::Numbers(lo.to_vec()),
        Property::Size => Value::Numbers(vec![hi[0] - lo[0], hi[1] - lo[1]]),
        Property::Rotation => Value::Numbers(vec![a.rotation_degrees]),
        Property::Opacity => Value::Numbers(vec![1.]),
        Property::Stroke => Value::Color(Some(a.color)),
        Property::StrokeWidth => Value::Numbers(vec![a.width as f32]),
        Property::Fill => Value::Color(a.callout.as_ref().map(|c| c.fill).or(a.fill)),
        Property::CornerRadius => Value::Numbers(vec![a.corner_radius]),
        Property::Text => Value::Text(a.text.clone().map(Box::new)),
        Property::Crop => Value::Numbers(vec![0., 0., 1., 1.]),
    }
}
fn set_annotation(a: &mut Annotation, p: Property, v: &Value) {
    if a.kind == crate::Kind::Step {
        match (p, v) {
            (Property::Fill, Value::Color(Some(v))) => return a.color = *v,
            (Property::Stroke, Value::Color(v)) => {
                let width = a.step_outline.as_ref().map(|o| o.width);
                a.step_outline = v.map(|color| crate::steps::Outline {
                    color,
                    width: width.unwrap_or(crate::steps::Outline::default().width),
                });
                return;
            }
            (Property::StrokeWidth, Value::Numbers(v)) => {
                if let Some(o) = &mut a.step_outline {
                    o.width = v[0];
                }
                return;
            }
            _ => (),
        }
    }
    match (p, v) {
        (Property::Position | Property::Size, Value::Numbers(v)) => {
            let (lo, hi) = a.body_bounds();
            let scale = [0, 1].map(|i| {
                if p == Property::Size && (hi[i] - lo[i]).abs() > 0.00001 {
                    v[i] / (hi[i] - lo[i])
                } else {
                    1.
                }
            });
            let offset = [0, 1].map(|i| if p == Property::Position { v[i] } else { lo[i] });
            let map = |point: &mut [f32; 2]| {
                for i in 0..2 {
                    point[i] = offset[i] + (point[i] - lo[i]) * scale[i];
                }
            };
            map(&mut a.start);
            map(&mut a.end);
            for point in &mut a.points {
                map(point)
            }
            if let Some(c) = &mut a.callout {
                map(&mut c.pointer)
            }
        }
        (Property::Rotation, Value::Numbers(v)) => a.rotation_degrees = v[0],
        (Property::StrokeWidth, Value::Numbers(v)) => a.width = v[0].round() as u32,
        (Property::CornerRadius, Value::Numbers(v)) => a.corner_radius = v[0],
        (Property::Stroke, Value::Color(Some(v))) => {
            a.color = *v;
            if let Some(c) = &mut a.callout {
                c.outline = *v;
            }
        }
        (Property::Fill, Value::Color(v)) => {
            a.fill = *v;
            if let (Some(c), Some(v)) = (&mut a.callout, v) {
                c.fill = *v;
            }
        }
        (Property::Text, Value::Text(v)) => a.text = v.as_deref().cloned(),
        (Property::Opacity, Value::Numbers(v)) => super::editor::apply_opacity(a, v[0]),
        _ => (),
    }
}
/// Apply a group of edits atomically to an otherwise unchanged project snapshot.
pub fn edit_annotations(doc: &mut crate::Document, edits: &[Annotation], auto: bool) -> Result<()> {
    let Some(p) = doc.video.as_mut() else {
        return Ok(());
    };
    for edited in edits {
        let Some(t) = p.objects.get_mut(&edited.id) else {
            continue;
        };
        if auto && !t.active(p.cursor) {
            return Err(Error::Invalid(
                "Put the playhead inside the object’s bar, then add the keyframe.".into(),
            ));
        }
        let Some(base) = doc.annotations.iter_mut().find(|a| a.id == edited.id) else {
            continue;
        };
        if auto || !t.keyframes.is_empty() {
            *base =
                t.keyframes
                    .edit_annotation(base, edited, p.cursor.saturating_sub(t.start), auto);
        } else {
            *base = edited.clone();
        }
        t.keyframes.validate()?;
    }
    Ok(())
}

impl super::editor::Project {
    pub fn sampled_media(&self, time: u64) -> std::borrow::Cow<'_, Self> {
        if !self
            .segments
            .iter()
            .any(|c| c.active(time) && !c.keyframes.is_empty())
        {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut p = self.clone();
        for c in &mut p.segments {
            if c.active(time) && !c.keyframes.is_empty() {
                *c = c.keyed(time);
                c.keyframes = Set::default();
            }
        }
        std::borrow::Cow::Owned(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn marker() -> Annotation {
        crate::steps::marker(
            1,
            [120., 120.],
            44,
            [20, 150, 190, 255],
            (1, false),
            [640, 360],
        )
        .unwrap()
    }
    #[test]
    fn edits_interpolate_and_whole_clip_changes_preserve_motion() {
        let base = marker();
        let mut keys = Set::default();
        let mut edited = base.translated([120., 40.], [640, 360]).unwrap();
        edited.rotation_degrees = 90.;
        let canonical = keys.edit_annotation(&base, &edited, 1000, true);
        assert_eq!(canonical, base);
        assert_eq!(keys.editable_annotation(&base, 1000), edited);
        let halfway = keys.editable_annotation(&base, 500);
        assert!((halfway.start[0] - base.start[0] - 60.).abs() < 0.01);
        assert_eq!(halfway.rotation_degrees, 45.);
        let moved = halfway.translated([20., 10.], [640, 360]).unwrap();
        let next = keys.edit_annotation(&base, &moved, 500, false);
        assert_eq!(keys.editable_annotation(&next, 500), moved);
        assert!(
            (keys.editable_annotation(&next, 1000).start[0] - edited.start[0] - 20.).abs() < 0.01
        );
        let mut end = keys.editable_annotation(&next, 1000);
        end.end[0] += 20.;
        let canonical = keys.edit_annotation(&next, &end, 1000, true);
        assert_eq!(keys.editable_annotation(&canonical, 1000), end);
        assert_eq!(
            serde_json::from_str::<Set>(&serde_json::to_string(&keys).unwrap()).unwrap(),
            keys
        );
    }
    #[test]
    fn text_keys_hold_and_complete_opacity_preserves_privacy() {
        let base = marker();
        let mut edited = base.clone();
        edited.text.as_mut().unwrap().content = "Go".into();
        let mut keys = Set::default();
        keys.edit_annotation(&base, &edited, 1000, true);
        assert_eq!(keys.annotation(&base, 999).text.unwrap().content, "1");
        assert_eq!(keys.annotation(&base, 1000).text.unwrap().content, "Go");
        let mut formatted = keys.editable_annotation(&base, 1000);
        formatted.text.as_mut().unwrap().bold = true;
        let canonical = keys.edit_annotation(&base, &formatted, 1000, false);
        let first = keys.annotation(&canonical, 0).text.unwrap();
        assert_eq!(first.content, "1");
        assert!(first.bold);
        assert_eq!(
            keys.annotation(&canonical, 1000).text.unwrap().content,
            "Go"
        );
        keys.insert(
            Property::Opacity,
            0,
            Value::Numbers(vec![1.]),
            Value::Numbers(vec![0.25]),
        );
        assert_eq!(keys.annotation(&base, 0).color[3], 64);
        let mut private = base.clone();
        private.kind = crate::Kind::Redact;
        private.text = None;
        assert_eq!(keys.annotation(&private, 0).color[3], 255);
    }
    #[test]
    fn media_keys_split_trim_and_retime_without_changing_source() {
        let mut p = super::super::editor::Project::new("movie.mp4".into(), [640, 360], 4000, 30);
        let c = &mut p.segments[0];
        c.keyframes.insert(
            Property::Position,
            2000,
            Value::Numbers(vec![0., 0.]),
            Value::Numbers(vec![0.4, 0.2]),
        );
        c.keyframes.insert(
            Property::Size,
            0,
            Value::Numbers(vec![1., 1.]),
            Value::Numbers(vec![0.5, 0.5]),
        );
        let before = c.keyed(1500);
        p.split_clip(0, 1000).unwrap();
        let i = p.segments.iter().position(|c| c.at == 1000).unwrap();
        assert_eq!(p.segments[i].keyed(1500).rect, before.rect);
        let mut edited = p.segments[i].clone();
        edited.at += 500;
        edited.start += 500;
        p.edit_linked(i, edited, false).unwrap();
        assert_eq!(p.segments[i].keyed(1500).rect, before.rect);
        let copy = p.sampled_media(2000);
        assert!(copy.segments[i].keyframes.is_empty());
        assert!(!p.segments[i].keyframes.is_empty());
    }
    /// Reported with Keyframes on: a fullscreen node, then a corner inset.
    /// Every frame up to the node stays exactly fullscreen.
    #[test]
    fn a_later_corner_inset_leaves_the_fullscreen_stretch_alone() {
        let p = super::super::editor::Project::new("movie.mp4".into(), [640, 360], 10_000, 30);
        let base = p.segments[0].clone();
        let mut keys = Set::default();
        for q in Property::Position.node(true) {
            let value = media_value(&base, *q);
            keys.insert(*q, 4000, value.clone(), value);
        }
        assert!(keys.channels.contains_key(&Property::Size));
        let mut edited = base.clone();
        edited.rect = [0.69, 0.62, 0.28, 0.32];
        let keyed = keys.edit_media(&base, &edited, 5000, true);
        assert_eq!(keyed.rect, [0., 0., 1., 1.]);
        for age in (0..=4000).step_by(100) {
            assert_eq!(keyed.keyed(age).rect, [0., 0., 1., 1.], "{age} ms");
        }
        assert_eq!(keyed.keyed(5000).rect, edited.rect);
    }
    /// A property first keyed after another has nodes starts from that node,
    /// and a move alone still records the size that goes with it.
    #[test]
    fn a_new_channel_starts_changing_at_the_latest_earlier_node() {
        let p = super::super::editor::Project::new("movie.mp4".into(), [640, 360], 10_000, 30);
        let base = p.segments[0].clone();
        let mut s = Set::default();
        let n = |v: f32| Value::Numbers(vec![v]);
        s.insert(Property::Opacity, 4000, n(1.), n(1.));
        s.insert(Property::Rotation, 5000, n(0.), n(90.));
        assert_eq!(s.sample(Property::Rotation, 4000), Some(n(0.)));
        assert_ne!(s.sample(Property::Rotation, 4500), Some(n(0.)));

        let mut keys = Set::default();
        let mut moved = base.clone();
        moved.rect[0] = 0.25;
        keys.edit_media(&base, &moved, 2000, true);
        assert!(keys.channels.contains_key(&Property::Size));
        // Whole-clip edits with Keyframes off still add no keys.
        let mut off = Set::default();
        off.edit_media(&base, &moved, 2000, false);
        assert!(off.is_empty());
    }
    #[test]
    fn key_retiming_collisions_and_invalid_values_are_rejected() {
        let mut s = Set::default();
        s.insert(
            Property::Rotation,
            1000,
            Value::Numbers(vec![0.]),
            Value::Numbers(vec![90.]),
        );
        assert!(s.move_key(1000, 0, None).is_err());
        s.move_key(1000, 2000, None).unwrap();
        assert_eq!(
            s.sample(Property::Rotation, 1000),
            Some(Value::Numbers(vec![45.]))
        );
        s.insert(
            Property::Opacity,
            0,
            Value::Numbers(vec![1.]),
            Value::Numbers(vec![f32::NAN]),
        );
        assert!(s.validate().is_err());
    }
    #[test]
    fn duplicate_previous_copies_every_property_at_that_key() {
        let mut s = Set::default();
        let at = |x: f32| Value::Numbers(vec![x, x]);
        s.insert(Property::Position, 1000, at(0.), at(50.));
        s.insert(
            Property::Rotation,
            1000,
            Value::Numbers(vec![0.]),
            Value::Numbers(vec![30.]),
        );
        s.insert(Property::Position, 2000, at(50.), at(90.));
        assert_eq!(s.duplicate_previous(1500, None), Some(1000));
        assert_eq!(s.sample(Property::Position, 1500), Some(at(50.)));
        assert_eq!(
            s.sample(Property::Rotation, 1500),
            Some(Value::Numbers(vec![30.]))
        );
        // Holding: the value stays put between the original and the copy.
        assert_eq!(s.sample(Property::Position, 1250), Some(at(50.)));
        assert_eq!(s.duplicate_previous(0, None), None);
        assert_eq!(
            s.duplicate_previous(2500, Some(Property::Rotation)),
            Some(1500)
        );
        assert_eq!(
            s.sample(Property::Rotation, 2500),
            Some(Value::Numbers(vec![30.]))
        );
        s.validate().unwrap();
    }
}
