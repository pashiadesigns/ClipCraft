//! Filters, color, border and edge (FX-001, FX-002, approved mockup
//! `artifacts/br-proposal/roadmap-ui.html`). Settings are stored and applied
//! when the picture is shown or exported, so the original pixels are kept.
use crate::cut_out::EdgeShape;
use crate::{Error, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

/// Each setting runs from -100 to 100; zero leaves the picture alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorAdjust {
    pub brightness: i8,
    pub contrast: i8,
    pub saturation: i8,
    pub warmth: i8,
}
impl ColorAdjust {
    /// Filters are starting points: each one only sets the sliders.
    pub const LOOKS: [(&'static str, Self); 6] = [
        ("Original", Self::new(0, 0, 0, 0)),
        ("Vivid", Self::new(5, 20, 40, 0)),
        ("Muted", Self::new(0, -10, -45, 0)),
        ("Warm", Self::new(5, 0, 10, 45)),
        ("Cool", Self::new(0, 5, 0, -45)),
        ("Mono", Self::new(0, 15, -100, 0)),
    ];
    const fn new(brightness: i8, contrast: i8, saturation: i8, warmth: i8) -> Self {
        Self {
            brightness,
            contrast,
            saturation,
            warmth,
        }
    }
    pub fn is_neutral(self) -> bool {
        self == Self::default()
    }
    fn validate(self) -> Result<()> {
        if [self.brightness, self.contrast, self.saturation, self.warmth]
            .iter()
            .any(|v| !(-100..=100).contains(v))
        {
            return Err(Error::Invalid(
                "Color settings run from -100 to 100.".into(),
            ));
        }
        Ok(())
    }
    fn table(self) -> [u8; 256] {
        let bright = 1. + f32::from(self.brightness) / 200.;
        let contrast = 1. + f32::from(self.contrast) / 150.;
        std::array::from_fn(|i| {
            let v = i as f32 * bright;
            ((v - 128.) * contrast + 128.).round().clamp(0., 255.) as u8
        })
    }
    fn pixel(self, table: &[u8; 256], p: &mut [u8; 4]) {
        let mut c = [0, 1, 2].map(|i| f32::from(table[usize::from(p[i])]));
        if self.saturation != 0 {
            let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            let amount = 1. + f32::from(self.saturation) / 100.;
            c = c.map(|v| luma + (v - luma) * amount);
        }
        let warm = f32::from(self.warmth) * 0.35;
        c[0] += warm;
        c[2] -= warm;
        for i in 0..3 {
            p[i] = c[i].round().clamp(0., 255.) as u8;
        }
    }
    pub fn apply(self, image: &mut RgbaImage) {
        if self.is_neutral() {
            return;
        }
        let table = self.table();
        for p in image.pixels_mut() {
            self.pixel(&table, &mut p.0);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Border {
    pub color: [u8; 4],
    pub width: f32,
    pub corners: f32,
}
impl Default for Border {
    fn default() -> Self {
        Self {
            color: [255; 4],
            width: 0.,
            corners: 0.,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeStyle {
    #[default]
    None,
    SoftFade,
    Torn,
    Wave,
    Zigzag,
}
impl EdgeStyle {
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::None, "None"),
        (Self::SoftFade, "Soft fade"),
        (Self::Torn, "Torn"),
        (Self::Wave, "Wave"),
        (Self::Zigzag, "Zigzag"),
    ];
    /// The same shapes as Cut Out, so they look alike in both places.
    fn shape(self) -> Option<EdgeShape> {
        match self {
            Self::Torn => Some(EdgeShape::Torn),
            Self::Wave => Some(EdgeShape::Wave),
            Self::Zigzag => Some(EdgeShape::Zigzag),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edge {
    pub style: EdgeStyle,
    /// 0 to 100.
    pub strength: u8,
    /// Top, right, bottom, left.
    pub sides: [bool; 4],
}
impl Default for Edge {
    fn default() -> Self {
        Self {
            style: EdgeStyle::None,
            strength: 40,
            sides: [true; 4],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Finish {
    pub color: ColorAdjust,
    pub border: Border,
    pub edge: Edge,
}
impl Eq for Finish {}
impl Finish {
    pub fn is_plain(&self) -> bool {
        self.color.is_neutral()
            && self.border.width == 0.
            && self.border.corners == 0.
            && (self.edge.style == EdgeStyle::None || self.edge.strength == 0)
    }
    pub fn validate(&self) -> Result<()> {
        self.color.validate()?;
        let ok = |v: f32, max: f32| v.is_finite() && (0.0..=max).contains(&v);
        if !ok(self.border.width, 256.)
            || !ok(self.border.corners, 4096.)
            || self.edge.strength > 100
        {
            return Err(Error::Invalid(
                "Unsupported border or edge settings.".into(),
            ));
        }
        Ok(())
    }
    /// How much of a pixel the edge shape keeps. `d` is the distance in from
    /// each side and `along` the position along it, in picture units.
    fn edge_mask(&self, d: [f32; 4], along: [f32; 4], size: [f32; 2], scale: f32) -> f32 {
        if self.edge.style == EdgeStyle::None || self.edge.strength == 0 {
            return 1.;
        }
        let strength = f32::from(self.edge.strength) / 100.;
        let mut keep = 1f32;
        for side in 0..4 {
            if !self.edge.sides[side] {
                continue;
            }
            keep *= match self.edge.style.shape() {
                None => {
                    let range = strength * size[0].min(size[1]) * 0.25;
                    (d[side] / range.max(0.001)).clamp(0., 1.)
                }
                Some(shape) => {
                    let swing = strength * 8.;
                    let line = swing * (1. + shape.offset(along[side]));
                    ((d[side] - line) * scale + 0.5).clamp(0., 1.)
                }
            };
        }
        keep
    }
    /// Finishes the part of `image` covered by `frame` (x, y, width, height in
    /// pixels). `scale` is pixels per picture unit, so border widths and edge
    /// shapes keep their size at any zoom or source resolution.
    pub fn apply(&self, image: &mut RgbaImage, frame: [f32; 4], scale: f32) {
        let scale = scale.max(0.0001);
        let table = self.color.table();
        let size = [frame[2] / scale, frame[3] / scale];
        let radius = self.border.corners.min(size[0].min(size[1]) * 0.5);
        let width = self.border.width;
        let shaped = radius > 0. || width > 0. || self.edge.style != EdgeStyle::None;
        let (columns, rows) = image.dimensions();
        let left = frame[0].floor().clamp(0., columns as f32) as u32;
        let top = frame[1].floor().clamp(0., rows as f32) as u32;
        let right = (frame[0] + frame[2]).ceil().clamp(0., columns as f32) as u32;
        let bottom = (frame[1] + frame[3]).ceil().clamp(0., rows as f32) as u32;
        for y in top..bottom {
            for x in left..right {
                let pixel = image.get_pixel_mut(x, y);
                if !self.color.is_neutral() {
                    self.color.pixel(&table, &mut pixel.0);
                }
                if !shaped {
                    continue;
                }
                let u = (x as f32 + 0.5 - frame[0]) / scale;
                let v = (y as f32 + 0.5 - frame[1]) / scale;
                // Signed distance to the rounded outline; negative is inside.
                let q = [
                    (u - size[0] * 0.5).abs() - (size[0] * 0.5 - radius),
                    (v - size[1] * 0.5).abs() - (size[1] * 0.5 - radius),
                ];
                let outside = q[0].max(0.).hypot(q[1].max(0.));
                let distance = outside + q[0].max(q[1]).min(0.) - radius;
                if width > 0. {
                    let band = ((distance + width) * scale + 0.5).clamp(0., 1.);
                    if band > 0. {
                        let mut ink = self.border.color;
                        ink[3] = (f32::from(ink[3]) * band).round() as u8;
                        crate::text::composite(pixel, ink);
                    }
                }
                let cover = (0.5 - distance * scale).clamp(0., 1.);
                let keep =
                    self.edge_mask([v, size[0] - u, size[1] - v, u], [u, v, u, v], size, scale);
                pixel[3] = (f32::from(pixel[3]) * cover * keep).round() as u8;
                if pixel[3] == 0 {
                    *pixel = image::Rgba([0; 4]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Command, Document, Session};
    use image::Rgba;
    fn sample() -> RgbaImage {
        RgbaImage::from_fn(100, 80, |x, y| {
            Rgba([(x * 2) as u8 + 20, (y * 2) as u8 + 30, 120, 255])
        })
    }
    #[test]
    fn neutral_settings_change_nothing_and_filters_only_set_sliders() {
        let mut image = sample();
        Finish::default().apply(&mut image, [0., 0., 100., 80.], 1.);
        assert_eq!(image, sample());
        assert!(Finish::default().is_plain());
        let mono = ColorAdjust::LOOKS[5].1;
        let mut image = sample();
        mono.apply(&mut image);
        for p in image.pixels() {
            assert!(
                p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1,
                "{p:?}"
            );
        }
        let warm = ColorAdjust::LOOKS[3].1;
        let mut image = sample();
        warm.apply(&mut image);
        assert!(image.get_pixel(30, 30)[0] > sample().get_pixel(30, 30)[0]);
        assert!(image.get_pixel(30, 30)[2] < sample().get_pixel(30, 30)[2]);
    }
    #[test]
    fn border_corners_and_edges_land_where_expected_at_any_scale() {
        let finish = Finish {
            border: Border {
                color: [255, 0, 0, 255],
                width: 4.,
                corners: 10.,
            },
            ..Default::default()
        };
        for scale in [1f32, 2.] {
            let mut image = RgbaImage::from_pixel(
                (100. * scale) as u32,
                (80. * scale) as u32,
                Rgba([0, 0, 255, 255]),
            );
            let frame = [0., 0., 100. * scale, 80. * scale];
            finish.apply(&mut image, frame, scale);
            let at = |x: f32, y: f32| image.get_pixel((x * scale) as u32, (y * scale) as u32).0;
            assert_eq!(at(0., 0.)[3], 0, "the corner is cut away");
            assert_eq!(at(50., 2.), [255, 0, 0, 255]);
            assert_eq!(at(50., 40.), [0, 0, 255, 255]);
        }
        let fade = Finish {
            edge: Edge {
                style: EdgeStyle::SoftFade,
                strength: 100,
                sides: [false, false, false, true],
            },
            ..Default::default()
        };
        let mut image = sample();
        fade.apply(&mut image, [0., 0., 100., 80.], 1.);
        assert!(image.get_pixel(1, 40)[3] < 20);
        assert_eq!(
            image.get_pixel(99, 40)[3],
            255,
            "the right side is not picked"
        );
        for style in [EdgeStyle::Torn, EdgeStyle::Wave, EdgeStyle::Zigzag] {
            let shaped = Finish {
                edge: Edge {
                    style,
                    strength: 100,
                    sides: [true; 4],
                },
                ..Default::default()
            };
            let mut image = sample();
            shaped.apply(&mut image, [0., 0., 100., 80.], 1.);
            assert_eq!(image.get_pixel(50, 40)[3], 255, "{style:?}");
            let top: Vec<_> = (0..100).map(|x| image.get_pixel(x, 6)[3]).collect();
            assert!(top.contains(&0) && top.contains(&255), "{style:?}");
        }
    }
    #[test]
    fn a_whole_picture_finish_previews_exports_and_undoes_as_one_step() {
        let doc = Document::from_image(sample()).unwrap();
        let mut session = Session::new(doc).unwrap();
        let before = session.document.render().unwrap();
        let finish = Finish {
            color: ColorAdjust::LOOKS[1].1,
            border: Border {
                width: 3.,
                corners: 6.,
                ..Default::default()
            },
            edge: Edge::default(),
        };
        session
            .execute(Command::SetFinish {
                finish: Some(finish),
            })
            .unwrap();
        let exported = session.document.render().unwrap();
        assert_ne!(exported, before);
        let shown = session
            .document
            .display_list()
            .unwrap()
            .render_viewport(crate::display::Viewport {
                size: [100, 80],
                origin: [0., 0.],
                scale: 1.,
            })
            .unwrap();
        let worst = exported
            .as_raw()
            .iter()
            .zip(shown.as_raw())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 1, "{worst}");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("finished.iclip");
        session.save(&path).unwrap();
        assert_eq!(Document::load(&path).unwrap().render().unwrap(), exported);
        session.execute(Command::Undo).unwrap();
        assert_eq!(session.document.render().unwrap(), before);
        assert!(!session.can_undo());
    }
    #[test]
    fn an_image_object_finish_keeps_its_original_pixels() {
        let source = sample();
        let raster = std::sync::Arc::new(crate::RasterImage::from_image(&source).unwrap());
        let bitmap = crate::bitmap::Bitmap::new(raster).unwrap();
        let finish = Finish {
            border: Border {
                color: [255, 0, 0, 255],
                width: 3.,
                corners: 0.,
            },
            ..Default::default()
        };
        let finished = std::sync::Arc::new(bitmap.with_finish(Some(finish)));
        let a = finished.clone().annotation(1);
        let mut doc = Document::blank(100, 80).unwrap();
        doc.annotations.push(a.clone());
        let image = doc.render().unwrap();
        assert_eq!(image.get_pixel(50, 1).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(50, 40), source.get_pixel(50, 40));
        assert_eq!(finished.pixels().unwrap().as_ref(), &source);
        assert_eq!(finished.with_finish(None), bitmap);
    }
}
