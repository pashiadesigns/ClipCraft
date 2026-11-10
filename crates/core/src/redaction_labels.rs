//! Optional labels are presentation only. The opaque redaction always remains.
use crate::{Document, Error, Kind, Result, text::TextStyle};

pub fn decorate(mut document: Document, enabled: bool) -> Result<Document> {
    if !enabled {
        return Ok(document);
    }
    // Video frames use reserved identifiers counted down from u64::MAX for
    // footage layers, so labels are numbered after the ordinary objects and
    // skip any identifier already in use.
    let used: std::collections::BTreeSet<u64> = document.annotations.iter().map(|a| a.id).collect();
    let mut next = used
        .iter()
        .copied()
        .filter(|id| *id < u64::MAX / 2)
        .max()
        .unwrap_or(0);
    let mut objects = Vec::with_capacity(document.annotations.len());
    for a in &document.annotations {
        objects.push(a.clone());
        if a.kind != Kind::Redact || !document.object_visible(a.id) {
            continue;
        }
        loop {
            next = next
                .checked_add(1)
                .ok_or_else(|| Error::Invalid("Too many object identifiers".into()))?;
            if !used.contains(&next) {
                break;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Annotation;

    fn object(id: u64, kind: Kind, rect: [f32; 4]) -> Annotation {
        let mut a = crate::steps::marker(
            id,
            [40., 40.],
            48,
            [35, 120, 190, 255],
            (1, false),
            [400, 300],
        )
        .unwrap();
        a.kind = kind;
        a.start = [rect[0], rect[1]];
        a.end = [rect[2], rect[3]];
        a.text = None;
        a
    }

    #[test]
    fn labels_coexist_with_reserved_video_layer_identifiers() {
        let mut d = Document::canvas(400, 300, [245, 245, 245, 255]).unwrap();
        d.annotations
            .push(object(u64::MAX, Kind::Rectangle, [0., 0., 400., 300.]));
        for (i, x) in [10., 120., 230.].into_iter().enumerate() {
            d.annotations
                .push(object(i as u64 + 1, Kind::Redact, [x, 10., x + 80., 40.]));
        }
        let out = decorate(d, true).expect("labels must not overflow");
        assert_eq!(out.annotations.len(), 7);
        assert_eq!(out.annotations[0].id, u64::MAX, "footage layer is kept");
        let mut ids: Vec<_> = out.annotations.iter().map(|a| a.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 7, "identifiers stay unique");
    }
}
