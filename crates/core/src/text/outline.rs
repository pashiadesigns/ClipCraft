//! Editable text stroke state and shared outer-edge coverage.
//!
//! Stroke/isolated glyph knockout adapted from DesignCraft paint_glyphs at
//! ea08e4c2ba290f092063c99d70c4bdd5afc45887 (MIT).
//! Copyright (c) 2026 ArtCraft Team and the DesignCraft contributors.
//! See licenses/designcraft. ClipDesigner changes: shared fitted glyph geometry,
//! decoration union, caller-applied alpha, bounded widths and content clipping.
use crate::{Annotation, Error, Result};
use serde::{Deserialize, Serialize};
use vello_cpu::{
    color::{AlphaColor, Srgb},
    kurbo::{Join, Rect, Shape, Stroke},
    peniko::{BlendMode, Compose, Mix},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub color: [u8; 4],
    pub width: f32,
}
impl Default for Outline {
    fn default() -> Self {
        Self {
            color: [0, 0, 0, 255],
            width: 1.,
        }
    }
}
impl Outline {
    pub fn validate(&self) -> Result<()> {
        if !self.width.is_finite() || !(0.0..=32.0).contains(&self.width) {
            return Err(Error::Invalid("Text stroke must be 0–32 pixels".into()));
        }
        Ok(())
    }
}

/// Paint opaque outer-edge coverage for the existing fitted text and rules.
/// The caller applies outline RGBA once when compositing this mask. The glyph
/// interior is removed inside an isolated layer; existing destination paint is
/// never knocked out. Fill and outline remain independently editable.
pub fn paint_coverage(
    context: &mut impl crate::display::vector::Painter,
    annotation: &Annotation,
    width: f64,
    tolerance: f64,
) -> Result<bool> {
    paint_coverage_clipped(context, annotation, width, tolerance, None)
}
pub(crate) fn paint_coverage_clipped(
    context: &mut impl crate::display::vector::Painter,
    annotation: &Annotation,
    width: f64,
    tolerance: f64,
    parent_clip: Option<Rect>,
) -> Result<bool> {
    if !width.is_finite() || !(0.0..=32.0).contains(&width) {
        return Err(Error::Invalid("Text stroke must be 0–32 pixels".into()));
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(Error::Invalid(
            "Text stroke tolerance must be positive".into(),
        ));
    }
    if width == 0.0 {
        return Ok(false);
    }
    let Some((path, clip, _)) = super::vector_geometry(annotation) else {
        return Ok(false);
    };
    let clip = parent_clip.unwrap_or(clip);
    let style = annotation.text.as_ref().expect("geometry requires text");
    let (min, max) = super::content_bounds(annotation);
    let bands: Vec<_> =
        super::decoration::bands(style, min, max, style.underline, style.strikethrough)
            .into_iter()
            .map(|b| {
                Rect::new(
                    b.min[0].into(),
                    b.min[1].into(),
                    b.max[0].into(),
                    b.max[1].into(),
                )
            })
            .collect();
    context.push_clip_path(&clip.to_path(tolerance));
    context.push_blend_layer(BlendMode::default());
    context.set_paint(AlphaColor::<Srgb>::from_rgba8(255, 255, 255, 255));
    context.set_stroke(Stroke::new(width * 2.0).with_join(Join::Round));
    context.stroke_path(&path);
    for band in &bands {
        context.stroke_path(&band.to_path(tolerance));
    }
    context.push_blend_layer(BlendMode::new(Mix::Normal, Compose::DestOut));
    context.fill_path(&path);
    for band in &bands {
        context.fill_rect(band);
    }
    context.pop_layer();
    context.pop_layer();
    context.pop_clip_path();
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vello_cpu::{Pixmap, RenderContext, Resources, kurbo::Affine};

    fn fixture() -> Annotation {
        let d: crate::Document =
            serde_json::from_str(include_str!("../../tests/fixtures/properties.capture.json"))
                .unwrap();
        let mut a = d.annotations[0].clone();
        a.start = [10.25, 20.5];
        a.end = [190.25, 130.5];
        a.text = Some(crate::text::TextStyle {
            content: "O H I\nA X O".into(),
            size_points: 28.0,
            color: Some([20, 90, 170, 0]),
            fit: crate::text::TextFit::None,
            ..Default::default()
        });
        a
    }
    fn render(a: &Annotation, outline: bool, background: bool) -> Vec<[u8; 4]> {
        let mut ctx = RenderContext::new(420, 300);
        if background {
            ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(40, 80, 120, 255));
            ctx.fill_rect(&Rect::new(0.0, 0.0, 420.0, 300.0));
        }
        ctx.set_transform(Affine::scale(2.0));
        ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(255, 255, 255, 255));
        if outline {
            assert!(paint_coverage(&mut ctx, a, 2.0, 0.1).unwrap());
        } else {
            super::super::paint_vector_coverage(
                &mut ctx,
                a,
                a.text.as_ref().unwrap().underline,
                a.text.as_ref().unwrap().strikethrough,
                0.1,
            )
            .unwrap();
        }
        let mut pixmap = Pixmap::new(420, 300);
        ctx.flush();
        ctx.render(&mut pixmap, &mut Resources::new());
        pixmap.data().iter().map(|p| [p.r, p.g, p.b, p.a]).collect()
    }
    #[test]
    fn outer_glyph_and_decoration_coverage_survives_transparent_fill_without_erasing_destination() {
        for font in [
            crate::text::TextFont::UbuntuLight,
            crate::text::TextFont::HackRegular,
        ] {
            for decorated in [false, true] {
                let mut a = fixture();
                let t = a.text.as_mut().unwrap();
                t.font = font;
                t.underline = decorated;
                t.strikethrough = decorated;
                assert!(super::super::vector_path(&a).is_none());
                let mask = render(&a, true, false);
                a.text.as_mut().unwrap().color.as_mut().unwrap()[3] = 255;
                let glyphs = render(&a, false, false);
                assert!(mask.iter().filter(|p| p[3] > 0).count() > 500);
                assert!(mask.iter().any(|p| p[3] > 0 && p[3] < 255), "antialiasing");
                assert_eq!(
                    mask,
                    render(&a, true, false),
                    "outline geometry independent of fill alpha"
                );
                for (p, glyph) in mask.iter().zip(&glyphs) {
                    if glyph[3] == 255 {
                        assert_eq!(p[3], 0, "glyph interior must be hollow");
                    }
                    let alpha = (u16::from(p[3]) * 96 + 127) / 255;
                    assert!(alpha <= 96);
                }
                let over = render(&a, true, true);
                assert!(
                    over.iter().all(|p| p[3] == 255),
                    "knockout must stay isolated"
                );
                for (i, p) in mask.iter().enumerate() {
                    let x = (i % 420) as f32 / 2.0;
                    let y = (i / 420) as f32 / 2.0;
                    if x < 10.0 || x > 191.0 || y < 20.0 || y > 131.0 {
                        assert_eq!(p[3], 0);
                    }
                    if glyphs[i][3] == 255 {
                        assert_eq!(over[i], [40, 80, 120, 255]);
                    }
                }
            }
        }
    }
    #[test]
    fn outline_rejects_invalid_parameters_and_empty_labels_without_painting() {
        let mut ctx = RenderContext::new(32, 32);
        let mut a = fixture();
        for width in [-1.0, 33.0, f64::NAN, f64::INFINITY] {
            assert!(paint_coverage(&mut ctx, &a, width, 0.1).is_err());
        }
        for tolerance in [0.0, -1.0, f64::NAN] {
            assert!(paint_coverage(&mut ctx, &a, 1.0, tolerance).is_err());
        }
        assert!(!paint_coverage(&mut ctx, &a, 0.0, 0.1).unwrap());
        a.text.as_mut().unwrap().content.clear();
        assert!(!paint_coverage(&mut ctx, &a, 2.0, 0.1).unwrap());
        let mut pixels = Pixmap::new(32, 32);
        ctx.flush();
        ctx.render(&mut pixels, &mut Resources::new());
        assert!(pixels.data().iter().all(|p| p.a == 0));
    }
}
