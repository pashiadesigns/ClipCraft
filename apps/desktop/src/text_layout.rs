//! Adapt the document engine's layout to egui's editable text surface. The
//! widget owns keyboard/caret behavior, never a second set of wrapping rules.
use crate::egui;
use capture_core::text::{TextFont, TextStyle, editing_layout};
use std::{collections::HashMap, sync::Arc};

pub(super) fn family(style: &TextStyle) -> egui::FontFamily {
    if !style.bold && !style.italic && !matches!(style.font, TextFont::System(_)) {
        return if style.font == TextFont::HackRegular {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        };
    }
    egui::FontFamily::Name(
        format!("ClipCraft {:?} {} {}", style.font, style.bold, style.italic).into(),
    )
}
pub(super) fn available_family(ctx: &egui::Context, style: &TextStyle) -> egui::FontFamily {
    let family = family(style);
    if let TextFont::System(id) = style.font {
        let egui::FontFamily::Name(name) = &family else {
            unreachable!()
        };
        if !ctx.fonts(|f| f.definitions().families.contains_key(&family)) {
            let mut data = egui::FontData::from_static(style.font.bytes(style.bold, style.italic));
            data.index = capture_core::text::system_fonts::index(id, style.bold, style.italic);
            ctx.add_font(egui::FontInsert {
                name: name.to_string(),
                data,
                families: vec![egui::InsertFontFamily {
                    family: family.clone(),
                    priority: egui::FontPriority::Highest,
                }],
            });
            ctx.add_font(egui::FontInsert {
                name: "ClipCraft Emoji".into(),
                data: egui::FontData::from_static(TextFont::Emoji.bytes(false, false)),
                families: vec![egui::InsertFontFamily {
                    family: family.clone(),
                    priority: egui::FontPriority::Lowest,
                }],
            });
            ctx.request_repaint();
            return egui::FontFamily::Proportional;
        }
    }
    family
}

pub(super) fn configure(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
        for (bold, italic) in [(true, false), (false, true), (true, true)] {
            let style = TextStyle {
                font,
                bold,
                italic,
                ..Default::default()
            };
            let key = family(&style);
            let egui::FontFamily::Name(name) = &key else {
                unreachable!()
            };
            fonts.font_data.insert(
                name.to_string(),
                Arc::new(egui::FontData::from_static(font.bytes(bold, italic))),
            );
            fonts.families.insert(key.clone(), vec![name.to_string()]);
        }
    }
    fonts.font_data.insert(
        "ClipCraft Emoji".into(),
        Arc::new(egui::FontData::from_static(
            TextFont::Emoji.bytes(false, false),
        )),
    );
    for family in fonts.families.values_mut() {
        family.push("ClipCraft Emoji".into());
    }
    ctx.set_fonts(fonts);
}

pub(super) fn galley(
    ui: &egui::Ui,
    style: &TextStyle,
    size: [f32; 2],
    scale: f32,
    color: egui::Color32,
) -> Arc<egui::Galley> {
    let style = &capture_core::text::fitted_style(style, size);
    let width = size[0];
    let layout = editing_layout(style, width);
    let font = egui::FontId::new(
        style.size_points * 4.0 / 3.0 * scale * style.typography().script.scale(),
        available_family(ui.ctx(), style),
    );
    ui.fonts_mut(|fonts| {
        let mut result =
            (*fonts.layout_no_wrap(style.content.clone(), font.clone(), color)).clone();
        let seed = fonts.layout_no_wrap("M".into(), font.clone(), color);
        let mut glyphs = HashMap::new();
        result.rows.clear();
        result.mesh_bounds = egui::Rect::NOTHING;
        result.num_vertices = 0;
        result.num_indices = 0;
        for (index, row) in layout.rows.iter().enumerate() {
            let height = row.height.unwrap_or(layout.line_height) * scale;
            let ascent = row.ascent.unwrap_or(layout.ascent) * scale;
            let mut placed = seed.rows[0].clone();
            let origin = row.start_x.unwrap_or(0.) * scale;
            placed.pos = egui::pos2(origin, row.y.map_or(index as f32 * height, |y| y * scale));
            placed.ends_with_newline = row.ends_with_newline;
            let output = Arc::make_mut(&mut placed.row);
            output.glyphs.clear();
            output.visuals = Default::default();
            let empty_x = match style.horizontal {
                capture_core::text::HorizontalAlignment::Left => 0.0,
                capture_core::text::HorizontalAlignment::Center => width * 0.5,
                capture_core::text::HorizontalAlignment::Right => width,
            };
            output.size = egui::vec2(
                (row.characters
                    .last()
                    .map_or(row.start_x.unwrap_or(empty_x), |c| c.x + c.advance)
                    * scale
                    - origin)
                    .max(0.),
                height,
            );
            for character in &row.characters {
                // A single character prevents shaping/ligature adjustments from
                // changing legacy document metrics. Glyph coverage is still
                // provided by the existing bundled font atlas.
                let visible = !character.character.is_whitespace();
                let source = glyphs.entry(character.character).or_insert_with(|| {
                    fonts.layout_no_wrap(
                        if visible { character.character } else { ' ' }.to_string(),
                        font.clone(),
                        color,
                    )
                });
                let source_row = &source.rows[0];
                let mut glyph = source_row.glyphs[0];
                let position = egui::pos2(character.x * scale - origin, ascent);
                let delta = position - glyph.pos;
                glyph.chr = character.character;
                glyph.pos = position;
                glyph.advance_width = character.advance * scale;
                glyph.line_height = height;
                glyph.font_ascent = ascent;
                glyph.font_height = height;
                glyph.font_face_ascent = glyph.font_ascent;
                glyph.font_face_height = height;
                glyph.first_vertex = output.visuals.mesh.vertices.len() as u32;
                if visible {
                    let mut mesh = source_row.visuals.mesh.clone();
                    mesh.translate(delta);
                    output.visuals.mesh.append(mesh);
                }
                output.glyphs.push(glyph);
            }
            if style.decorated() {
                // The canvas paints glyphs and rules as one coverage layer.
                // Keep the exact logical glyphs for caret/selection/navigation.
                output.visuals.mesh.clear();
                for glyph in &mut output.glyphs {
                    glyph.first_vertex = 0;
                }
            }
            output.visuals.mesh_bounds = output.visuals.mesh.calc_bounds();
            output.visuals.glyph_vertex_range = 0..output.visuals.mesh.vertices.len();
            result.mesh_bounds = result
                .mesh_bounds
                .union(output.visuals.mesh_bounds.translate(placed.pos.to_vec2()));
            result.num_vertices += output.visuals.mesh.vertices.len();
            result.num_indices += output.visuals.mesh.indices.len();
            result.rows.push(placed);
        }
        result.rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(
                width * scale,
                layout.rows.last().map_or(0., |r| {
                    r.y.unwrap_or((layout.rows.len() - 1) as f32 * layout.line_height)
                        + r.height.unwrap_or(layout.line_height)
                }) * scale,
            ),
        );
        Arc::new(result)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_font_registers_and_keeps_inline_caret_on_shared_metrics() {
        let Some(font) = capture_core::text::system_fonts::choices().find(|f| f.label() == "Arial")
        else {
            return;
        };
        let ctx = egui::Context::default();
        configure(&ctx);
        for frame in 0..3 {
            let _output = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
                Default::default(),
                |ui| {
                    for (bold, italic) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        let style = TextStyle {
                            font,
                            bold,
                            italic,
                            content: "Installed Aa 123".into(),
                            ..Default::default()
                        };
                        let actual = galley(ui, &style, [400., 100.], 1., egui::Color32::WHITE);
                        let layout = editing_layout(&style, 400.);
                        assert_eq!(actual.rows.len(), layout.rows.len());
                        for (row, expected) in actual.rows.iter().zip(&layout.rows) {
                            for (glyph, c) in row.glyphs.iter().zip(&expected.characters) {
                                assert!((glyph.advance_width - c.advance).abs() < 0.001);
                            }
                        }
                        if frame > 0 {
                            assert_eq!(available_family(&ctx, &style), family(&style));
                        }
                    }
                },
            ));
        }
    }
    #[test]
    fn typography_inline_caret_matches_export_metrics_without_double_painting() {
        use capture_core::text::typography::{Script, Typography};
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
                for script in [Script::Normal, Script::Superscript, Script::Subscript] {
                    for spacing in [-1., 0., 3.] {
                        let style = TextStyle {
                            content: "Abc 12\nsecond line".into(),
                            font,
                            typography: Some(Typography {
                                script,
                                spacing,
                                highlight: Some([255, 230, 20, 160]),
                            }),
                            ..Default::default()
                        };
                        let size = [160., 100.];
                        let fitted = capture_core::text::fitted_style(&style, size);
                        let expected = editing_layout(&fitted, size[0]);
                        for zoom in [0.5, 1., 2.5] {
                            let actual = galley(ui, &style, size, zoom, egui::Color32::BLACK);
                            assert_eq!(actual.rows.len(), expected.rows.len());
                            assert_eq!(actual.num_vertices, 0);
                            for (row, e) in actual.rows.iter().zip(&expected.rows) {
                                assert_eq!(row.glyphs.len(), e.characters.len());
                                for (g, c) in row.glyphs.iter().zip(&e.characters) {
                                    assert!((g.pos.x / zoom - c.x).abs() < 0.001);
                                    assert!((g.pos.y / zoom - expected.ascent).abs() < 0.001);
                                    assert!((g.advance_width / zoom - c.advance).abs() < 0.001);
                                }
                            }
                        }
                    }
                }
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn decorated_galley_preserves_caret_metrics_without_duplicate_ink_meshes() {
        let ctx = egui::Context::default();
        configure(&ctx);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
                for scale in [0.25, 1.0, 3.0] {
                    let mut style = TextStyle {
                        content: "AV text\n second row".into(),
                        font,
                        ..Default::default()
                    };
                    let plain = galley(ui, &style, [100.0, 70.0], scale, egui::Color32::WHITE);
                    style.underline = true;
                    style.strikethrough = true;
                    let ruled = galley(ui, &style, [100.0, 70.0], scale, egui::Color32::WHITE);
                    assert_eq!(plain.rect, ruled.rect);
                    assert_eq!(plain.rows.len(), ruled.rows.len());
                    assert_eq!(ruled.num_vertices, 0);
                    assert_eq!(ruled.num_indices, 0);
                    for (a, b) in plain.rows.iter().zip(&ruled.rows) {
                        assert_eq!(a.pos, b.pos);
                        assert_eq!(a.glyphs.len(), b.glyphs.len());
                        for (a, b) in a.glyphs.iter().zip(&b.glyphs) {
                            assert_eq!(a.pos, b.pos);
                            assert_eq!(a.chr, b.chr);
                            assert_eq!(a.advance_width, b.advance_width);
                        }
                    }
                }
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn fitted_caret_layout_matches_shared_size_and_wrapping_at_every_zoom() {
        let ctx = egui::Context::default();
        configure(&ctx);
        ctx.run_ui(Default::default(), |ui| {
            for (font, bold, italic) in [
                (TextFont::UbuntuLight, false, false),
                (TextFont::UbuntuLight, true, false),
                (TextFont::UbuntuLight, false, true),
                (TextFont::UbuntuLight, true, true),
                (TextFont::HackRegular, false, false),
                (TextFont::HackRegular, true, false),
                (TextFont::HackRegular, false, true),
                (TextFont::HackRegular, true, true),
            ] {
                for wrap in [true, false] {
                    let style = TextStyle {
                        content: "Lots of text inside a small shape\nSecond line".into(),
                        wrap,
                        font,
                        bold,
                        italic,
                        ..Default::default()
                    };
                    let size = [85.0, 38.0];
                    let fitted = capture_core::text::fitted_style(&style, size);
                    let expected = editing_layout(&fitted, size[0]);
                    for zoom in [0.25, 1.0, 3.0] {
                        let actual = galley(ui, &style, size, zoom, egui::Color32::BLACK);
                        assert_eq!(actual.rows.len(), expected.rows.len());
                        assert!(
                            (actual.rect.height() / zoom
                                - expected.rows.len() as f32 * expected.line_height)
                                .abs()
                                < 0.001
                        );
                        for (row, expected) in actual.rows.iter().zip(&expected.rows) {
                            assert_eq!(row.glyphs.len(), expected.characters.len());
                            for (glyph, ch) in row.glyphs.iter().zip(&expected.characters) {
                                assert!((glyph.pos.x / zoom - ch.x).abs() < 0.001);
                            }
                        }
                    }
                }
            }
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn borderline_wrap_and_cursor_indices_match_export_at_every_zoom() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for font in [TextFont::UbuntuLight, TextFont::HackRegular] {
                for content in ["Package", "AVATAR office", "one\ttwo\r\n\nthree  ", ""] {
                    for width in [0.0, 30.0, 98.0, 100.0, 180.0] {
                        let style = TextStyle {
                            content: content.into(),
                            font,
                            size_points: 20.0,
                            ..Default::default()
                        };
                        let expected = editing_layout(&style, width);
                        for scale in [0.25, 1.0, 1.5, 3.0] {
                            let actual =
                                galley(ui, &style, [width, 1000000.0], scale, egui::Color32::BLACK);
                            assert_eq!(actual.rows.len(), expected.rows.len());
                            assert_eq!(
                                actual
                                    .rows
                                    .iter()
                                    .map(|r| r.char_count_including_newline().0)
                                    .sum::<usize>(),
                                content.chars().count()
                            );
                            for (row, expected) in actual.rows.iter().zip(&expected.rows) {
                                assert_eq!(row.ends_with_newline, expected.ends_with_newline);
                                assert_eq!(row.glyphs.len(), expected.characters.len());
                                for (glyph, character) in
                                    row.glyphs.iter().zip(&expected.characters)
                                {
                                    assert_eq!(glyph.chr, character.character);
                                    assert!((glyph.pos.x / scale - character.x).abs() < 0.001);
                                    assert!(
                                        (glyph.advance_width / scale - character.advance).abs()
                                            < 0.001
                                    );
                                }
                            }
                            if content == "Package" && width == 98.0 {
                                assert_eq!(
                                    actual.rows.iter().map(|r| r.text()).collect::<Vec<_>>(),
                                    ["Packag", "e"]
                                );
                            }
                        }
                    }
                }
            }
        });
        output.textures_delta.clear();
    }
}
