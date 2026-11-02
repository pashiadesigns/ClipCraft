use super::*;

#[derive(Clone)]
struct Drag {
    id: u64,
    origin: egui::Pos2,
    rect: egui::Rect,
}
fn key() -> egui::Id {
    egui::Id::unique("guide-section-reorder")
}
pub(super) fn active(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Drag>(key()).is_some())
}

pub(super) fn destination(
    document: &Document,
    transform: Transform,
    id: u64,
    pointer: egui::Pos2,
) -> Option<(usize, usize, egui::Rect)> {
    if !transform.viewport.contains(pointer) {
        return None;
    }
    let layout = document.guide.as_ref()?;
    let page = (0..layout.page_count()).find(|&p| {
        pages::page_rect(document, p, transform)
            .expand2(egui::vec2(pages::rail_width(document, transform), 0.))
            .contains(pointer)
    })?;
    let rect = pages::page_rect(document, page, transform);
    let rows: Vec<_> = controls(document, transform)
        .into_iter()
        .filter(|c| {
            c.id != id
                && layout
                    .sections
                    .iter()
                    .any(|s| s.id == c.id && s.page == page)
        })
        .collect();
    let index = rows
        .iter()
        .position(|c| pointer.y < transform.screen_point([0., (c.top + c.bottom) * 0.5]).y)
        .unwrap_or(rows.len());
    let y = rows
        .get(index)
        .map(|c| transform.screen_point([0., c.top]).y)
        .or_else(|| {
            rows.last()
                .map(|c| transform.screen_point([0., c.bottom]).y)
        })
        .unwrap_or(rect.top() + layout.content_top(page) * transform.scale);
    Some((
        page,
        index,
        egui::Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), 3.)),
    ))
}

impl Editor {
    pub(super) fn show_section_reorder(
        &mut self,
        ui: &mut egui::Ui,
        transform: Transform,
        visible: &[egui::Rect],
    ) -> bool {
        if self.guide_drag.is_some() {
            return false;
        }
        let document = &self.session.document;
        for c in controls(document, transform) {
            for (side, rect) in c.grips.into_iter().enumerate() {
                if !visible.contains(&rect) || !transform.viewport.contains_rect(rect) {
                    continue;
                }
                let r = ui
                    .interact(
                        rect,
                        egui::Id::unique(("section-grip", c.id, side)),
                        egui::Sense::drag(),
                    )
                    .on_hover_cursor(egui::CursorIcon::Grab)
                    .on_hover_text("Drag section to reorder or move to another page");
                let color = if r.hovered() || r.dragged() {
                    crate::shell::ACCENT
                } else {
                    egui::Color32::from_gray(185)
                };
                for x in [-3., 3.] {
                    for y in [-6., 0., 6.] {
                        ui.painter()
                            .circle_filled(rect.center() + egui::vec2(x, y), 1.5, color);
                    }
                }
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique(("section-grip-test", c.id, side)), rect)
                });
                if r.drag_started() {
                    let origin = ui
                        .input(|i| i.pointer.press_origin())
                        .unwrap_or(rect.center());
                    let bounds = egui::Rect::from_min_max(
                        egui::pos2(c.left, transform.screen_point([0., c.top]).y),
                        egui::pos2(c.right, transform.screen_point([0., c.bottom]).y),
                    );
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            key(),
                            Drag {
                                id: c.id,
                                origin,
                                rect: bounds,
                            },
                        )
                    });
                }
            }
        }
        let Some(drag) = ui.ctx().data(|d| d.get_temp::<Drag>(key())) else {
            return false;
        };
        if ui.input(|i| !i.focused || i.key_pressed(egui::Key::Escape))
            || document
                .guide
                .as_ref()
                .is_none_or(|g| !g.sections.iter().any(|s| s.id == drag.id))
        {
            ui.ctx().data_mut(|d| d.remove::<Drag>(key()));
            return true;
        }
        let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else {
            return true;
        };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let p = ui.painter().with_clip_rect(transform.viewport);
        p.rect_filled(drag.rect, 0, crate::shell::ACCENT.gamma_multiply(0.2));
        let ghost = drag.rect.translate(pointer - drag.origin);
        p.rect_filled(
            ghost,
            4,
            egui::Color32::from_rgba_unmultiplied(21, 150, 190, 65),
        );
        p.rect_stroke(
            ghost,
            4,
            egui::Stroke::new(2., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
        // Keep the drag caption visible even when a grip carries the section
        // partly outside the workspace. The section outline retains its size.
        let caption = egui::Rect::from_min_size(
            egui::pos2(
                (pointer.x + 12.).clamp(
                    transform.viewport.left(),
                    (transform.viewport.right() - 132.).max(transform.viewport.left()),
                ),
                (pointer.y + 16.).clamp(
                    transform.viewport.top(),
                    (transform.viewport.bottom() - 26.).max(transform.viewport.top()),
                ),
            ),
            egui::vec2(132., 26.),
        );
        p.rect_filled(caption, 4, egui::Color32::from_rgb(37, 70, 83));
        p.text(
            caption.center(),
            egui::Align2::CENTER_CENTER,
            "Moving section",
            egui::FontId::proportional(15.),
            egui::Color32::WHITE,
        );
        let target = destination(document, transform, drag.id, pointer);
        if let Some((page, index, line)) = target {
            p.rect_filled(line, 0, crate::shell::ACCENT);
            for x in [line.left(), line.right()] {
                p.circle_filled(egui::pos2(x, line.center().y), 4., crate::shell::ACCENT);
            }
            let label =
                egui::Rect::from_min_size(line.min + egui::vec2(8., -25.), egui::vec2(175., 23.));
            p.rect_filled(label, 4, crate::shell::ACCENT);
            p.text(
                label.center(),
                egui::Align2::CENTER_CENTER,
                format!("Page {} · position {}", page + 1, index + 1),
                egui::FontId::proportional(12.),
                egui::Color32::WHITE,
            );
        }
        if !ui.input(|i| i.pointer.primary_down()) {
            ui.ctx().data_mut(|d| d.remove::<Drag>(key()));
            if let Some((page, index, _)) = target {
                self.apply_section(
                    Action::Relocate {
                        id: drag.id,
                        page,
                        index,
                    },
                    transform,
                );
            }
        }
        true
    }
}
