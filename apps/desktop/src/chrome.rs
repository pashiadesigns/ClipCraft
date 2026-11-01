//! Original window chrome. Document close requests still go through Editor.
#[cfg(windows)]
mod windows;
pub fn install_native(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    windows::install(crate::hotkey::owner(cc));
    #[cfg(not(windows))]
    let _ = cc;
}
pub fn record_ui_geometry(ui: &egui::Ui) {
    let key = egui::Id::unique("frame-focus-diagnostic");
    let focused = ui.input(|i| i.focused);
    if ui.ctx().data(|d| d.get_temp::<bool>(key)) == Some(focused) {
        return;
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, focused));
    let Some(path) = std::env::var_os("CLIPCRAFT_FRAME_DIAGNOSTICS") else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 131072) {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let rect = |r: egui::Rect| [r.left(), r.top(), r.right(), r.bottom()];
        let _ = writeln!(
            f,
            "{}",
            serde_json::json!({"ui_focus":focused,"viewport":rect(ui.ctx().viewport_rect()),"root":rect(ui.max_rect()),"clip":rect(ui.clip_rect()),"pixels_per_point":ui.ctx().pixels_per_point()})
        );
    }
}
use crate::egui::{
    self, Color32, CursorIcon, Pos2, Rect, ResizeDirection, Sense, Stroke, ViewportCommand,
};

pub fn title_and_controls(ui: &mut egui::Ui, title: &str) -> bool {
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    let mut close = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (name, symbol) in [
            ("Close window", 0),
            (
                if maximized {
                    "Restore window"
                } else {
                    "Maximize window"
                },
                1,
            ),
            ("Minimize window", 2),
        ] {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(43.0, 25.0), Sense::click());
            response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, name));
            if response.hovered() || response.has_focus() {
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    if symbol == 0 {
                        Color32::from_rgb(180, 43, 48)
                    } else {
                        Color32::from_gray(78)
                    },
                );
            }
            let r = Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0));
            let stroke = Stroke::new(1.0, Color32::WHITE);
            match symbol {
                0 => {
                    ui.painter()
                        .line_segment([r.left_top(), r.right_bottom()], stroke);
                    ui.painter()
                        .line_segment([r.right_top(), r.left_bottom()], stroke);
                }
                1 => {
                    if maximized {
                        ui.painter().rect_stroke(
                            r.translate(egui::vec2(2.0, -2.0)),
                            0.0,
                            stroke,
                            egui::StrokeKind::Inside,
                        );
                    }
                    ui.painter()
                        .rect_filled(r, 0.0, Color32::from_rgb(46, 51, 53));
                    ui.painter()
                        .rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
                }
                _ => {
                    ui.painter()
                        .line_segment([r.left_center(), r.right_center()], stroke);
                }
            }
            if response.clone().on_hover_text(name).clicked() {
                match symbol {
                    0 => close = true,
                    1 => ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
                    _ => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                }
            }
        }
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 25.0),
            Sense::click_and_drag(),
        );
        let painter = ui.painter().with_clip_rect(egui::Rect::from_min_max(
            rect.min + egui::vec2(8., 0.),
            rect.max - egui::vec2(8., 0.),
        ));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(12.0),
            Color32::from_gray(235),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, title));
        let drag_id = egui::Id::unique("window_title_drag");
        if !maximized
            && ctx.input(|i| i.focused)
            && response.is_pointer_button_down_on()
            && let Some(point) = ctx.input(|i| i.pointer.press_origin())
        {
            ctx.data_mut(|d| {
                if d.get_temp::<Pos2>(drag_id).is_none() {
                    d.insert_temp(drag_id, point);
                }
            });
        }
        if response.double_clicked() {
            ctx.data_mut(|d| d.remove::<Pos2>(drag_id));
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if !maximized
            && ctx.input(|i| i.focused && i.pointer.primary_down())
            && let Some(anchor) = ctx.data(|d| d.get_temp::<Pos2>(drag_id))
        {
            if let Some((point, outer)) =
                ctx.input(|i| i.pointer.interact_pos().zip(i.viewport().outer_rect))
            {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(outer.min + (point - anchor)));
            }
            if !ctx.input(|i| i.pointer.primary_down() && i.focused) {
                ctx.data_mut(|d| d.remove::<Pos2>(drag_id));
            }
        }
        if maximized || !ctx.input(|i| i.focused && i.pointer.primary_down()) {
            ctx.data_mut(|d| d.remove::<Pos2>(drag_id));
        }
        response.on_hover_text(title);
    });
    close
}

fn resize_direction(rect: Rect, point: Pos2) -> Option<(ResizeDirection, CursorIcon)> {
    if !rect.contains(point) {
        return None;
    }
    let left = point.x < rect.left() + 4.0;
    let right = point.x > rect.right() - 4.0;
    let top = point.y < rect.top() + 4.0;
    let bottom = point.y > rect.bottom() - 4.0;
    use ResizeDirection::*;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some((NorthWest, CursorIcon::ResizeNwSe)),
        (_, true, true, _) => Some((NorthEast, CursorIcon::ResizeNeSw)),
        (true, _, _, true) => Some((SouthWest, CursorIcon::ResizeNeSw)),
        (_, true, _, true) => Some((SouthEast, CursorIcon::ResizeNwSe)),
        (true, _, _, _) => Some((West, CursorIcon::ResizeHorizontal)),
        (_, true, _, _) => Some((East, CursorIcon::ResizeHorizontal)),
        (_, _, true, _) => Some((North, CursorIcon::ResizeVertical)),
        (_, _, _, true) => Some((South, CursorIcon::ResizeVertical)),
        _ => None,
    }
}

pub fn resize_edges(ctx: &egui::Context) {
    let id = egui::Id::unique("window_resize");
    if ctx.input(|i| {
        !i.focused
            || i.viewport().maximized.unwrap_or(false)
            || i.viewport().fullscreen.unwrap_or(false)
    }) {
        ctx.data_mut(|d| d.remove::<ResizeDrag>(id));
        return;
    }
    if let Some(drag) = ctx.data(|d| d.get_temp::<ResizeDrag>(id)) {
        if !ctx.input(|i| i.pointer.primary_down()) {
            ctx.data_mut(|d| d.remove::<ResizeDrag>(id));
            return;
        }
        if let Some((point, outer)) =
            ctx.input(|i| i.pointer.interact_pos().zip(i.viewport().outer_rect))
        {
            let rect = resized_rect(
                drag.rect,
                drag.direction,
                outer.min + point.to_vec2() - drag.pointer,
            );
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(rect.min));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(rect.size()));
        }
        if !ctx.input(|i| i.pointer.primary_down() && i.focused) {
            ctx.data_mut(|d| d.remove::<ResizeDrag>(id));
        }
        return;
    }
    if let Some(point) = ctx.input(|i| i.pointer.hover_pos())
        && let Some((direction, cursor)) = resize_direction(ctx.viewport_rect(), point)
    {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
            && let Some(rect) = ctx.input(|i| i.viewport().outer_rect)
        {
            ctx.data_mut(|d| {
                d.insert_temp(
                    id,
                    ResizeDrag {
                        rect,
                        pointer: rect.min + point.to_vec2(),
                        direction,
                    },
                )
            });
        }
    }
}

#[derive(Clone, Copy)]
struct ResizeDrag {
    rect: Rect,
    pointer: Pos2,
    direction: ResizeDirection,
}

fn resized_rect(mut rect: Rect, direction: ResizeDirection, delta: egui::Vec2) -> Rect {
    use ResizeDirection::*;
    if matches!(direction, West | NorthWest | SouthWest) {
        rect.min.x = (rect.min.x + delta.x).min(rect.max.x - 880.0);
    }
    if matches!(direction, East | NorthEast | SouthEast) {
        rect.max.x = (rect.max.x + delta.x).max(rect.min.x + 880.0);
    }
    if matches!(direction, North | NorthEast | NorthWest) {
        rect.min.y = (rect.min.y + delta.y).min(rect.max.y - 580.0);
    }
    if matches!(direction, South | SouthEast | SouthWest) {
        rect.max.y = (rect.max.y + delta.y).max(rect.min.y + 580.0);
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn focus_loss_maximize_and_released_pointer_cancel_stale_resize() {
        for (focused, maximized, held) in [
            (false, false, true),
            (true, true, true),
            (true, false, false),
        ] {
            let ctx = egui::Context::default();
            let id = egui::Id::unique("window_resize");
            let area = Rect::from_min_size(Pos2::ZERO, egui::vec2(1920., 1080.));
            ctx.data_mut(|d| {
                d.insert_temp(
                    id,
                    ResizeDrag {
                        rect: area,
                        pointer: area.right_center(),
                        direction: ResizeDirection::East,
                    },
                )
            });
            let mut raw = egui::RawInput {
                screen_rect: Some(area),
                focused,
                ..Default::default()
            };
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .maximized = Some(maximized);
            raw.viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .outer_rect = Some(area);
            raw.events
                .push(egui::Event::PointerMoved(egui::pos2(1750., 500.)));
            if held {
                raw.events.push(egui::Event::PointerButton {
                    pos: egui::pos2(1750., 500.),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let out = crate::property_layout::tests::PaintedOutput(
                ctx.run_ui(raw, |ui| resize_edges(ui.ctx())),
            );
            assert!(ctx.data(|d| d.get_temp::<ResizeDrag>(id).is_none()));
            assert!(
                out.viewport_output
                    .values()
                    .all(|v| v.commands.iter().all(|c| !matches!(
                        c,
                        ViewportCommand::InnerSize(_) | ViewportCommand::OuterPosition(_)
                    )))
            );
        }
    }
    #[test]
    fn resize_hit_regions_do_not_take_document_or_outside_clicks() {
        let rect = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(981.0, 687.0));
        assert_eq!(
            resize_direction(rect, rect.left_top()).unwrap().0,
            ResizeDirection::NorthWest
        );
        assert_eq!(
            resize_direction(rect, rect.right_bottom()).unwrap().0,
            ResizeDirection::SouthEast
        );
        assert_eq!(
            resize_direction(rect, rect.left_center()).unwrap().0,
            ResizeDirection::West
        );
        assert_eq!(
            resize_direction(rect, rect.center_bottom()).unwrap().0,
            ResizeDirection::South
        );
        assert!(resize_direction(rect, rect.center()).is_none());
        assert!(resize_direction(rect, rect.min + egui::vec2(5.0, 5.0)).is_none());
        assert!(resize_direction(rect, rect.min - egui::vec2(1.0, 1.0)).is_none());
    }
    #[test]
    fn resize_preserves_the_opposite_edge_and_minimum_size() {
        let rect = Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(981.0, 687.0));
        let resized = resized_rect(rect, ResizeDirection::NorthWest, egui::vec2(300.0, 300.0));
        assert_eq!(resized.max, rect.max);
        assert_eq!(resized.size(), egui::vec2(880.0, 580.0));
        let resized = resized_rect(rect, ResizeDirection::SouthEast, egui::vec2(100.0, 80.0));
        assert_eq!(resized.min, rect.min);
        assert_eq!(resized.size(), egui::vec2(1081.0, 767.0));
    }
}
