use super::*;
use capture_core::{Document, guide_sections::Action};

pub(super) enum Row<'a> {
    Header {
        key: egui::Id,
        label: String,
        depth: usize,
        page: Option<usize>,
    },
    Object(&'a Annotation),
}
#[derive(Clone, Copy)]
struct PageDrag(usize);
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
            page: Some(page),
        });
        if !expanded(ui, key, false) {
            continue;
        }
        for (index, section) in guide.sections.iter().filter(|s| s.page == page).enumerate() {
            let key = section_key(section.id);
            rows.push(Row::Header {
                key,
                label: format!("Section {} · {:?}", index + 1, section.kind),
                depth: 1,
                page: None,
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
    page: Option<usize>,
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
    if let Some(page) = page {
        r.dnd_set_drag_payload(PageDrag(page));
        if r.dnd_hover_payload::<PageDrag>().is_some() {
            ui.painter().rect_stroke(
                rect,
                3,
                Stroke::new(2., crate::shell::ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(from) = r.dnd_release_payload::<PageDrag>()
            && from.0 != page
        {
            return Some(Action::MovePage {
                page: from.0,
                index: page,
            });
        }
    }
    None
}
