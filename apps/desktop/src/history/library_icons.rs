//! Lucide geometry from the approved Library preview. See assets/library-icons/LICENSE.
use eframe::egui;
use std::collections::HashMap;
pub(super) fn button(
    ui: &mut egui::Ui,
    name: &str,
    tooltip: &str,
    enabled: bool,
    selected: bool,
    size: egui::Vec2,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (r, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let v = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() || response.has_focus() {
            ui.painter().rect_filled(r, 4., v.bg_fill);
        }
        paint(ui.painter(), r.center(), name, v.text_color());
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, enabled, selected, tooltip)
        });
        response.on_hover_text(tooltip)
    })
    .inner
}
pub(super) fn paint(p: &egui::Painter, center: egui::Pos2, name: &str, color: egui::Color32) {
    let name = match name {
        "Library" => "images",
        "Image" => "image",
        "Steps" => "list-ordered",
        "Template" => "panels-top-left",
        "Dates" => "calendar-days",
        "Tags" => "tags",
        "Applications" => "app-window",
        "Websites" => "globe",
        "Search" => "search",
        "Pin" => "pin",
        "Refresh" => "refresh-cw",
        _ => {
            crate::ribbon::draw_icon(p, center, name, color);
            return;
        }
    };
    let px = (18. * p.ctx().pixels_per_point()).round().clamp(18., 144.) as u16;
    let cache_id = egui::Id::unique("library-icon-textures");
    let mut cache = p
        .ctx()
        .data_mut(|d| d.remove_temp::<HashMap<(String, u16), egui::TextureHandle>>(cache_id))
        .unwrap_or_default();
    let key = (name.to_owned(), px);
    if !cache.contains_key(&key)
        && let Some(image) = crate::ui_glyphs::render(name, px)
    {
        if cache.len() >= 96 {
            cache.clear();
        }
        let pixels =
            egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], image.as_raw());
        cache.insert(
            key.clone(),
            p.ctx().load_texture(
                format!("library-{name}-{px}"),
                pixels,
                egui::TextureOptions::LINEAR,
            ),
        );
    }
    if let Some(texture) = cache.get(&key) {
        p.image(
            texture.id(),
            egui::Rect::from_center_size(center, egui::vec2(18., 18.)),
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
            color,
        );
    }
    p.ctx().data_mut(|d| d.insert_temp(cache_id, cache));
}
