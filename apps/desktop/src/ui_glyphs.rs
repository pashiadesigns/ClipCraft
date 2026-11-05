//! Preserve original SVG curves, subpaths, round caps/joins and fill semantics.
//! These are the approved Lucide assets; assets/library-icons/LICENSE applies.
use std::{collections::HashMap, sync::OnceLock};
use vello_cpu::{
    Pixmap, RenderContext,
    color::{AlphaColor, Srgb},
    kurbo::{Affine, BezPath, Cap, Join, Stroke},
};
#[derive(serde::Deserialize)]
struct Curve {
    data: String,
    fill: bool,
    stroke: f64,
}
pub(super) fn render(name: &str, px: u16) -> Option<image::RgbaImage> {
    static CURVES: OnceLock<HashMap<String, Vec<Curve>>> = OnceLock::new();
    let curves = CURVES.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/library-icons/curves.json"))
            .expect("bundled icon curves")
    });
    let paths = curves.get(name)?;
    let mut context = RenderContext::new(px, px);
    context.set_transform(Affine::scale(px as f64 / 24.));
    context.set_paint(AlphaColor::<Srgb>::from_rgba8(255, 255, 255, 255));
    for curve in paths {
        let path = BezPath::from_svg(&curve.data).expect("bundled SVG path");
        if curve.fill {
            context.fill_path(&path);
        }
        if curve.stroke > 0. {
            context.set_stroke(
                Stroke::new(curve.stroke)
                    .with_caps(Cap::Round)
                    .with_join(Join::Round),
            );
            context.stroke_path(&path);
        }
    }
    let mut pixels = Pixmap::new(px, px);
    context.flush();
    context.render(&mut pixels, &mut vello_cpu::Resources::new());
    image::RgbaImage::from_raw(
        px as u32,
        px as u32,
        pixels
            .data()
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect(),
    )
}
