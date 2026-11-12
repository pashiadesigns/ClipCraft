//! Shared, atomic selection edits; no GUI state or screen coordinates.
use crate::{Annotation, Document, Error, Result};
use std::collections::BTreeSet;

impl Document {
    /// Clone selected objects in stacking order with fresh IDs. No document
    /// mutation occurs until the caller commits AddMany; preview/cancel is free.
    pub fn duplicate_selection(&self, ids: &[u64]) -> Result<Vec<Annotation>> {
        let selected: BTreeSet<_> = ids.iter().copied().collect();
        if selected.is_empty()
            || selected.len() != ids.len()
            || selected
                .iter()
                .any(|id| !self.annotations.iter().any(|a| a.id == *id))
        {
            return Err(Error::Invalid("Invalid duplication selection".into()));
        }
        let mut next = self.annotations.iter().map(|a| a.id).max().unwrap_or(0);
        let mut copies = self
            .annotations
            .iter()
            .filter(|a| selected.contains(&a.id))
            .map(|a| {
                next = next.checked_add(1).ok_or_else(|| {
                    Error::Invalid("No more object identifiers are available".into())
                })?;
                let mut copy = a.clone();
                copy.id = next;
                Ok(copy)
            })
            .collect::<Result<Vec<_>>>()?;
        crate::groups::freshen(&mut copies, self)?;
        Ok(copies)
    }
    pub(crate) fn update_many(&mut self, annotations: Vec<Annotation>) -> Result<bool> {
        let mut ids = BTreeSet::new();
        let mut candidate = self.clone();
        for annotation in annotations {
            if !ids.insert(annotation.id) {
                return Err(Error::Invalid("Duplicate object in batch update".into()));
            }
            let target = candidate
                .annotations
                .iter_mut()
                .find(|a| a.id == annotation.id)
                .ok_or_else(|| Error::Invalid("Annotation does not exist".into()))?;
            *target = annotation;
        }
        if candidate
            .annotations
            .iter()
            .any(|a| a.rotation_degrees != 0.0)
        {
            candidate.version = crate::MAX_PROJECT_VERSION;
        }
        candidate.promote_features();
        candidate.validate()?;
        if self.annotations == candidate.annotations {
            return Ok(false);
        }
        self.version = candidate.version;
        self.annotations = candidate.annotations;
        Ok(true)
    }
}

/// Clamp a shared displacement once, preserving spacing even at image edges.
pub fn translated(
    objects: &[Annotation],
    delta: [f32; 2],
    canvas: [u32; 2],
) -> Result<Vec<Annotation>> {
    if !delta.iter().all(|v| v.is_finite()) || canvas.contains(&0) {
        return Err(Error::Invalid(
            "Invalid group displacement or canvas".into(),
        ));
    }
    let mut low = [f32::NEG_INFINITY; 2];
    let mut high = [f32::INFINITY; 2];
    for object in objects {
        let (min, max) = object.bounds();
        for axis in 0..2 {
            if !min[axis].is_finite()
                || !max[axis].is_finite()
                || min[axis] < 0.0
                || max[axis] > canvas[axis] as f32
            {
                return Err(Error::Invalid("Drop the object inside the canvas, or turn on Extend canvas in Preferences › Editor.".into()));
            }
            low[axis] = low[axis].max(-min[axis]);
            high[axis] = high[axis].min(canvas[axis] as f32 - max[axis]);
        }
    }
    let delta = [
        delta[0].clamp(low[0], high[0]),
        delta[1].clamp(low[1], high[1]),
    ];
    objects
        .iter()
        .map(|a| a.translated(delta, canvas))
        .collect()
}

/// A plain drawn shape: not a frame holding an image, not Focus or Magnify.
fn plain_shape(a: &Annotation) -> bool {
    a.is_shape() && !a.is_image_frame() && a.focus.is_none()
}
pub fn can_fill(a: &Annotation) -> bool {
    a.focus.is_none()
        && (a.kind == crate::Kind::Step || a.kind == crate::Kind::Callout || plain_shape(a))
}
pub fn fill(a: &Annotation) -> [u8; 4] {
    match (&a.callout, a.kind) {
        (_, crate::Kind::Step) => a.color,
        (Some(c), _) => c.fill,
        _ => a.fill.unwrap_or([0; 4]),
    }
}
pub fn set_fill(a: &mut Annotation, color: [u8; 4]) {
    if !can_fill(a) {
        return;
    }
    if a.kind == crate::Kind::Step {
        a.color = color;
    } else if let Some(c) = a.callout.as_mut() {
        c.fill = color;
    } else {
        a.fill = Some(color);
    }
}
pub fn can_outline(a: &Annotation) -> bool {
    a.focus.is_none()
        && (a.is_shape()
            || matches!(
                a.kind,
                crate::Kind::Step
                    | crate::Kind::Line
                    | crate::Kind::Arrow
                    | crate::Kind::Pen
                    | crate::Kind::Highlighter
                    | crate::Kind::Callout
            ))
}
pub fn outline(a: &Annotation) -> [u8; 4] {
    match (&a.callout, a.kind) {
        (_, crate::Kind::Step) => a.step_outline.clone().unwrap_or_default().color,
        (Some(c), _) => c.outline,
        _ => a.color,
    }
}
pub fn set_outline(a: &mut Annotation, color: [u8; 4]) {
    if !can_outline(a) {
        return;
    }
    if a.kind == crate::Kind::Step {
        a.step_outline.get_or_insert_with(Default::default).color = color;
    } else if let Some(c) = a.callout.as_mut() {
        c.outline = color;
    } else {
        // A shape's label inherits this color until it has its own.
        if a.is_shape()
            && let Some(text) = a.text.as_mut()
        {
            text.color.get_or_insert(a.color);
        }
        a.color = color;
    }
}
pub fn thickness(a: &Annotation) -> f32 {
    if a.kind == crate::Kind::Step {
        a.step_outline.as_ref().map_or(0., |o| o.width)
    } else {
        a.width as f32
    }
}
pub fn set_thickness(a: &mut Annotation, width: f32) {
    if !can_outline(a) {
        return;
    }
    if a.kind == crate::Kind::Step {
        if width > 0. {
            a.step_outline.get_or_insert_with(Default::default).width = width;
        } else {
            a.step_outline = None;
        }
    } else {
        a.width = (width.round() as u32).clamp(1, 32);
    }
}
/// Privacy effects stay fully opaque, and Focus has its own strength controls.
pub fn can_fade(a: &Annotation) -> bool {
    a.focus.is_none()
        && !matches!(
            a.kind,
            crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate | crate::Kind::Eraser
        )
}
pub fn opacity(a: &Annotation) -> u8 {
    a.color[3]
}
pub fn set_opacity(a: &mut Annotation, value: u8) {
    if !can_fade(a) {
        return;
    }
    a.color[3] = value;
    if let Some(fill) = a.fill.as_mut() {
        fill[3] = value;
    }
    if let Some(o) = a.step_outline.as_mut() {
        o.color[3] = value;
    }
    if let Some(c) = a.callout.as_mut() {
        c.fill[3] = value;
        c.outline[3] = value;
    }
}
pub fn can_shadow(a: &Annotation) -> bool {
    !matches!(
        a.kind,
        crate::Kind::Redact | crate::Kind::Blur | crate::Kind::Pixelate | crate::Kind::Eraser
    )
}
/// Text objects, Steps and callouts hold text. A plain shape takes text settings
/// too, kept for when text is added.
pub fn can_hold_text(a: &Annotation) -> bool {
    a.text.is_some() || plain_shape(a)
}
/// Gives a plain shape an empty text style so its settings have somewhere to live.
pub fn with_room_for_text(mut a: Annotation) -> Annotation {
    if a.text.is_none() && plain_shape(&a) {
        a.text = Some(crate::text::TextStyle {
            content: String::new(),
            ..Default::default()
        });
    }
    a
}

/// Copy only changed formatting to another object of exactly the same type.
/// Text content, position, path geometry, callout tail and identity stay intact.
pub fn format_like_change(
    target: &Annotation,
    before: &Annotation,
    after: &Annotation,
) -> Annotation {
    let mut result = target.clone();
    if target.kind != before.kind || before.kind != after.kind {
        return result;
    }
    if before.rotation_degrees != after.rotation_degrees && target.can_rotate() {
        result.rotation_degrees = after.rotation_degrees;
    }
    if before.line_style != after.line_style {
        result.line_style = match (result.line_style, before.line_style, after.line_style) {
            (Some(out), Some(old), Some(new)) => Some(out.changed_fields(old, new)),
            _ => after.line_style,
        };
    }
    if before.shadow != after.shadow {
        result.shadow = match (result.shadow, before.shadow, after.shadow) {
            (Some(out), Some(old), Some(new)) => Some(out.changed_fields(old, new)),
            _ => after.shadow,
        };
    }
    if before.step_outline != after.step_outline {
        if let (Some(out), Some(old), Some(new)) = (
            &mut result.step_outline,
            &before.step_outline,
            &after.step_outline,
        ) {
            if old.color != new.color {
                out.color = new.color;
            }
            if old.width != new.width {
                out.width = new.width;
            }
        } else {
            result.step_outline = after.step_outline.clone();
        }
    }
    if before.color != after.color {
        if matches!(
            result.kind,
            crate::Kind::Rectangle | crate::Kind::Ellipse | crate::Kind::Polygon
        ) && let Some(text) = &mut result.text
        {
            // An outline edit must retain each selected shape's own label ink.
            text.color.get_or_insert(target.color);
        }
        result.color = after.color;
    }
    if before.width != after.width {
        result.width = after.width;
    }
    if before.fill != after.fill {
        result.fill = after.fill;
    }
    if before.corner_radius != after.corner_radius {
        result.corner_radius = after.corner_radius.min(result.max_corner_radius());
    }
    if let (Some(out), Some(old), Some(new)) = (&mut result.text, &before.text, &after.text) {
        if old.typography != new.typography {
            out.set_typography(
                out.typography()
                    .changed_fields(old.typography(), new.typography()),
            );
        }
        if old.font != new.font {
            out.font = new.font;
        }
        if old.size_points != new.size_points {
            out.size_points = new.size_points;
        }
        if old.horizontal != new.horizontal {
            out.horizontal = new.horizontal;
        }
        if old.vertical != new.vertical {
            out.vertical = new.vertical;
        }
        if old.padding != new.padding {
            out.padding = new.padding;
        }
        let preserved_inherited_color = matches!(
            before.kind,
            crate::Kind::Rectangle | crate::Kind::Ellipse | crate::Kind::Polygon
        ) && before.color != after.color
            && old.color.is_none()
            && new.color == Some(before.color);
        if old.color != new.color && !preserved_inherited_color {
            out.color = new.color;
        }
        if old.outline != new.outline {
            match (&mut out.outline, old.outline, new.outline) {
                (Some(out), Some(old), Some(new)) => {
                    if old.color != new.color {
                        out.color = new.color;
                    }
                    if old.width != new.width {
                        out.width = new.width;
                    }
                }
                _ => out.outline = new.outline,
            }
        }
        if old.fit != new.fit {
            out.fit = new.fit;
        }
        if old.wrap != new.wrap {
            out.wrap = new.wrap;
        }
        if old.bold != new.bold {
            out.bold = new.bold;
        }
        if old.italic != new.italic {
            out.italic = new.italic;
        }
        if old.underline != new.underline {
            out.underline = new.underline;
        }
        if old.strikethrough != new.strikethrough {
            out.strikethrough = new.strikethrough;
        }
        if let Some(rich) = &mut out.rich {
            let old = crate::text::rich::Format::from_style(old);
            let new = crate::text::rich::Format::from_style(new);
            for run in &mut rich.runs {
                run.format.changed_fields(&old, &new);
            }
        }
    }
    if let (Some(out), Some(old), Some(new)) =
        (&mut result.callout, &before.callout, &after.callout)
    {
        if old.fill != new.fill {
            out.fill = new.fill;
        }
        if old.outline != new.outline {
            out.outline = new.outline;
        }
        if old.padding != new.padding {
            out.padding = new.padding;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Session};

    fn fixture() -> Document {
        serde_json::from_str(include_str!("../tests/fixtures/translation.capture.json")).unwrap()
    }
    #[test]
    fn shared_settings_reach_each_kind_where_it_keeps_them() {
        let step = crate::steps::marker(1, [50., 50.], 44, [9; 4], (1, false), [200, 200]).unwrap();
        let mut shape = step.clone();
        shape.kind = crate::Kind::Rectangle;
        shape.text = None;
        let mut line = shape.clone();
        line.kind = crate::Kind::Line;
        let mut s = step;
        set_fill(&mut s, [1, 2, 3, 255]);
        set_outline(&mut s, [4, 5, 6, 255]);
        set_thickness(&mut s, 5.);
        assert_eq!(s.color, [1, 2, 3, 255]);
        let o = s.step_outline.clone().unwrap();
        assert_eq!((o.color, o.width), ([4, 5, 6, 255], 5.));
        set_thickness(&mut s, 0.);
        assert!(s.step_outline.is_none(), "0 means no outline on a marker");
        set_fill(&mut shape, [1, 2, 3, 255]);
        set_outline(&mut shape, [4, 5, 6, 255]);
        assert_eq!(shape.fill, Some([1, 2, 3, 255]));
        assert_eq!(shape.color, [4, 5, 6, 255]);
        let before = line.clone();
        set_fill(&mut line, [1, 2, 3, 255]);
        assert_eq!(line, before, "lines have no fill");
        set_outline(&mut line, [4, 5, 6, 255]);
        set_thickness(&mut line, 7.);
        assert_eq!((line.color, line.width), ([4, 5, 6, 255], 7));
        let mut redact = before.clone();
        redact.kind = crate::Kind::Redact;
        let held = redact.clone();
        set_opacity(&mut redact, 10);
        set_outline(&mut redact, [4, 5, 6, 255]);
        assert_eq!(redact, held, "a redaction never fades or recolors");
        let empty = with_room_for_text(shape);
        assert!(empty.text.is_some());
    }
    #[test]
    fn duplicate_batch_preserves_source_labels_order_and_one_undo_after_reopen() {
        let original = fixture();
        let mut session = Session::new(original.clone()).unwrap();
        let ids = original
            .annotations
            .iter()
            .rev()
            .map(|a| a.id)
            .collect::<Vec<_>>();
        let copies = original.duplicate_selection(&ids).unwrap();
        for (a, b) in original.annotations.iter().zip(&copies) {
            let mut expected = a.clone();
            expected.id = b.id;
            assert_eq!(&expected, b);
            assert!(!ids.contains(&b.id));
        }
        let moved = translated(&copies, [20.0, 10.0], [original.width, original.height]).unwrap();
        let command = Command::AddMany {
            annotations: moved.clone(),
        };
        let command: Command =
            serde_json::from_slice(&serde_json::to_vec(&command).unwrap()).unwrap();
        session.execute(command).unwrap();
        assert_eq!(
            &session.document.annotations[..ids.len()],
            original.annotations.as_slice()
        );
        assert_eq!(&session.document.annotations[ids.len()..], moved.as_slice());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("copies.capture.json");
        session.save(&path).unwrap();
        let saved = Document::load(&path).unwrap();
        assert_eq!(saved.render().unwrap(), session.document.render().unwrap());
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, original.annotations);
        assert!(!session.can_undo());
        for annotations in [
            copies.iter().chain(copies.iter()).cloned().collect(),
            original.annotations.clone(),
        ] {
            assert!(session.execute(Command::AddMany { annotations }).is_err());
            assert_eq!(session.document.annotations, original.annotations);
            assert!(session.can_redo());
        }
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.annotations, saved.annotations);
        for ids in [vec![], vec![u64::MAX], vec![ids[0], ids[0]]] {
            assert!(original.duplicate_selection(&ids).is_err());
        }
        let mut exhausted = original;
        exhausted.annotations[0].id = u64::MAX;
        assert!(exhausted.duplicate_selection(&[u64::MAX]).is_err());
    }

    #[test]
    fn bounded_group_move_preserves_spacing_content_and_one_step_undo_after_reopen() {
        let original = fixture();
        let mut session = Session::new(original.clone()).unwrap();
        let moved = translated(
            &original.annotations,
            [10000.0, -10000.0],
            [original.width, original.height],
        )
        .unwrap();
        let delta = [
            moved[0].start[0] - original.annotations[0].start[0],
            moved[0].start[1] - original.annotations[0].start[1],
        ];
        for (a, b) in original.annotations.iter().zip(&moved) {
            for (axis, amount) in delta.iter().enumerate() {
                assert!((b.start[axis] - a.start[axis] - amount).abs() < 0.001);
            }
            assert_eq!(a.text, b.text);
        }
        session
            .execute(Command::UpdateMany {
                annotations: moved.clone(),
            })
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("group.capture.json");
        session.save(&path).unwrap();
        let loaded = Document::load(&path).unwrap();
        assert_eq!(loaded.annotations, moved);
        assert_eq!(loaded.source_png, original.source_png);
        assert_eq!(loaded.render().unwrap(), session.document.render().unwrap());
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.annotations, original.annotations);
        assert!(!session.can_undo());
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.annotations, moved);
    }

    #[test]
    fn invalid_or_noop_batch_never_partially_writes_or_consumes_redo() {
        let original = fixture();
        let mut s = Session::new(original.clone()).unwrap();
        s.execute(Command::UpdateMany {
            annotations: translated(
                &original.annotations,
                [1.0, 1.0],
                [original.width, original.height],
            )
            .unwrap(),
        })
        .unwrap();
        s.execute(Command::Undo).unwrap();
        let mut changed = original.annotations[0].clone();
        changed.color = [10, 20, 30, 255];
        let mut invalid = original.annotations[1].clone();
        invalid.start[0] = f32::NAN;
        let mut missing = invalid.clone();
        missing.id = u64::MAX;
        for batch in [
            vec![changed.clone(), invalid],
            vec![changed.clone(), missing],
            vec![changed.clone(), changed],
        ] {
            assert!(
                s.execute(Command::UpdateMany { annotations: batch })
                    .is_err()
            );
            assert_eq!(s.document.annotations, original.annotations);
            assert!(s.can_redo());
            assert!(!s.can_undo());
        }
        for batch in [vec![], original.annotations] {
            s.execute(Command::UpdateMany { annotations: batch })
                .unwrap();
            assert!(s.can_redo());
            assert!(!s.can_undo());
        }
    }

    #[test]
    fn removing_a_selection_is_atomic_and_undo_restores_original_stack() {
        let original = fixture();
        let mut s = Session::new(original.clone()).unwrap();
        let ids = vec![original.annotations[0].id, original.annotations[2].id];
        s.execute(Command::RemoveMany { ids: ids.clone() }).unwrap();
        assert_eq!(
            s.document.annotations,
            original
                .annotations
                .iter()
                .filter(|a| !ids.contains(&a.id))
                .cloned()
                .collect::<Vec<_>>()
        );
        s.execute(Command::Undo).unwrap();
        for invalid in [vec![ids[0], u64::MAX], vec![ids[0], ids[0]]] {
            assert!(s.execute(Command::RemoveMany { ids: invalid }).is_err());
            assert_eq!(s.document.annotations, original.annotations);
            assert!(s.can_redo());
            assert!(!s.can_undo());
        }
    }

    #[test]
    fn batch_text_formatting_keeps_labels_geometry_and_unedited_properties() {
        let a = crate::steps::marker(1, [70.0, 80.0], 40, [1, 2, 3, 255], (1, false), [640, 400])
            .unwrap();
        let b = crate::steps::marker(
            2,
            [150.0, 120.0],
            60,
            [4, 5, 6, 255],
            (2, false),
            [640, 400],
        )
        .unwrap();
        let mut edited = a.clone();
        edited.color = [30, 90, 150, 160];
        edited.text.as_mut().unwrap().content = "Z".into();
        edited.text.as_mut().unwrap().horizontal = crate::text::HorizontalAlignment::Right;
        let result = format_like_change(&b, &a, &edited);
        assert_eq!(result.id, b.id);
        assert_eq!(result.start, b.start);
        assert_eq!(result.end, b.end);
        assert_eq!(result.color, edited.color);
        let text = result.text.unwrap();
        assert_eq!(text.content, "2");
        assert_eq!(text.size_points, b.text.as_ref().unwrap().size_points);
        assert_eq!(text.horizontal, crate::text::HorizontalAlignment::Right);
        let mut different = b.clone();
        different.kind = crate::Kind::Text;
        assert_eq!(format_like_change(&different, &a, &edited), different);
    }
}
