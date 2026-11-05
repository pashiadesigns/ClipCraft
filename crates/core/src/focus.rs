//! Editable focus windows affect only the already-composited layers beneath them.
//! This is an emphasis effect, not a redaction or sensitive-data protection tool.
use crate::{Annotation, Error, Kind, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Focus {
    pub blur: f32,
    pub darken: f32,
    pub opacity: f32,
}
impl Default for Focus {
    fn default() -> Self {
        Self {
            blur: 0.,
            darken: 0.6,
            opacity: 1.,
        }
    }
}
impl Focus {
    pub fn validate(&self, kind: Kind) -> Result<()> {
        if !matches!(kind, Kind::Rectangle | Kind::Ellipse)
            || !self.blur.is_finite()
            || !(0. ..=32.).contains(&self.blur)
            || !self.darken.is_finite()
            || !(0. ..=1.).contains(&self.darken)
            || !self.opacity.is_finite()
            || !(0. ..=1.).contains(&self.opacity)
        {
            return Err(Error::Invalid(
                "Focus requires a rectangle or circle, blur 0–32 and strength 0–100%.".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn render(image: &mut RgbaImage, a: &Annotation) {
    let Some(focus) = a.focus else { return };
    let blurred = (focus.blur > 0.).then(|| image::imageops::fast_blur(image, focus.blur));
    let (lo, hi) = a.body_bounds();
    let extent = [hi[0] - lo[0], hi[1] - lo[1]];
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let (sin, cos) = (-a.rotation_degrees as f64).to_radians().sin_cos();
    for (x, y, p) in image.enumerate_pixels_mut() {
        let dx = x as f64 + 0.5 - center[0] as f64;
        let dy = y as f64 + 0.5 - center[1] as f64;
        let local = [
            dx * cos - dy * sin + extent[0] as f64 * 0.5,
            dx * sin + dy * cos + extent[1] as f64 * 0.5,
        ];
        let inside = if local[0] < 0.
            || local[1] < 0.
            || local[0] >= extent[0] as f64
            || local[1] >= extent[1] as f64
        {
            0.
        } else {
            crate::image_frame::coverage(a, local, extent, 1.) as f32
        };
        let mix = (1. - inside) * focus.opacity;
        if mix <= 0. {
            continue;
        }
        let source = blurred.as_ref().map_or(*p, |b| *b.get_pixel(x, y));
        for c in 0..3 {
            p[c] = (p[c] as f32 * (1. - mix) + source[c] as f32 * (1. - focus.darken) * mix).round()
                as u8;
        }
        p[3] = (p[3] as f32 * (1. - mix) + source[3] as f32 * mix).round() as u8;
    }
    if a.width > 0 && a.color[3] > 0 {
        let mut outline = a.clone();
        outline.focus = None;
        outline.fill = None;
        outline.text = None;
        outline.shadow = None;
        crate::render_annotation(image, &outline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_preserves_center_and_survives_undo_and_save() {
        let mut d = crate::Document::blank(160, 100).unwrap();
        let mut a =
            crate::steps::marker(1, [80., 50.], 40, [0; 4], (1, false), [160, 100]).unwrap();
        a.kind = Kind::Rectangle;
        a.start = [40., 25.];
        a.end = [120., 75.];
        a.text = None;
        a.fill = None;
        a.focus = Some(Focus::default());
        let mut session = crate::Session::new(d.clone()).unwrap();
        session
            .execute(crate::Command::Add {
                annotation: a.clone(),
            })
            .unwrap();
        d = session.document.clone();
        let mut pixels = RgbaImage::from_pixel(160, 100, image::Rgba([200, 150, 100, 255]));
        render(&mut pixels, &a);
        assert_eq!(pixels.get_pixel(80, 50).0, [200, 150, 100, 255]);
        assert_eq!(pixels.get_pixel(5, 5).0, [80, 60, 40, 255]);
        let json = serde_json::to_string(&d).unwrap();
        let restored: crate::Document = serde_json::from_str(&json).unwrap();
        restored.validate().unwrap();
        assert_eq!(restored.annotations[0].focus, a.focus);
        session.execute(crate::Command::Undo).unwrap();
        assert!(session.document.annotations.is_empty());
    }

    #[test]
    fn focus_image_video_display_and_export_respect_layer_order_and_timing() {
        use crate::video::{
            editor::{Animation, Project, Timing},
            frame,
            links::Layer,
        };
        let mut d = crate::Document::canvas(160, 100, [200, 150, 100, 255]).unwrap();
        let mut window =
            crate::steps::marker(1, [80., 50.], 40, [0; 4], (1, false), [160, 100]).unwrap();
        window.kind = Kind::Rectangle;
        window.start = [40., 25.];
        window.end = [120., 75.];
        window.text = None;
        window.width = 0;
        window.fill = None;
        window.focus = Some(Focus::default());
        let mut upper = window.clone();
        upper.id = 2;
        upper.focus = None;
        upper.width = 1;
        upper.start = [5., 5.];
        upper.end = [25., 20.];
        upper.color = [240, 60, 30, 255];
        upper.fill = Some(upper.color);
        d.annotations = vec![window.clone(), upper.clone()];
        let expected = d.render().unwrap();
        assert_eq!(expected.get_pixel(10, 10).0, upper.color);
        assert_eq!(expected.get_pixel(80, 50).0, [200, 150, 100, 255]);
        assert_eq!(expected.get_pixel(145, 85).0, [80, 60, 40, 255]);
        let displayed = d
            .display_list()
            .unwrap()
            .render_viewport(crate::display::Viewport {
                size: [160, 100],
                origin: [0., 0.],
                scale: 1.,
            })
            .unwrap();
        assert_eq!(displayed, expected);
        let style =
            crate::object_styles::ObjectStyle::from_annotation("Focus preset".into(), &window)
                .unwrap();
        let style: crate::object_styles::ObjectStyle =
            serde_json::from_str(&serde_json::to_string(&style).unwrap()).unwrap();
        assert_eq!(style.apply(&window).unwrap(), window);
        let painted = crate::object_styles::paint_format(&upper, &window).unwrap();
        assert!(painted.focus.is_some() && painted.fill.is_none() && painted.text.is_none());

        let mut project = Project::new("C:/local-video.mp4".into(), [160, 100], 2000, 30);
        project.background = [200, 150, 100, 255];
        project.tracks = vec![1, 2];
        project.layer_order = vec![Layer::Annotation(2), Layer::Annotation(1), Layer::Media(1)];
        let mut timing = Timing::new(1, 400, 1400);
        timing.entrance = Animation::Fade;
        timing.entrance_ms = 400;
        timing.smooth = false;
        project.objects.insert(1, timing);
        project.objects.insert(2, Timing::new(2, 0, 2000));
        d.video = Some(project);
        let mut cache = frame::CompositeCache::default();
        for (time, outside) in [(0, 200), (600, 140), (1000, 80), (1500, 200)] {
            let actual = frame::composite(&d, &[], time).unwrap();
            assert_eq!(cache.composite(&d, &[], time).unwrap(), actual);
            assert_eq!(actual.get_pixel(145, 85)[0], outside);
            assert_eq!(actual.get_pixel(10, 10).0, upper.color);
            assert_eq!(actual.get_pixel(80, 50).0, [200, 150, 100, 255]);
        }
        // Moving Focus above the other annotation changes only lower artwork.
        d.video.as_mut().unwrap().layer_order.swap(0, 1);
        assert_eq!(
            frame::composite(&d, &[], 1000).unwrap().get_pixel(10, 10).0,
            [96, 24, 12, 255]
        );
    }

    #[test]
    fn focus_blur_preserves_center_and_reduces_outside_detail() {
        let mut image = RgbaImage::from_fn(100, 100, |x, _| {
            image::Rgba(if x % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] })
        });
        let mut a =
            crate::steps::marker(1, [50., 50.], 40, [0; 4], (1, false), [100, 100]).unwrap();
        a.kind = Kind::Ellipse;
        a.text = None;
        a.width = 0;
        a.fill = None;
        a.focus = Some(Focus {
            blur: 4.,
            darken: 0.,
            opacity: 1.,
        });
        render(&mut image, &a);
        assert_eq!(image.get_pixel(50, 50).0, [255; 4]);
        assert_eq!(image.get_pixel(51, 50).0, [0, 0, 0, 255]);
        assert!(image.get_pixel(10, 10)[0].abs_diff(image.get_pixel(11, 10)[0]) < 10);
    }
}
