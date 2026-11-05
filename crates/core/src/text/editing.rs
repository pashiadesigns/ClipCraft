use super::{
    TextStyle,
    rich::{self, Format, List},
};
use crate::{Error, Result};
use std::ops::Range;

pub fn replace(s: &mut TextStyle, range: Range<usize>, replacement: &str) -> Result<()> {
    let before = s.content.clone();
    let len = before.chars().count();
    if range.start > range.end
        || range.end > len
        || len - range.len() + replacement.chars().count() > super::MAX_TEXT_CHARS
    {
        return Err(Error::Invalid(
            "Text replacement exceeds this box's 4096-character limit.".into(),
        ));
    }
    s.content = before
        .chars()
        .take(range.start)
        .chain(replacement.chars())
        .chain(before.chars().skip(range.end))
        .collect();
    rich::rebase(s, &before, None);
    Ok(())
}
pub fn find(text: &str, needle: &str, case: bool, whole: bool) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return vec![];
    }
    let mut folded = String::new();
    let mut map = Vec::new();
    for (i, c) in text.chars().enumerate() {
        let f = if case {
            c.to_string()
        } else {
            c.to_lowercase().collect()
        };
        map.extend(std::iter::repeat_n(i, f.len()));
        folded.push_str(&f);
    }
    let needle = if case {
        needle.to_string()
    } else {
        needle.to_lowercase()
    };
    let chars: Vec<_> = text.chars().collect();
    folded
        .match_indices(&needle)
        .filter_map(|(i, _)| {
            let a = map[i];
            let b = map[i + needle.len() - 1] + 1;
            (!whole
                || (a == 0 || !chars[a - 1].is_alphanumeric())
                    && (b == chars.len() || !chars[b].is_alphanumeric()))
            .then_some(a..b)
        })
        .collect()
}
pub fn replace_all(
    s: &mut TextStyle,
    needle: &str,
    replacement: &str,
    case: bool,
    whole: bool,
) -> Result<usize> {
    let matches = find(&s.content, needle, case, whole);
    let mut next = s.clone();
    for r in matches.iter().rev() {
        replace(&mut next, r.clone(), replacement)?;
    }
    next.validate()?;
    *s = next;
    Ok(matches.len())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Body,
    Title,
    Heading1,
    Heading2,
    Caption,
    Code,
}
impl Preset {
    pub const ALL: [Self; 6] = [
        Self::Body,
        Self::Title,
        Self::Heading1,
        Self::Heading2,
        Self::Caption,
        Self::Code,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Body => "Body",
            Self::Title => "Title",
            Self::Heading1 => "Heading",
            Self::Heading2 => "Subheading",
            Self::Caption => "Caption",
            Self::Code => "Code",
        }
    }
    /// The same definition drives both the styled choice and applied document text.
    pub fn format(self, font: super::TextFont) -> Format {
        let (size, bold) = match self {
            Self::Body | Self::Code => (13., false),
            Self::Title => (28., true),
            Self::Heading1 => (20., true),
            Self::Heading2 => (16., true),
            Self::Caption => (12., false),
        };
        Format::from_style(&TextStyle {
            size_points: size,
            bold,
            italic: self == Self::Caption,
            font: if self == Self::Code {
                super::TextFont::HackRegular
            } else {
                font
            },
            ..Default::default()
        })
    }
    pub fn apply(self, s: &mut TextStyle, range: Range<usize>) {
        let after = match self {
            Self::Body => 8.,
            Self::Title => 18.,
            Self::Heading1 => 12.,
            Self::Heading2 => 10.,
            Self::Caption => 4.,
            Self::Code => 8.,
        };
        let starts = rich::paragraph_starts(&s.content);
        let a = starts
            .iter()
            .copied()
            .take_while(|p| *p <= range.start)
            .last()
            .unwrap_or(0);
        let b = starts
            .iter()
            .copied()
            .find(|p| *p >= range.end.max(range.start + 1))
            .unwrap_or(s.content.chars().count());
        rich::format_range(s, a..b, |f| {
            let preset = self.format(f.font);
            f.size = preset.size;
            f.bold = preset.bold;
            f.italic = preset.italic;
            f.font = preset.font;
        });
        rich::paragraph_range(s, range, |p| {
            p.after = after;
            p.line_spacing = 1.15;
            p.list = List::None;
        });
    }
}
pub fn slice(s: &TextStyle, range: Range<usize>) -> TextStyle {
    let mut result = s.clone();
    result.content = s
        .content
        .chars()
        .skip(range.start)
        .take(range.len())
        .collect();
    result.rich = None;
    let mut runs = Vec::new();
    for i in 0..result.content.chars().count() {
        let format = rich::format_at(s, range.start + i);
        if let Some(last) = runs
            .last_mut()
            .filter(|r: &&mut rich::Run| r.format == format)
        {
            last.end = i + 1;
        } else {
            runs.push(rich::Run {
                start: i,
                end: i + 1,
                format,
            });
        }
    }
    let paragraphs = s
        .rich
        .as_ref()
        .map(|r| {
            r.paragraphs
                .iter()
                .filter(|p| p.start >= range.start && p.start < range.end)
                .cloned()
                .map(|mut p| {
                    p.start -= range.start;
                    p
                })
                .collect()
        })
        .unwrap_or_default();
    result.rich = Some(rich::RichText { runs, paragraphs });
    result
}
pub fn insert(s: &mut TextStyle, range: Range<usize>, fragment: &TextStyle) -> Result<()> {
    let start = range.start;
    replace(s, range, &fragment.content)?;
    for run in fragment.rich.as_ref().into_iter().flat_map(|r| &r.runs) {
        rich::format_range(s, start + run.start..start + run.end, |f| {
            *f = run.format.clone()
        });
    }
    if fragment.rich.is_none() {
        let format = Format::from_style(fragment);
        rich::format_range(s, start..start + fragment.content.chars().count(), |f| {
            *f = format.clone()
        });
    }
    let starts = rich::paragraph_starts(&s.content);
    for p in fragment
        .rich
        .as_ref()
        .into_iter()
        .flat_map(|r| &r.paragraphs)
    {
        if starts.contains(&(start + p.start)) {
            let at = start + p.start;
            rich::paragraph_range(s, at..at, |target| {
                *target = p.clone();
                target.start = at;
            });
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_find_replace_is_atomic_and_retains_format() {
        let mut s = TextStyle {
            content: "café CAFE caféine café".into(),
            ..Default::default()
        };
        rich::format_range(&mut s, 0..4, |f| f.bold = true);
        assert_eq!(replace_all(&mut s, "café", "tea", false, true).unwrap(), 2);
        assert_eq!(s.content, "tea CAFE caféine tea");
        assert!(rich::format_at(&s, 0).bold);
        let before = s.clone();
        assert!(replace_all(&mut s, "tea", &"x".repeat(4096), true, false).is_err());
        assert_eq!(s, before);
    }
}
