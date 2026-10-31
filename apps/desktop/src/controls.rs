//! Shared editable controls. Keep slider interactions native to egui, including
//! keyboard stepping, accessible values, typed input, and drag transactions.
use crate::egui;
use egui::{
    class::HasClasses as _,
    theme::{DefaultStyle, StyleProvider},
    widget_style::{ButtonStyle, StyleArgs, TextEditStyle, WidgetState},
};

// Numeric values use buttons until clicked, then become TextEdits. Theme both
// states as the same entry field without framing unrelated ribbon commands.
struct InputTheme;
fn input_frame(args: &StyleArgs<'_>, frame: &mut egui::Frame) {
    frame.fill = egui::Color32::from_rgb(65, 71, 74);
    frame.corner_radius = 2.into();
    let color = match args.state {
        WidgetState::Active => args.style.visuals.selection.bg_fill,
        WidgetState::Hovered => egui::Color32::from_gray(178),
        _ => egui::Color32::from_gray(132),
    };
    // Use an inset stroke, keeping hit targets and layout stable across states.
    frame.stroke = egui::Stroke::new(1.0, color);
    frame.outer_margin = (-1).into();
}
impl StyleProvider<ButtonStyle> for InputTheme {
    fn style(&mut self, args: &StyleArgs<'_>) -> ButtonStyle {
        let mut style: ButtonStyle = DefaultStyle.style(args);
        if args.classes.has_class(&egui::DragValue::CLASS) {
            input_frame(args, &mut style.atom_layout.frame);
        }
        style
    }
}
impl StyleProvider<TextEditStyle> for InputTheme {
    fn style(&mut self, args: &StyleArgs<'_>) -> TextEditStyle {
        let mut style: TextEditStyle = DefaultStyle.style(args);
        if !args.classes.has_class(&egui::TextEdit::CLASS_READ_ONLY) {
            input_frame(args, &mut style.atom_layout.frame);
        }
        style
    }
}
pub(super) fn configure(ctx: &egui::Context) {
    ctx.add_widget_theme::<ButtonStyle>(InputTheme);
    ctx.add_widget_theme::<TextEditStyle>(InputTheme);
}

/// Share the small circular handle diameter with the canvas corner controls.
pub(super) const HANDLE_RADIUS: f32 = 4.5;

/// `add_sized(Label)` centers the allocated text even with halign(Min).
/// Reserve a real column and lay its text out from the column's left edge.
pub(super) fn field_label(ui: &mut egui::Ui, title: &str, size: [f32; 2]) -> egui::Response {
    ui.allocate_ui_with_layout(
        size.into(),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(size[0]);
            ui.add(egui::Label::new(title).wrap().halign(egui::Align::Min))
        },
    )
    .inner
}

/// Property titles always precede their control. The whole labeled row is a
/// keyboard-accessible target; only the compact track is painted as a switch.
pub(super) fn toggle(ui: &mut egui::Ui, value: &mut bool, title: &str) -> egui::Response {
    ui.horizontal(|ui| {
        let label = ui.label(title);
        let (id, target) = ui.allocate_space(egui::vec2(28.0, 22.0));
        let mut response = ui.interact(label.rect.union(target), id, egui::Sense::click());
        if response.clicked() {
            *value = !*value;
            response.mark_changed();
        }
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *value, title)
        });
        let track = egui::Rect::from_center_size(target.center(), egui::vec2(24.0, 13.0));
        let visuals = ui.style().interact(&response);
        let fill = if *value {
            ui.visuals().selection.bg_fill
        } else {
            egui::Color32::from_gray(78)
        };
        ui.painter().rect_filled(track, 7, fill);
        if response.hovered() || response.has_focus() {
            ui.painter()
                .rect_stroke(track, 7, visuals.fg_stroke, egui::StrokeKind::Inside);
        }
        let x = if *value {
            track.right() - 6.5
        } else {
            track.left() + 6.5
        };
        ui.painter().circle_filled(
            egui::pos2(x, track.center().y),
            HANDLE_RADIUS,
            visuals.text_color(),
        );
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique(("property-toggle", title)),
                (label.rect, target),
            )
        });
        response
    })
    .inner
}

pub(super) fn slider(ui: &mut egui::Ui, slider: egui::Slider<'_>) -> egui::Response {
    ui.scope(|ui| {
        // Native fill follows the actual value mapping (including logarithmic
        // and reversed ranges) and stays under the thumb during input.
        ui.spacing_mut().slider_rail_height = 4.0;
        let visuals = ui.visuals_mut();
        visuals.slider_trailing_fill = true;
        visuals.handle_shape = egui::style::HandleShape::Circle;
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(70, 77, 80);
        for widget in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(98));
            widget.expansion = -1.5;
            widget.corner_radius = 3.into();
        }
        // Display precision must never rewrite a live canvas gesture. egui's
        // default Always clamping also rounds existing fractional values each
        // frame, accidentally starting a sidebar transaction during rounding.
        ui.add(
            slider
                .trailing_fill(true)
                .clamping(egui::SliderClamping::Edits),
        )
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn property_toggle_labels_precede_small_handles_and_support_pointer_keyboard_and_disabled_state()
     {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut value = false;
        let mut frame = |events, enabled| {
            let mut response = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        response = Some(toggle(ui, &mut value, "Text Outline"));
                    });
                },
            );
            (
                response.unwrap(),
                crate::property_layout::tests::PaintedOutput(output),
                value,
            )
        };
        frame(vec![], true);
        let (r, out, _) = frame(vec![], true);
        let (label, control) = ctx.data(|d| {
            d.get_temp::<(egui::Rect, egui::Rect)>(egui::Id::unique((
                "property-toggle",
                "Text Outline",
            )))
            .unwrap()
        });
        assert!(label.right() < control.left());
        assert!(
            out.shapes
                .iter()
                .any(|s| matches!(&s.shape,egui::Shape::Circle(c) if c.radius==HANDLE_RADIUS))
        );
        let click = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        frame(click(control.center(), true), true);
        let (changed, _, on) = frame(click(control.center(), false), true);
        assert!(changed.changed() && on);
        ctx.memory_mut(|m| m.request_focus(r.id));
        let key = |pressed| {
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        let (changed, _, on) = frame(key(true), true);
        assert!(changed.changed() && !on);
        frame(key(false), true);
        frame(click(label.center(), true), true);
        assert!(frame(click(label.center(), false), true).2);
        frame(vec![], false);
        frame(click(control.center(), true), false);
        let (changed, _, on) = frame(click(control.center(), false), false);
        assert!(!changed.changed() && on);
    }
    #[test]
    fn slider_fill_follows_live_native_thumb_and_preserves_numeric_field() {
        let ctx = egui::Context::default();
        crate::shell::configure(&ctx);
        let mut value = 20.0_f64;
        let frame = |value: &mut f64, events| {
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| {
                    rect = slider(ui, egui::Slider::new(value, 1.0..=100.0).logarithmic(true)).rect;
                },
            );
            (rect, crate::property_layout::tests::PaintedOutput(output))
        };
        frame(&mut value, vec![]);
        let (rect, _) = frame(&mut value, vec![]);
        let pos = egui::pos2(rect.left() + 85.0, rect.center().y);
        let (_, output) = frame(
            &mut value,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_ne!(value, 20.0);
        let thumb = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Circle(c) = &s.shape {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap();
        let fill = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Rect(r) = &s.shape {
                    (r.fill == ctx.style_of(egui::Theme::Dark).visuals.selection.bg_fill
                        && r.rect.height() == 4.0)
                        .then_some(r)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(fill.rect.left() <= rect.left() + 1.0);
        assert!(fill.rect.right() >= thumb.center.x && fill.rect.right() <= thumb.center.x + 3.1);
        assert!(output.shapes.iter().any(
            |s| matches!(&s.shape,egui::Shape::Rect(r) if r.fill==egui::Color32::from_rgb(65,71,74))
        ));
        if let Ok(root) = std::env::var("CLIPCRAFT_HEADLESS_CONTROLS_DIR") {
            let snapshot_ctx = egui::Context::default();
            crate::shell::configure(&snapshot_ctx);
            let render = || {
                crate::property_layout::tests::PaintedOutput(snapshot_ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(420.0, 210.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        ui.horizontal_top(|ui| {
                            crate::shell::tool_button_label(
                                ui,
                                crate::shell::TOOLS
                                    .iter()
                                    .find(|t| t.id == capture_core::toolbar::ToolId::Crop)
                                    .copied()
                                    .unwrap(),
                                "Crop / Cut",
                                false,
                                false,
                            );
                            ui.vertical(|ui| {
                                for id in [
                                    capture_core::toolbar::ToolId::Crop,
                                    capture_core::toolbar::ToolId::CutOut,
                                ] {
                                    crate::shell::tool_button(
                                        ui,
                                        crate::shell::TOOLS
                                            .iter()
                                            .find(|t| t.id == id)
                                            .copied()
                                            .unwrap(),
                                        id == capture_core::toolbar::ToolId::CutOut,
                                        false,
                                    );
                                }
                            });
                            ui.vertical(|ui| {
                                for mut n in [0.0, 25.0, 50.0, 75.0, 100.0] {
                                    slider(ui, egui::Slider::new(&mut n, 0.0..=100.0));
                                }
                            });
                        });
                    },
                ))
            };
            let first = render();
            let output = render();
            crate::property_layout::tests::dump_meshes(
                &snapshot_ctx,
                &root,
                "ChooserAndSliders",
                first,
                output,
            );
        }
    }
}
