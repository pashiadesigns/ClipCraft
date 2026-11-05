//! Numbered or lettered markers using the shared editable annotation model.
use crate::{
    Annotation, Document, Error, Kind, Result,
    text::{self, HorizontalAlignment, TextStyle, VerticalAlignment},
};
use image::RgbaImage;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub color: [u8; 4],
    pub width: f32,
}
impl Default for Outline {
    fn default() -> Self {
        Self {
            color: [30, 30, 30, 255],
            width: 3.0,
        }
    }
}
impl Outline {
    pub fn validate(&self) -> Result<()> {
        if !self.width.is_finite() || !(0.0..=32.0).contains(&self.width) {
            return Err(Error::Invalid(
                "Step outline width must be 0–32 pixels".into(),
            ));
        }
        Ok(())
    }
}

pub fn label(number: u32, letters: bool) -> Result<String> {
    if !(1..=999).contains(&number) {
        return Err(Error::Invalid("Step sequence must be 1–999".into()));
    }
    if !letters {
        return Ok(number.to_string());
    }
    let mut n = number;
    let mut chars = vec![];
    while n > 0 {
        n -= 1;
        chars.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    Ok(chars.into_iter().rev().collect())
}
pub fn marker(
    id: u64,
    center: [f32; 2],
    diameter: u32,
    color: [u8; 4],
    sequence: (u32, bool),
    canvas: [u32; 2],
) -> Result<Annotation> {
    if !(1..=16384).contains(&diameter)
        || !center.iter().all(|v| v.is_finite())
        || canvas.contains(&0)
    {
        return Err(Error::Invalid(
            "Step size must be 1–16384 pixels on a valid canvas".into(),
        ));
    }
    let size = (diameter as f32)
        .min(canvas[0] as f32)
        .min(canvas[1] as f32);
    let start = [
        (center[0] - size / 2.0).clamp(0.0, canvas[0] as f32 - size),
        (center[1] - size / 2.0).clamp(0.0, canvas[1] as f32 - size),
    ];
    Ok(Annotation {
        image: None,
        step_outline: None,
        line_style: None,
        shadow: None,

        focus: None,

        highlighter_rect: false,
        group: None,
        rotation_degrees: 0.0,
        mirror_x: false,
        id,
        kind: Kind::Step,
        start,
        end: [start[0] + size, start[1] + size],
        color,
        width: 1,
        corner_radius: 0.0,
        fill: None,
        points: vec![],
        callout: None,
        text: Some(TextStyle {
            content: label(sequence.0, sequence.1)?,
            horizontal: HorizontalAlignment::Center,
            vertical: VerticalAlignment::Middle,
            size_points: (size * 0.4).clamp(6.0, 64.0),
            ..TextStyle::default()
        }),
    })
}
pub fn next_number(document: &Document, letters: bool) -> u32 {
    highest_number(document, letters)
        .unwrap_or(0)
        .saturating_add(1)
}

/// Resize a marker about its center, retaining its label and formatting.
/// At an image edge, keep the complete marker inside the current canvas.
pub fn resized_marker(a: &Annotation, diameter: f32, canvas: [u32; 2]) -> Result<Annotation> {
    if a.kind != Kind::Step
        || !diameter.is_finite()
        || !(1.0..=16384.0).contains(&diameter)
        || canvas.contains(&0)
    {
        return Err(Error::Invalid(
            "Step size must be 1–16384 pixels on a valid canvas".into(),
        ));
    }
    let (min, max) = a.body_bounds();
    let size = diameter.min(canvas[0] as f32).min(canvas[1] as f32);
    let mut out = a.clone();
    for axis in 0..2 {
        out.start[axis] =
            ((min[axis] + max[axis] - size) / 2.0).clamp(0.0, canvas[axis] as f32 - size);
        out.end[axis] = out.start[axis] + size;
    }
    Ok(out)
}

/// Read the labels themselves, including manual edits and gaps in the sequence.
pub fn highest_number(document: &Document, letters: bool) -> Option<u32> {
    document
        .annotations
        .iter()
        .filter(|a| a.kind == Kind::Step)
        .filter_map(|a| a.text.as_ref())
        .filter_map(|t| {
            if letters {
                if t.content.is_empty() || !t.content.bytes().all(|b| b.is_ascii_uppercase()) {
                    return None;
                }
                t.content.bytes().try_fold(0u32, |n, b| {
                    n.checked_mul(26)?.checked_add(u32::from(b - b'A' + 1))
                })
            } else {
                t.content.parse().ok()
            }
        })
        .max()
}
pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let (min, max) = annotation.body_bounds();
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let radius = [(max[0] - min[0]) / 2.0, (max[1] - min[1]) / 2.0];
    if radius[0] <= 0.0 || radius[1] <= 0.0 {
        return;
    }
    for y in min[1].floor().max(0.0) as u32..(max[1].ceil() as u32).min(image.height()) {
        for x in min[0].floor().max(0.0) as u32..(max[0].ceil() as u32).min(image.width()) {
            let d = ((x as f32 - center[0]) / radius[0]).hypot((y as f32 - center[1]) / radius[1]);
            if d <= 1.0 {
                text::composite(image.get_pixel_mut(x, y), annotation.color);
            }
        }
    }
    text::render(image, &foreground(annotation));
}

/// Shared contrast and label sizing for export, vector display and inline entry.
pub fn foreground(annotation: &Annotation) -> Annotation {
    let (min, max) = annotation.body_bounds();
    let mut foreground = annotation.clone();
    foreground.color = if u32::from(annotation.color[0]) * 299
        + u32::from(annotation.color[1]) * 587
        + u32::from(annotation.color[2]) * 114
        > 150_000
    {
        [25, 25, 25, annotation.color[3]]
    } else {
        [255, 255, 255, annotation.color[3]]
    };
    if let Some(style) = &mut foreground.text {
        style.size_points = style.size_points.min(
            ((max[0] - min[0]) * 0.55 / (style.content.chars().count().max(1) as f32) * 0.75)
                .max(6.0),
        );
    }
    foreground
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};
    #[test]
    fn sequence_boundaries_editing_undo_and_persistence() {
        assert_eq!(label(26, true).unwrap(), "Z");
        assert_eq!(label(27, true).unwrap(), "AA");
        assert_eq!(label(999, true).unwrap(), "ALK");
        assert!(label(0, false).is_err());
        assert!(label(1000, true).is_err());
        let mut s = Session::new(Document::blank(120, 100).unwrap()).unwrap();
        for n in 1..=2 {
            s.execute(Command::Add {
                annotation: marker(
                    u64::from(n),
                    [n as f32 * 40.0, 50.0],
                    36,
                    [220, 50, 50, 255],
                    (n, false),
                    [120, 100],
                )
                .unwrap(),
            })
            .unwrap();
        }
        assert_eq!(next_number(&s.document, false), 3);
        s.execute(Command::Undo).unwrap();
        assert_eq!(next_number(&s.document, false), 2);
        s.execute(Command::Redo).unwrap();
        let mut edit = s.document.annotations[0].clone();
        edit.text.as_mut().unwrap().content = "7".into();
        s.execute(Command::Update { annotation: edit }).unwrap();
        assert_eq!(next_number(&s.document, false), 8);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("steps.json");
        s.save(&path).unwrap();
        let saved = Document::load(&path).unwrap();
        assert_eq!(saved.annotations, s.document.annotations);
        assert_eq!(saved.render().unwrap(), s.document.render().unwrap());
        assert_eq!(
            saved.render().unwrap().get_pixel(0, 0).0,
            [255, 255, 255, 255]
        );
        assert!(
            saved
                .render()
                .unwrap()
                .pixels()
                .any(|p| p.0 == [220, 50, 50, 255])
        );
        let edge = marker(4, [0.0, 0.0], 44, [0, 0, 0, 255], (1, true), [120, 100]).unwrap();
        assert_eq!(edge.start, [0.0, 0.0]);
        assert_eq!(edge.text.unwrap().content, "A");
    }
}

#[cfg(test)]
mod outline_tests;
