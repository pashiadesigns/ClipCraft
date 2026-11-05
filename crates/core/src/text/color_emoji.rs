//! Windows' installed emoji font supplies its own COLR v0 fallback palette.
//! Fonts are never redistributed or embedded. Color layers remain vector paths
//! through the shared canvas/PDF renderer; other platforms retain Noto fallback.
use super::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use vello_cpu::kurbo::{Affine, BezPath, Shape};
type Layers = Arc<Vec<(BezPath, [u8; 4])>>;

pub fn font_bytes() -> Option<&'static [u8]> {
    static BYTES: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    BYTES
        .get_or_init(|| {
            if !cfg!(windows) {
                return None;
            }
            let path =
                std::path::PathBuf::from(std::env::var_os("WINDIR")?).join("Fonts/seguiemj.ttf");
            if std::fs::metadata(&path).ok()?.len() > 32_000_000 {
                return None;
            }
            let bytes = std::fs::read(path).ok()?;
            let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
            face.tables().colr?;
            ab_glyph::FontRef::try_from_slice(&bytes).ok()?;
            Some(bytes)
        })
        .as_deref()
}
struct Outline(BezPath);
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }
    fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
        self.0.quad_to((x as f64, y as f64), (x1 as f64, y1 as f64));
    }
    fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.0.curve_to(
            (x as f64, y as f64),
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(i..i + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}
fn read_layers(id: u16) -> Option<Vec<(BezPath, [u8; 4])>> {
    let face = ttf_parser::Face::parse(font_bytes()?, 0).ok()?;
    let raw = face.raw_face();
    let colr = raw.table(ttf_parser::Tag::from_bytes(b"COLR"))?;
    let cpal = ttf_parser::cpal::Table::parse(raw.table(ttf_parser::Tag::from_bytes(b"CPAL"))?)?;
    let count = u16_at(colr, 2)? as usize;
    let base = u32_at(colr, 4)?;
    let layers = u32_at(colr, 8)?;
    let total = u16_at(colr, 12)? as usize;
    let mut lo = 0;
    let mut hi = count;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let pos = base + mid * 6;
        let glyph = u16_at(colr, pos)?;
        match glyph.cmp(&id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => {
                let first = u16_at(colr, pos + 2)? as usize;
                let n = u16_at(colr, pos + 4)? as usize;
                if n == 0 || n > 4096 || first + n > total {
                    return None;
                }
                let mut out = Vec::with_capacity(n);
                for index in first..first + n {
                    let p = layers + index * 4;
                    let glyph = ttf_parser::GlyphId(u16_at(colr, p)?);
                    let palette = u16_at(colr, p + 2)?;
                    let rgba = if palette == 0xffff {
                        [0, 0, 0, 255]
                    } else {
                        let c = cpal.get(0, palette)?;
                        [c.red, c.green, c.blue, c.alpha]
                    };
                    let mut path = Outline(BezPath::new());
                    face.outline_glyph(glyph, &mut path)?;
                    out.push((path.0, rgba));
                }
                return Some(out);
            }
        }
    }
    None
}
fn layers(id: u16) -> Layers {
    static CACHE: OnceLock<Mutex<HashMap<u16, Layers>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(v) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&id) {
        return v.clone();
    }
    let value = Arc::new(read_layers(id).unwrap_or_default());
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= 512 {
        cache.clear();
    }
    cache.insert(id, value.clone());
    value
}
pub(super) fn has_color(font: TextFont, id: GlyphId) -> bool {
    font == TextFont::Emoji && !layers(id.0).is_empty()
}
pub(super) fn ink_bounds(id: GlyphId) -> Option<vello_cpu::kurbo::Rect> {
    layers(id.0)
        .iter()
        .map(|(path, _)| path.bounding_box())
        .reduce(|a, b| a.union(b))
}
pub(crate) fn paint<P: crate::display::vector::Painter>(
    context: &mut P,
    a: &Annotation,
    tolerance: f64,
    finish: &mut impl FnMut(&mut P, [u8; 4]),
) {
    paint_clipped(context, a, tolerance, None, finish);
}
pub(crate) fn paint_clipped<P: crate::display::vector::Painter>(
    context: &mut P,
    a: &Annotation,
    tolerance: f64,
    parent: Option<vello_cpu::kurbo::Rect>,
    finish: &mut impl FnMut(&mut P, [u8; 4]),
) {
    let Some(style) = a.text.as_ref() else { return };
    if !unicode::needs_fallback(style) {
        return;
    }
    let opacity = style.color.unwrap_or(a.color)[3];
    if opacity == 0 {
        return;
    }
    let (min, max) = content_bounds(a);
    let style = fitted_style(style, [max[0] - min[0], max[1] - min[1]]);
    let clip =
        vello_cpu::kurbo::Rect::new(min[0].into(), min[1].into(), max[0].into(), max[1].into());
    let clip = parent
        .map_or(clip, |p| clip.intersect(p))
        .to_path(tolerance);
    for p in layout(&style, min, max).glyphs {
        if p.font != TextFont::Emoji || p.character.is_whitespace() {
            continue;
        }
        let f = p.font.face(false, false);
        let scaled = f.as_scaled(p.glyph.scale);
        let transform = Affine::new([
            scaled.h_scale_factor().into(),
            0.,
            0.,
            -f64::from(scaled.v_scale_factor()),
            p.glyph.position.x.into(),
            p.glyph.position.y.into(),
        ]);
        for (path, ink) in layers(p.glyph.id.0).iter() {
            context.push_clip_path(&clip);
            context.fill_path(&(transform * path));
            context.pop_clip_path();
            let mut ink = *ink;
            ink[3] = (ink[3] as u16 * opacity as u16 / 255) as u8;
            finish(context, ink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_emoji_layers_are_colored_and_exportable_paths() {
        if !cfg!(windows) {
            return;
        }
        assert!(font_bytes().is_some());
        let face = TextFont::Emoji.face(false, false);
        let colored = layers(face.glyph_id('😀').0);
        assert!(colored.len() > 2);
        assert!(colored.iter().any(|(_, c)| c[0] > c[2] + 20));
        assert!(Arc::ptr_eq(&colored, &layers(face.glyph_id('😀').0)));
    }
    #[test]
    fn colored_document_survives_rich_text_viewport_save_pdf_and_undo() {
        if !cfg!(windows) {
            return;
        }
        let mut a = crate::steps::marker(1, [80., 50.], 44, [0, 0, 0, 255], (1, false), [320, 160])
            .unwrap();
        a.kind = crate::Kind::Text;
        a.start = [10., 10.];
        a.end = [310., 150.];
        a.fill = None;
        a.text = Some(TextStyle {
            content: "Ready 😀 ❤️".into(),
            size_points: 30.,
            ..Default::default()
        });
        let mut s = crate::Session::new(crate::Document::blank(320, 160).unwrap()).unwrap();
        s.execute(crate::Command::Add {
            annotation: a.clone(),
        })
        .unwrap();
        let pixels = s.document.render().unwrap();
        assert!(
            pixels
                .pixels()
                .any(|p| p[0] > 180 && p[1] > 70 && p[2] < 80),
            "yellow emoji ink missing"
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("color.iclip");
        s.save(&path).unwrap();
        assert_eq!(
            crate::Document::load(&path).unwrap().render().unwrap(),
            pixels
        );
        assert!(
            crate::display::vector::record(&a)
                .iter()
                .any(|(_, c)| c[0] > 180 && c[2] < 80)
        );
        let text = a.text.as_mut().unwrap();
        super::super::rich::format_range(text, 0..5, |f| f.bold = true);
        s.execute(crate::Command::Update { annotation: a }).unwrap();
        assert!(
            s.document
                .render()
                .unwrap()
                .pixels()
                .any(|p| p[0] > 180 && p[1] > 70 && p[2] < 80)
        );
        s.execute(crate::Command::Undo).unwrap();
        assert_eq!(s.document.render().unwrap(), pixels);
    }
}
