//! Editable focus windows affect only the already-composited layers beneath them.
//! This is an emphasis effect, not a redaction or sensitive-data protection tool.
use crate::{Annotation, Error, Kind, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Focus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magnify: Option<crate::magnified::Magnified>,
    pub blur: f32,
    pub darken: f32,
    pub opacity: f32,
}
impl Default for Focus {
    fn default() -> Self {
        Self {
            magnify: None,
            blur: 0.,
            darken: 0.6,
            opacity: 1.,
        }
    }
}
impl Focus {
    pub fn validate(&self, kind: Kind) -> Result<()> {
        if let Some(m) = self.magnify {
            m.validate()?;
        }
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
/// Does this Focus blur or darken what lies outside its window?
pub fn dims(a: &Annotation) -> bool {
    a.focus.is_some_and(|f| f.blur > 0. || f.darken > 0.)
}
/// The same Focus without its own backdrop: only its window, Magnify view
/// and outline. Later members of a shared backdrop draw like this.
pub fn quiet(a: &Annotation) -> Annotation {
    let mut q = a.clone();
    if let Some(f) = q.focus.as_mut() {
        f.blur = 0.;
        f.darken = 0.;
    }
    q
}
/// Focus effects drawn together share one backdrop (BJ): the blur and darken
/// are applied once, at the lowest Focus, outside every member's window, with
/// the strongest settings among them. Each member keeps its window clear and
/// draws its Magnify view and outline at its own layer position, so any
/// number of highlights and zooms can be placed without dimming each other.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Backdrop {
    members: Vec<Annotation>,
}
impl Backdrop {
    /// The backdrop shared by the Focus effects in `list` (objects visible together).
    pub fn new(list: &[Annotation]) -> Self {
        let members: Vec<Annotation> = list.iter().filter(|a| dims(a)).cloned().collect();
        // A single Focus draws exactly as before and keeps nothing here, so
        // cached display patches still survive dragging it.
        Self {
            members: if members.len() > 1 { members } else { vec![] },
        }
    }
    pub fn members(&self) -> &[Annotation] {
        &self.members
    }
    fn shared(&self) -> bool {
        self.members.len() > 1
    }
    pub fn lead(&self) -> Option<u64> {
        self.members.first().map(|a| a.id)
    }
    /// A member that draws no backdrop of its own because an earlier one does.
    pub fn is_quiet(&self, a: &Annotation) -> bool {
        self.shared() && dims(a) && self.lead() != Some(a.id)
    }
    /// The settings the backdrop uses: the strongest among its members.
    pub fn settings(&self, a: &Annotation) -> Option<Focus> {
        let mut f = a.focus?;
        if self.shared() && self.lead() == Some(a.id) {
            for m in self.members.iter().filter_map(|m| m.focus) {
                f.blur = f.blur.max(m.blur);
                f.darken = f.darken.max(m.darken);
                f.opacity = f.opacity.max(m.opacity);
            }
        }
        Some(f)
    }
    /// Draws `a` if it is a Focus and returns true; other objects return false.
    pub fn render(&self, image: &mut RgbaImage, a: &Annotation) -> bool {
        if a.focus.is_none() {
            return false;
        }
        if self.is_quiet(a) {
            render(image, &quiet(a));
        } else if self.shared() && self.lead() == Some(a.id) {
            let others: Vec<&Annotation> = self.members.iter().filter(|m| m.id != a.id).collect();
            render_with(image, a, self.settings(a).unwrap(), &others);
        } else {
            render(image, a);
        }
        true
    }
}
pub(crate) fn render(image: &mut RgbaImage, a: &Annotation) {
    if let Some(focus) = a.focus {
        render_with(image, a, focus, &[]);
    }
}
/// Draws Focus `a` with `focus` settings; `others` are further windows kept clear.
fn render_with(image: &mut RgbaImage, a: &Annotation, focus: Focus, others: &[&Annotation]) {
    let source = focus.magnify.map(|_| image.clone());
    if focus.blur == 0. && focus.darken == 0. {
        if let Some(source) = &source {
            crate::magnified::render(image, source, a);
        }
        let mut outline = a.clone();
        outline.focus = None;
        outline.fill = None;
        outline.text = None;
        outline.shadow = None;
        crate::render_annotation(image, &outline);
        return;
    }
    let blurred = (focus.blur > 0.).then(|| image::imageops::fast_blur(image, focus.blur));
    struct Window<'a> {
        a: &'a Annotation,
        extent: [f32; 2],
        center: [f32; 2],
        sin: f64,
        cos: f64,
    }
    let windows: Vec<Window> = std::iter::once(a)
        .chain(others.iter().copied())
        .map(|w| {
            let (lo, hi) = w.body_bounds();
            let (sin, cos) = (-w.rotation_degrees as f64).to_radians().sin_cos();
            Window {
                a: w,
                extent: [hi[0] - lo[0], hi[1] - lo[1]],
                center: [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5],
                sin,
                cos,
            }
        })
        .collect();
    for (x, y, p) in image.enumerate_pixels_mut() {
        let mut inside = 0f32;
        for w in &windows {
            let dx = x as f64 + 0.5 - w.center[0] as f64;
            let dy = y as f64 + 0.5 - w.center[1] as f64;
            let local = [
                dx * w.cos - dy * w.sin + w.extent[0] as f64 * 0.5,
                dx * w.sin + dy * w.cos + w.extent[1] as f64 * 0.5,
            ];
            if local[0] < 0.
                || local[1] < 0.
                || local[0] >= w.extent[0] as f64
                || local[1] >= w.extent[1] as f64
            {
                continue;
            }
            inside = inside.max(crate::image_frame::coverage(w.a, local, w.extent, 1.) as f32);
            if inside >= 1. {
                break;
            }
        }
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
    if let Some(source) = source {
        crate::magnified::render(image, &source, a);
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

    /// BJ: several Focus windows share one backdrop: every window stays
    /// clear, the outside is darkened once (not once per Focus), and the
    /// canvas display and export match.
    #[test]
    fn several_focus_windows_share_one_backdrop() {
        let window = |id: u64, start: [f32; 2]| {
            let mut a =
                crate::steps::marker(1, [80., 50.], 40, [0; 4], (1, false), [400, 200]).unwrap();
            a.id = id;
            a.kind = Kind::Rectangle;
            a.text = None;
            a.width = 0;
            a.fill = None;
            a.start = start;
            a.end = [start[0] + 60., start[1] + 40.];
            a.focus = Some(Focus::default());
            a
        };
        let bg = [200, 150, 100, 255];
        for count in [2usize, 3] {
            let mut d = crate::Document::canvas(400, 200, bg).unwrap();
            for i in 0..count {
                d.annotations
                    .push(window(i as u64 + 1, [20. + i as f32 * 120., 40.]));
            }
            let out = d.render().unwrap();
            for i in 0..count {
                let c = [50 + i as u32 * 120, 60];
                assert_eq!(
                    out.get_pixel(c[0], c[1]).0,
                    bg,
                    "window {i} of {count} stays clear"
                );
            }
            // Darkened once: 40 % of the colour, not 16 % or 6 %.
            assert_eq!(
                out.get_pixel(390, 190).0,
                [80, 60, 40, 255],
                "{count} windows"
            );
            let shown = d
                .display_list()
                .unwrap()
                .render_viewport(crate::display::Viewport {
                    size: [400, 200],
                    origin: [0., 0.],
                    scale: 1.,
                })
                .unwrap();
            assert_eq!(shown, out, "display matches export with {count} windows");
        }
        // The strongest setting among the members applies.
        let mut d = crate::Document::canvas(400, 200, bg).unwrap();
        let mut soft = window(1, [20., 40.]);
        soft.focus.as_mut().unwrap().darken = 0.2;
        d.annotations = vec![soft, window(2, [200., 40.])];
        assert_eq!(d.render().unwrap().get_pixel(390, 190).0, [80, 60, 40, 255]);
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
            magnify: None,
        });
        render(&mut image, &a);
        assert_eq!(image.get_pixel(50, 50).0, [255; 4]);
        assert_eq!(image.get_pixel(51, 50).0, [0, 0, 0, 255]);
        assert!(image.get_pixel(10, 10)[0].abs_diff(image.get_pixel(11, 10)[0]) < 10);
    }
}
