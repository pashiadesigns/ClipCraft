//! Named color palettes, recent colors and palette files (UX-010, approved
//! bi-proposal 3). Kept beside preferences and isolated from document history.
//! The built-in ClipCraft palette is fixed; the chosen palette is shared by
//! every color picker and remembered between sessions.
use crate::{Error, Result, atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const BUILT_IN: &str = "ClipCraft";
pub const BUILT_IN_COLORS: [(&str, [u8; 4]); 7] = [
    ("White", [255, 255, 255, 255]),
    ("Gray", [145, 150, 151, 255]),
    ("Red", [237, 32, 36, 255]),
    ("Blue", [29, 160, 231, 255]),
    ("Black", [0, 0, 0, 255]),
    ("Green", [83, 191, 62, 255]),
    ("Yellow", [255, 222, 28, 255]),
];
const MAX_PALETTES: usize = 32;
const MAX_COLORS: usize = 64;
const MAX_RECENT: usize = 8;
const MAX_FILE: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 4]>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palettes {
    version: u32,
    /// The palette shown in every picker; [`BUILT_IN`] when none is chosen.
    pub selected: String,
    pub palettes: Vec<Palette>,
    pub recent: Vec<[u8; 4]>,
}
impl Default for Palettes {
    fn default() -> Self {
        Self {
            version: 1,
            selected: BUILT_IN.into(),
            palettes: vec![],
            recent: vec![],
        }
    }
}
impl Palettes {
    /// Colors of the selected palette (the built-in one when it is selected).
    pub fn colors(&self) -> Vec<[u8; 4]> {
        self.find(&self.selected).map_or_else(
            || BUILT_IN_COLORS.iter().map(|(_, c)| *c).collect(),
            |p| p.colors.clone(),
        )
    }
    pub fn names(&self) -> Vec<String> {
        std::iter::once(BUILT_IN.to_owned())
            .chain(self.palettes.iter().map(|p| p.name.clone()))
            .collect()
    }
    pub fn editable(&self) -> bool {
        self.find(&self.selected).is_some()
    }
    fn find(&self, name: &str) -> Option<&Palette> {
        self.palettes.iter().find(|p| p.name == name)
    }
    fn selected_mut(&mut self) -> Result<&mut Palette> {
        let name = self.selected.clone();
        self.palettes
            .iter_mut()
            .find(|p| p.name == name)
            .ok_or_else(|| {
                Error::Invalid(
                    "The ClipCraft palette can't be changed. Make a new palette first.".into(),
                )
            })
    }
    fn validate(&self) -> Result<()> {
        let bad = self.version != 1
            || self.palettes.len() > MAX_PALETTES
            || self.recent.len() > MAX_RECENT
            || self.palettes.iter().enumerate().any(|(i, p)| {
                p.colors.len() > MAX_COLORS
                    || check_palette_name(&p.name).is_err()
                    || p.name.eq_ignore_ascii_case(BUILT_IN)
                    || self.palettes[..i]
                        .iter()
                        .any(|o| o.name.eq_ignore_ascii_case(&p.name))
            });
        if bad {
            return Err(Error::Invalid(
                "Unsupported palettes; file preserved".into(),
            ));
        }
        Ok(())
    }
}
/// An edit to the stored palettes; each is one locked read-modify-write.
#[derive(Clone, Debug)]
pub enum Edit {
    Select(String),
    Add([u8; 4]),
    Remove([u8; 4]),
    Replace { old: [u8; 4], new: [u8; 4] },
    Move { color: [u8; 4], delta: i32 },
    Create { name: String, colors: Vec<[u8; 4]> },
    Rename(String),
    Duplicate,
    Delete,
    Recent([u8; 4]),
}
pub fn check_palette_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(Error::Invalid("Palette names need 1–60 characters.".into()));
    }
    Ok(name)
}
fn unique_name(p: &Palettes, base: &str) -> String {
    let taken = |n: &str| {
        n.eq_ignore_ascii_case(BUILT_IN)
            || p.palettes.iter().any(|o| o.name.eq_ignore_ascii_case(n))
    };
    if !taken(base) {
        return base.into();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|n| !taken(n))
        .unwrap()
}
fn apply(p: &mut Palettes, edit: Edit) -> Result<()> {
    match edit {
        Edit::Select(name) => {
            if name != BUILT_IN && p.find(&name).is_none() {
                return Err(Error::Invalid(
                    "That palette was removed in another window.".into(),
                ));
            }
            p.selected = name;
        }
        Edit::Add(color) => {
            let palette = p.selected_mut()?;
            if !palette.colors.contains(&color) {
                if palette.colors.len() == MAX_COLORS {
                    return Err(Error::Invalid(format!(
                        "This palette is full ({MAX_COLORS} colors). Remove one to add another."
                    )));
                }
                palette.colors.push(color);
            }
        }
        Edit::Remove(color) => p.selected_mut()?.colors.retain(|c| *c != color),
        Edit::Replace { old, new } => {
            let palette = p.selected_mut()?;
            let i = palette
                .colors
                .iter()
                .position(|c| *c == old)
                .ok_or_else(|| {
                    Error::Invalid("This color was removed in another window.".into())
                })?;
            if old != new {
                palette.colors[i] = new;
                let mut seen = false;
                palette
                    .colors
                    .retain(|c| *c != new || !std::mem::replace(&mut seen, true));
            }
        }
        Edit::Move { color, delta } => {
            let palette = p.selected_mut()?;
            if let Some(i) = palette.colors.iter().position(|c| *c == color) {
                let j =
                    (i as i64 + delta as i64).clamp(0, palette.colors.len() as i64 - 1) as usize;
                let c = palette.colors.remove(i);
                palette.colors.insert(j, c);
            }
        }
        Edit::Create { name, mut colors } => {
            if p.palettes.len() == MAX_PALETTES {
                return Err(Error::Invalid(format!(
                    "You have {MAX_PALETTES} palettes. Delete one to make another."
                )));
            }
            let name = unique_name(p, check_palette_name(&name)?);
            colors.dedup();
            colors.truncate(MAX_COLORS);
            p.palettes.push(Palette {
                name: name.clone(),
                colors,
            });
            p.selected = name;
        }
        Edit::Rename(name) => {
            let name = check_palette_name(&name)?.to_owned();
            let current = p.selected.clone();
            if !name.eq_ignore_ascii_case(&current)
                && (name.eq_ignore_ascii_case(BUILT_IN)
                    || p.palettes
                        .iter()
                        .any(|o| o.name.eq_ignore_ascii_case(&name)))
            {
                return Err(Error::Invalid(
                    "A palette with that name already exists.".into(),
                ));
            }
            p.selected_mut()?.name = name.clone();
            p.selected = name;
        }
        Edit::Duplicate => {
            let colors = p.colors();
            let base = format!("{} copy", p.selected);
            return apply(p, Edit::Create { name: base, colors });
        }
        Edit::Delete => {
            let name = p.selected.clone();
            p.selected_mut()?;
            p.palettes.retain(|o| o.name != name);
            p.selected = BUILT_IN.into();
        }
        Edit::Recent(color) => {
            p.recent.retain(|c| *c != color);
            p.recent.insert(0, color);
            p.recent.truncate(MAX_RECENT);
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let store = Self {
            root: fs::canonicalize(root)?,
        };
        store.read()?;
        Ok(store)
    }
    fn path(&self) -> PathBuf {
        self.root.join("palettes-v1.json")
    }
    pub fn read(&self) -> Result<Palettes> {
        let path = self.path();
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(self.first_run()),
            Err(e) => return Err(e.into()),
        };
        if metadata.len() > MAX_FILE {
            return Err(Error::Invalid(
                "Palettes file is too large; file preserved".into(),
            ));
        }
        let p: Palettes = serde_json::from_slice(&fs::read(path)?)?;
        p.validate()?;
        Ok(p)
    }
    /// Colors saved with the earlier My Colors shelf start out as a palette.
    fn first_run(&self) -> Palettes {
        let mut p = Palettes::default();
        if let Ok(colors) = crate::saved_colors::Store::open(&self.root).and_then(|s| s.read())
            && !colors.is_empty()
        {
            p.palettes.push(Palette {
                name: "My Colors".into(),
                colors,
            });
            p.selected = "My Colors".into();
        }
        p
    }
    pub fn edit(&self, edit: Edit) -> Result<Palettes> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("palettes.lock"))?;
        lock.try_lock()
            .map_err(|_| Error::Invalid("Palettes are busy in another window".into()))?;
        let mut p = self.read()?;
        apply(&mut p, edit)?;
        p.validate()?;
        atomic_write(&self.path(), &serde_json::to_vec_pretty(&p)?)?;
        Ok(p)
    }
}

/// Reads a GIMP (.gpl) or Adobe swatch exchange (.ase) palette.
pub fn import(path: &Path) -> Result<Palette> {
    let bytes = fs::read(path)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("Palette files must be under 1 MB.".into()));
    }
    let stem = path
        .file_stem()
        .map_or_else(|| "Imported".into(), |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (name, colors) = match ext.as_str() {
        "gpl" => read_gpl(&String::from_utf8_lossy(&bytes))?,
        "ase" => read_ase(&bytes)?,
        _ => return Err(Error::Invalid("Choose a .gpl or .ase palette file.".into())),
    };
    if colors.is_empty() {
        return Err(Error::Invalid(
            "That palette file has no colors ClipCraft can use.".into(),
        ));
    }
    let mut colors = colors;
    colors.dedup();
    colors.truncate(MAX_COLORS);
    Ok(Palette {
        name: check_palette_name(&name).map_or(stem, str::to_owned),
        colors,
    })
}
pub fn export(palette: &Palette, path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let bytes = match ext.as_str() {
        "gpl" => write_gpl(palette).into_bytes(),
        "ase" => write_ase(palette),
        _ => return Err(Error::Invalid("Save palettes as .gpl or .ase.".into())),
    };
    atomic_write(path, &bytes)
}
fn read_gpl(text: &str) -> Result<(String, Vec<[u8; 4]>)> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("GIMP Palette") {
        return Err(Error::Invalid("This is not a GIMP palette.".into()));
    }
    let mut name = String::new();
    let mut colors = vec![];
    for line in lines {
        let line = line.trim();
        if let Some(n) = line.strip_prefix("Name:") {
            name = n.trim().into();
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with("Columns:") {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().take(3).collect();
        if let [r, g, b] = parts[..]
            && let (Ok(r), Ok(g), Ok(b)) = (r.parse(), g.parse(), b.parse())
        {
            colors.push([r, g, b, 255]);
        }
    }
    Ok((name, colors))
}
fn write_gpl(p: &Palette) -> String {
    let mut out = format!("GIMP Palette\nName: {}\nColumns: 8\n#\n", p.name);
    for c in &p.colors {
        out.push_str(&format!(
            "{:3} {:3} {:3}\t#{:02X}{:02X}{:02X}\n",
            c[0], c[1], c[2], c[0], c[1], c[2]
        ));
    }
    out
}
fn read_ase(bytes: &[u8]) -> Result<(String, Vec<[u8; 4]>)> {
    let bad = || Error::Invalid("This is not a readable Adobe swatch file.".into());
    if bytes.len() < 12 || &bytes[..4] != b"ASEF" {
        return Err(bad());
    }
    let u16_at = |i: usize| {
        bytes
            .get(i..i + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    let u32_at = |i: usize| {
        bytes
            .get(i..i + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let f32_at = |i: usize| u32_at(i).map(f32::from_bits);
    let blocks = u32_at(8).ok_or_else(bad)?.min(10_000);
    let mut at = 12;
    let mut name = String::new();
    let mut colors = vec![];
    for _ in 0..blocks {
        let kind = u16_at(at).ok_or_else(bad)?;
        let len = u32_at(at + 2).ok_or_else(bad)? as usize;
        let body = at + 6;
        let end = body
            .checked_add(len)
            .filter(|e| *e <= bytes.len())
            .ok_or_else(bad)?;
        let name_len = u16_at(body).unwrap_or(0) as usize;
        let text_end = body + 2 + name_len * 2;
        let label = || {
            let units: Vec<u16> = (body + 2..text_end.min(end))
                .step_by(2)
                .filter_map(u16_at)
                .take_while(|u| *u != 0)
                .collect();
            String::from_utf16_lossy(&units)
        };
        match kind {
            0xC001 if name.is_empty() => name = label(),
            0x0001 if text_end + 4 <= end => {
                let model = &bytes[text_end..text_end + 4];
                let v = |n: usize| f32_at(text_end + 4 + n * 4).unwrap_or(0.).clamp(0., 1.);
                let byte = |x: f32| (x * 255.).round() as u8;
                let rgb = match model {
                    b"RGB " => Some([byte(v(0)), byte(v(1)), byte(v(2))]),
                    b"GRAY" => Some([byte(v(0)); 3]),
                    b"CMYK" => {
                        let k = 1. - v(3);
                        Some([
                            byte((1. - v(0)) * k),
                            byte((1. - v(1)) * k),
                            byte((1. - v(2)) * k),
                        ])
                    }
                    _ => None,
                };
                if let Some([r, g, b]) = rgb {
                    colors.push([r, g, b, 255]);
                }
            }
            _ => {}
        }
        at = end;
    }
    Ok((name, colors))
}
fn write_ase(p: &Palette) -> Vec<u8> {
    let text = |s: &str| {
        let mut units: Vec<u16> = s.encode_utf16().collect();
        units.push(0);
        let mut out = (units.len() as u16).to_be_bytes().to_vec();
        for u in units {
            out.extend(u.to_be_bytes());
        }
        out
    };
    let block = |kind: u16, body: Vec<u8>| {
        let mut out = kind.to_be_bytes().to_vec();
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
        out
    };
    let mut blocks = vec![block(0xC001, text(&p.name))];
    for c in &p.colors {
        let mut body = text(&format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
        body.extend(b"RGB ");
        for v in &c[..3] {
            body.extend((f32::from(*v) / 255.).to_bits().to_be_bytes());
        }
        body.extend(2u16.to_be_bytes());
        blocks.push(block(0x0001, body));
    }
    blocks.push(block(0xC002, vec![]));
    let mut out = b"ASEF".to_vec();
    out.extend(1u16.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend((blocks.len() as u32).to_be_bytes());
    for b in blocks {
        out.extend(b);
    }
    out
}

/// The main colors of an image by median cut, most common first (up to `count`).
pub fn from_image(image: &image::RgbaImage, count: usize) -> Vec<[u8; 4]> {
    let step = ((image.width() as u64 * image.height() as u64) / 40_000).max(1) as usize;
    let pixels: Vec<[u8; 3]> = image
        .pixels()
        .step_by(step)
        .filter(|p| p[3] > 128)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    if pixels.is_empty() {
        return vec![];
    }
    let mut boxes = vec![pixels];
    while boxes.len() < count.clamp(1, 16) {
        let Some((i, channel)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .map(|(i, b)| {
                let range = |c: usize| {
                    let (lo, hi) = b
                        .iter()
                        .fold((255, 0), |(lo, hi), p| (lo.min(p[c]), hi.max(p[c])));
                    hi - lo
                };
                let channel = (0..3).max_by_key(|c| range(*c)).unwrap();
                (i, channel, range(channel))
            })
            .filter(|(_, _, r)| *r > 8)
            .max_by_key(|(_, _, r)| *r)
            .map(|(i, c, _)| (i, c))
        else {
            break;
        };
        let mut b = boxes.swap_remove(i);
        b.sort_unstable_by_key(|p| p[channel]);
        let rest = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(rest);
    }
    boxes.sort_by_key(|b| std::cmp::Reverse(b.len()));
    let mut colors: Vec<[u8; 4]> = boxes
        .iter()
        .map(|b| {
            let sum = b.iter().fold([0u64; 3], |s, p| {
                [s[0] + p[0] as u64, s[1] + p[1] as u64, s[2] + p[2] as u64]
            });
            let n = b.len() as u64;
            [
                (sum[0] / n) as u8,
                (sum[1] / n) as u8,
                (sum[2] / n) as u8,
                255,
            ]
        })
        .collect();
    colors.dedup();
    colors
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn palettes_select_edit_and_keep_the_built_in_fixed() {
        let temp = tempfile::tempdir().unwrap();
        let a = Store::open(temp.path()).unwrap();
        let p = a.read().unwrap();
        assert_eq!(p.selected, BUILT_IN);
        assert_eq!(p.colors().len(), 7);
        assert!(
            a.edit(Edit::Add([1, 2, 3, 255])).is_err(),
            "built-in is fixed"
        );
        let p = a
            .edit(Edit::Create {
                name: "Acme brand".into(),
                colors: vec![[11, 61, 92, 255]],
            })
            .unwrap();
        assert_eq!(p.selected, "Acme brand");
        a.edit(Edit::Add([24, 145, 193, 255])).unwrap();
        a.edit(Edit::Add([24, 145, 193, 255])).unwrap();
        let p = a
            .edit(Edit::Move {
                color: [24, 145, 193, 255],
                delta: -1,
            })
            .unwrap();
        assert_eq!(p.colors(), [[24, 145, 193, 255], [11, 61, 92, 255]]);
        let p = a
            .edit(Edit::Replace {
                old: [11, 61, 92, 255],
                new: [24, 145, 193, 255],
            })
            .unwrap();
        assert_eq!(
            p.colors(),
            [[24, 145, 193, 255]],
            "replace never duplicates"
        );
        assert!(a.edit(Edit::Rename("clipcraft".into())).is_err());
        let p = a.edit(Edit::Duplicate).unwrap();
        assert_eq!(p.selected, "Acme brand copy");
        let p = a.edit(Edit::Delete).unwrap();
        assert_eq!(p.selected, BUILT_IN);
        assert_eq!(p.names(), ["ClipCraft", "Acme brand"]);
        for n in 0..10 {
            a.edit(Edit::Recent([n, 0, 0, 255])).unwrap();
        }
        let p = a.edit(Edit::Recent([3, 0, 0, 255])).unwrap();
        assert_eq!(p.recent.len(), 8);
        assert_eq!(p.recent[0], [3, 0, 0, 255]);
        // The choice is remembered for the next session.
        a.edit(Edit::Select("Acme brand".into())).unwrap();
        assert_eq!(
            Store::open(temp.path()).unwrap().read().unwrap().selected,
            "Acme brand"
        );
    }
    #[test]
    fn earlier_my_colors_become_a_palette() {
        let temp = tempfile::tempdir().unwrap();
        crate::saved_colors::Store::open(temp.path())
            .unwrap()
            .set([9, 8, 7, 255], true)
            .unwrap();
        let p = Store::open(temp.path()).unwrap().read().unwrap();
        assert_eq!(p.selected, "My Colors");
        assert_eq!(p.colors(), [[9, 8, 7, 255]]);
    }
    #[test]
    fn gpl_and_ase_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let palette = Palette {
            name: "Acme brand".into(),
            colors: vec![[11, 61, 92, 255], [242, 201, 76, 255], [255, 255, 255, 255]],
        };
        for ext in ["gpl", "ase"] {
            let path = temp.path().join(format!("acme.{ext}"));
            export(&palette, &path).unwrap();
            assert_eq!(import(&path).unwrap(), palette, "{ext}");
        }
        std::fs::write(
            temp.path().join("bad.ase"),
            b"ASEF\0\x01\0\0\xff\xff\xff\xff",
        )
        .unwrap();
        assert!(import(&temp.path().join("bad.ase")).is_err());
        assert!(import(&temp.path().join("acme.txt")).is_err());
    }
    #[test]
    fn image_palette_finds_the_main_colors() {
        let image = image::RgbaImage::from_fn(200, 100, |x, _| {
            image::Rgba(if x < 120 {
                [20, 110, 140, 255]
            } else {
                [240, 240, 240, 255]
            })
        });
        let colors = from_image(&image, 6);
        assert_eq!(colors[0], [20, 110, 140, 255]);
        assert!(colors.contains(&[240, 240, 240, 255]));
    }
}
