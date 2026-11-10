//! Perspective controls (FX-003, approved bi-proposal 5): a Perspective
//! group for image objects with presets, Angle, Depth and Free corner
//! handles on the canvas.
use crate::tips::Tip as _;
use crate::{Annotation, Editor, Kind, egui, property_layout};
use capture_core::bitmap::{FLAT, Perspective};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Flat,
    Preset(Perspective),
    Free,
}
#[derive(Clone, Copy)]
struct State {
    mode: Mode,
    angle: f32,
    depth: f32,
}
impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Flat,
            angle: 0.45,
            depth: 0.55,
        }
    }
}
fn state_id(id: u64) -> egui::Id {
    egui::Id::unique(("perspective-state", id))
}
fn free_id() -> egui::Id {
    egui::Id::unique("perspective-free")
}
/// The image object whose Perspective controls are in use, so it shows tilted
/// while it is selected. Any other selected tilted image shows flat for edits.
fn adjusting_id() -> egui::Id {
    egui::Id::unique("perspective-adjusting")
}
/// A Help picture of a preset with its Angle and Depth showing.
#[cfg(test)]
pub(crate) fn show_preset(ctx: &egui::Context, id: u64) {
    ctx.data_mut(|d| {
        d.insert_temp(
            state_id(id),
            State {
                mode: Mode::Preset(Perspective::TiltLeft),
                ..Default::default()
            },
        )
    });
}
#[cfg(test)]
pub(crate) fn show_free_corners(ctx: &egui::Context, id: u64) {
    ctx.data_mut(|d| {
        d.insert_temp(free_id(), id);
        d.insert_temp(adjusting_id(), id);
    });
}
/// The image object whose Free corner handles are showing.
pub(crate) fn free_mode(ctx: &egui::Context, id: u64) -> bool {
    ctx.data(|d| d.get_temp::<u64>(free_id()) == Some(id))
}
/// A preset button with a small picture of the shape it gives (BJ): the
/// image's outline as that preset tilts it, the label underneath.
fn preset_button(
    ui: &mut egui::Ui,
    selected: bool,
    label: &str,
    quad: [[f32; 2]; 4],
    handles: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(58., 52.), egui::Sense::click());
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::unique(("perspective-preset", label.to_owned())),
            rect,
        )
    });
    let visuals = ui.style().interact_selectable(&response, selected);
    let fill = if selected {
        egui::Color32::from_rgb(35, 65, 74)
    } else if response.hovered() {
        egui::Color32::from_rgb(52, 61, 64)
    } else {
        egui::Color32::from_rgb(36, 43, 46)
    };
    let p = ui.painter();
    p.rect_filled(rect, 6., fill);
    if selected {
        p.rect_stroke(
            rect,
            6.,
            egui::Stroke::new(1., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
    }
    let icon = egui::Rect::from_center_size(
        rect.center_top() + egui::vec2(0., 17.),
        egui::vec2(30., 20.),
    );
    let points: Vec<egui::Pos2> = quad
        .iter()
        .map(|[x, y]| icon.min + egui::vec2(x * icon.width(), y * icon.height()))
        .collect();
    let ink = if selected {
        egui::Color32::from_rgb(125, 214, 237)
    } else {
        egui::Color32::from_rgb(170, 182, 186)
    };
    p.add(egui::Shape::convex_polygon(
        points.clone(),
        ink.gamma_multiply(0.25),
        egui::Stroke::new(1.5, ink),
    ));
    if handles {
        for c in &points {
            p.circle_filled(*c, 2.5, ink);
        }
    }
    p.text(
        rect.center_bottom() - egui::vec2(0., 9.),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.),
        visuals.text_color(),
    );
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, selected, label));
    response
}
/// Shared preset row, Angle and Depth: returns the corners to use.
fn controls(
    ui: &mut egui::Ui,
    state: &mut State,
    current: Option<[[f32; 2]; 4]>,
    free: bool,
) -> Option<Option<[[f32; 2]; 4]>> {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
        if preset_button(ui, state.mode == Mode::Flat, "None", FLAT, false)
            .tip("No perspective: the image lies flat.")
            .clicked()
        {
            state.mode = Mode::Flat;
            changed = true;
        }
        for p in Perspective::ALL {
            // The picture uses a fixed, clearly visible tilt for reference.
            if preset_button(
                ui,
                state.mode == Mode::Preset(p),
                p.label(),
                p.quad(0.8, 0.6),
                false,
            )
            .tip(match p {
                Perspective::TiltLeft => "Tilt the image so its left edge leans away.",
                Perspective::TiltRight => "Tilt the image so its right edge leans away.",
                Perspective::LayBack => "Lay the image back so its top edge leans away.",
                Perspective::StandUp => "Stand the image up so its bottom edge leans away.",
            })
            .clicked()
            {
                state.mode = Mode::Preset(p);
                changed = true;
            }
        }
        if free
            && preset_button(
                ui,
                state.mode == Mode::Free,
                "Free",
                [[0.1, 0.15], [0.95, 0.], [0.85, 1.], [0., 0.8]],
                true,
            )
            .tip("Drag the four corner handles on the canvas. Hold Shift to keep edges level.")
            .clicked()
        {
            state.mode = Mode::Free;
            changed = true;
        }
    });
    if let Mode::Preset(_) = state.mode {
        let mut angle = state.angle * 100.;
        let mut depth = state.depth * 100.;
        if property_layout::slider(
            ui,
            "Angle",
            egui::Slider::new(&mut angle, 0.0..=100.0).suffix("%"),
        )
        .tip("How strongly the far edge narrows.")
        .changed()
        {
            state.angle = angle / 100.;
            changed = true;
        }
        if property_layout::slider(
            ui,
            "Depth",
            egui::Slider::new(&mut depth, 0.0..=100.0).suffix("%"),
        )
        .tip("How much the image shortens as it leans away.")
        .changed()
        {
            state.depth = depth / 100.;
            changed = true;
        }
    }
    changed.then(|| match state.mode {
        Mode::Flat => None,
        Mode::Preset(p) => Some(p.quad(state.angle, state.depth)),
        Mode::Free => current.or(Some(FLAT)),
    })
}
/// Perspective group in an image object's Style tab.
pub(crate) fn object_properties(ui: &mut egui::Ui, a: &mut Annotation) {
    let Some(bitmap) = a.image.clone() else {
        return;
    };
    if a.kind != Kind::Image
        || bitmap.is_cursor()
        || bitmap.vector().is_some()
        || bitmap.is_placeholder()
    {
        return;
    }
    property_layout::section_help(
        ui,
        "Perspective",
        "Tilt the image with a preset, or choose Free and drag its corners on the canvas. The original pixels stay; None lays it flat again. Annotations on top stay flat.",
    );
    let current = bitmap.perspective();
    let ctx = ui.ctx().clone();
    let mut state = ctx
        .data(|d| d.get_temp::<State>(state_id(a.id)))
        .unwrap_or_else(|| State {
            mode: if current.is_some() {
                Mode::Free
            } else {
                Mode::Flat
            },
            ..Default::default()
        });
    if current.is_none() && state.mode != Mode::Free {
        state.mode = Mode::Flat;
    }
    let chosen = controls(ui, &mut state, current, true);
    if let Some(quad) = chosen
        && let Ok(next) = bitmap.with_perspective(quad)
    {
        a.image = Some(std::sync::Arc::new(next));
    }
    ctx.data_mut(|d| {
        d.insert_temp(state_id(a.id), state);
        if chosen.is_some() {
            d.insert_temp(adjusting_id(), a.id);
        }
        // The corners appear when Free is chosen, not whenever a tilted image
        // is selected, and leave with the selection or another tool.
        if chosen.is_some() {
            if state.mode == Mode::Free {
                d.insert_temp(free_id(), a.id);
            } else if d.get_temp::<u64>(free_id()) == Some(a.id) {
                d.remove::<u64>(free_id());
            }
        }
    });
}

impl Editor {
    /// Free corner handles for the selected image object.
    pub(crate) fn show_perspective_handles(
        &mut self,
        ui: &mut egui::Ui,
        transform: crate::canvas_view::Transform,
    ) {
        let selected = self.selected_annotation().map(|a| a.id);
        let ended = std::mem::take(&mut self.shell.end_free_perspective);
        ui.ctx().data_mut(|d| {
            if let Some(id) = d.get_temp::<u64>(free_id())
                && (ended
                    || selected != Some(id)
                    || self.tool.is_some_and(|k| k != crate::Kind::Image))
            {
                d.remove::<u64>(free_id());
            }
        });
        // While a corner moves the canvas uses a GPU preview of the warp; the
        // engine's full-quality warp returns on release (BJ5).
        let dragging = self.perspective_handles(ui, transform);
        if dragging != self.shell.perspective_drag {
            self.shell.perspective_drag = dragging;
            self.needs_render = true;
        }
    }
    fn perspective_handles(
        &mut self,
        ui: &mut egui::Ui,
        transform: crate::canvas_view::Transform,
    ) -> bool {
        let Some(a) = self.selected_annotation() else {
            return false;
        };
        if self.selected_objects().len() != 1
            || !free_mode(ui.ctx(), a.id)
            || self.session.document.object_state(a.id).locked
        {
            return false;
        }
        let Some(bitmap) = a.image.clone() else {
            return false;
        };
        let quad = bitmap.perspective().unwrap_or(FLAT);
        let (min, max) = a.body_bounds();
        let rotation = a.rotation();
        let screen = |c: [f32; 2]| {
            transform.screen_point(rotation.point([
                min[0] + c[0] * (max[0] - min[0]),
                min[1] + c[1] * (max[1] - min[1]),
            ]))
        };
        let corners = quad.map(screen);
        let outline = egui::Stroke::new(1.5, crate::shell::ACCENT);
        for i in 0..4 {
            ui.painter()
                .line_segment([corners[i], corners[(i + 1) % 4]], outline);
        }
        let mut next = quad;
        let mut dragging = false;
        for (i, at) in corners.into_iter().enumerate() {
            let rect = egui::Rect::from_center_size(at, egui::vec2(16., 16.));
            let response = ui
                .interact(
                    rect,
                    egui::Id::unique(("perspective-corner", i)),
                    egui::Sense::drag(),
                )
                .on_hover_cursor(egui::CursorIcon::Crosshair)
                .tip("Drag to move this corner. Hold Shift to keep edges level.");
            ui.painter().circle_filled(at, 6., egui::Color32::WHITE);
            ui.painter().circle_stroke(
                at,
                6.,
                egui::Stroke::new(
                    2.,
                    if response.hovered() || response.dragged() {
                        crate::shell::ACCENT
                    } else {
                        egui::Color32::from_gray(90)
                    },
                ),
            );
            #[cfg(test)]
            ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::unique(("perspective-corner-rect", i)), rect)
            });
            dragging |= response.dragged();
            if response.dragged()
                && let Some(p) = response.interact_pointer_pos()
            {
                let local = rotation.inverse_point(transform.image_point(p));
                let mut c = [0, 1].map(|k| ((local[k] - min[k]) / (max[k] - min[k])).clamp(0., 1.));
                if ui.input(|i| i.modifiers.shift) {
                    // Keep the edge level with its horizontal neighbor.
                    let partner = [1, 0, 3, 2][i];
                    c[1] = quad[partner][1];
                }
                next[i] = c;
            }
        }
        if next != quad
            && let Ok(edited) = bitmap.with_perspective(Some(next))
        {
            let mut annotation = a.clone();
            annotation.image = Some(std::sync::Arc::new(edited));
            // A held preview, as with sliders: no document update or autosave
            // per frame, and one Undo entry when the corner is let go.
            self.preview_held_object(annotation);
        }
        dragging
    }
    /// A selected tilted image object shows in its original form for edits,
    /// unless its Perspective controls are the thing being used.
    fn shows_flat(&self, a: &Annotation) -> bool {
        a.image.as_ref().is_some_and(|b| b.perspective().is_some())
            && self.shell.perspective_adjusting != Some(a.id)
    }
    /// The canvas copy of `document` with selected tilted image objects flat.
    /// Export and the saved project keep the tilt.
    pub(crate) fn lay_selected_flat(&self, document: &mut crate::Document) {
        for a in &mut document.annotations {
            if self.is_selected(a.id)
                && self.shows_flat(a)
                && let Some(flat) = a.image.as_ref().and_then(|b| b.with_perspective(None).ok())
            {
                a.image = Some(std::sync::Arc::new(flat));
            }
        }
    }
    /// While an object is being edited the canvas shows the image flat.
    pub(crate) fn flat_while_editing(&self) -> bool {
        self.selected.is_some()
            || self.text_draft.is_some()
            || self.edit_drag.is_some()
            || self.drag_start.is_some()
    }
    /// The canvas is a cached picture, so entering or leaving editing has to
    /// rebuild it: the tilt comes back as soon as nothing is selected (BI9).
    pub(crate) fn sync_tilt_display(&mut self, ctx: &egui::Context) {
        let selected = self.selected_objects();
        self.shell.perspective_adjusting = ctx.data_mut(|d| {
            let id = d.get_temp::<u64>(adjusting_id());
            let kept = id.filter(|id| selected.iter().any(|a| a.id == *id));
            if kept.is_none() && id.is_some() {
                d.remove::<u64>(adjusting_id());
            }
            kept
        });
        let objects: Vec<u64> = selected
            .iter()
            .filter(|a| self.shows_flat(a))
            .map(|a| a.id)
            .collect();
        if objects != self.shell.flat_objects {
            self.needs_render = true;
            self.shell.flat_objects = objects;
        }
        let flat = self.flat_while_editing();
        if self.session.document.canvas_behavior.perspective.is_some()
            && flat != self.shell.tilt_flat
        {
            self.needs_render = true;
        }
        self.shell.tilt_flat = flat;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_whole_image_tilt_comes_back_when_nothing_is_selected() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        e.session.document.canvas_behavior.perspective = Some(capture_core::tilt::Tilt {
            quad: [[0.1, 0.], [0.9, 0.], [1., 1.], [0., 1.]],
            background: None,
        });
        e.selected = Some(1);
        e.needs_render = false;
        e.sync_tilt_display(&ctx);
        assert!(e.needs_render, "selecting shows it flat");
        e.needs_render = false;
        e.sync_tilt_display(&ctx);
        assert!(!e.needs_render, "no rebuild while nothing changes");
        e.selected = None;
        e.sync_tilt_display(&ctx);
        assert!(e.needs_render, "deselecting redisplays the tilt");
    }
    fn image_object(id: u64) -> Annotation {
        let pixels = std::sync::Arc::new(image::RgbaImage::from_pixel(
            40,
            30,
            image::Rgba([200, 90, 40, 255]),
        ));
        let mut a = std::sync::Arc::new(capture_core::bitmap::Bitmap::transient(pixels).unwrap())
            .annotation(id);
        a.kind = Kind::Image;
        a
    }
    fn panel(ctx: &egui::Context, a: &mut Annotation, events: Vec<egui::Event>) {
        let _ = crate::property_layout::tests::PaintedOutput(ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 500.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| object_properties(ui, a));
            },
        ));
    }
    #[test]
    fn every_preset_tilts_the_image_as_soon_as_it_is_chosen() {
        for p in Perspective::ALL {
            let ctx = egui::Context::default();
            crate::shell::configure(&ctx);
            let mut a = image_object(7);
            panel(&ctx, &mut a, vec![]);
            let rect: egui::Rect = ctx
                .data(|d| {
                    d.get_temp(egui::Id::unique((
                        "perspective-preset",
                        p.label().to_owned(),
                    )))
                })
                .unwrap();
            let at = rect.center();
            panel(&ctx, &mut a, vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                panel(
                    &ctx,
                    &mut a,
                    vec![egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            assert!(
                a.image.as_ref().unwrap().perspective().is_some(),
                "{}",
                p.label()
            );
        }
    }
    #[test]
    fn a_tilted_object_shows_flat_when_reselected_and_tilted_otherwise() {
        let ctx = egui::Context::default();
        let mut e = crate::canvas::tests::editor(&ctx);
        let mut a = image_object(90);
        let quad = Perspective::TiltLeft.quad(0.5, 0.5);
        a.image = Some(std::sync::Arc::new(
            a.image
                .as_ref()
                .unwrap()
                .with_perspective(Some(quad))
                .unwrap(),
        ));
        e.session.document.annotations.push(a);
        let shown = |e: &Editor| {
            let d = e.canvas_document();
            let a = d.annotations.iter().find(|a| a.id == 90).unwrap();
            a.image.as_ref().unwrap().perspective()
        };
        e.select_only(None);
        e.sync_tilt_display(&ctx);
        assert_eq!(shown(&e), Some(quad), "keeps the tilt when deselected");
        e.needs_render = false;
        e.select_only(Some(90));
        e.sync_tilt_display(&ctx);
        assert!(e.needs_render);
        assert_eq!(shown(&e), None, "reselecting shows the original for edits");
        ctx.data_mut(|d| d.insert_temp(adjusting_id(), 90u64));
        e.needs_render = false;
        e.sync_tilt_display(&ctx);
        assert!(e.needs_render);
        assert_eq!(shown(&e), Some(quad), "using Perspective shows the tilt");
        let saved = e.session.document.annotations.last().unwrap();
        assert_eq!(saved.image.as_ref().unwrap().perspective(), Some(quad));
        e.select_only(None);
        e.sync_tilt_display(&ctx);
        e.select_only(Some(90));
        e.sync_tilt_display(&ctx);
        assert_eq!(shown(&e), None, "the next visit starts flat again");
    }
}
