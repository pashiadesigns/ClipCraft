//! Whole-image perspective (FX-003, approved bi-proposal 5): Advanced >
//! Perspective tilts the finished image, annotations included. Objects stay
//! editable underneath; the tilt is applied on top when the image is shown
//! or exported, and removing it lays everything flat again.
use crate::{
    Error, Result,
    bitmap::Homography,
    display::{DisplayList, Viewport},
};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tilt {
    /// Where the image corners land on the canvas, top-left, top-right,
    /// bottom-right, bottom-left, as 0..1 fractions of the canvas.
    pub quad: [[f32; 2]; 4],
    /// Fill around the tilted image; `None` is transparent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<[u8; 4]>,
}
impl Eq for Tilt {}
impl Tilt {
    pub fn validate(&self) -> Result<()> {
        Homography::new(self.quad)
            .map(|_| ())
            .ok_or_else(|| Error::Invalid("Invalid image perspective".into()))
    }
}
fn sample(image: &RgbaImage, p: [f64; 2]) -> [f64; 4] {
    let [w, h] = [image.width() as f64, image.height() as f64];
    let x = (p[0] - 0.5).clamp(0., w - 1.);
    let y = (p[1] - 0.5).clamp(0., h - 1.);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x1, y1) = ((x0 + 1.).min(w - 1.), (y0 + 1.).min(h - 1.));
    let mut out = [0.; 4];
    for (px, py, wt) in [
        (x0, y0, (1. - fx) * (1. - fy)),
        (x1, y0, fx * (1. - fy)),
        (x0, y1, (1. - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        let c = image.get_pixel(px as u32, py as u32);
        let a = f64::from(c[3]) / 255.;
        for i in 0..3 {
            out[i] += f64::from(c[i]) * a * wt;
        }
        out[3] += a * wt;
    }
    out
}
fn put(target: &mut Rgba<u8>, sum: [f64; 4], samples: f64, background: Option<[u8; 4]>) {
    let alpha = sum[3] / samples;
    let mut c = background.unwrap_or([0; 4]);
    if alpha > 0. {
        let rgb = [0, 1, 2].map(|i| sum[i] / sum[3]);
        let bg_a = f64::from(c[3]) / 255. * (1. - alpha);
        let out_a = alpha + bg_a;
        for i in 0..3 {
            c[i] = ((rgb[i] * alpha + f64::from(c[i]) * bg_a) / out_a)
                .round()
                .clamp(0., 255.) as u8;
        }
        c[3] = (out_a * 255.).round().clamp(0., 255.) as u8;
    }
    *target = Rgba(c);
}
const SAMPLES: [[f64; 2]; 4] = [[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]];
/// Tilts a finished full-size image (export, copy, print).
pub fn apply(flat: &RgbaImage, tilt: &Tilt) -> Result<RgbaImage> {
    let h = Homography::new(tilt.quad)
        .ok_or_else(|| Error::Invalid("Invalid image perspective".into()))?;
    let [w, ht] = [flat.width() as f64, flat.height() as f64];
    let mut out = RgbaImage::new(flat.width(), flat.height());
    for (x, y, p) in out.enumerate_pixels_mut() {
        let mut sum = [0.; 4];
        for o in SAMPLES {
            if let Some(s) = h.inverse([(x as f64 + o[0]) / w, (y as f64 + o[1]) / ht]) {
                let c = sample(flat, [s[0] * w, s[1] * ht]);
                for i in 0..4 {
                    sum[i] += c[i];
                }
            }
        }
        put(p, sum, SAMPLES.len() as f64, tilt.background);
    }
    Ok(out)
}
/// Tilts one viewport of the editor display: the flat image is rendered at
/// the viewport's zoom only where the visible area maps back to.
pub(crate) fn viewport(list: &DisplayList, tilt: &Tilt, view: Viewport) -> Result<RgbaImage> {
    let h = Homography::new(tilt.quad)
        .ok_or_else(|| Error::Invalid("Invalid image perspective".into()))?;
    let size = [list.base.width() as f64, list.base.height() as f64];
    let doc = |x: f64, y: f64| {
        [
            (view.origin[0] + x / view.scale) / size[0],
            (view.origin[1] + y / view.scale) / size[1],
        ]
    };
    // Bounds of the flat image that the visible area shows.
    let (mut lo, mut hi) = ([1f64, 1.], [0f64, 0.]);
    let steps = 24;
    for i in 0..=steps {
        for j in 0..=steps {
            let u = doc(
                view.size[0] as f64 * i as f64 / steps as f64,
                view.size[1] as f64 * j as f64 / steps as f64,
            );
            if let Some(s) = h.inverse(u) {
                for k in 0..2 {
                    lo[k] = lo[k].min(s[k]);
                    hi[k] = hi[k].max(s[k]);
                }
            }
        }
    }
    let mut out = RgbaImage::new(view.size[0], view.size[1]);
    if hi[0] <= lo[0] || hi[1] <= lo[1] {
        for p in out.pixels_mut() {
            put(p, [0.; 4], 1., tilt.background);
        }
        return Ok(out);
    }
    // A margin covers the sampling between grid points.
    let pad = 0.05;
    let lo = [0, 1].map(|k| (lo[k] - pad).max(0.));
    let hi = [0, 1].map(|k| (hi[k] + pad).min(1.));
    // Render the flat source at this zoom, but never larger than 4096 px a side.
    let scale = view
        .scale
        .min(4096. / ((hi[0] - lo[0]) * size[0]).max(1.))
        .min(4096. / ((hi[1] - lo[1]) * size[1]).max(1.));
    let origin = [lo[0] * size[0], lo[1] * size[1]];
    let flat_size = [
        (((hi[0] - lo[0]) * size[0] * scale).ceil() as u32).max(1),
        (((hi[1] - lo[1]) * size[1] * scale).ceil() as u32).max(1),
    ];
    let flat = list.render_flat(Viewport {
        size: flat_size,
        origin,
        scale,
    })?;
    for (x, y, p) in out.enumerate_pixels_mut() {
        let mut sum = [0.; 4];
        for o in SAMPLES {
            let u = doc(x as f64 + o[0], y as f64 + o[1]);
            if let Some(s) = h.inverse(u) {
                let c = sample(
                    &flat,
                    [
                        (s[0] * size[0] - origin[0]) * scale,
                        (s[1] * size[1] - origin[1]) * scale,
                    ],
                );
                for i in 0..4 {
                    sum[i] += c[i];
                }
            }
        }
        put(p, sum, SAMPLES.len() as f64, tilt.background);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitmap::Perspective;
    #[test]
    fn tilt_keeps_objects_editable_and_display_matches_export() {
        let mut doc = crate::Document::canvas(200, 120, [255; 4]).unwrap();
        let mut a = crate::templates::text(1, [20., 20., 120., 40.], "Tilt", 24., true);
        a.color = [20, 40, 60, 255];
        doc.annotations.push(a);
        doc.promote_features();
        let flat = doc.render().unwrap();
        let tilt = Tilt {
            quad: Perspective::TiltLeft.quad(0.6, 0.5),
            background: None,
        };
        doc.canvas_behavior.perspective = Some(tilt);
        doc.validate().unwrap();
        let tilted = doc.render().unwrap();
        assert_eq!(
            tilted.dimensions(),
            flat.dimensions(),
            "the canvas keeps its size"
        );
        assert_eq!(
            tilted.get_pixel(1, 1)[3],
            0,
            "outside the tilt is transparent"
        );
        assert_ne!(tilted, flat);
        assert_eq!(doc.annotations.len(), 1, "objects stay editable");
        let shown = doc
            .display_list()
            .unwrap()
            .render_viewport(Viewport {
                size: [200, 120],
                origin: [0.; 2],
                scale: 1.,
            })
            .unwrap();
        let diff = shown
            .pixels()
            .zip(tilted.pixels())
            .map(|(a, b)| (0..4).map(|i| a[i].abs_diff(b[i]) as u32).max().unwrap())
            .max()
            .unwrap();
        assert!(diff <= 3, "display and export match ({diff})");
        let bad = Tilt {
            quad: [[0., 0.], [1., 1.], [1., 0.], [0., 1.]],
            background: None,
        };
        doc.canvas_behavior.perspective = Some(bad);
        assert!(doc.validate().is_err());
        doc.canvas_behavior.perspective = None;
        assert_eq!(
            doc.render().unwrap(),
            flat,
            "removing it lays the image flat"
        );
    }
}
