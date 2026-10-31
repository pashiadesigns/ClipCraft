//! Original shared row geometry for the pending underline/strike workflow.
//! Layout, fitting and clipping come from the same engine as glyph rendering.
use super::{TextStyle, fitted_style, layout};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    Underline,
    Strikethrough,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuleBand {
    pub kind: RuleKind,
    pub row: usize,
    pub min: [f32; 2],
    pub max: [f32; 2],
}

/// Continuous bands span the visible characters of each laid-out row, including
/// interior spaces but excluding leading/trailing whitespace and empty rows.
/// Rectangles are in image coordinates, clipped to the label's content bounds.
/// Callers must combine their coverage with glyphs before applying text alpha;
/// drawing strike and glyph independently would darken translucent overlaps.
pub fn bands(
    style: &TextStyle,
    min: [f32; 2],
    max: [f32; 2],
    underline: bool,
    strikethrough: bool,
) -> Vec<RuleBand> {
    if (!underline && !strikethrough)
        || min.iter().chain(&max).any(|v| !v.is_finite())
        || (0..2).any(|i| max[i] <= min[i])
    {
        return vec![];
    }
    let style = fitted_style(style, [max[0] - min[0], max[1] - min[1]]);
    let positioned = layout(&style, min, max);
    let mut spans: Vec<Option<(f32, f32, f32)>> = vec![None; positioned.rows];
    for glyph in positioned.glyphs {
        if glyph.character.is_whitespace() {
            continue;
        }
        let x = glyph.glyph.position.x;
        let end = glyph.advance_end;
        let baseline = glyph.glyph.position.y;
        spans[glyph.row] = Some(match spans[glyph.row] {
            Some((left, right, y)) => (left.min(x), right.max(end), y),
            None => (x, end, baseline),
        });
    }
    let em = style.size_points * (96.0 / 72.0) * style.typography().script.scale();
    let thickness = (em * 0.055).max(0.5);
    let mut result = vec![];
    for (row, span) in spans.into_iter().enumerate() {
        let Some((left, right, baseline)) = span else {
            continue;
        };
        for (enabled, kind, y) in [
            (underline, RuleKind::Underline, baseline + em * 0.08),
            (
                strikethrough,
                RuleKind::Strikethrough,
                baseline
                    - (positioned.ascent
                        - style.typography().script.shift(style.size_points * 4. / 3.))
                        * style.typography().script.scale()
                        * 0.35,
            ),
        ] {
            if !enabled {
                continue;
            }
            let lo = [left.max(min[0]), (y - thickness * 0.5).max(min[1])];
            let hi = [right.min(max[0]), (y + thickness * 0.5).min(max[1])];
            if (0..2).all(|i| hi[i] > lo[i]) {
                result.push(RuleBand {
                    kind,
                    row,
                    min: lo,
                    max: hi,
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{HorizontalAlignment as H, TextFit, VerticalAlignment as V};

    #[test]
    fn real_glyph_and_rule_coverage_unions_without_erasing_or_double_opacity() {
        use vello_cpu::{
            Pixmap, RenderContext, Resources,
            color::{AlphaColor, Srgb},
            kurbo::{Affine, Shape},
        };
        let fixture: crate::Document =
            serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json"))
                .unwrap();
        for font in [
            crate::text::TextFont::UbuntuLight,
            crate::text::TextFont::HackRegular,
        ] {
            let mut a = fixture.annotations[0].clone();
            a.start = [10.25, 20.5];
            a.end = [190.25, 120.5];
            a.text = Some(TextStyle {
                content: "I H A\nO X I".into(),
                font,
                size_points: 28.0,
                fit: TextFit::None,
                padding: 0.0,
                color: Some([30, 80, 160, 96]),
                ..Default::default()
            });
            let draw = |underline, strike, legacy| {
                let mut ctx = RenderContext::new(420, 280);
                ctx.set_transform(Affine::scale(2.0));
                ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(255, 255, 255, 255));
                let ink = if legacy {
                    let (p, clip, ink) = crate::text::vector_path(&a).unwrap();
                    ctx.push_clip_path(&clip.to_path(0.1));
                    ctx.fill_path(&p);
                    ctx.pop_clip_path();
                    ink
                } else {
                    crate::text::paint_vector_coverage(&mut ctx, &a, underline, strike, 0.1)
                        .unwrap()
                };
                let mut pixels = Pixmap::new(420, 280);
                ctx.flush();
                ctx.render(&mut pixels, &mut Resources::new());
                pixels
                    .data()
                    .iter()
                    .map(|c| ((u16::from(ink[3]) * u16::from(c.a) + 127) / 255) as u8)
                    .collect::<Vec<_>>()
            };
            let plain = draw(false, false, false);
            assert_eq!(plain, draw(false, false, true));
            for (u, s) in [(true, false), (false, true), (true, true)] {
                let decorated = draw(u, s, false);
                assert!(
                    plain
                        .iter()
                        .zip(&decorated)
                        .all(|(p, d)| d >= p && *d <= 96)
                );
                assert!(plain.iter().zip(&decorated).filter(|(p, d)| d > p).count() > 150);
                // Fully covered glyph stems remain at exactly one ink opacity.
                assert!(
                    plain
                        .iter()
                        .zip(&decorated)
                        .filter(|(p, _)| **p == 96)
                        .all(|(_, d)| *d == 96)
                );
            }
        }
    }

    #[test]
    fn continuous_rules_follow_aligned_rows_without_whitespace_seams() {
        for (h, v) in [
            (H::Left, V::Top),
            (H::Center, V::Middle),
            (H::Right, V::Bottom),
        ] {
            let style = TextStyle {
                content: "  A B  \n\nC\tD  ".into(),
                size_points: 24.0,
                fit: TextFit::None,
                wrap: false,
                horizontal: h,
                vertical: v,
                ..Default::default()
            };
            let min = [20.0, 30.0];
            let max = [400.0, 300.0];
            let rules = bands(&style, min, max, true, true);
            assert_eq!(rules.len(), 4);
            assert_eq!(
                rules.iter().map(|r| r.row).collect::<Vec<_>>(),
                vec![0, 0, 2, 2]
            );
            let positioned = layout(&style, min, max);
            for pair in rules.chunks(2) {
                let (underline, strike) = (pair[0], pair[1]);
                assert_eq!(underline.kind, RuleKind::Underline);
                assert_eq!(strike.kind, RuleKind::Strikethrough);
                assert_eq!(
                    [underline.min[0], underline.max[0]],
                    [strike.min[0], strike.max[0]]
                );
                let glyphs: Vec<_> = positioned
                    .glyphs
                    .iter()
                    .filter(|g| g.row == underline.row && !g.character.is_whitespace())
                    .collect();
                assert_eq!(underline.min[0], glyphs[0].glyph.position.x);
                assert_eq!(underline.max[0], glyphs.last().unwrap().advance_end);
                assert!(strike.max[1] < glyphs[0].glyph.position.y);
                assert!(underline.min[1] > glyphs[0].glyph.position.y);
            }
            assert!(bands(&style, min, max, false, false).is_empty());
            assert_eq!(bands(&style, min, max, true, false).len(), 2);
        }
    }

    #[test]
    fn rules_share_wrapping_fit_clipping_and_subpixel_translation() {
        let style = TextStyle {
            content: "Wrapped decoration with several words".into(),
            size_points: 48.0,
            ..Default::default()
        };
        let min = [0.25, 0.5];
        let max = [90.25, 50.5];
        let rules = bands(&style, min, max, true, true);
        assert!(!rules.is_empty());
        let fitted = fitted_style(&style, [90.0, 50.0]);
        let expected = layout(&fitted, min, max);
        assert!(fitted.size_points < 48.0);
        for r in &rules {
            assert!(r.row < expected.rows);
            assert!((0..2).all(|i| r.min[i] >= min[i] && r.max[i] <= max[i]));
        }
        let shifted = bands(&style, [10.25, 20.5], [100.25, 70.5], true, true);
        assert_eq!(rules.len(), shifted.len());
        for (a, b) in rules.iter().zip(shifted) {
            assert_eq!((a.kind, a.row), (b.kind, b.row));
            for i in 0..2 {
                assert!((b.min[i] - a.min[i] - [10.0, 20.0][i]).abs() < 0.001);
                assert!((b.max[i] - a.max[i] - [10.0, 20.0][i]).abs() < 0.001);
            }
        }
        assert!(bands(&style, [0.0; 2], [0.0; 2], true, true).is_empty());
        assert!(bands(&style, [f32::NAN, 0.0], max, true, true).is_empty());
    }
}
