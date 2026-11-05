//! Original instruction gestures. Key labels use the bundled licensed font outlines.
use super::vector::{Path, VectorIcon};
use vello_cpu::kurbo::BezPath;
struct Outline {
    path: BezPath,
    x: f64,
    scale: f64,
}
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path
            .move_to((self.x + x as f64 * self.scale, 75. - y as f64 * self.scale));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path
            .line_to((self.x + x as f64 * self.scale, 75. - y as f64 * self.scale));
    }
    fn quad_to(&mut self, x: f32, y: f32, a: f32, b: f32) {
        self.path.quad_to(
            (self.x + x as f64 * self.scale, 75. - y as f64 * self.scale),
            (self.x + a as f64 * self.scale, 75. - b as f64 * self.scale),
        );
    }
    fn curve_to(&mut self, x: f32, y: f32, a: f32, b: f32, c: f32, d: f32) {
        self.path.curve_to(
            (self.x + x as f64 * self.scale, 75. - y as f64 * self.scale),
            (self.x + a as f64 * self.scale, 75. - b as f64 * self.scale),
            (self.x + c as f64 * self.scale, 75. - d as f64 * self.scale),
        );
    }
    fn close(&mut self) {
        self.path.close_path();
    }
}
pub(super) fn icon(id: &str) -> Option<VectorIcon> {
    let blue = [24, 145, 193, 255];
    let ink = [40, 47, 52, 255];
    let mut icon = VectorIcon {
        colors: vec![blue, ink, [255; 4]],
        paths: vec![],
    };
    let mut add = |s: &str, color, stroke| {
        icon.paths.push(Path {
            data: s.into(),
            color,
            stroke,
        })
    };
    match id {
        "mouse-left" | "mouse-right" | "mouse-double" | "mouse-scroll" => {
            add(
                "M 64 18 C 44 18 32 32 32 52 L 32 82 C 32 119 96 119 96 82 L 96 52 C 96 32 84 18 64 18 Z",
                1,
                Some(5.),
            );
            add("M 32 57 L 96 57 M 64 19 L 64 57", 1, Some(4.));
            match id {
                "mouse-right" => add("M 68 24 C 84 26 91 38 91 52 L 68 52 Z", 0, None),
                "mouse-scroll" => {
                    add(
                        "M 64 28 L 64 47 M 58 34 L 64 28 L 70 34 M 58 41 L 64 47 L 70 41",
                        0,
                        Some(4.),
                    );
                    add(
                        "M 108 34 L 108 83 M 101 41 L 108 34 L 115 41 M 101 76 L 108 83 L 115 76",
                        0,
                        Some(4.),
                    );
                }
                _ => {
                    add("M 60 24 C 44 26 37 38 37 52 L 60 52 Z", 0, None);
                    if id == "mouse-double" {
                        add(
                            "M 12 13 L 20 22 M 28 7 L 29 18 M 6 31 L 18 34 M 14 57 L 21 49",
                            0,
                            Some(4.),
                        );
                    }
                }
            }
        }
        "drag" => {
            add(
                "M 24 12 L 24 81 L 43 63 L 56 94 L 69 88 L 54 59 L 81 59 Z",
                1,
                None,
            );
            add(
                "M 65 106 L 116 106 M 74 98 L 65 106 L 74 114 M 107 98 L 116 106 L 107 114",
                0,
                Some(5.),
            );
        }
        "key-enter" | "key-tab" | "key-ctrl" | "key-alt" | "key-shift" => {
            add(
                "M 19 32 L 109 32 Q 116 32 116 39 L 116 89 Q 116 96 109 96 L 19 96 Q 12 96 12 89 L 12 39 Q 12 32 19 32 Z",
                0,
                Some(5.),
            );
            if id == "key-enter" {
                add(
                    "M 91 48 L 91 69 L 38 69 M 50 57 L 38 69 L 50 81",
                    1,
                    Some(5.),
                );
            } else if id == "key-tab" {
                add(
                    "M 33 64 L 91 64 M 79 51 L 92 64 L 79 77 M 99 48 L 99 80",
                    1,
                    Some(5.),
                );
            } else {
                let label = match id {
                    "key-ctrl" => "Ctrl",
                    "key-alt" => "Alt",
                    _ => "Shift",
                };
                let face = ttf_parser::Face::parse(
                    crate::text::TextFont::UbuntuLight.bytes(true, false),
                    0,
                )
                .ok()?;
                let glyphs: Vec<_> = label.chars().filter_map(|c| face.glyph_index(c)).collect();
                let units: f64 = glyphs
                    .iter()
                    .map(|g| face.glyph_hor_advance(*g).unwrap_or(0) as f64)
                    .sum();
                let scale = (80. / units).min(30. / face.units_per_em() as f64);
                let mut out = Outline {
                    path: BezPath::new(),
                    x: 64. - units * scale / 2.,
                    scale,
                };
                for g in glyphs {
                    face.outline_glyph(g, &mut out);
                    out.x += face.glyph_hor_advance(g).unwrap_or(0) as f64 * scale;
                }
                icon.paths.push(Path {
                    data: out.path.to_svg(),
                    color: 1,
                    stroke: None,
                });
            }
        }
        _ => return None,
    }
    Some(icon)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instruction_icons_are_bounded_vectors_with_editable_colors() {
        for id in [
            "mouse-left",
            "mouse-right",
            "mouse-double",
            "mouse-scroll",
            "drag",
            "key-enter",
            "key-tab",
            "key-ctrl",
            "key-alt",
            "key-shift",
        ] {
            let v = icon(id).unwrap();
            v.validate().unwrap();
            assert!(v.colors.len() <= 5);
            assert!(v.paths.iter().any(|p| !p.data.is_empty()));
        }
    }
}
