//! Field layout mirrors the approved Page Setup design; all controls share a right edge.
use super::page_setup::Draft;
use crate::egui;
use crate::tips::Tip as _;
use capture_core::guide_sections::{Alignment, NumberStyle};

fn row<R>(ui: &mut egui::Ui, label: &str, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let row = ui.horizontal(|ui| {
        let (label_rect, _) = ui.allocate_exact_size(egui::vec2(132., 28.), egui::Sense::hover());
        ui.painter().text(
            label_rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.),
            ui.visuals().text_color(),
        );
        let width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(width, 28.),
            egui::Layout::right_to_left(egui::Align::Center),
            f,
        )
        .inner
    });
    // Every row can be ringed by a guided error ("page-setup:<label>").
    crate::guided::mark(ui, &format!("page-setup:{label}"), row.response.rect);
    row.inner
}
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(4.);
    ui.strong(title);
    ui.separator();
}
fn toggle(ui: &mut egui::Ui, label: &str, v: &mut bool) -> bool {
    row(ui, label, |ui| ui.checkbox(v, "").changed())
}
fn number(
    ui: &mut egui::Ui,
    n: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    speed: f64,
    width: f32,
) -> bool {
    ui.add_sized(
        [width, 28.],
        egui::DragValue::new(n)
            .range(range)
            .speed(speed)
            .max_decimals(3),
    )
    .changed()
}
fn distance(ui: &mut egui::Ui, label: &str, value: &mut f64, cm: bool) -> bool {
    let factor = if cm { 72. / 2.54 } else { 72. };
    let mut n = *value / factor;
    let changed = row(ui, label, |ui| number(ui, &mut n, 0.0..=200., 0.01, 104.));
    if changed {
        *value = n * factor;
    }
    changed
}
fn combo_align(ui: &mut egui::Ui, id: &str, a: &mut Alignment) {
    egui::ComboBox::from_id_salt(id)
        .width(85.)
        .selected_text(match a {
            Alignment::Left => "Left",
            Alignment::Center => "Center",
            Alignment::Right => "Right",
        })
        .show_ui(ui, |ui| {
            for (v, n) in [
                (Alignment::Left, "Left"),
                (Alignment::Center, "Center"),
                (Alignment::Right, "Right"),
            ] {
                ui.selectable_value(a, v, n);
            }
        });
}
pub(super) fn fields(ui: &mut egui::Ui, d: &mut Draft) {
    let s = &mut d.settings;
    let unit = if s.centimeters { "cm" } else { "in" };
    match d.tab {
        0 => {
            const PRESETS: [(&str, [f64; 2]); 5] = [
                ("Letter", [612., 792.]),
                ("A4", [595.276, 841.89]),
                ("A5", [419.528, 595.276]),
                ("Legal", [612., 1008.]),
                ("Tabloid", [792., 1224.]),
            ];
            let mut trim = s.trim_points();
            trim.sort_by(f64::total_cmp);
            let name = PRESETS
                .iter()
                .find(|(_, p)| (trim[0] - p[0]).abs() < 0.02 && (trim[1] - p[1]).abs() < 0.02)
                .map_or("Custom", |p| p.0);
            row(ui, "Paper size", |ui| {
                egui::ComboBox::from_id_salt("page-preset")
                    .width(ui.available_width())
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for (name, mut points) in PRESETS {
                            if ui.selectable_label(false, name).clicked() {
                                if s.size[0] > s.size[1] {
                                    points.swap(0, 1);
                                }
                                s.physical_points = Some(points);
                                s.size = points.map(|n| (n * s.ppi / 72.).round() as u32);
                            }
                        }
                        ui.selectable_label(name == "Custom", "Custom")
                            .tip("Edit the dimensions below for a custom size.");
                    });
            });
            row(ui, "Orientation", |ui| {
                let w = (ui.available_width() - 5.) / 2.;
                let landscape = s.size[0] > s.size[1];
                for (v, label) in [(true, "Landscape"), (false, "Portrait")] {
                    if ui
                        .add_sized([w, 28.], egui::Button::new(label).selected(v == landscape))
                        .clicked()
                        && v != landscape
                    {
                        s.size.swap(0, 1);
                        if let Some(p) = &mut s.physical_points {
                            p.swap(0, 1);
                        }
                    }
                }
            });
            row(ui, "Units", |ui| {
                egui::ComboBox::from_id_salt("page-unit")
                    .width(ui.available_width())
                    .selected_text(if s.centimeters {
                        "Centimeters"
                    } else {
                        "Inches"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut s.centimeters, false, "Inches");
                        ui.selectable_value(&mut s.centimeters, true, "Centimeters");
                    });
            });
            let field_width = ((ui.available_width() - 150.) / 2.).min(94.);
            row(ui, "", |ui| {
                ui.add_sized(
                    [field_width, 18.],
                    egui::Label::new(egui::RichText::new("Pixels").weak()),
                );
                ui.add_sized(
                    [field_width, 18.],
                    egui::Label::new(
                        egui::RichText::new(if s.centimeters {
                            "Centimeters"
                        } else {
                            "Inches"
                        })
                        .weak(),
                    ),
                );
            });
            for (i, label) in [(0, "Width"), (1, "Height")] {
                row(ui, label, |ui| {
                    if ui
                        .add_sized(
                            [field_width, 28.],
                            egui::DragValue::new(&mut s.size[i]).range(128..=32768),
                        )
                        .changed()
                    {
                        s.physical_points = None;
                    }
                    let factor = if s.centimeters { 72. / 2.54 } else { 72. };
                    let mut value = s.trim_points()[i] / factor;
                    if number(ui, &mut value, 0.1..=200., 0.01, field_width) {
                        let mut p = s.trim_points();
                        p[i] = value * factor;
                        s.physical_points = Some(p);
                        s.size[i] = (p[i] * s.ppi / 72.).round() as u32;
                    }
                });
            }
            row(ui, "Resolution (PPI)", |ui| {
                let p = s.trim_points();
                if number(ui, &mut s.ppi, 24.0..=2400., 1., 104.) {
                    s.physical_points = Some(p);
                    s.size = p.map(|n| (n * s.ppi / 72.).round() as u32);
                }
            });
            section(ui, "Page dimensions");
            let p = s.trim_points();
            let factor = if s.centimeters { 72. / 2.54 } else { 72. };
            ui.weak(format!(
                "{:.3} × {:.3} {unit} · {} × {} pixels at {} PPI",
                p[0] / factor,
                p[1] / factor,
                s.size[0],
                s.size[1],
                s.ppi
            ));
            ui.weak("Print at Actual Size / 100% to preserve these dimensions. Raising PPI does not add detail to existing screenshots.");
        }
        1 => {
            toggle(ui, "Link margins", &mut s.linked_margins);
            for (i, label) in ["Top", "Bottom", "Left", "Right"].iter().enumerate() {
                if distance(
                    ui,
                    &format!("{label} ({unit})"),
                    &mut s.margins[i],
                    s.centimeters,
                ) && s.linked_margins
                {
                    s.margins = [s.margins[i]; 4];
                }
            }
            section(ui, "Content when resizing");
            row(ui, "Resize behavior", |ui| {
                egui::ComboBox::from_id_salt("page-resize-behavior")
                    .width(ui.available_width())
                    .selected_text(if d.scale {
                        "Scale proportionally"
                    } else {
                        "Keep sizes / reflow"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.scale, false, "Keep sizes / reflow");
                        ui.selectable_value(&mut d.scale, true, "Scale proportionally");
                    });
            });
            toggle(ui, "Overflow pages", &mut d.automatic);
            toggle(ui, "Distribute sections", &mut d.distribute);
            ui.weak("Whole sections move together. A section must fit within the page's printable height.");
        }
        2 => {
            ui.spacing_mut().item_spacing.y = 7.;
            for (label, t) in [("Header", &mut s.header), ("Footer", &mut s.footer)] {
                toggle(ui, label, &mut t.enabled);
                ui.add_enabled_ui(t.enabled, |ui| {
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 94.).max(90.);
                        ui.add_sized(
                            [w, 28.],
                            crate::unicode_input::TextEdit::singleline(&mut t.text)
                                .hint_text(label),
                        );
                        combo_align(ui, label, &mut t.alignment);
                    });
                    distance(
                        ui,
                        &format!("From edge ({unit})"),
                        &mut t.distance,
                        s.centimeters,
                    );
                });
            }
            section(ui, "Page numbering");
            toggle(ui, "Show page numbers", &mut s.numbers);
            ui.add_enabled_ui(s.numbers, |ui| {
                row(ui, "Format", |ui| {
                    ui.add_sized(
                        [58., 28.],
                        egui::DragValue::new(&mut s.number_start).range(1..=100000),
                    )
                    .tip("Start at");
                    egui::ComboBox::from_id_salt("number-style")
                        .width(105.)
                        .selected_text(match s.number_style {
                            NumberStyle::Decimal => "1, 2, 3",
                            NumberStyle::Roman => "I, II, III",
                            NumberStyle::Letters => "A, B, C",
                            NumberStyle::PageTotal => "Page 1 of 5",
                        })
                        .show_ui(ui, |ui| {
                            for (v, n) in [
                                (NumberStyle::Decimal, "1, 2, 3"),
                                (NumberStyle::Roman, "I, II, III"),
                                (NumberStyle::Letters, "A, B, C"),
                                (NumberStyle::PageTotal, "Page 1 of 5"),
                            ] {
                                ui.selectable_value(&mut s.number_style, v, n);
                            }
                        });
                });
                row(ui, "Position", |ui| {
                    combo_align(ui, "number-align", &mut s.number_alignment);
                    egui::ComboBox::from_id_salt("number-position")
                        .width(85.)
                        .selected_text(if s.number_in_header {
                            "Header"
                        } else {
                            "Footer"
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut s.number_in_header, true, "Header");
                            ui.selectable_value(&mut s.number_in_header, false, "Footer");
                        });
                });
                toggle(ui, "Number first page", &mut s.number_first);
            });
        }
        _ => {
            let mut enabled = s.bleed > 0.;
            if toggle(ui, "Include bleed", &mut enabled) {
                s.bleed = if enabled { 9. } else { 0. };
            }
            ui.add_enabled_ui(enabled, |ui| {
                distance(
                    ui,
                    &format!("All edges ({unit})"),
                    &mut s.bleed,
                    s.centimeters,
                );
            });
            ui.weak(
                "Extend artwork past the trim edge to fill the bleed. Empty areas remain empty.",
            );
            section(ui, "Crop marks");
            toggle(ui, "Show crop marks", &mut s.crop_marks);
            ui.add_enabled_ui(s.crop_marks, |ui| {
                distance(
                    ui,
                    &format!("Offset ({unit})"),
                    &mut s.mark_offset,
                    s.centimeters,
                );
                distance(
                    ui,
                    &format!("Length ({unit})"),
                    &mut s.mark_length,
                    s.centimeters,
                );
                row(ui, "Weight (pt)", |ui| {
                    number(ui, &mut s.mark_weight, 0.05..=4., 0.05, 104.)
                });
            });
            section(ui, "Output size");
            ui.weak("Bleed and crop marks extend beyond the finished page size. PDF preserves trim and bleed boxes. Choose paper large enough for the output and your printer's margins.");
        }
    }
}
