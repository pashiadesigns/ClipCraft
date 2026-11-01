//! Bounded whole-label typography, shared by export, caret layout and styles.
//! Script sizing/shift follows the audited MIT DesignCraft effective_size pattern
//! (compose/src/shape.rs, ea08e4c2ba290f092063c99d70c4bdd5afc45887).
use super::{TextStyle, fitted_style, layout};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Script {
    #[default]
    Normal,
    Superscript,
    Subscript,
}
impl Script {
    pub fn scale(self) -> f32 {
        if self == Self::Normal { 1. } else { 0.583 }
    }
    pub fn shift(self, em: f32) -> f32 {
        match self {
            Self::Normal => 0.,
            Self::Superscript => -em * 0.333,
            Self::Subscript => em * 0.333,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Typography {
    #[serde(default)]
    pub script: Script,
    /// Extra advance after each glyph, in image pixels.
    #[serde(default)]
    pub spacing: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<[u8; 4]>,
}
impl Typography {
    pub fn validate(self) -> Result<()> {
        if !self.spacing.is_finite() || !(-4.0..=32.0).contains(&self.spacing) {
            return Err(Error::Invalid(
                "Character spacing must be -4 to 32 pixels".into(),
            ));
        }
        Ok(())
    }
    pub fn changed_fields(mut self, old: Self, new: Self) -> Self {
        if old.script != new.script {
            self.script = new.script;
        }
        if old.spacing != new.spacing {
            self.spacing = new.spacing;
        }
        if old.highlight != new.highlight {
            self.highlight = new.highlight;
        }
        self
    }
}
impl TextStyle {
    pub fn typography(&self) -> Typography {
        self.typography.unwrap_or_default()
    }
    pub fn set_typography(&mut self, t: Typography) {
        self.typography = (t != Typography::default()).then_some(t);
    }
}
#[derive(Clone, Copy)]
pub enum Case {
    Upper,
    Lower,
    Title,
}
impl Case {
    pub fn label(self) -> &'static str {
        match self {
            Self::Upper => "UPPERCASE",
            Self::Lower => "lowercase",
            Self::Title => "Title Case",
        }
    }
    pub fn apply(self, text: &str) -> String {
        match self {
            Self::Upper => text.to_uppercase(),
            Self::Lower => text.to_lowercase(),
            Self::Title => {
                let mut start = true;
                let mut result = String::new();
                for c in text.chars() {
                    if c.is_alphanumeric() {
                        if start {
                            result.extend(c.to_uppercase());
                        } else {
                            result.extend(c.to_lowercase());
                        }
                        start = false;
                    } else {
                        result.push(c);
                        start = c.is_whitespace() || matches!(c, '-' | '/' | '(' | '[');
                    }
                }
                result
            }
        }
    }
}
/// Highlight only nonempty text rows, leaving whitespace around the label clear.
pub(crate) fn highlight(
    context: &mut impl crate::display::vector::Painter,
    a: &crate::Annotation,
) -> Option<[u8; 4]> {
    let style = a.text.as_ref()?;
    let ink = style.typography().highlight?;
    if ink[3] == 0 {
        return None;
    }
    let (min, max) = super::content_bounds(a);
    let style = fitted_style(style, [max[0] - min[0], max[1] - min[1]]);
    let positioned = layout(&style, min, max);
    let font = style.font.face(style.bold, style.italic);
    use ab_glyph::{Font, ScaleFont};
    let em = style.size_points * 4. / 3. * style.typography().script.scale();
    let scale = font.height_unscaled() * em / font.units_per_em().unwrap_or(1000.);
    let scaled = font.as_scaled(scale);
    let mut spans = vec![None; positioned.rows];
    for g in positioned.glyphs {
        if g.character.is_whitespace() {
            continue;
        }
        let x = g.glyph.position.x;
        let end = g.advance_end;
        let y = g.glyph.position.y;
        spans[g.row] = Some(match spans[g.row] {
            Some((lo, hi, y)) => (f32::min(lo, x), f32::max(hi, end), y),
            None => (x, end, y),
        });
    }
    for (lo, hi, y) in spans.into_iter().flatten() {
        let lo = [lo.max(min[0]), (y - scaled.ascent()).max(min[1])];
        let hi = [hi.min(max[0]), (y - scaled.descent()).min(max[1])];
        if hi[0] > lo[0] && hi[1] > lo[1] {
            context.fill_rect(&vello_cpu::kurbo::Rect::new(
                lo[0].into(),
                lo[1].into(),
                hi[0].into(),
                hi[1].into(),
            ));
        }
    }
    Some(ink)
}
