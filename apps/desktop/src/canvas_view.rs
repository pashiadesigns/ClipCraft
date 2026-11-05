//! Workspace-only transforms. Document commands always use original image pixels.
use eframe::egui::{Pos2, Rect, Vec2, vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomMode {
    ShrinkToFit,
    Fit,
    Manual(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomAction {
    In,
    Out,
    Set(ZoomMode),
}

#[derive(Clone, Copy, Debug)]
pub struct Transform {
    pub image: Rect,
    pub viewport: Rect,
    pub scale: f32,
    pub zoom: f32,
}

impl Transform {
    pub fn image_point(self, screen: Pos2) -> [f32; 2] {
        let p = (screen - self.image.min) / self.scale;
        [p.x, p.y]
    }

    pub fn screen_point(self, image: [f32; 2]) -> Pos2 {
        self.image.min + vec2(image[0], image[1]) * self.scale
    }
}

pub struct CanvasView {
    pub mode: ZoomMode,
    pan: Vec2,
    last: Option<Transform>,
    pub panning: bool,
    pub pan_button: eframe::egui::PointerButton,
    free_pan: bool,
}

impl Default for CanvasView {
    fn default() -> Self {
        Self {
            mode: ZoomMode::ShrinkToFit,
            pan: Vec2::ZERO,
            last: None,
            panning: false,
            pan_button: eframe::egui::PointerButton::Primary,
            free_pan: false,
        }
    }
}

impl CanvasView {
    pub(super) fn current_transform(&self) -> Option<Transform> {
        self.last
    }
    /// Keep the original image in place when a drag pads its canvas on release.
    pub(super) fn retain_expanded_position(
        &mut self,
        old: Transform,
        offset: [i32; 2],
        dimensions: [u32; 2],
    ) {
        self.mode = ZoomMode::Manual(old.zoom);
        self.free_pan = true;
        let min = old.image.min - vec2(offset[0] as f32, offset[1] as f32) * old.scale;
        let size = vec2(dimensions[0] as f32, dimensions[1] as f32) * old.scale;
        self.pan = min + size * 0.5 - old.viewport.center();
        self.last = Some(Transform {
            image: Rect::from_min_size(min, size),
            ..old
        });
    }

    pub fn label(&self) -> String {
        let zoom = match self.mode {
            ZoomMode::Manual(zoom) => zoom,
            _ => self.last.map_or(1.0, |t| t.zoom),
        };
        let percent = zoom * 100.0;
        // A fitted 99.6% view is resampled and must not masquerade as 1:1.
        if (percent - percent.round()).abs() < 0.001 {
            format!("{percent:.0}%")
        } else {
            format!("{percent:.2}%")
        }
    }

    pub fn transform(&mut self, viewport: Rect, dimensions: Vec2, ppp: f32) -> Transform {
        let ppp = ppp.max(0.01);
        let fit = (viewport.width() / dimensions.x)
            .min(viewport.height() / dimensions.y)
            .max(0.0001)
            * ppp;
        let zoom = match self.mode {
            ZoomMode::ShrinkToFit => fit.min(1.0),
            ZoomMode::Fit => fit,
            ZoomMode::Manual(zoom) => zoom.clamp(0.1, 16.0),
        };
        let scale = zoom / ppp;
        let size = dimensions * scale;
        // Middle drag can move a small image too. Keep a small reachable strip
        // in view; Fit/Actual Size remain available to recenter the workspace.
        let limit = if self.free_pan {
            (size + viewport.size()) * 0.5 - (size * 0.5).min(vec2(32.0, 32.0))
        } else {
            ((size - viewport.size()) * 0.5).max(Vec2::ZERO)
        };
        self.pan = if matches!(self.mode, ZoomMode::Manual(_)) {
            self.pan.clamp(-limit, limit)
        } else {
            Vec2::ZERO
        };
        let mut image = Rect::from_center_size(viewport.center() + self.pan, size);
        if zoom >= 1.0 && (zoom - zoom.round()).abs() < 1e-6 {
            // On startup, centering an even-sized image in an odd/fractional
            // viewport lands between physical pixels. Texture filtering then
            // changes its sharpness with the phase of a later zoom/pan. Keep
            // integer zooms pixel-aligned from the first frame, including DPI.
            let physical = image.min.to_vec2() * ppp;
            let aligned = vec2(physical.x.round(), physical.y.round()) / ppp;
            image = Rect::from_min_size(Pos2::ZERO + aligned, size);
        }
        let transform = Transform {
            image,
            viewport,
            scale,
            zoom,
        };
        self.last = Some(transform);
        transform
    }

    pub fn apply(&mut self, action: ZoomAction, anchor: Option<Pos2>) {
        let current = self.last.map_or(1.0, |t| t.zoom);
        let steps = [
            0.1, 0.125, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 8.0, 16.0,
        ];
        self.mode = match action {
            ZoomAction::Set(mode) => mode,
            ZoomAction::In => ZoomMode::Manual(
                steps
                    .into_iter()
                    .find(|step| *step > current + 0.001)
                    .unwrap_or(16.0),
            ),
            ZoomAction::Out => ZoomMode::Manual(
                steps
                    .into_iter()
                    .rev()
                    .find(|step| *step < current - 0.001)
                    .unwrap_or(0.1),
            ),
        };
        if let ZoomMode::Manual(zoom) = self.mode {
            if anchor.is_some() {
                self.free_pan = true;
            }
            let zoom = zoom.clamp(0.1, 16.0);
            self.mode = ZoomMode::Manual(zoom);
            if let Some(old) = self.last {
                let anchor = anchor.unwrap_or_else(|| old.viewport.center());
                // Keep the original image point beneath the zoom anchor. The
                // next transform clamps this when an image edge is reached.
                let center = anchor + (old.image.center() - anchor) * (zoom / old.zoom);
                self.pan = center - old.viewport.center();
            }
        } else {
            self.pan = Vec2::ZERO;
            self.free_pan = false;
        }
    }

    pub fn pan_freely_by(&mut self, delta: Vec2) {
        if let Some(last) = self.last {
            self.mode = ZoomMode::Manual(last.zoom);
            self.free_pan = true;
            self.pan += delta;
        }
    }

    pub fn pan_by(&mut self, delta: Vec2) {
        if let Some(last) = self.last
            && (last.image.width() > last.viewport.width()
                || last.image.height() > last.viewport.height())
        {
            self.free_pan = false;
            self.mode = ZoomMode::Manual(last.zoom);
            self.pan += delta;
        }
    }
}

/// Shared by keyboard handling; exact modifiers prevent a draft or another
/// command from being reinterpreted as a workspace change.
pub fn shortcut(input: &mut eframe::egui::InputState, owns_input: bool) -> Option<ZoomAction> {
    use crate::shortcuts::consume_exact;
    use eframe::egui::{Key, Modifiers};
    if !owns_input {
        return None;
    }
    if consume_exact(input, Modifiers::COMMAND, Key::Plus)
        || consume_exact(input, Modifiers::COMMAND | Modifiers::SHIFT, Key::Plus)
        || consume_exact(input, Modifiers::COMMAND, Key::Equals)
        || consume_exact(input, Modifiers::COMMAND | Modifiers::SHIFT, Key::Equals)
    {
        Some(ZoomAction::In)
    } else if consume_exact(input, Modifiers::COMMAND, Key::Minus) {
        Some(ZoomAction::Out)
    } else if consume_exact(input, Modifiers::COMMAND, Key::Num0) {
        Some(ZoomAction::Set(ZoomMode::Manual(1.0)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use capture_core::{Annotation, Command, Document, Kind, Session};
    use eframe::egui::pos2;

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(30.0, 70.0), vec2(600.0, 400.0))
    }

    #[test]
    fn automatic_fit_near_actual_size_does_not_claim_one_to_one() {
        let mut view = CanvasView::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, vec2(996.0, 700.0));
        view.transform(viewport, vec2(1000.0, 600.0), 1.0);
        assert_eq!(view.label(), "99.60%");
        view.apply(ZoomAction::Set(ZoomMode::Manual(1.0)), None);
        assert_eq!(view.label(), "100%");
    }

    #[test]
    fn actual_size_means_physical_pixels_and_fit_modes_reflow() {
        for ppp in [1.0, 1.25, 2.0] {
            let mut view = CanvasView::default();
            view.apply(ZoomAction::Set(ZoomMode::Manual(1.0)), None);
            let actual = view.transform(viewport(), vec2(100.0, 80.0), ppp);
            assert_eq!(actual.image.width() * ppp, 100.0);
            view.apply(ZoomAction::Set(ZoomMode::Fit), None);
            let fit = view.transform(viewport(), vec2(100.0, 80.0), ppp);
            assert_eq!(fit.image.height(), 400.0);
            view.apply(ZoomAction::Set(ZoomMode::ShrinkToFit), None);
            assert_eq!(view.transform(viewport(), vec2(100.0, 80.0), ppp).zoom, 1.0);
            let narrow = Rect::from_min_size(pos2(-50.0, 20.0), vec2(120.0, 80.0));
            assert!(
                view.transform(narrow, vec2(2000.0, 1000.0), ppp)
                    .image
                    .width()
                    <= 120.001
            );
        }
    }

    #[test]
    fn anchored_zoom_roundtrips_and_pan_is_bounded_after_resize() {
        let mut view = CanvasView::default();
        let dimensions = vec2(960.0, 600.0);
        let first = view.transform(viewport(), dimensions, 1.25);
        let anchor = pos2(400.0, 250.0);
        let image_point = first.image_point(anchor);
        view.apply(ZoomAction::Set(ZoomMode::Manual(3.0)), Some(anchor));
        let zoomed = view.transform(viewport(), dimensions, 1.25);
        // Pixel alignment may adjust each screen axis by at most half a pixel.
        let error = (zoomed.screen_point(image_point) - anchor) * 1.25;
        assert!(error.x.abs() <= 0.5001 && error.y.abs() <= 0.5001);
        view.pan_by(vec2(1e6, -1e6));
        let panned = view.transform(viewport(), dimensions, 1.25);
        assert!((panned.image.left() - viewport().left()).abs() * 1.25 <= 0.5001);
        assert!((panned.image.bottom() - viewport().bottom()).abs() * 1.25 <= 0.5001);
        let larger = Rect::from_min_size(pos2(-300.0, -200.0), vec2(3000.0, 2000.0));
        let resized = view.transform(larger, dimensions, 1.25);
        assert_eq!(resized.image.center(), larger.center());
        view.apply(ZoomAction::Set(ZoomMode::ShrinkToFit), None);
        assert_eq!(
            view.transform(viewport(), dimensions, 1.25).image.center(),
            viewport().center()
        );
    }

    #[test]
    fn editing_at_multiple_zooms_and_pan_preserves_document_pixels_and_undo() {
        let mut expected = None;
        for zoom in [0.25, 1.0, 2.0, 16.0] {
            let mut view = CanvasView::default();
            view.apply(ZoomAction::Set(ZoomMode::Manual(zoom)), None);
            view.transform(viewport(), vec2(960.0, 600.0), 1.25);
            view.pan_by(vec2(35.0, -50.0));
            let transform = view.transform(viewport(), vec2(960.0, 600.0), 1.25);
            let mut session = Session::new(Document::blank(960, 600).unwrap()).unwrap();
            let clean = session.document.render().unwrap();
            session
                .execute(Command::Add {
                    annotation: Annotation {
                        image: None,
                        step_outline: None,
                        line_style: None,
                        shadow: None,

                        focus: None,

                        highlighter_rect: false,
                        group: None,
                        rotation_degrees: 0.0,
                        mirror_x: false,
                        id: 1,
                        kind: Kind::Rectangle,
                        start: transform.image_point(transform.screen_point([180.0, 160.0])),
                        end: transform.image_point(transform.screen_point([380.0, 290.0])),
                        color: [255, 0, 0, 255],
                        width: 3,
                        text: None,
                        corner_radius: 0.0,
                        fill: None,
                        points: Vec::new(),
                        callout: None,
                    },
                })
                .unwrap();
            let image = session.document.render().unwrap();
            if let Some(expected) = &expected {
                assert_eq!(&image, expected);
            } else {
                expected = Some(image.clone());
            }
            assert!(session.document.hit_test([180.0, 220.0], 1.0).is_some());
            session.execute(Command::Undo).unwrap();
            assert_eq!(session.document.render().unwrap(), clean);
            assert!(!session.is_modified());
            session.execute(Command::Redo).unwrap();
            assert_eq!(session.document.render().unwrap(), image);
        }
    }

    #[test]
    fn zoom_keys_respect_modal_input_modifiers_and_repeat() {
        use eframe::egui::{Event, InputState, Key, Modifiers};
        let mut input = InputState::default();
        let event = |key, modifiers, repeat| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        };
        let ctrl = Modifiers::CTRL | Modifiers::COMMAND;
        input
            .events
            .push(event(Key::Plus, ctrl | Modifiers::SHIFT, false));
        assert_eq!(shortcut(&mut input, false), None);
        assert_eq!(input.events.len(), 1);
        assert_eq!(shortcut(&mut input, true), Some(ZoomAction::In));
        input
            .events
            .push(event(Key::Minus, ctrl | Modifiers::ALT, false));
        assert_eq!(shortcut(&mut input, true), None);
        input.events.clear();
        input.events.push(event(Key::Num0, ctrl, true));
        assert_eq!(shortcut(&mut input, true), None);
        input.events.push(event(Key::Num0, ctrl, false));
        assert_eq!(
            shortcut(&mut input, true),
            Some(ZoomAction::Set(ZoomMode::Manual(1.0)))
        );
    }
}
