//! Original geometry-free object templates. Reuses ClipCraft's validated style
//! storage and atomic batch-command pattern; no image, label or pointer is saved.
use crate::text::{HorizontalAlignment, TextFont, TextStyle, VerticalAlignment};
use crate::{Annotation, Command, Error, Kind, Result, atomic_write};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const MAX_TEMPLATES: usize = 96;
/// Copy supported presentation roles, never labels, identity, geometry or rotation.
/// A field unsupported by the target is ignored; compatible fields still apply.
pub fn paint_format(source: &Annotation, target: &Annotation) -> Result<Annotation> {
    if source.kind == Kind::Image {
        let mut result = target.clone();
        result.shadow = source.shadow;
        return Ok(result);
    }
    let source_style = ObjectStyle::from_annotation("Format painter".into(), source)?;
    if source.kind == target.kind || (is_shape(source.kind) && is_shape(target.kind)) {
        let mut style = source_style;
        style.kind = target.kind;
        style.highlighter_rect = target.highlighter_rect;
        if !matches!(
            target.kind,
            Kind::Rectangle | Kind::Callout | Kind::Simplify
        ) {
            style.corner_radius = 0.0;
        }
        return style.apply(target);
    }
    let mut result = target.clone();
    result.shadow = source.shadow;
    if matches!(source.kind, Kind::Line | Kind::Arrow)
        && matches!(target.kind, Kind::Line | Kind::Arrow)
    {
        result.line_style = source.line_style;
    }
    let line = |kind| {
        matches!(
            kind,
            Kind::Line | Kind::Arrow | Kind::Pen | Kind::Highlighter
        )
    };
    let outline = if let Some(step) = &source.step_outline {
        Some(step.color)
    } else if let Some(c) = &source.callout {
        Some(c.outline)
    } else if is_shape(source.kind) || line(source.kind) {
        Some(source.color)
    } else {
        None
    };
    if let Some(outline) = outline {
        let width = source
            .step_outline
            .as_ref()
            .map_or(source.width, |s| s.width.round().clamp(1.0, 32.0) as u32);
        if result.kind == Kind::Step {
            result.step_outline = Some(crate::steps::Outline {
                color: outline,
                width: source
                    .step_outline
                    .as_ref()
                    .map_or(width as f32, |s| s.width),
            });
        } else if let Some(c) = &mut result.callout {
            c.outline = outline;
            result.width = width;
        } else if is_shape(result.kind) || line(result.kind) {
            result.color = outline;
            result.width = width;
        }
    }
    // Some(None) explicitly removes a shape fill; None means no fill role exists.
    let fill = if let Some(c) = &source.callout {
        Some(Some(c.fill))
    } else if is_shape(source.kind) {
        Some(source.fill)
    } else if matches!(source.kind, Kind::Step | Kind::Simplify) {
        Some(Some(source.color))
    } else {
        None
    };
    if let Some(fill) = fill {
        if let Some(c) = &mut result.callout {
            c.fill = fill.unwrap_or([0; 4]);
        } else if is_shape(result.kind) {
            result.fill = fill;
        } else if matches!(result.kind, Kind::Step | Kind::Simplify) {
            result.color = fill.unwrap_or([0; 4]);
        }
    }
    if matches!(
        source.kind,
        Kind::Rectangle | Kind::Callout | Kind::Simplify
    ) && matches!(
        target.kind,
        Kind::Rectangle | Kind::Callout | Kind::Simplify
    ) {
        result.corner_radius = source.corner_radius.min(result.max_corner_radius());
    }
    if let Some(text) = &source.text {
        let label_color = text.color.unwrap_or_else(|| {
            if source.kind == Kind::Step {
                crate::steps::foreground(source).color
            } else {
                source.color
            }
        });
        if result.text.is_some() || is_shape(result.kind) {
            let target = result.text.get_or_insert_with(|| TextStyle {
                content: String::new(),
                ..Default::default()
            });
            TextFormat::from_text(text).apply(target);
            target.color = Some(label_color);
            let padding = source.callout.as_ref().map_or(text.padding, |c| c.padding);
            if let Some(c) = &mut result.callout {
                c.padding = padding;
            } else {
                target.padding = padding;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod shared_format_tests;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    typography: Option<crate::text::typography::Typography>,
    font: TextFont,
    size_points: f32,
    horizontal: HorizontalAlignment,
    vertical: VerticalAlignment,
    padding: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<[u8; 4]>,
    #[serde(default)]
    fit: crate::text::TextFit,
    #[serde(default = "crate::text::wrap_default")]
    wrap: bool,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
    #[serde(default, skip_serializing_if = "crate::text::is_false")]
    underline: bool,
    #[serde(default, skip_serializing_if = "crate::text::is_false")]
    strikethrough: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outline: Option<crate::text::outline::Outline>,
}
impl TextFormat {
    fn from_text(t: &TextStyle) -> Self {
        Self {
            typography: t.typography,
            font: t.font,
            size_points: t.size_points,
            horizontal: t.horizontal,
            vertical: t.vertical,
            padding: t.padding,
            color: t.color,
            fit: t.fit,
            wrap: t.wrap,
            bold: t.bold,
            italic: t.italic,
            underline: t.underline,
            strikethrough: t.strikethrough,
            outline: t.outline,
        }
    }
    fn apply(&self, t: &mut TextStyle) {
        t.typography = self.typography;
        t.font = self.font;
        t.size_points = self.size_points;
        t.horizontal = self.horizontal;
        t.vertical = self.vertical;
        t.padding = self.padding;
        t.color = self.color;
        t.fit = self.fit;
        t.wrap = self.wrap;
        t.bold = self.bold;
        t.italic = self.italic;
        t.underline = self.underline;
        t.strikethrough = self.strikethrough;
        t.outline = self.outline;
    }
    fn validate(&self) -> Result<()> {
        let mut sample = TextStyle {
            content: String::new(),
            ..Default::default()
        };
        self.apply(&mut sample);
        sample.validate()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalloutFormat {
    fill: [u8; 4],
    outline: [u8; 4],
    padding: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectStyle {
    #[serde(default, skip_serializing_if = "crate::text::is_false")]
    pub highlighter_rect: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_outline: Option<crate::steps::Outline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<crate::line_style::Style>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<crate::shadow::Shadow>,
    pub name: String,
    pub kind: Kind,
    pub color: [u8; 4],
    pub width: u32,
    pub corner_radius: f32,
    pub fill: Option<[u8; 4]>,
    text: Option<TextFormat>,
    callout: Option<CalloutFormat>,
}
impl ObjectStyle {
    pub fn same_variant(&self, other: &Self) -> bool {
        self.kind == other.kind && self.highlighter_rect == other.highlighter_rect
    }
    pub fn text_font(&self) -> Option<TextFont> {
        self.text.as_ref().map(|t| t.font)
    }
    pub fn text_style(&self) -> Option<TextStyle> {
        self.text.as_ref().map(|format| {
            let mut style = TextStyle::default();
            format.apply(&mut style);
            style
        })
    }
    pub fn callout_colors(&self) -> Option<([u8; 4], [u8; 4])> {
        self.callout.as_ref().map(|c| (c.fill, c.outline))
    }
    pub fn accepts(&self, kind: Kind) -> bool {
        kind == self.kind
            || (is_shape(kind) && is_shape(self.kind))
            || (matches!(kind, Kind::Blur | Kind::Pixelate)
                && matches!(self.kind, Kind::Blur | Kind::Pixelate))
    }
    /// Category changes preserve the bounding box, label and identity. Only
    /// explicit shape-family changes replace the outline geometry. Blur and
    /// Pixelate can exchange intensity presets without changing their region;
    /// opaque Redact is deliberately outside that family.
    pub fn apply_compatible(&self, original: &Annotation) -> Result<Annotation> {
        if original.kind == self.kind {
            return self.apply(original);
        }
        if !self.accepts(original.kind) {
            return Err(Error::Invalid("Select compatible objects first.".into()));
        }
        let mut converted = original.clone();
        converted.kind = self.kind;
        converted.corner_radius = 0.0;
        converted.points = if self.kind == Kind::Polygon {
            crate::polygon::outline(converted.start, converted.end, 5)
        } else {
            vec![]
        };
        self.apply(&converted)
    }
    pub fn compatible_command(&self, objects: &[Annotation]) -> Result<Command> {
        if objects.is_empty() {
            return Err(Error::Invalid("Select objects first.".into()));
        }
        Ok(Command::UpdateMany {
            annotations: objects
                .iter()
                .map(|a| self.apply_compatible(a))
                .collect::<Result<_>>()?,
        })
    }
    pub fn from_annotation(name: String, a: &Annotation) -> Result<Self> {
        if a.kind == Kind::Image {
            return Err(Error::Invalid(
                "Image objects do not have drawing presets".into(),
            ));
        }
        let result = Self {
            name,
            highlighter_rect: a.highlighter_rect,
            step_outline: a.step_outline.clone(),
            line_style: a.line_style,
            shadow: a.shadow,
            kind: a.kind,
            color: a.color,
            width: a.width,
            corner_radius: a.corner_radius,
            fill: a.fill,
            text: a.text.as_ref().map(TextFormat::from_text),
            callout: a.callout.as_ref().map(|c| CalloutFormat {
                fill: c.fill,
                outline: c.outline,
                padding: c.padding,
            }),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<()> {
        if self.highlighter_rect && self.kind != Kind::Highlighter {
            return Err(Error::Invalid(
                "Rectangle highlighting requires a Highlighter".into(),
            ));
        }
        if let Some(style) = self.line_style {
            style.validate()?;
            if !matches!(self.kind, Kind::Arrow | Kind::Line) {
                return Err(Error::Invalid(
                    "Line settings require a line or arrow".into(),
                ));
            }
        }
        if let Some(shadow) = self.shadow {
            shadow.validate()?;
        }
        if let Some(outline) = &self.step_outline {
            outline.validate()?;
            if self.kind != Kind::Step {
                return Err(Error::Invalid("Step outline only applies to Steps".into()));
            }
        }
        if self.name.trim() != self.name
            || self.name.is_empty()
            || self.name.chars().count() > 40
            || self.name.chars().any(char::is_control)
            || !(1..=32).contains(&self.width)
        {
            return Err(Error::Invalid(
                "Template needs a name of 1–40 characters and width/intensity of 1–32.".into(),
            ));
        }
        if !self.corner_radius.is_finite()
            || !(0.0..=16384.0).contains(&self.corner_radius)
            || (self.corner_radius != 0.0
                && !matches!(self.kind, Kind::Rectangle | Kind::Callout | Kind::Simplify))
            || (self.fill.is_some()
                && !matches!(self.kind, Kind::Rectangle | Kind::Ellipse | Kind::Polygon))
            || (self.text.is_none() && matches!(self.kind, Kind::Text | Kind::Callout | Kind::Step))
            || (self.text.is_some()
                && !matches!(
                    self.kind,
                    Kind::Text
                        | Kind::Callout
                        | Kind::Step
                        | Kind::Rectangle
                        | Kind::Ellipse
                        | Kind::Polygon
                ))
            || self.callout.is_some() != (self.kind == Kind::Callout)
        {
            return Err(Error::Invalid(
                "Template formatting does not match its object type.".into(),
            ));
        }
        if let Some(text) = &self.text {
            text.validate()?;
        }
        if let Some(callout) = &self.callout
            && (!callout.padding.is_finite() || !(0.0..=128.0).contains(&callout.padding))
        {
            return Err(Error::Invalid(
                "Callout padding must be 0–128 pixels.".into(),
            ));
        }
        Ok(())
    }
    pub fn apply(&self, original: &Annotation) -> Result<Annotation> {
        self.validate()?;
        if original.kind != self.kind
            || (original.text.is_some() != self.text.is_some()
                && !matches!(self.kind, Kind::Rectangle | Kind::Ellipse | Kind::Polygon))
            || original.callout.is_some() != self.callout.is_some()
        {
            return Err(Error::Invalid(
                "Select objects of the template's type.".into(),
            ));
        }
        let mut result = original.clone();
        if result.kind == Kind::Highlighter && result.highlighter_rect != self.highlighter_rect {
            let (min, max) = result.body_bounds();
            result.start = min;
            result.end = max;
            result.points = if self.highlighter_rect {
                vec![]
            } else {
                vec![min, max]
            };
        }
        result.highlighter_rect = self.highlighter_rect;
        result.color = self.color;
        result.width = self.width;
        result.step_outline = self.step_outline.clone();
        result.line_style = self.line_style;
        result.shadow = self.shadow;
        result.fill = self.fill;
        result.corner_radius = self.corner_radius.min(result.max_corner_radius());
        if let Some(format) = &self.text {
            let text = result.text.get_or_insert_with(|| TextStyle {
                content: String::new(),
                ..Default::default()
            });
            format.apply(text);
            text.validate()?;
        }
        if let (Some(format), Some(callout)) = (&self.callout, &mut result.callout) {
            callout.fill = format.fill;
            callout.outline = format.outline;
            callout.padding = format.padding;
        }
        Ok(result)
    }
    /// Every target must be compatible. No partial application or hidden skips.
    pub fn command(&self, objects: &[Annotation]) -> Result<Command> {
        if objects.is_empty() {
            return Err(Error::Invalid("Select objects first.".into()));
        }
        let annotations = objects
            .iter()
            .map(|a| self.apply(a))
            .collect::<Result<_>>()?;
        Ok(Command::UpdateMany { annotations })
    }
    pub fn matches(&self, objects: &[Annotation]) -> bool {
        !objects.is_empty() && objects.iter().all(|a| self.apply(a).is_ok_and(|b| b == *a))
    }
}

pub fn is_shape(kind: Kind) -> bool {
    matches!(kind, Kind::Rectangle | Kind::Ellipse | Kind::Polygon)
}

pub struct PresetGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub styles: Vec<ObjectStyle>,
}

/// Only categories backed by implemented geometry are offered. Custom styles
/// remain separate and are never overwritten by a catalog refresh.
pub fn groups(kind: Kind) -> Vec<PresetGroup> {
    let definitions: Vec<(&str, &str, Kind, bool)> = if is_shape(kind) {
        vec![
            ("shape_rectangle", "Rectangle", Kind::Rectangle, false),
            ("shape_rounded", "Rounded rectangle", Kind::Rectangle, true),
            ("shape_ellipse", "Circle / ellipse", Kind::Ellipse, false),
            ("shape_other", "Other shapes", Kind::Polygon, false),
        ]
    } else {
        match kind {
            Kind::Simplify => vec![
                ("simplify_block", "Block", kind, false),
                ("simplify_rounded", "Rounded block", kind, true),
            ],
            Kind::Text => vec![
                ("text_sans", "Text", kind, false),
                ("text_mono", "Monospace", kind, false),
            ],
            Kind::Callout => vec![
                ("callout_box", "Callout", kind, false),
                ("callout_rounded", "Rounded callout", kind, true),
            ],
            Kind::Line => vec![("line", "Line", kind, false)],
            Kind::Step => vec![("step", "Step", kind, false)],
            Kind::Blur | Kind::Pixelate => vec![
                ("blur", "Blur", Kind::Blur, false),
                ("pixelate", "Pixelate", Kind::Pixelate, false),
            ],
            Kind::Redact => vec![("redact", "Redact", kind, false)],
            Kind::Arrow => vec![("arrow", "Straight arrow", kind, false)],
            Kind::Pen => vec![("pen", "Pen", kind, false)],
            Kind::Highlighter => vec![
                ("highlighter", "Pen", kind, false),
                (
                    "highlighter_rectangle",
                    "Highlighter rectangle",
                    kind,
                    false,
                ),
            ],
            Kind::Eraser => vec![("eraser", "Eraser", kind, false)],
            _ => unreachable!(),
        }
    };
    definitions
        .into_iter()
        .map(|(id, label, kind, rounded)| {
            let mut styles = presets(kind);
            if kind == Kind::Line {
                let base = styles[0].clone();
                styles.extend(crate::styles::connector_presets(kind).into_iter().map(|s| {
                    ObjectStyle {
                        name: s.name,
                        color: s.color,
                        width: s.width,
                        line_style: s.line_style,
                        ..base.clone()
                    }
                }));
            }
            for style in &mut styles {
                style.highlighter_rect = id == "highlighter_rectangle";
                if rounded {
                    style.corner_radius = 16.0;
                }
                if id == "text_mono" {
                    style.text.as_mut().unwrap().font = TextFont::HackRegular;
                }
            }
            if is_shape(kind) {
                let filled: Vec<_> = styles
                    .iter()
                    .cloned()
                    .map(|mut s| {
                        s.name = format!("{} filled", s.name);
                        s.fill = Some(s.color);
                        s
                    })
                    .collect();
                styles.extend(filled);
            }
            PresetGroup { id, label, styles }
        })
        .collect()
}

/// Original presets for currently implemented annotation types. Effects offer
/// intensity choices instead of misleading color tiles. Redact stays opaque.
pub fn presets(kind: Kind) -> Vec<ObjectStyle> {
    let colors = [
        ("Coral", [240, 85, 55, 255]),
        ("Lime", [135, 196, 105, 255]),
        ("Ocean", [38, 155, 194, 255]),
        ("Gold", [220, 190, 80, 255]),
        ("White", [255, 255, 255, 255]),
        ("Plum", [191, 140, 207, 255]),
    ];
    let text = matches!(kind, Kind::Text | Kind::Callout | Kind::Step).then(|| TextFormat {
        typography: None,
        font: TextFont::default(),
        size_points: 18.0,
        horizontal: HorizontalAlignment::Center,
        vertical: VerticalAlignment::Middle,
        padding: 0.0,
        color: None,
        fit: crate::text::TextFit::Shrink,
        wrap: true,
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        outline: None,
    });
    let effect = matches!(kind, Kind::Blur | Kind::Pixelate);
    colors
        .into_iter()
        .enumerate()
        .take(if kind == Kind::Redact { 1 } else { 6 })
        .map(|(i, (name, mut color))| {
            if kind == Kind::Highlighter {
                color[3] = 80;
            }
            if kind == Kind::Redact {
                color = [0, 0, 0, 255];
            }
            let width = if effect { [2, 4, 8, 12, 20, 32][i] } else { 3 };
            ObjectStyle {
                highlighter_rect: false,
                step_outline: None,
                line_style: None,
                shadow: None,
                name: if effect {
                    format!("Intensity {width}")
                } else if kind == Kind::Redact {
                    "Opaque black".into()
                } else {
                    name.into()
                },
                kind,
                color,
                width,
                corner_radius: 0.0,
                fill: None,
                text: text.clone(),
                callout: (kind == Kind::Callout).then_some(CalloutFormat {
                    fill: [255, 255, 255, 255],
                    outline: color,
                    padding: 8.0,
                }),
            }
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    version: u32,
    styles: Vec<ObjectStyle>,
}
fn load(root: &Path) -> Result<Vec<ObjectStyle>> {
    let path = root.join("object-templates.json");
    let metadata = match fs::metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    if metadata.len() > 131072 {
        return Err(Error::Invalid(
            "Template file is too large; it was preserved.".into(),
        ));
    }
    let index: Index = serde_json::from_slice(&fs::read(path)?)?;
    if index.version != 1 || index.styles.len() > MAX_TEMPLATES {
        return Err(Error::Invalid(
            "Unsupported template file; it was preserved.".into(),
        ));
    }
    let mut keys = std::collections::HashSet::new();
    for style in &index.styles {
        style.validate()?;
        if !keys.insert((format!("{:?}", style.kind), style.name.to_lowercase())) {
            return Err(Error::Invalid(
                "Duplicate template name for this object type.".into(),
            ));
        }
    }
    Ok(index.styles)
}
pub struct Store {
    root: PathBuf,
    styles: Vec<ObjectStyle>,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let styles = load(&root)?;
        Ok(Self { root, styles })
    }
    pub fn entries(&self) -> &[ObjectStyle] {
        &self.styles
    }
    pub fn refresh(&mut self) -> Result<()> {
        self.styles = load(&self.root)?;
        Ok(())
    }
    pub fn save(&mut self, style: ObjectStyle) -> Result<()> {
        style.validate()?;
        self.change(|items| {
            if let Some(old) = items.iter_mut().find(|s| {
                s.kind == style.kind && s.name.to_lowercase() == style.name.to_lowercase()
            }) {
                *old = style;
            } else if items.len() < MAX_TEMPLATES {
                items.push(style);
            } else {
                return Err(Error::Invalid(
                    "96 custom templates are already saved. Remove one first.".into(),
                ));
            }
            Ok(())
        })
    }
    pub fn remove(&mut self, kind: Kind, name: &str) -> Result<()> {
        self.change(|items| {
            let i = items
                .iter()
                .position(|s| s.kind == kind && s.name.to_lowercase() == name.to_lowercase())
                .ok_or_else(|| Error::Invalid("Saved template no longer exists.".into()))?;
            items.remove(i);
            Ok(())
        })
    }
    fn change(&mut self, edit: impl FnOnce(&mut Vec<ObjectStyle>) -> Result<()>) -> Result<()> {
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("object-templates.lock"))?;
        lock.try_lock().map_err(|_| {
            Error::Invalid("Templates are busy in another window. Try again.".into())
        })?;
        let mut next = load(&self.root)?;
        edit(&mut next)?;
        atomic_write(
            &self.root.join("object-templates.json"),
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
    use crate::{Document, Session};
    #[test]
    fn blur_pixelate_presets_convert_mixed_regions_atomically_and_preserve_redaction() {
        let mut doc = fixture();
        doc.annotations
            .retain(|a| matches!(a.kind, Kind::Blur | Kind::Pixelate));
        let original = doc.annotations.clone();
        for kind in [Kind::Blur, Kind::Pixelate] {
            for style in presets(kind) {
                let mut session = Session::new(doc.clone()).unwrap();
                session
                    .execute(style.compatible_command(&original).unwrap())
                    .unwrap();
                for (before, after) in original.iter().zip(&session.document.annotations) {
                    assert_eq!(
                        (before.id, before.start, before.end, before.rotation_degrees),
                        (after.id, after.start, after.end, after.rotation_degrees)
                    );
                    assert_eq!((after.kind, after.width), (kind, style.width));
                }
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("effect-regions.capture.json");
                session.save(&path).unwrap();
                let reopened = Document::load(&path).unwrap();
                assert_eq!(reopened.annotations, session.document.annotations);
                assert_eq!(
                    reopened.render().unwrap(),
                    session.document.render().unwrap()
                );
                session.execute(Command::Undo).unwrap();
                assert_eq!(session.document.annotations, original);
                assert!(!session.can_undo());
                let mut redact = original[0].clone();
                redact.kind = Kind::Redact;
                assert!(!style.accepts(Kind::Redact));
                assert!(
                    style
                        .compatible_command(&[original[0].clone(), redact])
                        .is_err()
                );
                assert!(!presets(Kind::Redact)[0].accepts(kind));
                assert_eq!(session.document.annotations, original);
            }
        }
    }
    #[test]
    fn grouped_shapes_convert_atomically_keep_labels_and_reopen_with_one_undo() {
        let mut doc = fixture();
        doc.annotations.retain(|a| is_shape(a.kind));
        for a in &mut doc.annotations {
            a.text = Some(TextStyle {
                content: format!("Label {}", a.id),
                ..Default::default()
            });
        }
        let original = doc.annotations.clone();
        for group in groups(Kind::Rectangle) {
            for style in group.styles {
                let mut session = Session::new(doc.clone()).unwrap();
                session
                    .execute(style.compatible_command(&original).unwrap())
                    .unwrap();
                let expected = session.document.annotations.clone();
                for (before, after) in original.iter().zip(&expected) {
                    assert_eq!(
                        (before.id, before.start, before.end),
                        (after.id, after.start, after.end)
                    );
                    assert_eq!(before.text, after.text);
                    assert_eq!(after.kind, style.kind);
                }
                let temp = tempfile::tempdir().unwrap();
                let path = temp.path().join("group.capture.json");
                session.save(&path).unwrap();
                let reopened = Document::load(&path).unwrap();
                assert_eq!(reopened.annotations, expected);
                assert_eq!(
                    reopened.render().unwrap(),
                    session.document.render().unwrap()
                );
                session.execute(Command::Undo).unwrap();
                assert_eq!(session.document.annotations, original);
                assert!(!session.can_undo());
            }
        }
        let mut mixed = original;
        mixed.push(
            crate::steps::marker(
                99,
                [100.0, 100.0],
                44,
                [1, 2, 3, 255],
                (1, false),
                [640, 400],
            )
            .unwrap(),
        );
        assert!(
            groups(Kind::Ellipse)[2].styles[0]
                .compatible_command(&mixed)
                .is_err()
        );
    }
    #[test]
    fn every_group_uses_valid_supported_styles_and_stable_unique_keys() {
        let mut keys = std::collections::HashMap::new();
        for kind in [
            Kind::Rectangle,
            Kind::Text,
            Kind::Callout,
            Kind::Line,
            Kind::Step,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Redact,
            Kind::Arrow,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Eraser,
        ] {
            let mut local_keys = std::collections::HashSet::new();
            for group in groups(kind) {
                assert!(local_keys.insert(group.id));
                if let Some(existing) = keys.insert(group.id, group.styles.clone()) {
                    // The same effect families are reachable from either tool;
                    // shared preference keys must mean identical choices.
                    assert!(matches!(kind, Kind::Blur | Kind::Pixelate));
                    assert_eq!(existing, group.styles);
                }
                for style in group.styles {
                    style.validate().unwrap();
                }
            }
        }
        assert_eq!(
            groups(Kind::Text)[1].styles[0].text.as_ref().unwrap().font,
            TextFont::HackRegular
        );
    }
    fn fixture() -> Document {
        let mut d: Document =
            serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json"))
                .unwrap();
        for kind in [
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Line,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Eraser,
        ] {
            let mut a = d.annotations[0].clone();
            a.id = d.annotations.len() as u64 + 1;
            a.kind = kind;
            a.text = None;
            a.callout = None;
            a.corner_radius = 0.0;
            a.points = match kind {
                Kind::Polygon => vec![a.start, [a.end[0], a.start[1]], a.end],
                Kind::Pen | Kind::Highlighter | Kind::Eraser => vec![a.start, a.end],
                _ => vec![],
            };
            d.annotations.push(a);
        }
        d.annotations.push(
            crate::steps::marker(
                d.annotations.len() as u64 + 1,
                [300.0, 200.0],
                44,
                [80, 90, 180, 255],
                (1, false),
                [640, 400],
            )
            .unwrap(),
        );
        d.validate().unwrap();
        d
    }
    #[test]
    fn templates_preserve_geometry_content_and_render_after_save_with_one_undo() {
        let doc = fixture();
        for a in &doc.annotations {
            let mut style = presets(a.kind)[2].clone();
            style.width = 12;
            let mut second = a.clone();
            second.id = 100;
            if let Some(t) = &mut second.text {
                t.content = "Different label".into();
            }
            let mut d = doc.clone();
            d.annotations = vec![a.clone(), second];
            let original = d.annotations.clone();
            let mut s = Session::new(d).unwrap();
            s.execute(style.command(&s.document.annotations).unwrap())
                .unwrap();
            for (before, after) in original.iter().zip(&s.document.annotations) {
                assert_eq!(before.id, after.id);
                assert_eq!(before.kind, after.kind);
                assert_eq!(before.start, after.start);
                assert_eq!(before.end, after.end);
                assert_eq!(before.points, after.points);
                assert_eq!(
                    before.text.as_ref().map(|t| &t.content),
                    after.text.as_ref().map(|t| &t.content)
                );
                assert_eq!(
                    before.callout.as_ref().map(|c| c.pointer),
                    after.callout.as_ref().map(|c| c.pointer)
                );
            }
            assert!(style.matches(&s.document.annotations));
            let expected = s.document.annotations.clone();
            s.execute(style.command(&expected).unwrap()).unwrap();
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("styled.capture.json");
            s.save(&path).unwrap();
            let reopened = Document::load(&path).unwrap();
            assert_eq!(reopened.annotations, expected);
            assert_eq!(reopened.render().unwrap(), s.document.render().unwrap());
            s.execute(Command::Undo).unwrap();
            assert_eq!(s.document.annotations, original);
            assert!(!s.can_undo());
        }
    }
    #[test]
    fn saved_templates_exclude_content_geometry_and_preserve_old_arrow_store() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("arrow-styles.json");
        fs::write(&old, b"existing arrow store").unwrap();
        let doc = fixture();
        let a = doc
            .annotations
            .iter()
            .find(|a| a.kind == Kind::Callout)
            .unwrap();
        let style = ObjectStyle::from_annotation("Example".into(), a).unwrap();
        let value = serde_json::to_value(&style).unwrap();
        assert!(value.get("start").is_none());
        assert!(value.get("points").is_none());
        assert!(value["text"].get("content").is_none());
        assert!(value["callout"].get("pointer").is_none());
        let mut first = Store::open(dir.path()).unwrap();
        let mut stale = Store::open(dir.path()).unwrap();
        first.save(style.clone()).unwrap();
        stale.save(presets(Kind::Line)[0].clone()).unwrap();
        first.refresh().unwrap();
        assert_eq!(first.entries().len(), 2);
        assert_eq!(Store::open(dir.path()).unwrap().entries()[0], style);
        stale.remove(Kind::Callout, "example").unwrap();
        assert_eq!(fs::read(old).unwrap(), b"existing arrow store");
        assert_eq!(stale.entries().len(), 1);
    }
    #[test]
    fn invalid_mixed_and_corrupt_templates_leave_document_and_disk_unchanged() {
        let d = fixture();
        let style = presets(Kind::Arrow)[0].clone();
        assert!(style.command(&d.annotations).is_err());
        assert!(!style.matches(&d.annotations));
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.save(style.clone()).unwrap();
        let path = dir.path().join("object-templates.json");
        let bytes = fs::read(&path).unwrap();
        let mut invalid = style.clone();
        invalid.corner_radius = f32::NAN;
        assert!(store.save(invalid).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join("object-templates.lock"))
            .unwrap();
        lock.lock().unwrap();
        assert!(store.save(presets(Kind::Line)[0].clone()).is_err());
        drop(lock);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&path, b"broken").unwrap();
        assert!(store.save(style).is_err());
        assert!(Store::open(dir.path()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
    }
    #[test]
    fn radius_clamps_without_resizing_and_all_preset_types_validate() {
        let mut a = fixture()
            .annotations
            .into_iter()
            .find(|a| a.kind == Kind::Rectangle)
            .unwrap();
        a.end = [a.start[0] + 12.0, a.start[1] + 10.0];
        let mut style = presets(Kind::Rectangle)[0].clone();
        style.corner_radius = 30.0;
        let b = style.apply(&a).unwrap();
        assert_eq!(b.corner_radius, 5.0);
        assert_eq!(a.start, b.start);
        assert_eq!(a.end, b.end);
        assert!(style.matches(&[b]));
        for kind in [
            Kind::Arrow,
            Kind::Line,
            Kind::Text,
            Kind::Callout,
            Kind::Step,
            Kind::Rectangle,
            Kind::Ellipse,
            Kind::Polygon,
            Kind::Blur,
            Kind::Pixelate,
            Kind::Redact,
            Kind::Pen,
            Kind::Highlighter,
            Kind::Eraser,
        ] {
            for style in presets(kind) {
                style.validate().unwrap();
            }
        }
    }
}
