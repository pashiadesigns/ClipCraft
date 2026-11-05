//! Validated, portable creative presets. No media, paths, or project identity.
use crate::{
    Annotation, Error, Result,
    video::{
        editor::{Animation, Timing},
        effects::{Chain, Effect},
        frame::Frame,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
const LIMIT: usize = 256;
const MAX_BYTES: u64 = 2_000_000;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub entrance: Animation,
    pub exit: Animation,
    #[serde(default)]
    pub entrance_stack: Vec<Animation>,
    #[serde(default)]
    pub exit_stack: Vec<Animation>,
    pub entrance_ms: u64,
    pub exit_ms: u64,
    pub paired: bool,
    pub smooth: bool,
    pub direction: u8,
    pub path_reverse: bool,
    #[serde(default)]
    pub retract_from_start: bool,
    #[serde(default)]
    pub entrance_durations: std::collections::BTreeMap<Animation, u64>,
    #[serde(default)]
    pub exit_durations: std::collections::BTreeMap<Animation, u64>,
}
impl From<&Timing> for Motion {
    fn from(t: &Timing) -> Self {
        Self {
            entrance: t.entrance,
            exit: t.exit,
            entrance_stack: t.entrance_stack.clone(),
            exit_stack: t.exit_stack.clone(),
            entrance_ms: t.entrance_ms,
            exit_ms: t.exit_ms,
            paired: t.paired,
            smooth: t.smooth,
            direction: t.direction,
            path_reverse: t.path_reverse,
            retract_from_start: t.retract_from_start,
            entrance_durations: t.entrance_durations.clone(),
            exit_durations: t.exit_durations.clone(),
        }
    }
}
impl Motion {
    pub fn apply(&self, a: &Annotation, t: &mut Timing) -> Result<()> {
        Payload::Motion(self.clone()).validate()?;
        if !self.entrance.supports(a)
            || !self.exit.supports(a)
            || self
                .entrance_stack
                .iter()
                .chain(&self.exit_stack)
                .any(|e| !e.supports(a))
        {
            return Err(Error::Invalid("This motion preset is not compatible with the selected object. Choose a text, shape, or path object that supports both animations.".into()));
        }
        t.entrance = self.entrance;
        t.exit = self.exit;
        t.entrance_stack = self.entrance_stack.clone();
        t.exit_stack = self.exit_stack.clone();
        t.entrance_ms = self.entrance_ms;
        t.exit_ms = self.exit_ms;
        t.paired = self.paired;
        t.smooth = self.smooth;
        t.direction = self.direction;
        t.path_reverse = self.path_reverse;
        t.retract_from_start = self.retract_from_start;
        t.entrance_durations = self.entrance_durations.clone();
        t.exit_durations = self.exit_durations.clone();
        t.clamp_animations();
        Ok(())
    }
    pub fn builtins() -> Vec<(&'static str, Self)> {
        [
            ("Soft fade", Animation::Fade, 350, 350),
            ("Float from left", Animation::Float, 450, 350),
            ("Draw & retract", Animation::Path, 800, 500),
            ("Type & erase", Animation::Typewriter, 900, 500),
            ("Expand & shrink", Animation::Scale, 400, 300),
        ]
        .into_iter()
        .map(|(name, a, enter, exit)| {
            (
                name,
                Self {
                    entrance: a,
                    exit: a,
                    entrance_stack: vec![],
                    exit_stack: vec![],
                    entrance_ms: enter,
                    exit_ms: exit,
                    paired: true,
                    smooth: true,
                    direction: 0,
                    path_reverse: false,
                    retract_from_start: false,
                    entrance_durations: Default::default(),
                    exit_durations: Default::default(),
                },
            )
        })
        .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "settings", rename_all = "snake_case")]
pub enum Payload {
    AudioChain(Chain),
    AudioEffect(Effect),
    Motion(Motion),
    VideoFrame { frame: Frame, rect: [f32; 4] },
}
impl Payload {
    pub fn category(&self) -> &'static str {
        match self {
            Self::AudioChain(_) => "Audio chains",
            Self::AudioEffect(_) => "Audio effects",
            Self::Motion(_) => "Motion",
            Self::VideoFrame { .. } => "Video frames",
        }
    }
    pub fn compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::AudioEffect(a), Self::AudioEffect(b)) => a.kind == b.kind,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::AudioChain(c) => c.validate(),
            Self::AudioEffect(e) if e.validate() => Ok(()),
            Self::Motion(m)
                if m.direction < 4
                    && m.entrance_ms <= 60_000
                    && m.exit_ms <= 60_000
                    && m.entrance_durations.len() <= 4
                    && m.exit_durations.len() <= 4
                    && m.entrance_durations
                        .values()
                        .chain(m.exit_durations.values())
                        .all(|v| *v <= 60_000)
                    && [(m.entrance, &m.entrance_stack), (m.exit, &m.exit_stack)]
                        .into_iter()
                        .all(|(primary, stack)| {
                            stack.len() <= 4
                                && stack.iter().enumerate().all(|(i, e)| {
                                    Timing::stack_compatible(primary, &stack[..i], *e)
                                })
                        }) =>
            {
                Ok(())
            }
            Self::VideoFrame { frame, rect }
                if rect
                    .iter()
                    .all(|v| v.is_finite() && (-4. ..=4.).contains(v))
                    && rect[2] > 0.
                    && rect[3] > 0. =>
            {
                frame.validate()
            }
            _ => Err(Error::Invalid(
                "The preset contains invalid settings.".into(),
            )),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u64,
    pub name: String,
    pub favorite: bool,
    pub payload: Payload,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Pack {
    version: u32,
    entries: Vec<Entry>,
}
fn name(value: &str) -> Result<String> {
    let n = value.trim();
    if n.is_empty() || n.chars().count() > 80 || n.chars().any(char::is_control) {
        Err(Error::Invalid(
            "Enter a preset name between 1 and 80 characters without control characters.".into(),
        ))
    } else {
        Ok(n.to_owned())
    }
}
fn validate(entries: &[Entry]) -> Result<()> {
    if entries.len() > LIMIT {
        return Err(Error::Invalid(
            "The preset library holds up to 256 entries. Delete an unused preset first.".into(),
        ));
    }
    for (i, e) in entries.iter().enumerate() {
        name(&e.name)?;
        e.payload.validate()?;
        if e.id == 0
            || entries[..i].iter().any(|o| {
                o.id == e.id
                    || (o.name.eq_ignore_ascii_case(&e.name)
                        && o.payload.category() == e.payload.category())
            })
        {
            return Err(Error::Invalid(
                "Preset identifiers and names must be unique within a category.".into(),
            ));
        }
    }
    Ok(())
}
fn encode(entries: &[Entry]) -> Result<Vec<u8>> {
    validate(entries)?;
    let bytes = serde_json::to_vec_pretty(&Pack {
        version: 1,
        entries: entries.to_vec(),
    })?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Invalid(
            "Preset library exceeds 2 MB. Remove unused presets before saving.".into(),
        ));
    }
    Ok(bytes)
}
fn read(path: &Path) -> Result<Vec<Entry>> {
    let f = fs::File::open(path)?;
    let mut b = vec![];
    f.take(MAX_BYTES + 1).read_to_end(&mut b)?;
    if b.len() as u64 > MAX_BYTES {
        return Err(Error::Invalid("Preset pack exceeds the 2 MB limit.".into()));
    }
    let p: Pack = serde_json::from_slice(&b)?;
    if p.version != 1 {
        return Err(Error::Invalid("Unsupported preset pack version.".into()));
    }
    validate(&p.entries)?;
    Ok(p.entries)
}
fn next_id(entries: &[Entry]) -> Result<u64> {
    entries
        .iter()
        .map(|e| e.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| Error::Invalid("Preset identifier limit reached.".into()))
}
pub struct Store {
    root: PathBuf,
    entries: Vec<Entry>,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let path = root.join("creative-presets.json");
        let entries = if path.exists() { read(&path)? } else { vec![] };
        Ok(Self { root, entries })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    fn change<T>(&mut self, edit: impl FnOnce(&mut Vec<Entry>) -> Result<T>) -> Result<T> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("creative-presets.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Presets are busy. Try again.".into()))?;
        let path = self.root.join("creative-presets.json");
        let mut entries = if path.exists() { read(&path)? } else { vec![] };
        let result = edit(&mut entries)?;
        validate(&entries)?;
        crate::atomic_write(&path, &encode(&entries)?)?;
        self.entries = entries;
        Ok(result)
    }
    pub fn save(&mut self, label: &str, payload: Payload) -> Result<u64> {
        let label = name(label)?;
        payload.validate()?;
        self.change(|v| {
            let id = next_id(v)?;
            v.push(Entry {
                id,
                name: label,
                favorite: false,
                payload,
            });
            Ok(id)
        })
    }
    pub fn rename(&mut self, id: u64, label: &str) -> Result<()> {
        let label = name(label)?;
        self.change(|v| {
            find(v, id)?.name = label;
            Ok(())
        })
    }
    pub fn update(&mut self, id: u64, label: &str, payload: Payload) -> Result<()> {
        let label = name(label)?;
        payload.validate()?;
        self.change(|entries| {
            let entry = find(entries, id)?;
            if !entry.payload.compatible(&payload) {
                return Err(Error::Invalid(
                    "Choose a compatible preset to update.".into(),
                ));
            }
            entry.name = label;
            entry.payload = payload;
            Ok(())
        })
    }
    pub fn replace(&mut self, id: u64, payload: Payload) -> Result<()> {
        payload.validate()?;
        self.change(|v| {
            let e = find(v, id)?;
            if !e.payload.compatible(&payload) {
                return Err(Error::Invalid(
                    "Choose a compatible preset to update.".into(),
                ));
            }
            e.payload = payload;
            Ok(())
        })
    }
    pub fn favorite(&mut self, id: u64, value: bool) -> Result<()> {
        self.change(|v| {
            find(v, id)?.favorite = value;
            Ok(())
        })
    }
    pub fn remove(&mut self, id: u64) -> Result<()> {
        self.change(|v| {
            find(v, id)?;
            v.retain(|e| e.id != id);
            Ok(())
        })
    }
    pub fn duplicate(&mut self, id: u64) -> Result<u64> {
        self.change(|v| {
            let mut e = find(v, id)?.clone();
            e.id = next_id(v)?;
            e.name = unique_name(v, &e);
            let id = e.id;
            v.push(e);
            Ok(id)
        })
    }
    pub fn export(&self, path: &Path) -> Result<()> {
        crate::atomic_write(path, &encode(&self.entries)?)
    }
    pub fn import(&mut self, path: &Path) -> Result<usize> {
        let entries = read(path)?;
        self.change(|v| {
            let n = entries.len();
            for mut e in entries {
                e.id = next_id(v)?;
                if v.iter().any(|o| {
                    o.name.eq_ignore_ascii_case(&e.name)
                        && o.payload.category() == e.payload.category()
                }) {
                    e.name = unique_name(v, &e);
                }
                v.push(e);
            }
            Ok(n)
        })
    }
}
fn find(v: &mut [Entry], id: u64) -> Result<&mut Entry> {
    v.iter_mut().find(|e| e.id == id).ok_or_else(|| {
        Error::Invalid("This preset no longer exists. Reopen the preset library.".into())
    })
}
fn unique_name(v: &[Entry], e: &Entry) -> String {
    let base: String = e.name.chars().take(65).collect();
    for n in 2..=LIMIT + 2 {
        let name = format!("{base} ({n})");
        if !v.iter().any(|x| {
            x.name.eq_ignore_ascii_case(&name) && x.payload.category() == e.payload.category()
        }) {
            return name;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "clipcraft-preset-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn serialization_limit_rejects_unreadable_library_before_replacing_saved_data() {
        use crate::video::effects::{Band, BandKind, Effect, Kind, MAX_EFFECTS, MAX_EQ_BANDS};
        let dir = root();
        let mut store = Store::open(&dir).unwrap();
        store
            .save("Existing", Payload::AudioChain(Chain::narration()))
            .unwrap();
        let before = fs::read(dir.join("creative-presets.json")).unwrap();
        let chain = Chain {
            bypass: false,
            mix: 1.,
            effects: (1..=MAX_EFFECTS)
                .map(|id| {
                    let mut e = Effect::new(id as u64, Kind::Equalizer);
                    e.bands = vec![Band::new(BandKind::Bell, 1000.); MAX_EQ_BANDS];
                    e
                })
                .collect(),
        };
        let result = store.change(|v| {
            v.clear();
            for id in 1..=LIMIT {
                v.push(Entry {
                    id: id as u64,
                    name: format!("Chain {id}"),
                    favorite: false,
                    payload: Payload::AudioChain(chain.clone()),
                });
            }
            Ok(())
        });
        assert!(result.unwrap_err().to_string().contains("2 MB"));
        assert_eq!(fs::read(dir.join("creative-presets.json")).unwrap(), before);
        assert_eq!(store.entries().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn saved_chain_preserves_order_parameters_and_pack_merge_is_atomic() {
        let dir = root();
        let mut s = Store::open(&dir).unwrap();
        let mut chain = Chain::narration();
        chain.effects.reverse();
        let id = s
            .save("My voice", Payload::AudioChain(chain.clone()))
            .unwrap();
        s.favorite(id, true).unwrap();
        assert!(
            s.save("my voice", Payload::AudioChain(chain.clone()))
                .is_err()
        );
        s.duplicate(id).unwrap();
        s.rename(id, "Studio").unwrap();
        let p = dir.join("shared.ccpresets");
        s.export(&p).unwrap();
        let mut other = Store::open(&dir.join("other")).unwrap();
        assert_eq!(other.import(&p).unwrap(), 2);
        assert_eq!(other.entries()[0].payload, Payload::AudioChain(chain));
        other.import(&p).unwrap();
        assert_eq!(other.entries().len(), 4);
        let original = other.entries().to_vec();
        fs::write(&p, br#"{"version":1,"entries":[{}]}"#).unwrap();
        assert!(other.import(&p).is_err());
        assert_eq!(other.entries(), original);
        assert_eq!(Store::open(&dir.join("other")).unwrap().entries(), original);
        s.remove(id).unwrap();
        assert_eq!(s.entries().len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn motion_preserves_clip_identity_clamps_short_clips_and_rejects_redaction() {
        let a = crate::steps::marker(1, [50., 50.], 44, [0, 0, 0, 255], (1, false), [200, 200])
            .unwrap();
        let m = Motion::builtins()[0].1.clone();
        let mut t = Timing::new(7, 100, 300);
        t.label = "My step".into();
        m.apply(&a, &mut t).unwrap();
        assert_eq!(
            (t.track, t.start, t.end, t.label.as_str()),
            (7, 100, 300, "My step")
        );
        assert!(t.entrance_ms + t.exit_ms <= 200);
        let mut redact = a;
        redact.kind = crate::Kind::Redact;
        let before = t.clone();
        assert!(m.apply(&redact, &mut t).is_err());
        assert_eq!(t, before);
    }
    #[test]
    fn rejects_invalid_audio_frame_and_oversize_import() {
        let dir = root();
        let mut s = Store::open(&dir).unwrap();
        let mut e = Effect::new(1, crate::video::effects::Kind::Compressor);
        e.values[0] = f32::NAN;
        assert!(s.save("Invalid", Payload::AudioEffect(e)).is_err());
        assert!(
            s.save(
                "Frame",
                Payload::VideoFrame {
                    frame: Frame::default(),
                    rect: [0., 0., 0., 1.]
                }
            )
            .is_err()
        );
        let p = dir.join("huge");
        fs::write(&p, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(s.import(&p).is_err());
        assert!(s.entries().is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
}
