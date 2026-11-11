//! Export > Watermark (FX-003, approved bi-proposal 5): text or a logo,
//! tiled or in one place, with presets and an optional default. It is an
//! export setting, so the project itself never changes.
use crate::egui;
use crate::tips::Tip as _;
use capture_core::watermark::{Placement, Watermark};

pub(crate) enum Outcome {
    None,
    Changed,
    Presets(Vec<Watermark>, Option<String>),
}
#[cfg_attr(test, allow(dead_code))]
fn logo(path: &std::path::Path) -> Result<std::sync::Arc<capture_core::bitmap::Bitmap>, String> {
    let doc = capture_core::project_file::open(path).map_err(|e| e.to_string())?;
    let pixels = doc.render().map_err(|e| e.to_string())?;
    let raster = capture_core::clipboard_payload::ImagePayload::from_rgba(
        pixels.width(),
        pixels.height(),
        pixels.into_raw(),
    )
    .map_err(|e| e.to_string())?
    .image;
    capture_core::bitmap::Bitmap::new(raster)
        .map(std::sync::Arc::new)
        .map_err(|e| e.to_string())
}
/// Width of both export dialogs: the watermark's controls fit whether it is on
/// or off, so turning it on doesn't change the dialog.
pub(crate) const DIALOG_WIDTH: f32 = 520.;
/// Width of the label column in export dialogs (bj-proposal).
pub(crate) const LABEL_WIDTH: f32 = 92.;
/// One labelled row: the label in a fixed column, the controls after it.
pub(crate) fn row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_WIDTH, 24.),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_width(LABEL_WIDTH);
                ui.label(egui::RichText::new(label).size(12.));
            },
        );
        add(ui)
    })
    .inner
}
/// A titled dialog section with a divider above it and help in a tooltip.
pub(crate) fn heading(ui: &mut egui::Ui, title: &str, help: &str) {
    ui.add_space(4.);
    ui.separator();
    crate::controls::dialog_section(ui, title, Some(help));
}
#[derive(Clone, Default)]
struct Naming {
    /// `Some(old)` renames that preset; `None` saves a new one.
    rename: Option<String>,
    name: String,
}
fn naming_id() -> egui::Id {
    egui::Id::unique("watermark-naming")
}
/// Export > Watermark, approved bj-proposal layout: a switch in the heading,
/// then Preset (with + to save and a menu), Kind, Text or Logo, Placement
/// (Tile or Once with a 3 × 3 position grid), Opacity, Size and Rotation.
pub(crate) fn section(
    ui: &mut egui::Ui,
    watermark: &mut Option<Watermark>,
    presets: &[Watermark],
    default: &Option<String>,
) -> Outcome {
    let mut outcome = Outcome::None;
    let mut on = watermark.is_some();
    ui.add_space(4.);
    ui.separator();
    ui.add_space(6.);
    ui.horizontal(|ui| {
        crate::controls::dialog_section_label(
            ui,
            "Watermark",
            Some("Lay text or a logo over the exported, copied or printed output. The project itself doesn't change. The watermark marks the picture only, not transparent areas."),
        );
        if crate::controls::toggle(ui, &mut on, "")
            .tip("Add a watermark")
            .changed()
        {
            *watermark = on.then(|| {
                default
                    .as_ref()
                    .and_then(|n| presets.iter().find(|w| &w.name == n))
                    .or(presets.first())
                    .cloned()
                    .unwrap_or_default()
            });
            outcome = Outcome::Changed;
        }
    });
    let Some(w) = watermark.as_mut() else {
        return outcome;
    };
    let before = w.clone();
    let mut naming = ui.ctx().data(|d| d.get_temp::<Naming>(naming_id()));
    row(ui, "Preset", |ui| {
        let width = (ui.available_width() - 110.).max(80.);
        egui::ComboBox::from_id_salt("watermark-preset")
            .selected_text(if default.as_ref() == Some(&w.name) {
                format!("{} (default)", w.name)
            } else {
                w.name.clone()
            })
            .width(width)
            .show_ui(ui, |ui| {
                for p in presets {
                    let label = if default.as_ref() == Some(&p.name) {
                        format!("{} (default)", p.name)
                    } else {
                        p.name.clone()
                    };
                    if ui.selectable_label(p.name == w.name, label).clicked() {
                        *w = p.clone();
                    }
                }
            })
            .response
            .tip("Choose a saved watermark.");
        if ui
            .add_sized([28., 24.], egui::Button::new("+"))
            .tip("Save these settings as a preset")
            .clicked()
        {
            naming = Some(Naming {
                rename: None,
                name: String::new(),
            });
        }
        let menu = ui.add_sized([28., 24.], egui::Button::new(""));
        let c = menu.rect.center();
        for dx in [-5., 0., 5.] {
            ui.painter()
                .circle_filled(c + egui::vec2(dx, 0.), 1.8, ui.visuals().text_color());
        }
        let saved = presets.iter().any(|p| p.name == w.name);
        egui::Popup::menu(&menu).show(|ui| {
            if ui
                .add_enabled(saved, egui::Button::new("Rename…"))
                .tip("Rename this preset")
                .clicked()
            {
                naming = Some(Naming {
                    rename: Some(w.name.clone()),
                    name: w.name.clone(),
                });
                ui.close();
            }
            let mut use_default = default.as_ref() == Some(&w.name);
            if ui
                .add_enabled(saved, egui::Checkbox::new(&mut use_default, "Use by default"))
                .tip("Add this watermark to every export, copy and print, starting next time ClipDesigner opens.")
                .changed()
            {
                outcome = Outcome::Presets(
                    presets.to_vec(),
                    use_default.then(|| w.name.clone()),
                );
            }
            if ui
                .add_enabled(
                    saved,
                    egui::Button::new(
                        egui::RichText::new("Delete preset")
                            .color(egui::Color32::from_rgb(228, 135, 127)),
                    ),
                )
                .tip("Delete this saved preset. The watermark stays on for this export.")
                .clicked()
            {
                let list: Vec<Watermark> =
                    presets.iter().filter(|p| p.name != w.name).cloned().collect();
                let keep = default.clone().filter(|d| d != &w.name);
                outcome = Outcome::Presets(list, keep);
                ui.close();
            }
        })
        .map(|r| r.response)
        .unwrap_or(menu)
        .tip("Preset menu: rename, use by default, delete");
    });
    if let Some(n) = naming.as_mut() {
        let mut done = None;
        row(ui, "", |ui| {
            let field = ui.add(
                egui::TextEdit::singleline(&mut n.name)
                    .hint_text("Preset name")
                    .char_limit(60)
                    .desired_width((ui.available_width() - 170.).max(80.)),
            );
            if !field.has_focus() && !field.lost_focus() {
                field.request_focus();
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let label = if n.rename.is_some() { "Rename" } else { "Save" };
            if ui
                .add_enabled(!n.name.trim().is_empty(), egui::Button::new(label))
                .clicked()
                || (enter && !n.name.trim().is_empty())
            {
                done = Some(true);
            }
            if ui.button("Cancel").clicked() {
                done = Some(false);
            }
        });
        if let Some(apply) = done {
            if apply {
                let name = n.name.trim().to_owned();
                let mut list: Vec<Watermark> = presets
                    .iter()
                    .filter(|p| p.name != name && Some(&p.name) != n.rename.as_ref())
                    .cloned()
                    .collect();
                let mut saved = w.clone();
                saved.name = name.clone();
                if saved.validate().is_ok() {
                    list.push(saved);
                    w.name = name.clone();
                    let keep = match (&n.rename, default) {
                        (Some(old), Some(d)) if d == old => Some(name),
                        _ => default.clone(),
                    };
                    outcome = Outcome::Presets(list, keep);
                }
            }
            naming = None;
        }
    }
    ui.ctx().data_mut(|d| match &naming {
        Some(n) => {
            d.insert_temp(naming_id(), n.clone());
        }
        None => {
            d.remove::<Naming>(naming_id());
        }
    });
    // BI10: Image stays chosen while no logo is loaded yet, so the Choose
    // button is always there; load errors stay until the next attempt.
    let kind_id = egui::Id::unique("watermark-logo-mode");
    let error_id = egui::Id::unique("watermark-error");
    let mut logo_mode = w.image.is_some()
        || ui
            .ctx()
            .data(|d| d.get_temp::<bool>(kind_id))
            .unwrap_or(false);
    row(ui, "Type", |ui| {
        if ui
            .selectable_label(!logo_mode, "Text")
            .tip("Use words such as CONFIDENTIAL or DRAFT.")
            .clicked()
        {
            logo_mode = false;
            w.image = None;
        }
        if ui
            .selectable_label(logo_mode, "Image")
            .tip("Use a logo or picture from a PNG, JPEG, WebP, BMP or SVG file.")
            .clicked()
        {
            logo_mode = true;
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(kind_id, logo_mode));
    if !logo_mode {
        row(ui, "Text", |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut w.text)
                    .char_limit(120)
                    .desired_width((ui.available_width() - 110.).max(80.)),
            );
            crate::color_widgets::canvas_rgba(ui, &mut w.color);
        });
    } else {
        row(ui, "Logo", |ui| {
            if let Some(image) = &w.image {
                let [width, height] = image.size();
                ui.weak(format!("{width} × {height}"));
            } else {
                ui.weak("No image chosen yet");
            }
            let label = if w.image.is_some() {
                "Choose another…"
            } else {
                "Choose image…"
            };
            if ui
                .button(label)
                .tip("Pick the logo or picture to lay over the output")
                .clicked()
            {
                #[cfg(not(test))]
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp", "svg"])
                    .pick_file()
                {
                    match logo(&path) {
                        Ok(bitmap) => {
                            w.image = Some(bitmap);
                            ui.ctx().data_mut(|d| d.remove::<String>(error_id));
                        }
                        Err(e) => ui.ctx().data_mut(|d| {
                            d.insert_temp(error_id, format!("Couldn't use that image: {e}"));
                        }),
                    }
                }
            }
        });
    }
    row(ui, "Placement", |ui| {
        let tiled = w.placement == Placement::Tile;
        if ui
            .selectable_label(tiled, "Tile")
            .tip("Repeat the watermark across the whole output.")
            .clicked()
        {
            w.placement = Placement::Tile;
        }
        if ui
            .selectable_label(!tiled, "Once")
            .tip("Show the watermark once, in the position chosen on the grid.")
            .clicked()
            && tiled
        {
            w.placement = Placement::BottomRight;
        }
        // 3 × 3 position grid.
        let (rect, _) = ui.allocate_exact_size(egui::vec2(76., 44.), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 5., egui::Color32::from_rgb(21, 26, 28));
        let cell = egui::vec2(20., 10.);
        for (i, p) in Placement::GRID.into_iter().enumerate() {
            let at = rect.min + egui::vec2(5. + (i % 3) as f32 * 23., 5. + (i / 3) as f32 * 12.);
            let r = egui::Rect::from_min_size(at, cell);
            let id = ui.scope_id().with(("watermark-place", i));
            let response = ui.interact(r, id, egui::Sense::click()).tip(p.label());
            let on = w.placement == p;
            ui.painter().rect_filled(
                r,
                2.,
                if on {
                    crate::shell::ACCENT
                } else if response.hovered() {
                    egui::Color32::from_rgb(60, 70, 74)
                } else if tiled {
                    egui::Color32::from_rgb(36, 42, 45)
                } else {
                    egui::Color32::from_rgb(44, 52, 56)
                },
            );
            if response.clicked() {
                w.placement = p;
            }
        }
        ui.weak(if tiled {
            "Repeats across the output".to_owned()
        } else {
            w.placement.label().to_owned()
        });
    });
    for (label, help, slider) in [
        (
            "Opacity",
            "How strongly the watermark shows.",
            egui::Slider::new(&mut w.opacity, 5..=100).suffix(" %"),
        ),
        (
            "Size",
            "Watermark height as a share of the output's shorter side.",
            egui::Slider::new(&mut w.size, 5..=60).suffix(" %"),
        ),
    ] {
        row(ui, label, |ui| ui.add(slider).tip(help));
    }
    row(ui, "Rotation", |ui| {
        ui.add(egui::Slider::new(&mut w.rotation, -90.0..=90.0).suffix("°"))
            .tip("Turn the watermark. −30° is a common diagonal.")
    });
    if let Some(e) = ui.ctx().data(|d| d.get_temp::<String>(error_id)) {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
    if matches!(outcome, Outcome::None) && *w != before {
        outcome = Outcome::Changed;
    }
    outcome
}
