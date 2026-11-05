//! Bounded character runs and paragraph layout, shared by editing and export.
use super::{
    HorizontalAlignment, TextFit, TextFont, TextStyle, VerticalAlignment, outline::Outline,
    typography::Typography,
};
use crate::{Annotation, Error, Kind, Result};
use ab_glyph::{Font, ScaleFont};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Format {
    pub font: TextFont,
    pub size: f32,
    pub color: Option<[u8; 4]>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub typography: Typography,
    pub outline: Option<Outline>,
    pub link: Option<String>,
}
impl Format {
    pub fn changed_fields(&mut self, old: &Self, new: &Self) {
        if old.font != new.font {
            self.font = new.font;
        }
        if old.size != new.size {
            self.size = new.size;
        }
        if old.color != new.color {
            self.color = new.color;
        }
        if old.bold != new.bold {
            self.bold = new.bold;
        }
        if old.italic != new.italic {
            self.italic = new.italic;
        }
        if old.underline != new.underline {
            self.underline = new.underline;
        }
        if old.strike != new.strike {
            self.strike = new.strike;
        }
        if old.outline != new.outline {
            self.outline = new.outline;
        }
        self.typography = self
            .typography
            .changed_fields(old.typography, new.typography);
    }
    pub fn from_style(s: &TextStyle) -> Self {
        Self {
            font: s.font,
            size: s.size_points,
            color: s.color,
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            strike: s.strikethrough,
            typography: s.typography(),
            outline: s.outline,
            link: None,
        }
    }
    pub fn apply(&self, s: &mut TextStyle) {
        s.font = self.font;
        s.size_points = self.size;
        s.color = self.color;
        s.bold = self.bold;
        s.italic = self.italic;
        s.underline = self.underline;
        s.strikethrough = self.strike;
        s.set_typography(self.typography);
        s.outline = self.outline;
    }
    pub fn style(&self) -> TextStyle {
        let mut s = TextStyle::default();
        self.apply(&mut s);
        s
    }
    fn validate(&self) -> Result<()> {
        if self.link.as_ref().is_some_and(|s| !safe_link(s)) {
            return Err(Error::Invalid("Links must start with https://, http:// or mailto: and contain no spaces or control characters.".into()));
        }
        let mut style = self.style();
        style.content.clear();
        style.validate()
    }
}
pub fn safe_link(s: &str) -> bool {
    s.len() <= 2048
        && ["https://", "http://", "mailto:"]
            .iter()
            .any(|p| s.starts_with(p) && s.len() > p.len())
        && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub start: usize,
    pub end: usize,
    pub format: Format,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum List {
    #[default]
    None,
    Bullet,
    Numbered,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Paragraph {
    pub start: usize,
    pub alignment: Alignment,
    pub line_spacing: f32,
    pub before: f32,
    pub after: f32,
    pub indent: f32,
    pub first_indent: f32,
    pub tab: f32,
    pub list: List,
    pub level: u8,
    pub number: u16,
    pub continue_numbering: bool,
}
impl Default for Paragraph {
    fn default() -> Self {
        Self {
            start: 0,
            alignment: Alignment::Left,
            line_spacing: 1.,
            before: 0.,
            after: 0.,
            indent: 0.,
            first_indent: 0.,
            tab: 32.,
            list: List::None,
            level: 0,
            number: 1,
            continue_numbering: true,
        }
    }
}
impl Paragraph {
    pub fn for_style(s: &TextStyle, start: usize) -> Self {
        Self {
            start,
            alignment: match s.horizontal {
                HorizontalAlignment::Left => Alignment::Left,
                HorizontalAlignment::Center => Alignment::Center,
                HorizontalAlignment::Right => Alignment::Right,
            },
            ..Default::default()
        }
    }
    fn validate(&self) -> Result<()> {
        if !(0.5..=4.).contains(&self.line_spacing)
            || !self.line_spacing.is_finite()
            || [self.before, self.after, self.indent]
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=512.).contains(v))
            || !self.first_indent.is_finite()
            || !(-512. ..=512.).contains(&self.first_indent)
            || !self.tab.is_finite()
            || !(4. ..=512.).contains(&self.tab)
            || self.level > 8
            || self.number == 0
        {
            return Err(Error::Invalid("Use line spacing 0.5–4, spacing/indents up to 512 px, tabs 4–512 px, and list levels 0–8.".into()));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RichText {
    pub runs: Vec<Run>,
    pub paragraphs: Vec<Paragraph>,
}
impl RichText {
    pub fn validate(&self, s: &TextStyle) -> Result<()> {
        let n = s.content.chars().count();
        let starts = paragraph_starts(&s.content);
        let mut end = 0;
        if self.runs.len() > super::MAX_TEXT_CHARS
            || self.paragraphs.len() > super::MAX_TEXT_CHARS + 1
        {
            return Err(Error::Invalid("Too many text formatting runs.".into()));
        }
        for r in &self.runs {
            if r.start < end || r.start >= r.end || r.end > n {
                return Err(Error::Invalid("Text formatting ranges are invalid.".into()));
            }
            r.format.validate()?;
            end = r.end;
        }
        let mut previous = None;
        for p in &self.paragraphs {
            if !starts.contains(&p.start) || previous.is_some_and(|v| v >= p.start) {
                return Err(Error::Invalid(
                    "Paragraph formatting positions are invalid.".into(),
                ));
            }
            p.validate()?;
            previous = Some(p.start);
        }
        Ok(())
    }
}
pub fn paragraph_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(
            text.chars()
                .enumerate()
                .filter(|(_, c)| *c == '\n')
                .map(|(i, _)| i + 1),
        )
        .collect()
}
pub fn format_at(s: &TextStyle, index: usize) -> Format {
    s.rich
        .as_ref()
        .and_then(|r| r.runs.iter().find(|r| r.start <= index && index < r.end))
        .map_or_else(|| Format::from_style(s), |r| r.format.clone())
}
fn compact(formats: Vec<Option<Format>>) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (i, format) in formats.into_iter().enumerate() {
        if let Some(format) = format {
            if let Some(last) = runs.last_mut().filter(|r| r.end == i && r.format == format) {
                last.end = i + 1;
            } else {
                runs.push(Run {
                    start: i,
                    end: i + 1,
                    format,
                });
            }
        }
    }
    runs
}
pub fn format_range(s: &mut TextStyle, range: Range<usize>, mut change: impl FnMut(&mut Format)) {
    let n = s.content.chars().count();
    let mut formats = Vec::with_capacity(n);
    for i in 0..n {
        let prior = s
            .rich
            .as_ref()
            .and_then(|r| r.runs.iter().find(|r| r.start <= i && i < r.end))
            .map(|r| r.format.clone());
        formats.push(if range.contains(&i) {
            let mut f = prior.unwrap_or_else(|| Format::from_style(s));
            change(&mut f);
            Some(f)
        } else {
            prior
        });
    }
    s.rich.get_or_insert_with(Default::default).runs = compact(formats);
}
pub fn paragraph_range(
    s: &mut TextStyle,
    range: Range<usize>,
    mut change: impl FnMut(&mut Paragraph),
) {
    let starts = paragraph_starts(&s.content);
    let mut paragraphs = s
        .rich
        .as_ref()
        .map(|r| r.paragraphs.clone())
        .unwrap_or_default();
    for (i, start) in starts.iter().copied().enumerate() {
        let end = starts
            .get(i + 1)
            .copied()
            .unwrap_or(s.content.chars().count() + 1);
        if start < range.end.max(range.start + 1) && end > range.start {
            if let Some(p) = paragraphs.iter_mut().find(|p| p.start == start) {
                change(p);
            } else {
                let mut p = Paragraph::for_style(s, start);
                change(&mut p);
                paragraphs.push(p);
            }
        }
    }
    paragraphs.sort_by_key(|p| p.start);
    s.rich.get_or_insert_with(Default::default).paragraphs = paragraphs;
}
/// Rebase styles after a TextBuffer replacement. Source indexes count Unicode
/// scalars, matching the editor rather than UTF-8 bytes or UTF-16 units.
pub fn rebase(s: &mut TextStyle, before: &str, typing: Option<&Format>) {
    let old: Vec<_> = before.chars().collect();
    let new: Vec<_> = s.content.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    if prefix == old_end && prefix == new_end {
        return;
    }
    if s.rich.is_none() && typing.is_none() {
        return;
    }
    let mut original = s.clone();
    original.content = before.into();
    let inherited = typing
        .cloned()
        .unwrap_or_else(|| format_at(&original, prefix.saturating_sub(1)));
    let formats = (0..new.len())
        .map(|i| {
            Some(if i < prefix {
                format_at(&original, i)
            } else if i < new_end {
                inherited.clone()
            } else {
                format_at(&original, old_end + i - new_end)
            })
        })
        .collect();
    let old_starts = paragraph_starts(before);
    let mut paragraphs = Vec::new();
    for start in paragraph_starts(&s.content) {
        let source = if start <= prefix {
            start
        } else if start >= new_end {
            old_end + start - new_end
        } else {
            prefix
        };
        let origin = old_starts
            .iter()
            .copied()
            .take_while(|p| *p <= source)
            .last()
            .unwrap_or(0);
        if let Some(mut p) = original
            .rich
            .as_ref()
            .and_then(|r| r.paragraphs.iter().find(|p| p.start == origin))
            .cloned()
        {
            p.start = start;
            paragraphs.push(p);
        }
    }
    s.rich = Some(RichText {
        runs: compact(formats),
        paragraphs,
    });
}

#[derive(Clone)]
pub struct Cell {
    pub character: char,
    pub index: usize,
    pub x: f32,
    pub advance: f32,
    pub format: Format,
}
pub struct Row {
    pub start_x: f32,
    pub cells: Vec<Cell>,
    pub y: f32,
    pub height: f32,
    pub ascent: f32,
    pub newline: bool,
    pub marker: Option<(String, f32, Format)>,
}
pub struct Layout {
    pub rows: Vec<Row>,
    pub width: f32,
    pub height: f32,
}
fn metrics(f: &Format) -> (f32, f32) {
    let face = f.font.face(f.bold, f.italic);
    let px = f.size * 4. / 3.;
    let scaled = face.as_scaled(face.height_unscaled() * px / face.units_per_em().unwrap_or(1000.));
    (scaled.ascent(), scaled.height() + scaled.line_gap())
}
fn ink_metrics(f: &Format, text: &str) -> (f32, f32) {
    let (ascent, height) = metrics(f);
    let (top, bottom) = super::unicode::ink_metrics(&f.style(), text);
    let descent = (height - ascent).max(bottom);
    let ascent = ascent.max(top);
    (ascent, ascent + descent)
}
fn advance(f: &Format, c: char, previous: Option<(char, &Format)>) -> (f32, f32) {
    let face = f.font.face(f.bold, f.italic);
    let px = f.size * 4. / 3.;
    let scaled = face.as_scaled(
        face.height_unscaled() * px / face.units_per_em().unwrap_or(1000.)
            * f.typography.script.scale(),
    );
    let id = scaled.glyph_id(c);
    let kern = previous
        .filter(|(_, p)| *p == f)
        .map_or(0., |(c, _)| scaled.kern(scaled.glyph_id(c), id));
    (
        kern,
        (scaled.h_advance(id) + f.typography.spacing).max(0.01),
    )
}
pub fn layout(s: &TextStyle, width: f32) -> Layout {
    let mut rows = Vec::new();
    let mut y = 0.;
    let mut max_width: f32 = 0.;
    let mut index = 0;
    let mut numbers = [0u16; 9];
    for paragraph in s.content.split('\n') {
        let p = s
            .rich
            .as_ref()
            .and_then(|r| r.paragraphs.iter().find(|p| p.start == index))
            .cloned()
            .unwrap_or_else(|| Paragraph::for_style(s, index));
        let start = index;
        let chars: Vec<_> = paragraph.chars().collect();
        let mut clusters = std::collections::HashMap::new();
        let mut cluster_index = 0;
        for cluster in super::unicode::clusters(paragraph) {
            clusters.insert(cluster_index, cluster);
            cluster_index += cluster.chars().count();
        }
        let mut cursor = 0;
        let mut first = true;
        let marker = match p.list {
            List::None => None,
            List::Bullet => Some("•".into()),
            List::Numbered => {
                let n = &mut numbers[p.level as usize];
                *n = if p.continue_numbering && *n > 0 {
                    n.saturating_add(1)
                } else {
                    p.number
                };
                let value = *n;
                numbers[p.level as usize + 1..].fill(0);
                Some(format!("{value}."))
            }
        };
        if p.list == List::None {
            numbers.fill(0);
        }
        y += p.before;
        loop {
            let format = format_at(
                s,
                (start + cursor).min(s.content.chars().count().saturating_sub(1)),
            );
            let marker_width = if marker.is_some() {
                format.size * 4. / 3. * 1.5
            } else {
                0.
            };
            let indent = (p.indent
                + f32::from(p.level) * 24.
                + marker_width
                + if first { p.first_indent } else { 0. })
            .max(0.);
            let begin = cursor;
            let mut x = indent;
            let mut cells: Vec<Cell> = Vec::new();
            let mut last_break = None;
            while cursor < chars.len() {
                let c = chars[cursor];
                let mut f = format_at(s, start + cursor);
                let cluster = clusters.get(&cursor).copied().unwrap_or("");
                let family = super::unicode::font_for(&f.style(), cluster);
                let emoji = family == TextFont::Emoji;
                let (kern, mut step) = if emoji {
                    f.font = family;
                    (0., super::unicode::shape(&f.style(), cluster).2)
                } else {
                    advance(&f, c, cells.last().map(|c| (c.character, &c.format)))
                };
                if c == '\t' {
                    step = p.tab - ((x - indent) % p.tab);
                }
                if c == '\r' {
                    step = 0.;
                }
                if s.wrap && x + kern + step > width && !cells.is_empty() && !c.is_whitespace() {
                    if let Some((count, at)) = last_break.filter(|(count, _)| *count < cells.len())
                    {
                        cells.truncate(count);
                        cursor = at;
                    }
                    break;
                }
                cells.push(Cell {
                    character: c,
                    index: start + cursor,
                    x: x + kern,
                    advance: step,
                    format: f.clone(),
                });
                x += kern + step;
                cursor += 1;
                if emoji {
                    for continuation in cluster.chars().skip(1) {
                        cells.push(Cell {
                            character: continuation,
                            index: start + cursor,
                            x,
                            advance: 0.,
                            format: f.clone(),
                        });
                        cursor += 1;
                    }
                }
                if c.is_whitespace() {
                    last_break = Some((cells.len(), cursor));
                }
            }
            let (mut ascent, mut height) = metrics(&format);
            for c in &cells {
                let (a, h) = ink_metrics(&c.format, &c.character.to_string());
                ascent = ascent.max(a);
                height = height.max(h);
            }
            height *= p.line_spacing;
            let ink_end = cells
                .iter()
                .rfind(|c| !c.character.is_whitespace())
                .map_or(indent, |c| c.x + c.advance);
            let spare = (width - ink_end).max(0.);
            let offset = match p.alignment {
                Alignment::Center => spare * 0.5,
                Alignment::Right => spare,
                _ => 0.,
            };
            let spaces = cells.iter().filter(|c| c.character == ' ').count();
            let mut extra = 0.;
            for c in &mut cells {
                c.x += offset + extra;
                if p.alignment == Alignment::Justify
                    && cursor < chars.len()
                    && spaces > 0
                    && c.character == ' '
                {
                    let add = spare / spaces as f32;
                    c.advance += add;
                    extra += add;
                }
            }
            max_width = max_width.max(ink_end + offset + extra);
            let label = if first {
                marker
                    .clone()
                    .map(|m| (m, (indent - marker_width).max(0.) + offset, format.clone()))
            } else {
                None
            };
            rows.push(Row {
                start_x: indent + offset,
                cells,
                y,
                height,
                ascent,
                newline: false,
                marker: label,
            });
            y += height;
            first = false;
            if cursor >= chars.len() || cursor == begin {
                break;
            }
        }
        index += chars.len() + 1;
        if index <= s.content.chars().count() {
            rows.last_mut().unwrap().newline = true;
        }
        y += p.after;
    }
    Layout {
        rows,
        width: max_width,
        height: y,
    }
}
pub fn fitted(s: &TextStyle, size: [f32; 2]) -> TextStyle {
    let mut out = s.clone();
    if s.fit != TextFit::Shrink || size.iter().any(|v| *v <= 0. || !v.is_finite()) {
        return out;
    }
    let fits = |s: &TextStyle| {
        let l = layout(s, size[0]);
        l.width <= size[0] + 0.001 && l.height <= size[1] + 0.001
    };
    if fits(s) {
        return out;
    }
    let (mut lo, mut hi) = (0.001, 1.);
    for _ in 0..16 {
        let ratio = (lo + hi) * 0.5;
        out = scaled(s, ratio);
        if fits(&out) { lo = ratio } else { hi = ratio }
    }
    scaled(s, lo)
}
fn scaled(s: &TextStyle, ratio: f32) -> TextStyle {
    let mut out = s.clone();
    out.size_points *= ratio;
    if let Some(r) = &mut out.rich {
        for run in &mut r.runs {
            run.format.size *= ratio;
            run.format.typography.spacing *= ratio;
        }
        for p in &mut r.paragraphs {
            p.before *= ratio;
            p.after *= ratio;
            p.indent *= ratio;
            p.first_indent *= ratio;
            p.tab *= ratio;
        }
    }
    out
}
pub fn scale_runs(s: &mut TextStyle, ratio: f32) {
    if let Some(r) = &mut s.rich {
        for run in &mut r.runs {
            run.format.size *= ratio;
            run.format.typography.spacing *= ratio;
            if let Some(o) = &mut run.format.outline {
                o.width *= ratio;
            }
        }
        for p in &mut r.paragraphs {
            p.before *= ratio;
            p.after *= ratio;
            p.indent *= ratio;
            p.first_indent *= ratio;
            p.tab *= ratio;
        }
    }
}
/// Fragments retain vector fonts and decorations; the parent owns its clip and
/// transform. Whitespace separates fragments so justified advances stay exact.
pub fn fragments(a: &Annotation) -> Vec<Annotation> {
    let Some(style) = &a.text else {
        return vec![];
    };
    let (min, max) = super::content_bounds(a);
    let style = fitted(style, [max[0] - min[0], max[1] - min[1]]);
    let layout = layout(&style, max[0] - min[0]);
    let dy = match style.vertical {
        VerticalAlignment::Top => 0.,
        VerticalAlignment::Middle => ((max[1] - min[1] - layout.height) * 0.5).max(0.),
        VerticalAlignment::Bottom => (max[1] - min[1] - layout.height).max(0.),
    };
    let mut parts = Vec::new();
    let mut emit = |text: String, x: f32, y: f32, height: f32, format: &Format| {
        let mut part = a.clone();
        part.kind = Kind::Text;
        part.image = None;
        part.callout = None;
        part.points.clear();
        part.shadow = None;
        part.fill = None;
        part.start = [min[0] + x, min[1] + dy + y];
        part.end = [max[0].max(part.start[0] + 1.), part.start[1] + height + 1.];
        let mut s = format.style();
        s.content = text;
        s.rich = None;
        s.padding = 0.;
        s.horizontal = HorizontalAlignment::Left;
        s.vertical = VerticalAlignment::Top;
        s.fit = TextFit::None;
        s.wrap = false;
        part.text = Some(s);
        parts.push(part);
    };
    for row in layout.rows {
        if let Some((marker, x, f)) = &row.marker {
            let (ascent, h) = metrics(f);
            emit(marker.clone(), *x, row.y + row.ascent - ascent, h, f);
        }
        let mut i = 0;
        while i < row.cells.len() {
            let c = &row.cells[i];
            if c.character.is_whitespace() {
                i += 1;
                continue;
            }
            let mut text = c.character.to_string();
            let mut j = i + 1;
            while j < row.cells.len()
                && row.cells[j].format == c.format
                && !matches!(row.cells[j].character, '\t' | '\r')
                && !(row.cells[j - 1].character == ' '
                    && (row.cells[j - 1].advance - advance(&row.cells[j - 1].format, ' ', None).1)
                        .abs()
                        > 0.01)
            {
                text.push(row.cells[j].character);
                j += 1;
            }
            let (ascent, h) = ink_metrics(&c.format, &text);
            emit(text, c.x, row.y + row.ascent - ascent, h, &c.format);
            i = j;
        }
    }
    parts
}
pub(crate) fn paint<P: crate::display::vector::Painter>(
    context: &mut P,
    a: &Annotation,
    tolerance: f64,
    finish: &mut impl FnMut(&mut P, [u8; 4]),
) {
    use vello_cpu::kurbo::{Rect, Shape};
    let (min, max) = super::content_bounds(a);
    let parent = Rect::new(min[0].into(), min[1].into(), max[0].into(), max[1].into());
    let clip = parent.to_path(tolerance);
    let mut groups: Vec<(Format, Vec<Annotation>)> = Vec::new();
    for part in fragments(a) {
        let f = Format::from_style(part.text.as_ref().unwrap());
        if let Some((_, parts)) = groups.iter_mut().find(|(key, _)| *key == f) {
            parts.push(part);
        } else {
            groups.push((f, vec![part]));
        }
    }
    for (f, parts) in groups {
        for pass in 0..3 {
            if pass == 0 && f.typography.highlight.is_none() || pass == 1 && f.outline.is_none() {
                continue;
            }
            context.push_clip_path(&clip);
            let mut ink = None;
            for part in &parts {
                let color = match pass {
                    0 => super::typography::highlight(context, part),
                    1 => f.outline.filter(|o| o.color[3] > 0).and_then(|o| {
                        super::outline::paint_coverage_clipped(
                            context,
                            part,
                            o.width.into(),
                            tolerance,
                            Some(parent),
                        )
                        .ok()
                        .filter(|v| *v)
                        .map(|_| o.color)
                    }),
                    _ => super::paint_vector_coverage_clipped(
                        context,
                        part,
                        f.underline,
                        f.strike,
                        tolerance,
                        Some(parent),
                    ),
                };
                if color.is_some() {
                    ink = color;
                }
            }
            context.pop_clip_path();
            // Even an empty pass must be drained: recorders retain clip ops.
            finish(context, ink.unwrap_or([0; 4]));
        }
        for part in &parts {
            super::color_emoji::paint_clipped(context, part, tolerance, Some(parent), finish);
        }
    }
}
pub fn links(a: &Annotation) -> Vec<([f32; 4], String)> {
    let Some(style) = &a.text else {
        return vec![];
    };
    if style.rich.is_none() {
        return vec![];
    }
    let (min, max) = super::content_bounds(a);
    let s = fitted(style, [max[0] - min[0], max[1] - min[1]]);
    let layout = layout(&s, max[0] - min[0]);
    let dy = match style.vertical {
        VerticalAlignment::Top => 0.,
        VerticalAlignment::Middle => ((max[1] - min[1] - layout.height) * 0.5).max(0.),
        VerticalAlignment::Bottom => (max[1] - min[1] - layout.height).max(0.),
    };
    let transform = a.rotation();
    let mut links: Vec<([f32; 4], String)> = Vec::new();
    for row in layout.rows {
        let mut row_links: Vec<([f32; 4], String)> = Vec::new();
        for c in row.cells {
            let Some(url) = c.format.link.filter(|v| safe_link(v)) else {
                continue;
            };
            if c.format.color.unwrap_or(a.color)[3] == 0 {
                continue;
            }
            let b = [
                (min[0] + c.x).max(min[0]),
                (min[1] + dy + row.y).max(min[1]),
                (min[0] + c.x + c.advance).min(max[0]),
                (min[1] + dy + row.y + row.height).min(max[1]),
            ];
            if b[2] <= b[0] || b[3] <= b[1] {
                continue;
            }
            if let Some((last, _)) = row_links
                .last_mut()
                .filter(|(b0, u)| u == &url && (b0[2] - b[0]).abs() < 2.)
            {
                last[2] = b[2];
            } else {
                row_links.push((b, url));
            }
        }
        for (b, url) in row_links {
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for p in [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]] {
                let p = transform.point(p);
                for i in 0..2 {
                    bounds[i] = bounds[i].min(p[i]);
                    bounds[i + 2] = bounds[i + 2].max(p[i]);
                }
            }
            links.push((bounds, url));
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_replacement_preserves_runs_and_new_paragraphs() {
        let mut s = TextStyle {
            content: "a café\nnext".into(),
            ..Default::default()
        };
        format_range(&mut s, 2..6, |f| f.bold = true);
        paragraph_range(&mut s, 0..6, |p| p.list = List::Bullet);
        let old = s.content.clone();
        s.content = "a caXYfé\nnext".into();
        rebase(&mut s, &old, None);
        assert!(format_at(&s, 4).bold);
        assert!(!format_at(&s, 9).bold);
        s.validate().unwrap();
    }
    #[test]
    fn mixed_font_layout_fits_and_preserves_requested_sizes() {
        let mut s = TextStyle {
            content: "Heading\nLong mixed text wraps correctly.".into(),
            horizontal: HorizontalAlignment::Left,
            ..Default::default()
        };
        format_range(&mut s, 0..7, |f| {
            f.size = 40.;
            f.bold = true;
        });
        paragraph_range(&mut s, 8..9, |p| {
            p.list = List::Numbered;
            p.level = 1;
        });
        let fitted = fitted(&s, [160., 90.]);
        let l = layout(&fitted, 160.);
        assert!(l.height <= 90.01);
        assert!(l.width <= 160.01);
        assert_eq!(format_at(&s, 0).size, 40.);
        assert!(format_at(&fitted, 0).size < 40.);
        assert!(!safe_link("javascript:alert(1)"));
        assert!(safe_link("https://example.com/a"));
    }
}
