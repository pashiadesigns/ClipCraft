//! Kits (approved beta 6 tab 3): one `.cdkit` file holding styles and
//! building blocks to share with other people. A kit is a ZIP holding
//! `kit.json`. It carries settings only: no projects, Library items, paths
//! or preferences beyond what was chosen. Opening one changes nothing until
//! its items are added.
use crate::{
    Document, Error, Result, object_styles::ObjectStyle, palettes::Palette,
    privacy::custom::CustomType, reusable_presets::Payload, styles::ArrowStyle,
    watermark::Watermark,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EXTENSION: &str = "cdkit";
const FORMAT: &str = "clipdesigner-kit";
/// Kits are refused above this size, packed or unpacked.
const MAX_BYTES: u64 = 96 << 20;

/// What a kit can hold, in the order the dialogs list them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Presets,
    Blocks,
    Palettes,
    SensitiveTypes,
    Watermarks,
    Templates,
}
impl Category {
    pub const ALL: [Category; 6] = [
        Category::Presets,
        Category::Blocks,
        Category::Palettes,
        Category::SensitiveTypes,
        Category::Watermarks,
        Category::Templates,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Category::Presets => "Presets",
            Category::Blocks => "Blocks",
            Category::Palettes => "Color palettes",
            Category::SensitiveTypes => "Custom sensitive types",
            Category::Watermarks => "Watermarks",
            Category::Templates => "Templates",
        }
    }
    /// "14 presets", "1 color palette".
    pub fn count_label(self, n: usize) -> String {
        let (one, many) = match self {
            Category::Presets => ("preset", "presets"),
            Category::Blocks => ("Block", "Blocks"),
            Category::Palettes => ("color palette", "color palettes"),
            Category::SensitiveTypes => ("custom sensitive type", "custom sensitive types"),
            Category::Watermarks => ("watermark", "watermarks"),
            Category::Templates => ("template", "templates"),
        };
        format!("{n} {}", if n == 1 { one } else { many })
    }
    /// Included when sharing unless the person turns it off; watermarks
    /// and templates can carry logos and pictures, so they start off.
    pub fn shared_by_default(self) -> bool {
        !matches!(self, Category::Watermarks | Category::Templates)
    }
}

/// A saved Block, motion, audio or frame preset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Creative {
    pub name: String,
    pub payload: Payload,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub document: Document,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Kit {
    format: String,
    version: u32,
    pub name: String,
    /// "ClipDesigner 1.0.0-beta.6".
    pub made_with: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<ObjectStyle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrow_styles: Vec<ArrowStyle>,
    /// Blocks (group payloads) and the other saved presets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub creative: Vec<Creative>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub palettes: Vec<Palette>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitive_types: Vec<CustomType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub watermarks: Vec<Watermark>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<Template>,
}

fn check_name(name: &str, what: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(Error::Invalid(format!(
            "A {what} in this kit has a name ClipDesigner can't use."
        )));
    }
    Ok(())
}

impl Kit {
    pub fn new(name: &str, made_with: &str) -> Self {
        Self {
            format: FORMAT.into(),
            version: 1,
            name: name.trim().into(),
            made_with: made_with.into(),
            presets: vec![],
            arrow_styles: vec![],
            creative: vec![],
            palettes: vec![],
            sensitive_types: vec![],
            watermarks: vec![],
            templates: vec![],
        }
    }
    /// How many items of a category the kit holds.
    pub fn count(&self, category: Category) -> usize {
        match category {
            Category::Presets => {
                self.presets.len()
                    + self.arrow_styles.len()
                    + self
                        .creative
                        .iter()
                        .filter(|c| !matches!(c.payload, Payload::Group(_)))
                        .count()
            }
            Category::Blocks => self
                .creative
                .iter()
                .filter(|c| matches!(c.payload, Payload::Group(_)))
                .count(),
            Category::Palettes => self.palettes.len(),
            Category::SensitiveTypes => self.sensitive_types.len(),
            Category::Watermarks => self.watermarks.len(),
            Category::Templates => self.templates.len(),
        }
    }
    pub fn total(&self) -> usize {
        Category::ALL.iter().map(|c| self.count(*c)).sum()
    }
    /// The kit with only the chosen categories.
    pub fn only(&self, keep: impl Fn(Category) -> bool) -> Self {
        let mut kit = self.clone();
        if !keep(Category::Presets) {
            kit.presets.clear();
            kit.arrow_styles.clear();
            kit.creative
                .retain(|c| matches!(c.payload, Payload::Group(_)));
        }
        if !keep(Category::Blocks) {
            kit.creative
                .retain(|c| !matches!(c.payload, Payload::Group(_)));
        }
        if !keep(Category::Palettes) {
            kit.palettes.clear();
        }
        if !keep(Category::SensitiveTypes) {
            kit.sensitive_types.clear();
        }
        if !keep(Category::Watermarks) {
            kit.watermarks.clear();
        }
        if !keep(Category::Templates) {
            kit.templates.clear();
        }
        kit
    }
    pub fn validate(&self) -> Result<()> {
        if self.format != FORMAT || self.version != 1 {
            return Err(Error::Invalid(
                "This kit was made by a newer ClipDesigner. Update ClipDesigner to add it.".into(),
            ));
        }
        check_name(&self.name, "kit")?;
        if self.made_with.chars().count() > 80 || self.made_with.chars().any(char::is_control) {
            return Err(Error::Invalid("This kit's details are damaged.".into()));
        }
        let too_many = self.presets.len() > 96
            || self.arrow_styles.len() > 24
            || self.creative.len() > 256
            || self.palettes.len() > 32
            || self.sensitive_types.len() > 64
            || self.watermarks.len() > 32
            || self.templates.len() > 32;
        if too_many {
            return Err(Error::Invalid(
                "This kit holds more than ClipDesigner allows.".into(),
            ));
        }
        for s in &self.presets {
            s.validate()?;
        }
        for s in &self.arrow_styles {
            s.validate()?;
        }
        for c in &self.creative {
            check_name(&c.name, "preset")?;
            c.payload.validate()?;
        }
        for p in &self.palettes {
            crate::palettes::check_palette_name(&p.name)?;
            if p.colors.is_empty() || p.colors.len() > 64 {
                return Err(Error::Invalid(format!(
                    "The {} palette in this kit has too many colors.",
                    p.name
                )));
            }
        }
        crate::privacy::custom::validate_all(&self.sensitive_types)?;
        for w in &self.watermarks {
            w.validate()?;
        }
        for t in &self.templates {
            check_name(&t.name, "template")?;
            if t.document.video.is_some() {
                return Err(Error::Invalid("Kits hold image templates only.".into()));
            }
            t.document.validate()?;
        }
        Ok(())
    }
    /// The `.cdkit` file's bytes.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let json = serde_json::to_vec(self)?;
        if json.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid(
                "This kit is too large. Leave out templates or watermarks with large pictures."
                    .into(),
            ));
        }
        crate::zip::to_vec(&[("kit.json", &json)])
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("This kit is too large to add.".into()));
        }
        let mut reader = crate::zip::Reader::new(std::io::Cursor::new(bytes), MAX_BYTES)?;
        let json = reader.read("kit.json", MAX_BYTES)?;
        let kit: Self = serde_json::from_slice(&json).map_err(|_| {
            Error::Invalid("This kit is damaged or wasn't made by ClipDesigner.".into())
        })?;
        kit.validate()?;
        Ok(kit)
    }
    pub fn read(path: &Path) -> Result<Self> {
        let size = std::fs::metadata(path)?.len();
        if size > MAX_BYTES {
            return Err(Error::Invalid("This kit is too large to add.".into()));
        }
        Self::decode(&std::fs::read(path)?)
    }
    pub fn write(&self, path: &Path) -> Result<()> {
        crate::atomic_write(path, &self.encode()?)
    }
}

/// `name`, or `name (2)`, `name (3)`... until nothing in `taken` has it
/// (ignoring case), for "Keep both".
pub fn unique_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_owned();
    }
    let base: String = name.chars().take(100).collect();
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|n| !taken(n))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Kit {
        let mut kit = Kit::new("Support team styles", "ClipDesigner test");
        kit.palettes.push(Palette {
            name: "Brand".into(),
            colors: vec![[24, 145, 193, 255], [242, 177, 52, 255]],
        });
        kit.sensitive_types
            .push(CustomType::new("Employee ID", "EMP-\\d{5}"));
        kit.presets.extend(
            crate::object_styles::presets(crate::Kind::Rectangle)
                .into_iter()
                .take(2),
        );
        kit
    }

    #[test]
    fn a_kit_round_trips_and_counts_its_items() {
        let kit = sample();
        assert_eq!(kit.count(Category::Presets), 2);
        assert_eq!(kit.count(Category::Palettes), 1);
        assert_eq!(kit.total(), 4);
        let back = Kit::decode(&kit.encode().unwrap()).unwrap();
        assert_eq!(back.name, "Support team styles");
        assert_eq!(back.palettes, kit.palettes);
        assert_eq!(back.sensitive_types, kit.sensitive_types);
        let only = kit.only(|c| c == Category::Palettes);
        assert_eq!(only.total(), 1);
    }

    #[test]
    fn damaged_or_foreign_kits_are_refused() {
        let mut kit = sample();
        kit.version = 2;
        let json = serde_json::to_vec(&kit).unwrap();
        let bytes = crate::zip::to_vec(&[("kit.json", &json)]).unwrap();
        assert!(
            Kit::decode(&bytes)
                .unwrap_err()
                .to_string()
                .contains("newer")
        );
        assert!(Kit::decode(b"PK not really").is_err());
        let bytes = crate::zip::to_vec(&[("kit.json", b"{\"format\":1}")]).unwrap();
        assert!(Kit::decode(&bytes).is_err());
        let mut bad = sample();
        bad.palettes.push(Palette {
            name: "Empty".into(),
            colors: vec![],
        });
        assert!(bad.encode().is_err());
    }

    #[test]
    fn keep_both_names_count_up() {
        let taken = ["Brand", "Brand (2)"];
        let is = |n: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(n));
        assert_eq!(unique_name("brand", is), "brand (3)");
        assert_eq!(unique_name("Other", is), "Other");
    }
}
