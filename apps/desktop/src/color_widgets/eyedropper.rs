//! Canvas-only sampling. No desktop capture or system clipboard access.
use super::*;
use crate::{Editor, canvas_view::Transform};
use std::sync::Arc;
#[derive(Clone)]
struct Pick {
    draft: egui::Id,
    popup: egui::Id,
    owner: Option<(Vec<u64>, Option<crate::Kind>)>,
    image: Option<Arc<image::RgbaImage>>,
    picked: Option<[u8; 4]>,
}
fn key() -> egui::Id {
    egui::Id::unique("canvas-color-sample")
}
pub(super) fn active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Pick>(key()).is_some())
}
pub(super) fn owns_input(ctx: &egui::Context) -> bool {
    let waiting = ctx
        .data(|d| d.get_temp::<bool>(key().with("release")))
        .unwrap_or(false);
    if waiting {
        if !ctx.input(|i| i.pointer.primary_down()) {
            let frame = ctx.cumulative_frame_nr();
            ctx.data_mut(|d| {
                d.remove::<bool>(key().with("release"));
                d.insert_temp(key().with("consumed"), frame);
            });
        }
        return true;
    }
    active(ctx)
        || ctx.data(|d| d.get_temp::<u64>(key().with("consumed")))
            == Some(ctx.cumulative_frame_nr())
}
pub(super) fn cancel(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Pick>(key()));
}
pub(crate) fn consume_escape(ctx: &egui::Context) -> bool {
    if active(ctx) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        cancel(ctx);
        true
    } else {
        false
    }
}
pub(super) fn take(ctx: &egui::Context, draft: egui::Id) -> Option<[u8; 4]> {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        let p = d.get_temp::<Pick>(key())?;
        if p.draft != draft {
            return None;
        }
        let color = p.picked?;
        d.remove::<Pick>(key());
        d.insert_temp(key().with("consumed"), frame);
        d.insert_temp(key().with("release"), true);
        Some(color)
    })
}
pub(super) fn button(ui: &mut egui::Ui, popup: egui::Id, draft: egui::Id) {
    let active = active(ui.ctx());
    let r = ui.add_sized([28., 24.], egui::Button::new("").selected(active));
    let c = r.rect.center();
    let ink = ui.visuals().text_color();
    let p = ui.painter();
    let stroke = egui::Stroke::new(1.5, ink);
    p.add(egui::Shape::line(
        vec![
            c + egui::vec2(-7., 7.),
            c + egui::vec2(-5., 1.),
            c + egui::vec2(3., -7.),
            c + egui::vec2(7., -3.),
            c + egui::vec2(-1., 5.),
            c + egui::vec2(-7., 7.),
        ],
        stroke,
    ));
    p.line_segment(
        [c + egui::vec2(0., -6.), c + egui::vec2(6., 0.)],
        egui::Stroke::new(3., ink),
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, "Pick color from canvas"));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("eyedropper-button"), r.rect));
    if r.clicked() {
        if active {
            cancel(ui.ctx());
        } else {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    key(),
                    Pick {
                        draft,
                        popup,
                        owner: None,
                        image: None,
                        picked: None,
                    },
                )
            });
        }
    }
    r.tip(if active {
        "Cancel color sampling · Esc"
    } else {
        "Pick color from canvas, including transparency"
    });
}
impl Editor {
    pub(crate) fn sample_canvas_color(&mut self, ui: &egui::Ui, t: Transform) -> bool {
        let ctx = ui.ctx();
        let Some(mut pick) = ctx.data(|d| d.get_temp::<Pick>(key())) else {
            return owns_input(ctx);
        };
        if !egui::Popup::is_id_open(ctx, pick.popup) || !ctx.input(|i| i.focused) {
            cancel(ctx);
            return false;
        }
        if consume_escape(ctx) {
            return true;
        }
        if pick.picked.is_some() {
            return true;
        }
        let owner = (self.selection_ordered_ids(), self.tool);
        if pick.owner.as_ref().is_some_and(|old| *old != owner) {
            cancel(ctx);
            return true;
        }
        if pick.image.is_none() && self.is_video() {
            // Video: sample the frame on screen, not the empty still (BI6).
            match self.video_frame_pixels() {
                Some(image) => {
                    pick.image = Some(Arc::new(image));
                    pick.owner = Some(owner.clone());
                }
                None => {
                    ctx.data_mut(|d| d.insert_temp(key(), pick));
                    return true;
                }
            }
        }
        if pick.image.is_none() {
            let mut document = self.preview_document();
            if self.shell.annotations_hidden {
                document.annotations.clear();
                document.object_states.clear();
                document.guide = None;
            }
            match document.render() {
                Ok(image) => {
                    pick.image = Some(Arc::new(image));
                    pick.owner = Some(owner);
                }
                Err(error) => {
                    self.status = error.to_string();
                    cancel(ctx);
                    return true;
                }
            }
        }
        let pointer = ctx.input(|i| i.pointer.interact_pos());
        if let Some(pos) = pointer.filter(|p| t.image.intersect(t.viewport).contains(*p)) {
            let covered = ctx
                .data(|d| d.get_temp::<LivePopup>(live_popup_key()))
                .is_some_and(|popup| popup.rect.contains(pos) || popup.swatch.contains(pos));
            if !covered {
                let point = t.image_point(pos);
                let image = pick.image.as_ref().unwrap();
                let x = point[0].floor().max(0.) as u32;
                let y = point[1].floor().max(0.) as u32;
                if x < image.width() && y < image.height() {
                    let color = image.get_pixel(x, y).0;
                    ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                    let p = ui.painter().with_clip_rect(t.viewport);
                    let rect = egui::Rect::from_min_size(
                        pos + egui::vec2(18., 18.),
                        egui::vec2(132., 28.),
                    );
                    p.rect_filled(rect, 3, egui::Color32::from_gray(35));
                    egui::color_picker::show_color_at(
                        &p,
                        egui::Color32::from_rgba_unmultiplied(
                            color[0], color[1], color[2], color[3],
                        ),
                        egui::Rect::from_min_size(
                            rect.min + egui::vec2(4., 4.),
                            egui::vec2(20., 20.),
                        ),
                    );
                    p.text(
                        rect.min + egui::vec2(30., 14.),
                        egui::Align2::LEFT_CENTER,
                        hex(color),
                        egui::FontId::monospace(12.),
                        egui::Color32::WHITE,
                    );
                    if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary)) {
                        pick.picked = Some(color);
                        ctx.request_repaint();
                    }
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(key(), pick));
        true
    }
}
#[cfg(test)]
mod tests;
