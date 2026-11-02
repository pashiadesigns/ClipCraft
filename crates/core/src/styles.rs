//! Reusable arrow formatting, including caps, curves and shadows. Endpoint
//! positions stay in the document. Custom styles use bounded atomic storage.
use crate::{Annotation, Command, Error, Kind, Result, atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MAX_STYLES: usize = 24;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArrowStyle {
    pub name: String,
    pub color: [u8; 4],
    pub width: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<crate::shadow::Shadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<crate::line_style::Style>,
}
impl ArrowStyle {
    pub fn validate(&self) -> Result<()> {
        if let Some(style) = self.line_style {
            style.validate()?;
        }
        if let Some(shadow) = self.shadow {
            shadow.validate()?;
        }
        if self.name.trim() != self.name
            || self.name.is_empty()
            || self.name.chars().count() > 40
            || self.name.chars().any(char::is_control)
            || !(1..=32).contains(&self.width)
        {
            return Err(Error::Invalid(
                "Style needs a name of 1–40 characters and width of 1–32 pixels.".into(),
            ));
        }
        Ok(())
    }
    /// Return one ordinary update command, or no command for an unchanged style.
    pub fn command(&self, original: &Annotation) -> Result<Option<Command>> {
        self.validate()?;
        if original.kind != Kind::Arrow {
            return Err(Error::Invalid("This style applies to arrows only.".into()));
        }
        if original.color == self.color
            && original.width == self.width
            && original.shadow == self.shadow
            && original.line_style == self.line_style
        {
            return Ok(None);
        }
        let mut annotation = original.clone();
        annotation.color = self.color;
        annotation.width = self.width;
        annotation.shadow = self.shadow;
        annotation.line_style = self.line_style;
        Ok(Some(Command::Update { annotation }))
    }
}
pub fn presets() -> Vec<ArrowStyle> {
    let mut result: Vec<_> = [
        ("Coral", [240, 85, 55, 255], 3),
        ("Lime", [135, 196, 105, 255], 3),
        ("Ocean", [38, 155, 194, 255], 3),
        ("Gold", [220, 190, 80, 255], 3),
        ("White", [255, 255, 255, 255], 3),
        ("Plum", [191, 140, 207, 255], 3),
        ("Bold Coral", [240, 85, 55, 255], 6),
        ("Bold Ocean", [38, 155, 194, 255], 6),
    ]
    .into_iter()
    .map(|(name, color, width)| ArrowStyle {
        shadow: None,
        line_style: None,
        name: name.into(),
        color,
        width,
    })
    .collect();
    result.extend(connector_presets(Kind::Arrow));
    result
}
/// Geometry variations shared by arrow and line style galleries.
pub fn connector_presets(kind: Kind) -> Vec<ArrowStyle> {
    use crate::line_style::{Endpoint as E, Pattern as P, Style};
    [
        ("Circle dashed circle", E::Circle, E::Circle, P::Dashed),
        ("Circle to arrow", E::Circle, E::Triangle, P::Solid),
        ("Double arrow", E::OpenArrow, E::OpenArrow, P::Solid),
        ("Square dotted diamond", E::Square, E::Diamond, P::Dotted),
        ("Bar dash-dot arrow", E::Bar, E::Stealth, P::DashDot),
        (
            "Diamond long-dash circle",
            E::Diamond,
            E::Circle,
            P::LongDash,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (name, start, end, pattern))| ArrowStyle {
        name: name.into(),
        color: [
            [38, 155, 194, 255],
            [240, 85, 55, 255],
            [135, 196, 105, 255],
        ][i % 3],
        width: 3,
        shadow: None,
        line_style: Some(Style {
            start,
            end,
            pattern,
            start_size: 4.,
            end_size: 4.,
            ..Style::for_kind(kind)
        }),
    })
    .collect()
}
pub fn curved_presets() -> Vec<ArrowStyle> {
    presets()
        .into_iter()
        .map(|mut style| {
            style.name = format!("Curved {}", style.name);
            style.line_style = Some(crate::line_style::Style {
                curve: crate::line_style::Curve::Circular,
                bend: 0.25,
                ..style
                    .line_style
                    .unwrap_or(crate::line_style::Style::for_kind(Kind::Arrow))
            });
            style
        })
        .collect()
}
#[derive(Default, Serialize, Deserialize)]
struct Index {
    version: u32,
    styles: Vec<ArrowStyle>,
}
fn load(root: &Path) -> Result<Vec<ArrowStyle>> {
    let path = root.join("arrow-styles.json");
    let metadata = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    if metadata.len() > 32_768 {
        return Err(Error::Invalid(
            "Style file is too large; it was preserved.".into(),
        ));
    }
    let index: Index = serde_json::from_slice(&fs::read(path)?)?;
    let mut names = std::collections::HashSet::new();
    if index.version != 1 || index.styles.len() > MAX_STYLES {
        return Err(Error::Invalid(
            "Invalid style file; it was preserved.".into(),
        ));
    }
    for style in &index.styles {
        style.validate()?;
        if !names.insert(style.name.to_lowercase()) {
            return Err(Error::Invalid("Duplicate saved style name.".into()));
        }
    }
    Ok(index.styles)
}
pub struct Store {
    root: PathBuf,
    styles: Vec<ArrowStyle>,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let styles = load(&root)?;
        Ok(Self { root, styles })
    }
    pub fn entries(&self) -> &[ArrowStyle] {
        &self.styles
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.styles = load(&self.root)?;
        Ok(())
    }
    pub fn save(&mut self, style: ArrowStyle) -> Result<()> {
        style.validate()?;
        self.change(|items| {
            if let Some(old) = items
                .iter_mut()
                .find(|s| s.name.to_lowercase() == style.name.to_lowercase())
            {
                *old = style;
            } else if items.len() < MAX_STYLES {
                items.push(style);
            } else {
                return Err(Error::Invalid(
                    "24 custom styles are already saved. Remove a style first.".into(),
                ));
            }
            Ok(())
        })
    }
    pub fn remove(&mut self, name: &str) -> Result<()> {
        self.change(|items| {
            let i = items
                .iter()
                .position(|s| s.name.to_lowercase() == name.to_lowercase())
                .ok_or_else(|| Error::Invalid("Saved style no longer exists.".into()))?;
            items.remove(i);
            Ok(())
        })
    }
    fn change(&mut self, edit: impl FnOnce(&mut Vec<ArrowStyle>) -> Result<()>) -> Result<()> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("styles.lock"))?;
        lock.try_lock().map_err(|_| {
            Error::Invalid("Styles are busy in another ClipCraft window. Try again.".into())
        })?;
        let mut next = load(&self.root)?;
        edit(&mut next)?;
        atomic_write(
            &self.root.join("arrow-styles.json"),
            &serde_json::to_vec_pretty(&Index {
                version: 1,
                styles: next.clone(),
            })?,
        )?;
        self.styles = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrow_alpha_composites_once_over_background_with_consistent_coverage() {
        for background in [[255, 255, 255, 255], [10, 30, 70, 100], [91, 63, 22, 0]] {
            let mut doc = crate::Document::canvas(90, 75, background).unwrap();
            let arrow = Annotation {
                image: None,
                step_outline: None,
                line_style: None,
                shadow: None,
                analysis: None,
                highlighter_rect: false,
                group: None,
                rotation_degrees: 0.0,
                id: 1,
                kind: Kind::Arrow,
                start: [3.5, 2.5],
                end: [84.5, 68.5],
                color: [38, 155, 194, 255],
                width: 8,
                text: None,
                corner_radius: 0.0,
                fill: None,
                points: Vec::new(),
                callout: None,
            };
            doc.annotations.push(arrow);
            let mut footprint = crate::Document::canvas(90, 75, [0, 0, 0, 0]).unwrap();
            footprint.annotations = doc.annotations.clone();
            let opaque = footprint.render().unwrap();
            doc.annotations[0].color[3] = 128;
            let translucent = doc.render().unwrap();
            let mut covered = 0;
            for (base, actual) in opaque.pixels().zip(translucent.pixels()) {
                let expected = if base[3] > 0 {
                    covered += 1;
                    let alpha = (128 * u64::from(base[3]) + 127) / 255;
                    let inv = 255 - alpha;
                    let destination_alpha = background[3] as u64;
                    let total = alpha * 255 + destination_alpha * inv;
                    let mut out = [0u8; 4];
                    for i in 0..3 {
                        out[i] = ((([38u64, 155, 194][i] * alpha * 255
                            + background[i] as u64 * destination_alpha * inv)
                            * 2
                            + total)
                            / (total * 2)) as u8;
                    }
                    out[3] = ((total * 2 + 255) / (255 * 2)) as u8;
                    out
                } else {
                    background
                };
                assert_eq!(actual.0, expected);
            }
            assert!(covered > 300);
            doc.annotations[0].color[3] = 0;
            assert!(doc.render().unwrap().pixels().all(|p| p.0 == background));
        }
    }
    #[test]
    fn style_application_preserves_geometry_and_other_objects_with_undo_and_reopen() {
        let doc: crate::Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        let mut session = crate::Session::new(doc).unwrap();
        let original = session.document.annotations.clone();
        let arrow = original.iter().find(|a| a.kind == Kind::Arrow).unwrap();
        let style = ArrowStyle {
            shadow: None,
            line_style: None,
            name: "Translucent".into(),
            color: [12, 170, 99, 123],
            width: 12,
        };
        session
            .execute(style.command(arrow).unwrap().unwrap())
            .unwrap();
        let changed = session
            .document
            .annotations
            .iter()
            .find(|a| a.id == arrow.id)
            .unwrap();
        assert_eq!(
            (changed.start, changed.end, changed.id),
            (arrow.start, arrow.end, arrow.id)
        );
        assert_eq!(changed.color, style.color);
        assert_eq!(changed.width, 12);
        assert!(style.command(changed).unwrap().is_none());
        for a in original.iter().filter(|a| a.id != arrow.id) {
            assert!(session.document.annotations.contains(a));
        }
        let pixels = session.document.render().unwrap();
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, original);
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.render().unwrap(), pixels);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.capture.json");
        session.document.save(&path).unwrap();
        assert_eq!(
            crate::Document::load(&path).unwrap().render().unwrap(),
            pixels
        );
        assert!(
            style
                .command(original.iter().find(|a| a.kind != Kind::Arrow).unwrap())
                .is_err()
        );
        let mut invalid = style;
        invalid.width = 0;
        assert!(invalid.command(arrow).is_err());
    }
    #[test]
    fn custom_styles_persist_merge_and_remove_without_touching_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("unrelated.txt"), "keep").unwrap();
        let mut first = Store::open(dir.path()).unwrap();
        let mut stale = Store::open(dir.path()).unwrap();
        let presets = presets();
        first.save(presets[0].clone()).unwrap();
        stale.save(presets[1].clone()).unwrap();
        first.refresh().unwrap();
        assert_eq!(first.entries().len(), 2);
        let mut changed = presets[0].clone();
        changed.color[3] = 0;
        changed.name = "CORAL".into();
        first.save(changed.clone()).unwrap();
        assert_eq!(Store::open(dir.path()).unwrap().entries()[0], changed);
        stale.remove("coral").unwrap();
        assert_eq!(stale.entries(), &presets[1..2]);
        assert_eq!(
            fs::read_to_string(dir.path().join("unrelated.txt")).unwrap(),
            "keep"
        );
        assert!(stale.remove("missing").is_err());
    }
    #[test]
    fn invalid_busy_full_and_corrupt_style_writes_preserve_existing_data() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        for n in 0..MAX_STYLES {
            let mut s = presets()[0].clone();
            s.name = format!("Style {n}");
            store.save(s).unwrap();
        }
        let path = dir.path().join("arrow-styles.json");
        let before = fs::read(&path).unwrap();
        assert!(store.save(presets()[0].clone()).is_err());
        let mut invalid = presets()[0].clone();
        invalid.name = "\n".into();
        assert!(store.save(invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("styles.lock"))
            .unwrap();
        lock.lock().unwrap();
        assert!(store.remove("Style 0").is_err());
        drop(lock);
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, b"broken").unwrap();
        assert!(store.remove("Style 0").is_err());
        assert!(Store::open(dir.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
    }
}
