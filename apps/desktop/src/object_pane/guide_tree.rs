use super::*;
use capture_core::{Document, guide_sections::Action};

pub(super) enum Row<'a> {
    Header {
        key: egui::Id,
        label: String,
        depth: usize,
        target: Target,
    },
    Object(&'a Annotation),
}
#[derive(Clone, Copy)]
pub(super) enum Target {
    Page(usize),
    Section { id: u64, page: usize, index: usize },
}
impl Target {
    fn action(self, to: Self) -> Option<Action> {
        match (self, to) {
            (Self::Page(page), Self::Page(index)) if page != index => {
                Some(Action::MovePage { page, index })
            }
            (
                Self::Section { id, page, .. },
                Self::Section {
                    id: other,
                    page: target,
                    index,
                },
            ) if page == target && id != other => Some(Action::Move { id, index }),
            _ => None,
        }
    }
}
fn page_key(page: usize) -> egui::Id {
    egui::Id::unique(("objects-page-expanded", page))
}
fn section_key(id: u64) -> egui::Id {
    egui::Id::unique(("objects-section-expanded", id))
}
fn expanded(ui: &egui::Ui, key: egui::Id, default: bool) -> bool {
    ui.ctx()
        .data(|d| d.get_temp::<bool>(key))
        .unwrap_or(default)
}
pub(super) fn rows<'a>(
    ui: &egui::Ui,
    document: &Document,
    objects: Vec<&'a Annotation>,
) -> Vec<Row<'a>> {
    let Some(guide) = &document.guide else {
        return objects.into_iter().map(Row::Object).collect();
    };
    let mut rows = Vec::new();
    for page in 0..guide.page_count() {
        let key = page_key(page);
        rows.push(Row::Header {
            key,
            label: format!("Page {}", page + 1),
            depth: 0,
            target: Target::Page(page),
        });
        if !expanded(ui, key, false) {
            continue;
        }
        for (index, (global_index, section)) in guide
            .sections
            .iter()
            .enumerate()
            .filter(|(_, s)| s.page == page)
            .enumerate()
        {
            let key = section_key(section.id);
            rows.push(Row::Header {
                key,
                label: format!("Section {} · {:?}", index + 1, section.kind),
                depth: 1,
                target: Target::Section {
                    id: section.id,
                    page,
                    index: global_index,
                },
            });
            if expanded(ui, key, true) {
                rows.extend(
                    objects
                        .iter()
                        .filter(|a| section.objects.contains(&a.id))
                        .map(|a| Row::Object(a)),
                );
            }
        }
        rows.extend(
            objects
                .iter()
                .filter(|a| {
                    !guide.sections.iter().any(|s| s.objects.contains(&a.id))
                        && guide.page_at(a.start) == page
                })
                .map(|a| Row::Object(a)),
        );
    }
    rows
}
pub(super) fn header(
    ui: &mut egui::Ui,
    key: egui::Id,
    label: &str,
    depth: usize,
    target: Target,
) -> Option<Action> {
    let open = expanded(ui, key, depth > 0);
    let (rect, r) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 31.),
        egui::Sense::click_and_drag(),
    );
    ui.painter().rect_filled(
        rect,
        3,
        Color32::from_gray(if depth == 0 { 49 } else { 42 }),
    );
    let center = egui::pos2(rect.left() + 12. + depth as f32 * 12., rect.center().y);
    let points = if open {
        vec![
            center + egui::vec2(-4., -2.),
            center + egui::vec2(0., 2.),
            center + egui::vec2(4., -2.),
        ]
    } else {
        vec![
            center + egui::vec2(-2., -4.),
            center + egui::vec2(2., 0.),
            center + egui::vec2(-2., 4.),
        ]
    };
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.5, Color32::WHITE)));
    ui.painter().text(
        egui::pos2(center.x + 12., center.y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.),
        Color32::WHITE,
    );
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    if r.clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(key, !open));
    }
    #[cfg(test)]
    ui.ctx().data_mut(|d| d.insert_temp(key.with("rect"), rect));
    r.dnd_set_drag_payload(target);
    if r.dragged()
        && let Some(pointer) = ui.input(|i| i.pointer.interact_pos())
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let ghost = egui::Rect::from_min_size(pointer + egui::vec2(12., 12.), rect.size());
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            key.with("drag-preview"),
        ));
        painter.rect(
            ghost,
            4,
            Color32::from_rgb(53, 77, 89),
            Stroke::new(1., crate::shell::ACCENT),
            egui::StrokeKind::Inside,
        );
        painter.text(
            ghost.left_center() + egui::vec2(12., 0.),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.),
            Color32::WHITE,
        );
        #[cfg(test)]
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique("guide-tree-drag-preview"), ghost));
    }
    if let Some(from) = r.dnd_hover_payload::<Target>()
        && from.action(target).is_some()
    {
        let below = match (*from, target) {
            (Target::Page(a), Target::Page(b)) => a < b,
            (Target::Section { index: a, .. }, Target::Section { index: b, .. }) => a < b,
            _ => false,
        };
        let y = if below { rect.bottom() } else { rect.top() };
        ui.painter().line_segment(
            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
            Stroke::new(3., crate::shell::ACCENT),
        );
        if let Some(from) = r.dnd_release_payload::<Target>() {
            return from.action(target);
        }
    }
    None
}
