//! Portable, bounded left-to-right text layout shared by GUI and CLI.
use crate::{Annotation, Error, Result};
use ab_glyph::{Font, FontRef, Glyph, GlyphId, ScaleFont, point};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

pub mod clipboard;
pub mod color_emoji;
pub mod decoration;
#[cfg(test)]
mod decoration_workflow_tests;
pub mod editing;
pub mod outline;
#[cfg(test)]
mod outline_workflow_tests;
pub mod recognition;
pub mod rich;
#[cfg(test)]
mod rich_workflow_tests;
pub mod system_fonts;
pub mod typography;
pub mod unicode;

pub const MAX_TEXT_CHARS: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFont {
    #[default]
    UbuntuLight,
    HackRegular,
    Emoji,
    System(u64),
}

impl TextFont {
    pub fn label(self) -> &'static str {
        match self {
            Self::UbuntuLight => "Ubuntu Light",
            Self::HackRegular => "Hack Regular",
            Self::Emoji => {
                if color_emoji::font_bytes().is_some() {
                    "Segoe UI Emoji"
                } else {
                    "Noto Emoji"
                }
            }
            Self::System(id) => {
                system_fonts::label(id).unwrap_or("Unavailable font (Ubuntu fallback)")
            }
        }
    }

    pub fn bytes(self, bold: bool, italic: bool) -> &'static [u8] {
        match (self, bold, italic) {
            (Self::Emoji, _, _) => {
                color_emoji::font_bytes().unwrap_or(include_bytes!("../assets/fonts/NotoEmoji.ttf"))
            }
            (Self::System(id), bold, italic) => system_fonts::bytes(id, bold, italic)
                .unwrap_or_else(|| Self::UbuntuLight.bytes(bold, italic)),
            (Self::UbuntuLight, false, false) => epaint_default_fonts::UBUNTU_LIGHT,
            (Self::HackRegular, false, false) => epaint_default_fonts::HACK_REGULAR,
            (Self::UbuntuLight, true, false) => include_bytes!("../assets/fonts/Ubuntu-Bold.ttf"),
            (Self::UbuntuLight, false, true) => {
                include_bytes!("../assets/fonts/Ubuntu-LightItalic.ttf")
            }
            (Self::UbuntuLight, true, true) => {
                include_bytes!("../assets/fonts/Ubuntu-BoldItalic.ttf")
            }
            (Self::HackRegular, true, false) => include_bytes!("../assets/fonts/Hack-Bold.ttf"),
            (Self::HackRegular, false, true) => include_bytes!("../assets/fonts/Hack-Italic.ttf"),
            (Self::HackRegular, true, true) => {
                include_bytes!("../assets/fonts/Hack-BoldItalic.ttf")
            }
        }
    }
    fn face(self, bold: bool, italic: bool) -> &'static FontRef<'static> {
        if self == Self::Emoji {
            static EMOJI: OnceLock<FontRef<'static>> = OnceLock::new();
            return EMOJI.get_or_init(|| {
                FontRef::try_from_slice(Self::Emoji.bytes(false, false))
                    .expect("bundled emoji font")
            });
        }
        if let Self::System(id) = self {
            return system_fonts::face(id, bold, italic)
                .unwrap_or_else(|| Self::UbuntuLight.face(bold, italic));
        }
        static FACES: [OnceLock<FontRef<'static>>; 8] = [const { OnceLock::new() }; 8];
        let family = match self {
            Self::UbuntuLight => 0,
            Self::HackRegular => 4,
            Self::System(_) => unreachable!(),
            Self::Emoji => unreachable!(),
        };
        FACES[family + usize::from(bold) * 2 + usize::from(italic)].get_or_init(|| {
            FontRef::try_from_slice(self.bytes(bold, italic)).expect("bundled font is valid")
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalAlignment {
    #[default]
    Left,
    Center,
    Right,
}
impl HorizontalAlignment {
    fn is_default(&self) -> bool {
        *self == Self::Left
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
    fn factor(self) -> f32 {
        match self {
            Self::Left => 0.0,
            Self::Center => 0.5,
            Self::Right => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerticalAlignment {
    #[default]
    Top,
    Middle,
    Bottom,
}
impl VerticalAlignment {
    fn is_default(&self) -> bool {
        *self == Self::Top
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Middle => "Middle",
            Self::Bottom => "Bottom",
        }
    }
    fn factor(self) -> f32 {
        match self {
            Self::Top => 0.0,
            Self::Middle => 0.5,
            Self::Bottom => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFit {
    #[default]
    None,
    Shrink,
    ResizeShape,
}
impl TextFit {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Do not autofit",
            Self::Shrink => "Shrink text on overflow",
            Self::ResizeShape => "Resize shape to fit text",
        }
    }
}
pub fn wrap_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rich: Option<rich::RichText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typography: Option<typography::Typography>,
    pub content: String,
    pub font: TextFont,
    /// Typographic points at 96 DPI: 1 pt = 96/72 image pixels per em.
    pub size_points: f32,
    #[serde(default, skip_serializing_if = "HorizontalAlignment::is_default")]
    pub horizontal: HorizontalAlignment,
    #[serde(default, skip_serializing_if = "VerticalAlignment::is_default")]
    pub vertical: VerticalAlignment,
    /// Image-pixel inset for plain text boxes. Callouts use CalloutStyle.padding.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub padding: f32,
    /// None retains legacy annotation-color behavior. Shape labels can use an
    /// independent color without changing their outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 4]>,
    /// Missing fields preserve legacy fixed-size, wrapped layouts.
    #[serde(default)]
    pub fit: TextFit,
    #[serde(default = "wrap_default")]
    pub wrap: bool,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub strikethrough: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<outline::Outline>,
}

pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            typography: None,
            rich: None,
            content: "Add text here".into(),
            font: TextFont::default(),
            size_points: 18.0,
            horizontal: HorizontalAlignment::Center,
            vertical: VerticalAlignment::Middle,
            padding: 0.0,
            color: None,
            fit: TextFit::Shrink,
            wrap: true,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            outline: None,
        }
    }
}

impl TextStyle {
    pub fn decorated(&self) -> bool {
        unicode::needs_fallback(self)
            || self.rich.is_some()
            || self.underline
            || self.strikethrough
            || self.outline.is_some()
            || self.typography.is_some()
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(rich) = &self.rich {
            rich.validate(self)?;
        }
        if let Some(t) = self.typography {
            t.validate()?;
        }
        if let Some(outline) = self.outline {
            outline.validate()?;
        }
        if !self.padding.is_finite() || !(0.0..=128.0).contains(&self.padding) {
            return Err(Error::Invalid("Text padding must be 0–128 pixels".into()));
        }
        if !self.size_points.is_finite() || !(6.0..=144.0).contains(&self.size_points) {
            return Err(Error::Invalid("Text size must be 6–144 points".into()));
        }
        if self.content.len() > MAX_TEXT_CHARS * 4 || self.content.chars().count() > MAX_TEXT_CHARS
        {
            return Err(Error::Invalid(
                "Text is limited to 4096 characters per box".into(),
            ));
        }
        for ch in self.content.chars() {
            if matches!(ch, '\n' | '\r' | '\t') {
                continue;
            }
            if ch.is_control()
                || (ch as u32 & 0xffff) >= 0xfffe
                || (0xfdd0..=0xfdef).contains(&(ch as u32))
            {
                return Err(Error::Invalid(format!(
                    "Control character U+{:04X} cannot be used in text",
                    ch as u32
                )));
            }
        }
        Ok(())
    }
}

struct Positioned {
    font: TextFont,
    glyph: Glyph,
    character: char,
    row: usize,
    advance_end: f32,
    index: usize,
}

struct Layout {
    glyphs: Vec<Positioned>,
    line_height: f32,
    ascent: f32,
    rows: usize,
    width: f32,
}

/// Character geometry for editing the same layout used by image export. No UI
/// types or font-atlas state are needed in the document engine.
pub struct EditingCharacter {
    pub character: char,
    pub x: f32,
    pub advance: f32,
}

#[derive(Default)]
pub struct EditingRow {
    pub start_x: Option<f32>,
    pub characters: Vec<EditingCharacter>,
    pub ends_with_newline: bool,
    pub y: Option<f32>,
    pub height: Option<f32>,
    pub ascent: Option<f32>,
}

pub struct EditingLayout {
    pub rows: Vec<EditingRow>,
    pub line_height: f32,
    pub ascent: f32,
}

fn normalized_text(text: &str) -> (String, Vec<usize>) {
    let mut normalized = String::new();
    let mut indices = Vec::new();
    let mut chars = text.chars().enumerate().peekable();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '\r' => {
                normalized.push('\n');
                indices.push(index);
                if chars.peek().is_some_and(|(_, c)| *c == '\n') {
                    chars.next();
                }
            }
            '\t' => {
                normalized.push_str("    ");
                indices.extend([index; 4]);
            }
            _ => {
                normalized.push(ch);
                indices.push(index);
            }
        }
    }
    (normalized, indices)
}

/// Shared wrapping and caret geometry, in image pixels. Preserve one entry per
/// source character (tabs and CRLF included) so selection/cursor indexes match
/// the original editable string without inserting artificial line breaks.
pub fn editing_layout(style: &TextStyle, width: f32) -> EditingLayout {
    if style.rich.is_some() {
        let layout = rich::layout(style, width);
        let rows = layout
            .rows
            .into_iter()
            .map(|r| EditingRow {
                start_x: Some(r.start_x),
                characters: r
                    .cells
                    .into_iter()
                    .map(|c| EditingCharacter {
                        character: c.character,
                        x: c.x,
                        advance: c.advance,
                    })
                    .collect(),
                ends_with_newline: r.newline,
                y: Some(r.y),
                height: Some(r.height),
                ascent: Some(r.ascent),
            })
            .collect();
        return EditingLayout {
            rows,
            line_height: 0.,
            ascent: 0.,
        };
    }
    let layout = layout(style, [0.0, 0.0], [width.max(0.0), 0.0]);
    let (_, indices) = normalized_text(&style.content);
    let mut source: Vec<Option<(usize, f32, f32)>> = vec![None; style.content.chars().count()];
    for glyph in &layout.glyphs {
        let slot = &mut source[indices[glyph.index]];
        match slot {
            Some((_, _, end)) => *end = glyph.advance_end,
            None => *slot = Some((glyph.row, glyph.glyph.position.x, glyph.advance_end)),
        }
    }
    let mut rows: Vec<_> = (0..layout.rows).map(|_| EditingRow::default()).collect();
    let mut previous = (0, 0.0, 0.0);
    for (index, character) in style.content.chars().enumerate() {
        let (row, x, end) = source[index].unwrap_or(previous);
        previous = (row, x, end);
        if character == '\n' {
            rows[row].ends_with_newline = true;
        } else {
            rows[row].characters.push(EditingCharacter {
                character,
                x,
                advance: if character == '\r' { 0.0 } else { end - x },
            });
        }
    }
    EditingLayout {
        rows,
        line_height: layout.line_height,
        ascent: layout.ascent,
    }
}

fn layout(style: &TextStyle, min: [f32; 2], max: [f32; 2]) -> Layout {
    if unicode::needs_fallback(style) {
        return unicode::layout(style, min, max);
    }
    let font = style.font.face(style.bold, style.italic);
    let pixels_per_em = style.size_points * (96.0 / 72.0);
    let scale = font.height_unscaled() * pixels_per_em / font.units_per_em().unwrap_or(1000.0);
    let base = font.as_scaled(scale);
    let typography = style.typography();
    let scale = scale * typography.script.scale();
    let scaled = font.as_scaled(scale);
    let ascent = base.ascent() + typography.script.shift(pixels_per_em);
    let line_height = if typography.script == typography::Script::Normal {
        base.height() + base.line_gap()
    } else {
        (base.height() + base.line_gap()).max(ascent - scaled.descent() + base.line_gap())
    };
    let advance = |id| {
        if typography.spacing == 0. {
            scaled.h_advance(id)
        } else {
            (scaled.h_advance(id) + typography.spacing).max(0.01)
        }
    };
    let mut x = min[0];
    let mut baseline = min[1] + ascent;
    let mut previous: Option<GlyphId> = None;
    let mut positioned = Vec::new();
    let mut row = 0usize;
    let (normalized, _) = normalized_text(&style.content);
    let mut index = 0;
    for (line_index, line) in normalized.split('\n').enumerate() {
        if line_index != 0 {
            positioned.push(Positioned {
                font: style.font,
                glyph: GlyphId(0).with_scale_and_position(scale, point(x, baseline)),
                character: '\n',
                row,
                advance_end: x,
                index,
            });
            index += 1;
            x = min[0];
            baseline += line_height;
            row += 1;
            previous = None;
        }
        // Wrap at whitespace when possible, then split words wider than the box.
        for word in line.split_inclusive(char::is_whitespace) {
            let mut width = 0.0;
            let mut prev = previous;
            for ch in word.trim_end().chars() {
                let id = scaled.glyph_id(ch);
                width += prev.map_or(0.0, |p| scaled.kern(p, id)) + advance(id);
                prev = Some(id);
            }
            if style.wrap && x > min[0] && x + width > max[0] {
                x = min[0];
                baseline += line_height;
                row += 1;
                previous = None;
            }
            for ch in word.chars() {
                let id = scaled.glyph_id(ch);
                let mut kern = previous.map_or(0.0, |p| scaled.kern(p, id));
                let advance = advance(id);
                if style.wrap && x > min[0] && x + kern + advance > max[0] {
                    if ch.is_whitespace() {
                        positioned.push(Positioned {
                            font: style.font,
                            glyph: id.with_scale_and_position(scale, point(x, baseline)),
                            character: ch,
                            row,
                            advance_end: x,
                            index,
                        });
                        index += 1;
                        continue;
                    }
                    x = min[0];
                    baseline += line_height;
                    row += 1;
                    kern = 0.0;
                }
                positioned.push(Positioned {
                    font: style.font,
                    glyph: id.with_scale_and_position(scale, point(x + kern, baseline)),
                    character: ch,
                    row,
                    advance_end: x + kern + advance,
                    index,
                });
                index += 1;
                x += kern + advance;
                previous = Some(id);
            }
        }
    }
    // Align each wrapped line independently, excluding trailing whitespace.
    // Overflow stays anchored at the top/left and is clipped by the renderer.
    let mut widths = vec![0.0_f32; row + 1];
    for glyph in &positioned {
        if !glyph.character.is_whitespace() {
            widths[glyph.row] = widths[glyph.row].max(glyph.advance_end - min[0]);
        }
    }
    let vertical =
        ((max[1] - min[1]) - (row + 1) as f32 * line_height).max(0.0) * style.vertical.factor();
    for glyph in &mut positioned {
        let offset = ((max[0] - min[0]) - widths[glyph.row]).max(0.0) * style.horizontal.factor();
        glyph.glyph.position.x += offset;
        glyph.advance_end += offset;
        glyph.glyph.position.y += vertical;
    }
    Layout {
        width: widths.into_iter().fold(0.0, f32::max),
        glyphs: positioned,
        line_height,
        ascent,
        rows: row + 1,
    }
}

/// Resolve overflow without changing the user's requested font size. Both the
/// inline caret layout and exported pixels consume this result.
pub fn fitted_style(style: &TextStyle, size: [f32; 2]) -> TextStyle {
    if style.rich.is_some() {
        return rich::fitted(style, size);
    }
    let mut resolved = style.clone();
    if style.fit != TextFit::Shrink || size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return resolved;
    }
    let fits = |candidate: &TextStyle| {
        let measured = layout(candidate, [0.0; 2], size);
        measured.width <= size[0] && measured.rows as f32 * measured.line_height <= size[1]
    };
    if fits(style) {
        return resolved;
    }
    let (mut low, mut high) = (0.0, style.size_points);
    for _ in 0..18 {
        resolved.size_points = (low + high) * 0.5;
        if fits(&resolved) {
            low = resolved.size_points;
        } else {
            high = resolved.size_points;
        }
    }
    resolved.size_points = low.max(0.001);
    resolved
}

impl Annotation {
    /// Size controls retain the object's overflow policy. Shrink never changes
    /// frame geometry; only ResizeShape may enlarge it.
    pub fn change_text_size(&self, size: f32, relative: bool, canvas: [u32; 2]) -> Result<Self> {
        let Some(style) = self.text.as_ref() else {
            return Ok(self.clone());
        };
        if !size.is_finite() {
            return Err(Error::Invalid("Font size must be finite".into()));
        }
        let mut next = self.clone();
        next.text.as_mut().unwrap().size_points = (if relative {
            style.size_points + size
        } else {
            size
        })
        .clamp(6.0, 144.0);
        if let Some(rich) = &mut next.text.as_mut().unwrap().rich {
            for run in &mut rich.runs {
                run.format.size = (if relative {
                    run.format.size + size
                } else {
                    size
                })
                .clamp(6., 144.);
            }
        }
        next.fit_text_shape(canvas)
    }

    /// Canonicalize shape-to-text sizing before a command commits. Wrapped text
    /// retains width and fits height; unwrapped text fits both dimensions. Near
    /// canvas edges shift only as needed, keeping a callout's pointer fixed.
    pub fn fit_text_shape(&self, canvas: [u32; 2]) -> Result<Self> {
        let Some(style) = &self.text else {
            return Ok(self.clone());
        };
        if style.fit != TextFit::ResizeShape {
            return Ok(self.clone());
        }
        style.validate()?;
        let (min, max) = self.body_bounds();
        if canvas.contains(&0) || min.iter().chain(&max).any(|v| !v.is_finite()) {
            return Err(Error::Invalid("Invalid text shape bounds".into()));
        }
        let (inner, _) = content_bounds(self);
        let padding = (inner[0] - min[0]).max(0.0);
        let width = (max[0] - min[0]).max(2.0 * padding + 1.0);
        let measured = layout(style, [0.0; 2], [width - 2.0 * padding, 0.0]);
        let rich_measured = style
            .rich
            .as_ref()
            .map(|_| rich::layout(style, width - 2. * padding));
        let measured_width = rich_measured.as_ref().map_or(measured.width, |r| r.width);
        let measured_height = rich_measured
            .as_ref()
            .map_or(measured.rows as f32 * measured.line_height, |r| r.height);
        let size = [
            if style.wrap {
                width
            } else {
                measured_width.ceil().max(1.0) + 2.0 * padding
            },
            measured_height.ceil().max(1.0) + 2.0 * padding,
        ];
        if size
            .iter()
            .zip(canvas)
            .any(|(s, limit)| !s.is_finite() || *s > limit as f32)
        {
            return Err(Error::Invalid(
                "Text needs a larger canvas. Choose Shrink text on overflow or enlarge the image."
                    .into(),
            ));
        }
        let origin = [0, 1].map(|i| min[i].clamp(0.0, canvas[i] as f32 - size[i]));
        let mut result = self.clone();
        result.start = origin;
        result.end = [0, 1].map(|i| origin[i] + size[i]);
        if !self.points.is_empty() {
            result.points = self
                .points
                .iter()
                .map(|p| {
                    [0, 1].map(|i| {
                        let extent = max[i] - min[i];
                        origin[i]
                            + if extent > f32::EPSILON {
                                (p[i] - min[i]) / extent * size[i]
                            } else {
                                size[i] * 0.5
                            }
                    })
                })
                .collect();
            result.start = result.points[0];
            result.end = *result.points.last().unwrap();
        }
        result.corner_radius = result.corner_radius.min(result.max_corner_radius());
        Ok(result)
    }
}

/// Document::validate runs before this method, including payload validation.
pub(crate) fn render(image: &mut RgbaImage, annotation: &Annotation) {
    let (min, max) = content_bounds(annotation);
    render_in_bounds(image, annotation, min, max);
}

/// Shared image-space content box for rendering and in-canvas text entry.
pub fn content_bounds(annotation: &Annotation) -> ([f32; 2], [f32; 2]) {
    let (min, max) = annotation.body_bounds();
    let padding = annotation.callout.as_ref().map_or_else(
        || annotation.text.as_ref().map_or(0.0, |text| text.padding),
        |callout| callout.padding + annotation.width as f32 * 0.5,
    );
    (
        [min[0] + padding, min[1] + padding],
        [max[0] - padding, max[1] - padding],
    )
}

/// Callouts supply their own content bounds, including body padding and outline.
pub(crate) fn render_in_bounds(
    image: &mut RgbaImage,
    annotation: &Annotation,
    min: [f32; 2],
    max: [f32; 2],
) {
    let Some(style) = &annotation.text else {
        return;
    };
    let text_color = style.color.unwrap_or(annotation.color);
    if max[0] <= min[0] || max[1] <= min[1] || text_color[3] == 0 {
        return;
    }
    let style = fitted_style(style, [max[0] - min[0], max[1] - min[1]]);
    // Rasterize near the text box's origin. Adding an integer canvas offset
    // to every glyph first changes f32 rounding and can alter anti-aliasing.
    let origin = [min[0].floor(), min[1].floor()];
    let local_min = [min[0] - origin[0], min[1] - origin[1]];
    let local_max = [max[0] - origin[0], max[1] - origin[1]];
    for positioned in layout(&style, local_min, local_max).glyphs {
        let font = positioned.font.face(style.bold, style.italic);
        if positioned.character.is_whitespace() {
            continue;
        }
        let Some(outlined) = font.outline_glyph(positioned.glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        if bounds.min.y >= local_max[1] || bounds.min.x >= local_max[0] {
            continue;
        }
        outlined.draw(|x, y, coverage| {
            let px = origin[0] as i32 + bounds.min.x as i32 + x as i32;
            let py = origin[1] as i32 + bounds.min.y as i32 + y as i32;
            if px < min[0].ceil() as i32
                || py < min[1].ceil() as i32
                || px >= max[0].ceil() as i32
                || py >= max[1].ceil() as i32
                || px < 0
                || py < 0
                || px as u32 >= image.width()
                || py as u32 >= image.height()
            {
                return;
            }
            let mut color = text_color;
            color[3] = (f32::from(color[3]) * coverage).round() as u8;
            composite(image.get_pixel_mut(px as u32, py as u32), color);
        });
    }
}

/// The viewport uses the same fitted layout and font outlines as export; only
/// rasterization resolution changes. Do not independently shape/wrap UI labels.
pub(crate) fn vector_path(
    annotation: &Annotation,
) -> Option<(vello_cpu::kurbo::BezPath, vello_cpu::kurbo::Rect, [u8; 4])> {
    let style = annotation.text.as_ref()?;
    if style.color.unwrap_or(annotation.color)[3] == 0 {
        return None;
    }
    vector_geometry(annotation)
}

// Outlines need geometry even when the glyph fill is transparent. Keep the
// legacy fill-only fast path above, so existing output and work remain unchanged.
fn vector_geometry(
    annotation: &Annotation,
) -> Option<(vello_cpu::kurbo::BezPath, vello_cpu::kurbo::Rect, [u8; 4])> {
    use ab_glyph::OutlineCurve;
    use vello_cpu::kurbo::{BezPath, Point, Rect};
    let style = annotation.text.as_ref()?;
    let color = style.color.unwrap_or(annotation.color);
    let (min, max) = content_bounds(annotation);
    if max[0] <= min[0] || max[1] <= min[1] || style.content.is_empty() {
        return None;
    }
    let style = fitted_style(style, [max[0] - min[0], max[1] - min[1]]);
    let origin = [min[0].floor(), min[1].floor()];
    let local_min = [min[0] - origin[0], min[1] - origin[1]];
    let local_max = [max[0] - origin[0], max[1] - origin[1]];
    let mut path = BezPath::new();
    for positioned in layout(&style, local_min, local_max).glyphs {
        let font = positioned.font.face(style.bold, style.italic);
        if positioned.character.is_whitespace() {
            continue;
        }
        let glyph = positioned.glyph;
        if color_emoji::has_color(positioned.font, glyph.id) {
            continue;
        }
        let Some(outline) = font.outline(glyph.id) else {
            continue;
        };
        let scaled = font.as_scaled(glyph.scale);
        let transform = |p: ab_glyph::Point| {
            Point::new(
                f64::from(p.x * scaled.h_scale_factor() + glyph.position.x) + f64::from(origin[0]),
                f64::from(-p.y * scaled.v_scale_factor() + glyph.position.y) + f64::from(origin[1]),
            )
        };
        let mut last = None;
        for curve in outline.curves {
            let start = match curve {
                OutlineCurve::Line(p, _)
                | OutlineCurve::Quad(p, _, _)
                | OutlineCurve::Cubic(p, _, _, _) => p,
            };
            if last != Some(start) {
                if last.is_some() {
                    path.close_path();
                }
                path.move_to(transform(start));
            }
            last = Some(match curve {
                OutlineCurve::Line(_, end) => {
                    path.line_to(transform(end));
                    end
                }
                OutlineCurve::Quad(_, control, end) => {
                    path.quad_to(transform(control), transform(end));
                    end
                }
                OutlineCurve::Cubic(_, a, b, end) => {
                    path.curve_to(transform(a), transform(b), transform(end));
                    end
                }
            });
        }
        if last.is_some() {
            path.close_path();
        }
    }
    Some((
        path,
        Rect::new(
            f64::from(min[0]),
            f64::from(min[1]),
            f64::from(max[0]),
            f64::from(max[1]),
        ),
        color,
    ))
}

/// Paint glyphs and rules into the caller's opaque coverage layer, then return
/// the ink for one final alpha application. Separate white fills union coverage
/// without winding cancellation where a rule crosses a glyph or its counter.
pub(crate) fn paint_vector_coverage(
    context: &mut impl crate::display::vector::Painter,
    annotation: &Annotation,
    underline: bool,
    strikethrough: bool,
    tolerance: f64,
) -> Option<[u8; 4]> {
    paint_vector_coverage_clipped(
        context,
        annotation,
        underline,
        strikethrough,
        tolerance,
        None,
    )
}
pub(crate) fn paint_vector_coverage_clipped(
    context: &mut impl crate::display::vector::Painter,
    annotation: &Annotation,
    underline: bool,
    strikethrough: bool,
    tolerance: f64,
    parent_clip: Option<vello_cpu::kurbo::Rect>,
) -> Option<[u8; 4]> {
    use vello_cpu::kurbo::{Rect, Shape};
    let (path, clip, ink) = vector_path(annotation)?;
    let clip = parent_clip.unwrap_or(clip);
    context.push_clip_path(&clip.to_path(tolerance));
    context.fill_path(&path);
    if underline || strikethrough {
        let (min, max) = content_bounds(annotation);
        for band in decoration::bands(
            annotation.text.as_ref()?,
            min,
            max,
            underline,
            strikethrough,
        ) {
            context.fill_rect(&Rect::new(
                f64::from(band.min[0]),
                f64::from(band.min[1]),
                f64::from(band.max[0]),
                f64::from(band.max[1]),
            ));
        }
    }
    context.pop_clip_path();
    Some(ink)
}

pub(crate) fn composite(destination: &mut Rgba<u8>, source: [u8; 4]) {
    if source[3] == 0 {
        return;
    }
    if source[3] == 255 || destination.0[3] == 0 {
        *destination = Rgba(source);
        return;
    }
    let alpha = f32::from(source[3]) / 255.0;
    let remaining = (f32::from(destination.0[3]) / 255.0) * (1.0 - alpha);
    let combined = alpha + remaining;
    for (out, input) in destination.0[..3].iter_mut().zip(&source[..3]) {
        *out = ((f32::from(*input) * alpha + f32::from(*out) * remaining) / combined).round() as u8;
    }
    destination.0[3] = (combined * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Kind, Session};

    fn annotation() -> Annotation {
        Annotation {
            image: None,
            step_outline: None,
            line_style: None,
            shadow: None,

            focus: None,

            highlighter_rect: false,
            group: None,
            rotation_degrees: 0.0,
            mirror_x: false,
            id: 1,
            kind: Kind::Text,
            start: [10.0, 10.0],
            end: [210.0, 130.0],
            color: [20, 80, 160, 200],
            width: 1,
            corner_radius: 0.0,
            fill: None,
            points: Vec::new(),
            callout: None,
            text: Some(TextStyle {
                content: "Café\nEditable text".into(),
                ..TextStyle::default()
            }),
        }
    }

    #[test]
    fn explicit_plain_text_size_preserves_shrink_bounds_and_undo() {
        for wrap in [false, true] {
            let mut original = annotation();
            original.end = [125.0, 44.0];
            let style = original.text.as_mut().unwrap();
            style.content = "Small text frame with several words".into();
            style.size_points = 40.0;
            style.wrap = wrap;
            let changed = original.change_text_size(144.0, false, [640, 400]).unwrap();
            assert_eq!(changed.start, original.start);
            assert_eq!(changed.end, original.end);
            let (min, max) = content_bounds(&changed);
            let resolved = fitted_style(
                changed.text.as_ref().unwrap(),
                [max[0] - min[0], max[1] - min[1]],
            );
            assert!(resolved.size_points < 40.0);
            assert_eq!(changed.text.as_ref().unwrap().size_points, 144.0);
            assert_eq!(changed.text.as_ref().unwrap().fit, TextFit::Shrink);
            let mut document = Document::blank(640, 400).unwrap();
            document.annotations.push(original.clone());
            let mut session = Session::new(document).unwrap();
            session
                .execute(Command::Update {
                    annotation: changed.clone(),
                })
                .unwrap();
            let reopened: Document =
                serde_json::from_slice(&serde_json::to_vec(&session.document).unwrap()).unwrap();
            assert_eq!(reopened.annotations[0], changed);
            assert_eq!(
                reopened.render().unwrap(),
                session.document.render().unwrap()
            );
            session.execute(Command::Undo).unwrap();
            assert_eq!(session.document.annotations[0], original);
        }
    }

    #[test]
    fn explicit_font_resize_rejects_impossible_growth_and_keeps_shape_fit_semantics() {
        let mut a = annotation();
        a.end = [50.0, 35.0];
        assert!(a.change_text_size(f32::NAN, false, [640, 400]).is_err());
        assert!(a.change_text_size(144.0, false, [60, 40]).is_ok());
        a.text.as_mut().unwrap().fit = TextFit::ResizeShape;
        assert!(a.change_text_size(144.0, false, [60, 40]).is_err());
        a.text.as_mut().unwrap().fit = TextFit::Shrink;
        for kind in [Kind::Rectangle, Kind::Callout] {
            a.kind = kind;
            let changed = a.change_text_size(1.0, true, [640, 400]).unwrap();
            assert_eq!(changed.start, a.start);
            assert_eq!(changed.end, a.end);
            assert_eq!(
                changed.text.as_ref().unwrap().size_points,
                a.text.as_ref().unwrap().size_points + 1.0
            );
        }
    }

    #[test]
    fn current_projects_and_text_roundtrip_without_pixel_changes() {
        let old: Document = serde_json::from_str(r#"{"version":31,"width":20,"height":20,"source_png":"","annotations":[{"id":1,"kind":"rectangle","start":[1,1],"end":[15,15],"color":[255,0,0,255],"width":2}]}"#).unwrap();
        assert!(old.annotations[0].text.is_none());
        old.render().unwrap();
        let old_style: TextStyle =
            serde_json::from_str(r#"{"content":"Legacy","font":"ubuntu_light","size_points":18}"#)
                .unwrap();
        assert_eq!(old_style.horizontal, HorizontalAlignment::Left);
        assert_eq!(old_style.vertical, VerticalAlignment::Top);
        assert_eq!(old_style.padding, 0.0);
        let old_json = serde_json::to_value(&old_style).unwrap();
        assert!(old_json.get("horizontal").is_none());
        assert!(old_json.get("vertical").is_none());
        assert!(old_json.get("padding").is_none());
        let mut session = Session::new(Document::blank(240, 160).unwrap()).unwrap();
        let original = annotation();
        session
            .execute(Command::Add {
                annotation: original.clone(),
            })
            .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("text.json");
        session.save(&file).unwrap();
        let before = session.document.render().unwrap();
        assert!(before.pixels().any(|p| p.0 != [255; 4]));
        let loaded = Document::load(&file).unwrap();
        assert_eq!(loaded.annotations, vec![original.clone()]);
        assert_eq!(loaded.render().unwrap(), before);
        let mut edited = original.clone();
        edited.text.as_mut().unwrap().content = "Changed".into();
        session
            .execute(Command::Update {
                annotation: edited.clone(),
            })
            .unwrap();
        assert!(session.is_modified());
        session.execute(Command::Undo).unwrap();
        assert!(!session.is_modified());
        assert_eq!(session.document.render().unwrap(), before);
        session.execute(Command::Redo).unwrap();
        assert_eq!(session.document.annotations, vec![edited]);
        assert!(
            session
                .save(&temp.path().join("missing/text.json"))
                .is_err()
        );
        assert!(session.is_modified());
    }

    #[test]
    fn invalid_text_rolls_back_without_losing_redo() {
        let mut session = Session::new(Document::blank(240, 160).unwrap()).unwrap();
        session
            .execute(Command::Add {
                annotation: annotation(),
            })
            .unwrap();
        session.execute(Command::Undo).unwrap();
        for invalid in [
            TextStyle {
                size_points: f32::NAN,
                ..TextStyle::default()
            },
            TextStyle {
                size_points: 145.0,
                ..TextStyle::default()
            },
            TextStyle {
                content: "x".repeat(MAX_TEXT_CHARS + 1),
                ..TextStyle::default()
            },
            TextStyle {
                content: "\u{10ffff}".into(),
                ..TextStyle::default()
            },
        ] {
            let mut bad = annotation();
            bad.text = Some(invalid);
            assert!(session.execute(Command::Add { annotation: bad }).is_err());
            assert!(session.document.annotations.is_empty());
            assert!(session.can_redo());
            assert!(!session.can_undo());
        }
        let mut missing = annotation();
        missing.text = None;
        assert!(
            session
                .execute(Command::Add {
                    annotation: missing
                })
                .is_err()
        );
    }

    #[test]
    fn layout_wraps_newlines_and_long_words_in_both_fonts() {
        for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
            let style = TextStyle {
                content: "one two\nSupercalifragilistic".into(),
                font,
                size_points: 18.0,
                ..TextStyle::default()
            };
            let glyphs = layout(&style, [0.0, 0.0], [55.0, 300.0]).glyphs;
            let mut rows = glyphs
                .iter()
                .map(|p| p.glyph.position.y as i32)
                .collect::<Vec<_>>();
            rows.dedup();
            assert!(rows.len() >= 4);
            assert!(
                glyphs
                    .iter()
                    .all(|p| p.glyph.position.x >= 0.0 && p.glyph.position.x < 55.0)
            );
            assert_eq!(
                glyphs
                    .iter()
                    .filter(|p| !p.character.is_whitespace())
                    .count(),
                26
            );
        }
    }

    #[test]
    fn centered_and_right_lines_ignore_trailing_space_and_align_each_wrapped_line() {
        for horizontal in [HorizontalAlignment::Center, HorizontalAlignment::Right] {
            let style = TextStyle {
                content: "I\nWide".into(),
                horizontal,
                ..TextStyle::default()
            };
            let glyphs = layout(&style, [0.0, 0.0], [200.0, 120.0]).glyphs;
            let first = glyphs.iter().find(|p| p.character == 'I').unwrap();
            let second = glyphs.iter().find(|p| p.character == 'W').unwrap();
            assert!(first.glyph.position.x > second.glyph.position.x);
            let spaced = TextStyle {
                content: "I   \nWide   ".into(),
                ..style
            };
            let positions = |s: &TextStyle| {
                layout(s, [0.0, 0.0], [200.0, 120.0])
                    .glyphs
                    .into_iter()
                    .filter(|g| !g.character.is_whitespace())
                    .map(|g| g.glyph.position)
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                positions(&spaced),
                positions(&TextStyle {
                    content: "I\nWide".into(),
                    ..spaced.clone()
                })
            );
        }
    }

    #[test]
    fn middle_and_bottom_move_the_text_block_and_overflow_remains_clipped_from_top() {
        let style = TextStyle {
            content: "One\nTwo".into(),
            ..TextStyle::default()
        };
        let y = |vertical, height| {
            layout(
                &TextStyle {
                    vertical,
                    ..style.clone()
                },
                [0.0, 0.0],
                [180.0, height],
            )
            .glyphs[0]
                .glyph
                .position
                .y
        };
        let top = y(VerticalAlignment::Top, 150.0);
        let middle = y(VerticalAlignment::Middle, 150.0);
        let bottom = y(VerticalAlignment::Bottom, 150.0);
        assert!(bottom > middle && middle > top);
        assert!((middle - top - (bottom - top) * 0.5).abs() < 0.001);
        assert_eq!(
            y(VerticalAlignment::Bottom, 10.0),
            y(VerticalAlignment::Top, 10.0)
        );
        assert_eq!(
            y(VerticalAlignment::Middle, 10.0),
            y(VerticalAlignment::Top, 10.0)
        );
    }

    #[test]
    fn alignment_edits_are_one_undo_and_persist_with_exact_clipped_pixels() {
        let mut session = Session::new(Document::canvas(240, 160, [0; 4]).unwrap()).unwrap();
        session
            .execute(Command::Add {
                annotation: annotation(),
            })
            .unwrap();
        let before = session.document.render().unwrap();
        let mut edited = session.document.annotations[0].clone();
        let style = edited.text.as_mut().unwrap();
        style.horizontal = HorizontalAlignment::Right;
        style.vertical = VerticalAlignment::Bottom;
        session
            .execute(Command::Update {
                annotation: edited.clone(),
            })
            .unwrap();
        let after = session.document.render().unwrap();
        assert_ne!(before, after);
        for (x, y, pixel) in after.enumerate_pixels() {
            if x < 10 || y < 10 || x >= 210 || y >= 130 {
                assert_eq!(pixel.0, [0; 4]);
            }
        }
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), before);
        session.execute(Command::Redo).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aligned.json");
        session.save(&path).unwrap();
        let reopened = Document::load(&path).unwrap();
        assert_eq!(reopened.annotations, vec![edited]);
        assert_eq!(reopened.render().unwrap(), after);
    }

    #[test]
    fn text_clips_composites_and_translates_exactly() {
        let mut a = annotation();
        a.end = [110.0, 42.0];
        a.text.as_mut().unwrap().content = "Clipped line\nHidden".into();
        let mut doc = Document::from_image(RgbaImage::from_pixel(240, 160, Rgba([0; 4]))).unwrap();
        doc.annotations.push(a.clone());
        let rendered = doc.render().unwrap();
        let mut changed = 0;
        for (x, y, p) in rendered.enumerate_pixels() {
            if p.0[3] != 0 {
                changed += 1;
                assert!((10..110).contains(&x) && (10..42).contains(&y));
                assert!(p.0[3] <= 200);
                assert_eq!(&p.0[..3], &a.color[..3]);
            }
        }
        assert!(changed > 0);
        doc.annotations[0] = a.translated([20.0, 30.0], [240, 160]).unwrap();
        let moved = doc.render().unwrap();
        for y in 0..130 {
            for x in 0..220 {
                assert_eq!(rendered.get_pixel(x, y), moved.get_pixel(x + 20, y + 30));
            }
        }
        a.color[3] = 0;
        doc.annotations[0] = a;
        assert!(doc.render().unwrap().pixels().all(|p| p.0 == [0; 4]));
    }
}
