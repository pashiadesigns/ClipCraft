//! Headless design preview only. This does not change the application theme.
use super::*;
use crate::property_layout::tests::{PaintedOutput, dump_meshes};

#[test]
#[ignore = "Optional headless design preview; no desktop access"]
fn render_blue_surround_design_preview() {
    let root = std::env::var("CLIPDESIGNER_RIBBON_DESIGN_DIR").expect("Preview output directory");
    for (connected, tab, name) in [
        (false, RibbonTab::Insert, "Current-Home"),
        (true, RibbonTab::Insert, "Proposed-Home"),
        (true, RibbonTab::Formatting, "Proposed-ToolSettings"),
    ] {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        ctx.style_mut_of(egui::Theme::Dark, |s| s.animation_time = 0.0);
        let mut e = crate::canvas::tests::editor(&ctx);
        e.shell.ribbon_tab = tab;
        e.add_step([120.0, 120.0]);
        let mut render = || {
            PaintedOutput(ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 220.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let blue = Color32::from_rgb(24, 145, 193);
                    e.show_quick_access(ui);
                    egui::Panel::top("design_chrome")
                        .frame(egui::Frame::new().fill(Color32::from_rgb(37, 39, 41)))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let tabs = egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(8, 2))
                                .show(ui, |ui| ui.horizontal(|ui| e.ribbon_tabs(ui)).inner)
                                .inner;
                            let ribbon = egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(5, 3))
                                .show(ui, |ui| {
                                    egui::Frame::new()
                                        .fill(Color32::from_rgb(50, 53, 55))
                                        .corner_radius(10)
                                        .stroke(if connected {
                                            Stroke::new(2.0, blue)
                                        } else {
                                            Stroke::NONE
                                        })
                                        .inner_margin(egui::Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.set_min_width(ui.available_width());
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 3.0;
                                                match tab {
                                                    RibbonTab::Insert => e.insert_ribbon(ui, 58.0),
                                                    _ => e.formatting_ribbon(ui, 70.0),
                                                }
                                            });
                                        })
                                        .response
                                        .rect
                                })
                                .inner;
                            if connected {
                                let active =
                                    tabs[if tab == RibbonTab::Insert { 0 } else { 1 }].rect;
                                // One uninterrupted blue neck meets the rounded outline.
                                // A single panel avoids a separator through the join.
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_max(
                                        egui::pos2(active.left(), active.bottom() - 4.0),
                                        egui::pos2(active.right(), ribbon.top() + 2.0),
                                    ),
                                    0.0,
                                    blue,
                                );
                            }
                        });
                    ui.painter().rect_filled(
                        ui.available_rect_before_wrap(),
                        0.0,
                        Color32::from_rgb(27, 29, 31),
                    );
                },
            ))
        };
        let first = render();
        let output = render();
        dump_meshes(&ctx, &root, name, first, output);
    }
}
