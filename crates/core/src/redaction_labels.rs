//! Optional labels are presentation only. The opaque redaction always remains.
use crate::{Document, Error, Kind, Result, text::TextStyle};

pub fn decorate(mut document: Document, enabled: bool) -> Result<Document> {
    if !enabled {
        return Ok(document);
    }
    let mut next = document.annotations.iter().map(|a| a.id).max().unwrap_or(0);
    let mut objects = Vec::with_capacity(document.annotations.len());
    for a in &document.annotations {
        objects.push(a.clone());
        if a.kind != Kind::Redact || !document.object_visible(a.id) {
            continue;
        }
        next = next
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("Too many object identifiers".into()))?;
        let mut label = a.clone();
        label.id = next;
        label.kind = Kind::Text;
        label.color = [170, 174, 178, 255];
        label.width = 1;
        label.fill = None;
        label.points.clear();
        label.image = None;
        label.callout = None;
        label.shadow = None;
        label.line_style = None;
        label.step_outline = None;
        label.highlighter_rect = false;
        label.corner_radius = 0.;
        label.group = None;
        label.text = Some(TextStyle {
            content: "REDACTED".into(),
            size_points: ((a.end[1] - a.start[1]).abs() * 0.35).clamp(6., 14.),
            horizontal: crate::text::HorizontalAlignment::Center,
            vertical: crate::text::VerticalAlignment::Middle,
            wrap: false,
            ..TextStyle::default()
        });
        if let Some(guide) = document.guide.as_mut()
            && let Some(section) = guide
                .sections
                .iter_mut()
                .find(|s| s.objects.contains(&a.id))
        {
            section.objects.push(next);
        }
        objects.push(label);
    }
    document.annotations = objects;
    Ok(document)
}
