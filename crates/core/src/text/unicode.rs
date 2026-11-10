//! Portable emoji fallback and grapheme boundaries. Existing ASCII layout stays
//! unchanged; emoji sequences use the same shaped outlines in canvas and export.
use super::*;
use unicode_segmentation::UnicodeSegmentation;
pub fn boundaries(text: &str) -> Vec<usize> {
    let mut out = vec![0];
    let mut index = 0;
    for g in text.graphemes(true) {
        index += g.chars().count();
        out.push(index);
    }
    out
}
pub fn font_for(style: &TextStyle, text: &str) -> TextFont {
    let face = style.font.face(style.bold, style.italic);
    let emoji = TextFont::Emoji.face(false, false);
    if text.chars().any(|c| {
        c == '\u{fe0f}' || c == '\u{200d}' || (face.glyph_id(c).0 == 0 && emoji.glyph_id(c).0 != 0)
    }) {
        TextFont::Emoji
    } else {
        style.font
    }
}
pub(super) fn needs_fallback(style: &TextStyle) -> bool {
    style.font == TextFont::Emoji
        || style
            .content
            .graphemes(true)
            .any(|s| font_for(style, s) != style.font)
}
pub fn clusters(text: &str) -> impl Iterator<Item = &str> {
    text.graphemes(true)
}
pub(super) fn shape(style: &TextStyle, text: &str) -> (TextFont, Vec<(Glyph, f32)>, f32) {
    let family = font_for(style, text);
    let font = family.face(style.bold, style.italic);
    let face = harfrust::FontRef::from_index(
        family.bytes(style.bold, style.italic),
        if let TextFont::System(id) = family {
            system_fonts::index(id, style.bold, style.italic)
        } else {
            0
        },
    )
    .expect("validated font");
    let data = harfrust::ShaperData::new(&face);
    let shaper = data.shaper(&face).build();
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let shaped = shaper.shape(buffer, harfrust::ShapeOptions::new());
    let px = style.size_points * 4. / 3. * style.typography().script.scale();
    let units = font.units_per_em().unwrap_or(1000.);
    let scale = font.height_unscaled() * px / units;
    let mut x = 0.;
    let mut glyphs = Vec::new();
    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        let glyph = GlyphId(info.glyph_id as u16).with_scale_and_position(
            scale,
            point(
                x + pos.x_offset as f32 * px / units,
                -pos.y_offset as f32 * px / units,
            ),
        );
        x += pos.x_advance as f32 * px / units;
        glyphs.push((glyph, x));
    }
    (family, glyphs, x + style.typography().spacing)
}
pub(super) fn layout(style: &TextStyle, min: [f32; 2], max: [f32; 2]) -> Layout {
    let font = style.font.face(style.bold, style.italic);
    let px = style.size_points * 4. / 3.;
    let scaled = font.as_scaled(font.height_unscaled() * px / font.units_per_em().unwrap_or(1000.));
    let (ink_ascent, ink_descent) = ink_metrics(style, &style.content);
    let ascent = scaled.ascent().max(ink_ascent) + style.typography().script.shift(px);
    let descent = (-scaled.descent()).max(ink_descent);
    let line_height = (ascent + descent + scaled.line_gap()).max(px * 1.25);
    let mut glyphs = Vec::new();
    let mut row = 0;
    let mut x = min[0];
    let mut index = 0;
    let mut widths = vec![0f32];
    let (text, _) = normalized_text(&style.content);
    for cluster in text.graphemes(true) {
        if cluster == "\n" {
            glyphs.push(Positioned {
                font: style.font,
                glyph: GlyphId(0).with_scale_and_position(
                    1.,
                    point(x, min[1] + ascent + row as f32 * line_height),
                ),
                character: '\n',
                row,
                advance_end: x,
                index,
            });
            index += 1;
            row += 1;
            x = min[0];
            widths.push(0.);
            continue;
        }
        let (family, shaped, advance) = shape(style, cluster);
        if style.wrap && x > min[0] && x + advance > max[0] {
            row += 1;
            x = min[0];
            widths.push(0.);
        }
        for (mut glyph, _) in shaped {
            glyph.position.x += x;
            glyph.position.y += min[1] + ascent + row as f32 * line_height;
            glyphs.push(Positioned {
                font: family,
                glyph,
                character: cluster.chars().next().unwrap(),
                row,
                advance_end: x + advance,
                index,
            });
        }
        // Invisible source cells preserve every Unicode character for caret/selection.
        for (n, c) in cluster.chars().enumerate().skip(1) {
            glyphs.push(Positioned {
                font: family,
                glyph: GlyphId(0).with_scale_and_position(
                    1.,
                    point(x + advance, min[1] + ascent + row as f32 * line_height),
                ),
                character: if c.is_whitespace() { c } else { ' ' },
                row,
                advance_end: x + advance,
                index: index + n,
            });
        }
        x += advance;
        index += cluster.chars().count();
        if !cluster.chars().all(char::is_whitespace) {
            widths[row] = (x - min[0]).max(widths[row]);
        }
    }
    let vertical =
        ((max[1] - min[1]) - (row + 1) as f32 * line_height).max(0.) * style.vertical.factor();
    for g in &mut glyphs {
        let offset = ((max[0] - min[0]) - widths[g.row]).max(0.) * style.horizontal.factor();
        g.glyph.position.x += offset;
        g.advance_end += offset;
        g.glyph.position.y += vertical;
    }
    Layout {
        width: widths.into_iter().fold(0., f32::max),
        glyphs,
        line_height,
        ascent,
        rows: row + 1,
    }
}
/// COLR layers may extend beyond the monochrome glyph's nominal ascender.
/// Reserve the actual colored ink bounds before fitting or clipping a line.
pub(super) fn ink_metrics(style: &TextStyle, text: &str) -> (f32, f32) {
    let mut bounds = (0f32, 0f32);
    for cluster in clusters(text) {
        if font_for(style, cluster) != TextFont::Emoji {
            continue;
        }
        let (family, glyphs, _) = shape(style, cluster);
        let face = family.face(false, false);
        for (glyph, _) in glyphs {
            if let Some(ink) = super::color_emoji::ink_bounds(glyph.id) {
                let scale = face.as_scaled(glyph.scale).v_scale_factor();
                bounds.0 = bounds.0.max(ink.y1 as f32 * scale - glyph.position.y);
                bounds.1 = bounds.1.max(-ink.y0 as f32 * scale + glyph.position.y);
            }
        }
    }
    bounds
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emoji_survives_validation_layout_render_and_save() {
        let content = "Ready ✅ 😀 👍🏽 👩‍💻 🇺🇸 ❤️";
        let style = TextStyle {
            content: content.into(),
            size_points: 28.,
            ..Default::default()
        };
        style.validate().unwrap();
        for text in ["✅", "😀", "👍🏽", "👩‍💻", "🇺🇸", "❤️"] {
            let (_, glyphs, advance) = shape(&style, text);
            assert!(advance > 0.);
            assert!(
                glyphs.iter().all(|g| g.0.id.0 != 0),
                "missing glyph in {text}"
            );
        }
        let layout = layout(&style, [0., 0.], [900., 150.]);
        assert!(
            layout
                .glyphs
                .iter()
                .any(|g| g.font == TextFont::Emoji && g.glyph.id.0 != 0)
        );
        assert_eq!(boundaries("a👩‍💻b"), vec![0, 1, 4, 5]);
        assert_eq!(
            serde_json::from_str::<TextStyle>(&serde_json::to_string(&style).unwrap()).unwrap(),
            style
        );
        let mut doc = crate::Document::blank(960, 240).unwrap();
        let mut a =
            crate::steps::marker(1, [200., 80.], 44, [0, 0, 0, 255], (1, false), [960, 240])
                .unwrap();
        a.kind = crate::Kind::Text;
        a.start = [20., 20.];
        a.end = [940., 220.];
        a.fill = None;
        a.text = Some(style);
        doc.annotations = vec![a];
        doc.promote_features();
        doc.validate().unwrap();
        let pixels = doc.render().unwrap();
        assert!(pixels.pixels().any(|p| p[0] < 200));
        let copy: crate::Document =
            serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
        assert_eq!(copy.render().unwrap(), pixels);
        if let Ok(root) = std::env::var("CLIPDESIGNER_NEXT_FIXTURES") {
            std::fs::create_dir_all(&root).unwrap();
            doc.save(&std::path::Path::new(&root).join("emoji.capture.json"))
                .unwrap();
        }
    }
}
